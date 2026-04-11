use std::{
    env, fs,
    io::{ErrorKind, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{self, Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::Duration,
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, watch},
    time,
};
use tracing::{debug, error, info};

use crate::{
    api,
    capture::{
        screenshot,
        window::{self, WindowInfo},
    },
    config::AppConfig,
    storage::{
        db::StorageDb,
        metrics,
        models::{Capture, NewCapture},
        prune,
    },
};

const INTERNAL_DAEMON_SUBCOMMAND: &str = "__daemon-child";
const PID_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const PID_POLL_INTERVAL: Duration = Duration::from_millis(50);
const STOP_TIMEOUT: Duration = Duration::from_secs(15);
const DAILY_PRUNE_HOUR_UTC: u32 = 2;

pub static CAPTURE_PAUSED: AtomicBool = AtomicBool::new(false);

static APP_CHANGE_TRIGGER_SENDER: OnceLock<Mutex<Option<mpsc::Sender<()>>>> = OnceLock::new();

fn app_change_trigger_sender_slot() -> &'static Mutex<Option<mpsc::Sender<()>>> {
    APP_CHANGE_TRIGGER_SENDER.get_or_init(|| Mutex::new(None))
}

fn set_app_change_trigger_sender(sender: mpsc::Sender<()>) {
    let mut guard = app_change_trigger_sender_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(sender);
}

fn clear_app_change_trigger_sender() {
    let mut guard = app_change_trigger_sender_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = None;
}

extern "C" fn on_active_app_change() {
    let sender = app_change_trigger_sender_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    if let Some(sender) = sender {
        let _ = sender.try_send(());
    }
}

struct AppChangeListenerGuard;

impl AppChangeListenerGuard {
    fn start(sender: mpsc::Sender<()>) -> Self {
        set_app_change_trigger_sender(sender);
        window::start_app_change_listener(on_active_app_change);
        Self
    }
}

impl Drop for AppChangeListenerGuard {
    fn drop(&mut self) {
        window::stop_app_change_listener();
        clear_app_change_trigger_sender();
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonState {
    Running,
    Stopped,
}

impl DaemonState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatus {
    pub state: DaemonState,
    pub pid: Option<u32>,
    pub uptime_secs: u64,
    pub captures_today: u64,
    pub storage_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PidRecord {
    pid: u32,
    started_at: DateTime<Utc>,
}

#[derive(Debug)]
struct PidFileGuard {
    path: PathBuf,
    record: PidRecord,
}

impl PidFileGuard {
    fn acquire(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create daemon pid directory at {}",
                    parent.display()
                )
            })?;
        }

        for _ in 0..2 {
            if let Some(active) = load_live_pid_record(&path)? {
                bail!("daemon is already running with pid {}", active.pid);
            }

            let record = PidRecord {
                pid: process::id(),
                started_at: Utc::now(),
            };
            let serialized = toml::to_string(&record).context("failed to serialize pid record")?;

            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    file.write_all(serialized.as_bytes()).with_context(|| {
                        format!("failed to write pid file at {}", path.display())
                    })?;
                    file.sync_all().with_context(|| {
                        format!("failed to flush pid file at {}", path.display())
                    })?;
                    return Ok(Self { path, record });
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("failed to create pid file at {}", path.display())
                    })
                }
            }
        }

        Err(anyhow!(
            "failed to acquire daemon pid file at {}",
            path.display()
        ))
    }
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        let _ = remove_pid_file_if_matches(&self.path, &self.record);
    }
}

pub async fn run_foreground(config: &AppConfig) -> Result<()> {
    let home = runtime_home_dir()?;
    run_foreground_at_home(config, &home).await
}

pub async fn start_background(config: &AppConfig) -> Result<u32> {
    let home = runtime_home_dir()?;
    start_background_at_home(config, &home).await
}

pub async fn stop(config: &AppConfig) -> Result<u32> {
    let home = runtime_home_dir()?;
    stop_at_home(config, &home).await
}

pub fn status(config: &AppConfig) -> Result<DaemonStatus> {
    let home = runtime_home_dir()?;
    status_at_home(config, &home)
}

async fn run_foreground_at_home(config: &AppConfig, home: &Path) -> Result<()> {
    prune::run_startup_prune(config, home)?;
    let capture_loop = CaptureLoop::open(config.clone(), home.to_path_buf())?;
    let listener = api::server::bind(config).await?;
    let _pid_guard = PidFileGuard::acquire(AppConfig::pid_file_path(home))?;

    info!(
        idle_interval_secs = config.capture.idle_interval_secs,
        jpeg_quality = config.capture.jpeg_quality,
        port = config.server.port,
        "capture loop started"
    );

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut capture_task = tokio::spawn(run_capture_loop(
        capture_loop,
        config.capture.idle_interval_secs,
        config.storage.max_age_days,
        shutdown_rx.clone(),
    ));
    let mut server_task = tokio::spawn(api::server::serve(
        listener,
        config.clone(),
        home.to_path_buf(),
        shutdown_rx,
    ));

    tokio::select! {
        result = shutdown_signal() => {
            result?;
            info!("shutdown signal received");
            let _ = shutdown_tx.send(true);
            capture_task.await.context("capture loop task panicked")??;
            server_task.await.context("api server task panicked")??;
            Ok(())
        }
        result = &mut capture_task => {
            let _ = shutdown_tx.send(true);
            let capture_result = result.context("capture loop task panicked")?;
            server_task.await.context("api server task panicked")??;
            match capture_result {
                Ok(()) => Err(anyhow!("capture loop exited before shutdown signal")),
                Err(err) => Err(err),
            }
        }
        result = &mut server_task => {
            let _ = shutdown_tx.send(true);
            let server_result = result.context("api server task panicked")?;
            capture_task.await.context("capture loop task panicked")??;
            match server_result {
                Ok(()) => Err(anyhow!("api server exited before shutdown signal")),
                Err(err) => Err(err),
            }
        }
    }
}

async fn run_capture_loop(
    mut capture_loop: CaptureLoop,
    idle_interval_secs: u64,
    retention_days: u32,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let idle_interval = Duration::from_secs(idle_interval_secs);
    let (app_change_trigger_tx, mut app_change_trigger_rx) = mpsc::channel(1);
    let _app_change_listener_guard = AppChangeListenerGuard::start(app_change_trigger_tx);

    let mut next_capture_at = time::Instant::now();
    let mut last_prune_date: Option<NaiveDate> = None;
    loop {
        tokio::select! {
            _ = time::sleep_until(next_capture_at) => {}
            maybe_trigger = app_change_trigger_rx.recv() => {
                if maybe_trigger.is_none() {
                    next_capture_at = time::Instant::now() + idle_interval;
                    continue;
                }
                debug!("capture triggered by active application change");
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
                continue;
            }
        }

        if CAPTURE_PAUSED.load(Ordering::SeqCst) {
            debug!("capture is paused; skipping capture cycle");
        } else if let Err(err) = capture_loop.capture_once() {
            error!(error = %err, "capture cycle failed");
        }

        if let Err(err) = run_daily_prune_if_due(
            &capture_loop,
            retention_days,
            &mut last_prune_date,
            Utc::now(),
        ) {
            error!(error = %err, retention_days, "daily prune cycle failed");
        }

        next_capture_at = time::Instant::now() + idle_interval;
    }

    Ok(())
}

fn run_daily_prune_if_due(
    capture_loop: &CaptureLoop,
    retention_days: u32,
    last_prune_date: &mut Option<NaiveDate>,
    now: DateTime<Utc>,
) -> Result<()> {
    if retention_days == 0 {
        return Ok(());
    }

    let today = now.date_naive();
    if now.hour() < DAILY_PRUNE_HOUR_UTC || *last_prune_date == Some(today) {
        return Ok(());
    }

    let rows_deleted = capture_loop.db.prune_old_data(retention_days)?;
    *last_prune_date = Some(today);
    info!(
        rows_deleted,
        retention_days,
        run_date = %today,
        "completed daily data prune"
    );

    Ok(())
}
async fn start_background_at_home(_config: &AppConfig, home: &Path) -> Result<u32> {
    let pid_path = AppConfig::pid_file_path(home);
    if let Some(active) = load_live_pid_record(&pid_path)? {
        bail!("daemon is already running with pid {}", active.pid);
    }

    let executable = env::current_exe().context("failed to resolve current executable")?;
    let mut child = Command::new(executable);
    child
        .arg(INTERNAL_DAEMON_SUBCOMMAND)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    unsafe {
        child.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(())
        });
    }

    let mut child = child.spawn().context("failed to spawn background daemon")?;
    wait_for_background_start(&mut child, &pid_path).await
}

async fn stop_at_home(_config: &AppConfig, home: &Path) -> Result<u32> {
    let pid_path = AppConfig::pid_file_path(home);
    let Some(active) = load_live_pid_record(&pid_path)? else {
        bail!("daemon is not running");
    };

    send_signal(active.pid, libc::SIGTERM)?;
    wait_for_process_exit(&active, &pid_path).await?;
    Ok(active.pid)
}

fn status_at_home(config: &AppConfig, home: &Path) -> Result<DaemonStatus> {
    let pid_path = AppConfig::pid_file_path(home);
    let active = load_live_pid_record(&pid_path)?;
    let captures_today = count_captures_today(config, home)?;
    let storage_bytes = metrics::directory_size(&config.storage_root(home))?;
    let uptime_secs = active
        .as_ref()
        .map(|record| {
            let elapsed = Utc::now().signed_duration_since(record.started_at);
            u64::try_from(elapsed.num_seconds().max(0)).unwrap_or(0)
        })
        .unwrap_or(0);

    Ok(DaemonStatus {
        state: if active.is_some() {
            DaemonState::Running
        } else {
            DaemonState::Stopped
        },
        pid: active.as_ref().map(|record| record.pid),
        uptime_secs,
        captures_today,
        storage_bytes,
    })
}

async fn wait_for_background_start(child: &mut Child, pid_path: &Path) -> Result<u32> {
    let deadline = time::Instant::now() + PID_WAIT_TIMEOUT;

    loop {
        if let Some(status) = child
            .try_wait()
            .context("failed to poll background daemon child")?
        {
            bail!("background daemon exited before startup completed: {status}");
        }

        if let Some(active) = load_live_pid_record(pid_path)? {
            return Ok(active.pid);
        }

        if time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("timed out waiting for background daemon startup");
        }

        time::sleep(PID_POLL_INTERVAL).await;
    }
}

async fn wait_for_process_exit(record: &PidRecord, pid_path: &Path) -> Result<()> {
    let deadline = time::Instant::now() + STOP_TIMEOUT;

    loop {
        match read_pid_record(pid_path)? {
            None => return Ok(()),
            Some(current) if current != *record => return Ok(()),
            Some(_) => {}
        }

        if !process_exists(record.pid)? {
            remove_pid_file_if_matches(pid_path, record)?;
            return Ok(());
        }

        if time::Instant::now() >= deadline {
            bail!(
                "timed out waiting for daemon process {} to exit",
                record.pid
            );
        }

        time::sleep(PID_POLL_INTERVAL).await;
    }
}

async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .context("failed to register SIGTERM handler")?;

        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.context("failed to listen for SIGINT")?;
            }
            _ = terminate.recv() => {}
        }

        Ok(())
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for shutdown signal")
    }
}

fn count_captures_today(config: &AppConfig, home: &Path) -> Result<u64> {
    let db_path = config.storage_root(home).join("screencap.db");
    if !db_path.exists() {
        return Ok(0);
    }

    let db = StorageDb::open_at_path(&db_path)
        .with_context(|| format!("failed to open capture database at {}", db_path.display()))?;

    metrics::count_captures_today(&db, Utc::now())
}

fn runtime_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable is not set"))
}

fn read_pid_record(path: &Path) -> Result<Option<PidRecord>> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read pid file at {}", path.display()))
        }
    };

    toml::from_str(&raw)
        .with_context(|| format!("failed to parse pid file at {}", path.display()))
        .map(Some)
}

fn load_live_pid_record(path: &Path) -> Result<Option<PidRecord>> {
    let Some(record) = read_pid_record(path)? else {
        return Ok(None);
    };

    if process_exists(record.pid)? {
        return Ok(Some(record));
    }

    remove_pid_file_if_matches(path, &record)?;
    Ok(None)
}

fn remove_pid_file_if_matches(path: &Path, record: &PidRecord) -> Result<()> {
    let Some(existing) = read_pid_record(path)? else {
        return Ok(());
    };

    if existing != *record {
        return Ok(());
    }

    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => {
            Err(err).with_context(|| format!("failed to remove pid file at {}", path.display()))
        }
    }
}

fn process_exists(pid: u32) -> Result<bool> {
    let pid = i32::try_from(pid).context("pid exceeds supported range")?;
    let result = unsafe { libc::kill(pid, 0) };
    if result == 0 {
        return Ok(!process_is_zombie(pid)?);
    }

    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(code) if code == libc::ESRCH => Ok(false),
        Some(code) if code == libc::EPERM => Ok(true),
        _ => Err(err).with_context(|| format!("failed to probe daemon process {pid}")),
    }
}

#[cfg(target_os = "macos")]
fn process_is_zombie(pid: i32) -> Result<bool> {
    use std::mem::{size_of, MaybeUninit};

    let mut info = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let info_size = i32::try_from(size_of::<libc::proc_bsdinfo>())
        .expect("proc_bsdinfo size should fit in i32");
    let bytes_read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast::<std::ffi::c_void>(),
            info_size,
        )
    };
    if bytes_read == 0 {
        let err = std::io::Error::last_os_error();
        return match err.raw_os_error() {
            Some(code) if code == libc::ESRCH => Ok(false),
            _ => Err(err).with_context(|| format!("failed to inspect daemon process {pid}")),
        };
    }
    if bytes_read != info_size {
        return Err(anyhow!(
            "unexpected proc_pidinfo size for process {pid}: expected {info_size}, got {bytes_read}"
        ));
    }

    let info = unsafe { info.assume_init() };
    Ok(info.pbi_status == libc::SZOMB)
}

#[cfg(not(target_os = "macos"))]
fn process_is_zombie(_pid: i32) -> Result<bool> {
    Ok(false)
}

fn send_signal(pid: u32, signal: i32) -> Result<()> {
    let pid = i32::try_from(pid).context("pid exceeds supported range")?;
    let result = unsafe { libc::kill(pid, signal) };
    if result == 0 {
        return Ok(());
    }

    Err(std::io::Error::last_os_error())
        .with_context(|| format!("failed to signal daemon process {pid}"))
}

#[derive(Debug)]
struct CaptureLoop {
    config: AppConfig,
    home: PathBuf,
    db: StorageDb,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureCycleOutcome {
    Captured {
        capture_count: usize,
        app_name: String,
        window_title: String,
    },
    SkippedExcluded {
        app_name: String,
        window_title: String,
    },
    SkippedDuplicate {
        app_name: String,
        window_title: String,
        last_capture_at: DateTime<Utc>,
    },
}

impl CaptureLoop {
    fn open(config: AppConfig, home: PathBuf) -> Result<Self> {
        ensure!(
            config.capture.idle_interval_secs > 0,
            "capture idle_interval_secs must be greater than 0"
        );

        let db_path = config.storage_root(&home).join("screencap.db");
        let db = StorageDb::open_at_path(&db_path)
            .with_context(|| format!("failed to open capture database at {}", db_path.display()))?;

        Ok(Self { config, home, db })
    }

    fn capture_once(&mut self) -> Result<CaptureCycleOutcome> {
        self.capture_once_at(Utc::now())
    }

    fn capture_once_at(&mut self, timestamp: DateTime<Utc>) -> Result<CaptureCycleOutcome> {
        let active_window = window::get_active_window().context("failed to fetch active window")?;

        if self.is_excluded(&active_window) {
            info!(
                app_name = %active_window.app_name,
                window_title = %active_window.window_title,
                "skipping capture for excluded window context"
            );
            return Ok(CaptureCycleOutcome::SkippedExcluded {
                app_name: active_window.app_name,
                window_title: active_window.window_title,
            });
        }

        if let Some(last_capture) = self.duplicate_capture(&active_window, timestamp)? {
            debug!(
                app_name = %active_window.app_name,
                window_title = %active_window.window_title,
                last_capture_at = %last_capture.timestamp,
                "skipping duplicate capture within idle interval"
            );
            return Ok(CaptureCycleOutcome::SkippedDuplicate {
                app_name: active_window.app_name,
                window_title: active_window.window_title,
                last_capture_at: last_capture.timestamp,
            });
        }

        let display_ids = screenshot::display_ids().context("failed to enumerate displays")?;
        ensure!(
            !display_ids.is_empty(),
            "capture bridge returned no displays"
        );

        let mut screenshot_paths = Vec::with_capacity(display_ids.len());
        let mut captures = Vec::with_capacity(display_ids.len());
        for display_id in display_ids {
            let screenshot_path = self.screenshot_path(timestamp, display_id);
            if let Err(err) = screenshot::capture_screenshot(
                display_id,
                &screenshot_path,
                self.config.capture.jpeg_quality,
            ) {
                cleanup_files(&screenshot_paths);
                return Err(err).with_context(|| {
                    format!("failed to capture screenshot for display {display_id}")
                });
            }

            screenshot_paths.push(screenshot_path.clone());
            captures.push(NewCapture {
                timestamp,
                app_name: Some(active_window.app_name.clone()),
                window_title: Some(active_window.window_title.clone()),
                bundle_id: Some(active_window.bundle_id.clone()),
                display_id: Some(i64::from(display_id)),
                screenshot_path: screenshot_path.to_string_lossy().into_owned(),
            });
        }

        if let Err(err) = self.db.insert_captures(&captures) {
            cleanup_files(&screenshot_paths);
            return Err(err).context("failed to persist captured screenshots");
        }

        info!(
            capture_count = screenshot_paths.len(),
            app_name = %active_window.app_name,
            window_title = %active_window.window_title,
            "captured screen state"
        );

        Ok(CaptureCycleOutcome::Captured {
            capture_count: screenshot_paths.len(),
            app_name: active_window.app_name,
            window_title: active_window.window_title,
        })
    }

    fn is_excluded(&self, active_window: &WindowInfo) -> bool {
        self.config
            .capture
            .excluded_apps
            .iter()
            .any(|app_name| app_name == &active_window.app_name)
            || self
                .config
                .capture
                .excluded_window_titles
                .iter()
                .any(|title| title == &active_window.window_title)
    }

    fn duplicate_capture(
        &self,
        active_window: &WindowInfo,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<Capture>> {
        let Some(last_capture) = self.db.get_latest_capture()? else {
            return Ok(None);
        };

        if last_capture.app_name.as_deref() != Some(active_window.app_name.as_str())
            || last_capture.window_title.as_deref() != Some(active_window.window_title.as_str())
        {
            return Ok(None);
        }

        let elapsed = timestamp.signed_duration_since(last_capture.timestamp);
        if elapsed < ChronoDuration::zero() || elapsed >= self.idle_interval()? {
            return Ok(None);
        }

        Ok(Some(last_capture))
    }

    fn idle_interval(&self) -> Result<ChronoDuration> {
        let seconds = i64::try_from(self.config.capture.idle_interval_secs)
            .context("capture idle_interval_secs exceeds supported range")?;
        Ok(ChronoDuration::seconds(seconds))
    }

    fn screenshot_path(&self, timestamp: DateTime<Utc>, display_id: u32) -> PathBuf {
        let filename = format!("{}-{display_id}.jpg", timestamp.format("%H%M%S"));
        self.config
            .screenshots_root(&self.home)
            .join(timestamp.format("%Y").to_string())
            .join(timestamp.format("%m").to_string())
            .join(timestamp.format("%d").to_string())
            .join(filename)
    }
}

fn cleanup_files(paths: &[PathBuf]) {
    for path in paths {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                debug!(path = %path.display(), error = %err, "failed to clean up screenshot")
            }
        }
    }
}

#[cfg(all(test, feature = "mock-capture"))]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::Result;
    use chrono::TimeZone;

    use super::{
        clear_app_change_trigger_sender, on_active_app_change, set_app_change_trigger_sender,
        status_at_home, CaptureCycleOutcome, CaptureLoop, DaemonState, PidRecord,
    };
    use crate::config::AppConfig;
    use tokio::sync::mpsc::error::TryRecvError;

    fn temp_home_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("screencap-daemon-tests-{name}-{unique}"))
    }

    fn test_config(home: &Path) -> AppConfig {
        let app_root = home.join(".screencap");
        let mut config =
            AppConfig::load_from_root_and_home(&app_root, home).expect("config should load");
        config.capture.idle_interval_secs = 300;
        config.capture.excluded_apps.clear();
        config.capture.excluded_window_titles.clear();
        config
    }

    #[test]
    fn active_app_change_callback_enqueues_trigger() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        set_app_change_trigger_sender(sender);

        on_active_app_change();
        assert!(matches!(receiver.try_recv(), Ok(())));
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));

        clear_app_change_trigger_sender();
    }

    #[test]
    fn excluded_app_is_skipped() -> Result<()> {
        let home = temp_home_root("excluded-app");
        let mut config = test_config(&home);
        config.capture.excluded_apps = vec!["MockApp".into()];
        let timestamp = chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();

        let mut capture_loop = CaptureLoop::open(config, home.clone())?;
        let outcome = capture_loop.capture_once_at(timestamp)?;
        assert!(matches!(
            outcome,
            CaptureCycleOutcome::SkippedExcluded { ref app_name, .. } if app_name == "MockApp"
        ));
        assert!(capture_loop.db.get_pending_captures()?.is_empty());

        drop(capture_loop);
        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[test]
    fn dedup_skips_unchanged_context_within_idle_interval() -> Result<()> {
        let home = temp_home_root("dedup");
        let config = test_config(&home);
        let first_time = chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
        let second_time = first_time + chrono::Duration::seconds(60);

        let mut capture_loop = CaptureLoop::open(config, home.clone())?;
        let first = capture_loop.capture_once_at(first_time)?;
        assert!(matches!(
            first,
            CaptureCycleOutcome::Captured {
                capture_count: 1,
                ..
            }
        ));

        let second = capture_loop.capture_once_at(second_time)?;
        assert!(matches!(
            second,
            CaptureCycleOutcome::SkippedDuplicate {
                ref app_name,
                ref window_title,
                last_capture_at,
            } if app_name == "MockApp"
                && window_title == "Mock Window"
                && last_capture_at == first_time
        ));

        let captures = capture_loop.db.get_pending_captures()?;
        assert_eq!(captures.len(), 1);
        let skipped_path = capture_loop.screenshot_path(second_time, 0);
        assert!(!skipped_path.exists());

        drop(capture_loop);
        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[test]
    fn persisted_capture_uses_partitioned_screenshot_path() -> Result<()> {
        let home = temp_home_root("path");
        let config = test_config(&home);
        let timestamp = chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 5, 6).unwrap();
        let expected_path = home
            .join(".screencap")
            .join("screenshots")
            .join("2026")
            .join("04")
            .join("10")
            .join("140506-0.jpg");

        let mut capture_loop = CaptureLoop::open(config, home.clone())?;
        let outcome = capture_loop.capture_once_at(timestamp)?;
        assert!(matches!(
            outcome,
            CaptureCycleOutcome::Captured {
                capture_count: 1,
                ..
            }
        ));

        let captures = capture_loop.db.get_pending_captures()?;
        assert_eq!(captures.len(), 1);
        assert_eq!(PathBuf::from(&captures[0].screenshot_path), expected_path);
        assert_eq!(captures[0].display_id, Some(0));
        assert_eq!(captures[0].app_name.as_deref(), Some("MockApp"));
        assert_eq!(captures[0].window_title.as_deref(), Some("Mock Window"));
        assert!(expected_path.exists());
        assert_eq!(captures[0].extraction_status.as_str(), "pending");

        drop(capture_loop);
        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[test]
    fn status_removes_stale_pid_file() -> Result<()> {
        let home = temp_home_root("stale-pid");
        let config = test_config(&home);
        let pid_path = AppConfig::pid_file_path(&home);
        let stale_pid = PidRecord {
            pid: 999_999,
            started_at: chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap(),
        };

        fs::create_dir_all(pid_path.parent().unwrap())?;
        fs::write(&pid_path, toml::to_string(&stale_pid)?)?;

        let status = status_at_home(&config, &home)?;
        assert_eq!(status.state, DaemonState::Stopped);
        assert_eq!(status.pid, None);
        assert!(!pid_path.exists());

        fs::remove_dir_all(&home)?;
        Ok(())
    }
}
