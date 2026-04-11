use std::{
    env, fs,
    io::{ErrorKind, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{self, Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, Timelike, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, watch},
    time,
};
use tracing::{debug, error, info, warn};

use crate::{
    ai::provider::ProviderError,
    api,
    capture::{
        events, screenshot,
        window::{self, WindowInfo},
    },
    config::AppConfig,
    pipeline::{scheduler, synthesis},
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
const LAUNCHD_AGENT_LABEL: &str = "dev.screencap.daemon";

pub static CAPTURE_PAUSED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureWakeReason {
    TimerFallback,
    Event(events::CaptureTrigger),
}

#[derive(Debug, Clone, Copy)]
struct PendingEventCapture {
    trigger: events::CaptureTrigger,
    capture_at: time::Instant,
}

#[derive(Debug, Clone, Copy)]
struct CaptureWakeScheduler {
    idle_interval: Duration,
    event_settle: Duration,
    next_timer_capture_at: time::Instant,
    pending_event_capture: Option<PendingEventCapture>,
}

impl CaptureWakeScheduler {
    fn new(now: time::Instant, idle_interval: Duration, event_settle: Duration) -> Self {
        Self {
            idle_interval,
            event_settle,
            next_timer_capture_at: now,
            pending_event_capture: None,
        }
    }

    fn timer_deadline(&self) -> Option<time::Instant> {
        self.pending_event_capture
            .is_none()
            .then_some(self.next_timer_capture_at)
    }

    fn event_deadline(&self) -> Option<time::Instant> {
        self.pending_event_capture.map(|pending| pending.capture_at)
    }

    fn record_event(&mut self, trigger: events::CaptureTrigger, now: time::Instant) {
        self.pending_event_capture = Some(PendingEventCapture {
            trigger,
            capture_at: now + self.event_settle,
        });
    }

    fn take_pending_event_wake(&mut self) -> Option<CaptureWakeReason> {
        self.pending_event_capture
            .take()
            .map(|pending| CaptureWakeReason::Event(pending.trigger))
    }

    fn capture_completed(&mut self, now: time::Instant) {
        self.pending_event_capture = None;
        self.next_timer_capture_at = now + self.idle_interval;
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
    pub launchd_installed: bool,
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
    AppConfig::ensure_prompts_dir(&home)?;
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

pub fn install_launch_agent() -> Result<PathBuf> {
    let home = runtime_home_dir()?;
    install_launch_agent_at_home(&home)
}

pub fn uninstall_launch_agent() -> Result<bool> {
    let home = runtime_home_dir()?;
    uninstall_launch_agent_at_home(&home)
}

fn install_launch_agent_at_home(home: &Path) -> Result<PathBuf> {
    let launch_agent_path = AppConfig::launch_agent_path(home);
    let executable =
        env::current_exe().context("failed to resolve current executable for launchd")?;
    let executable = fs::canonicalize(&executable).unwrap_or(executable);
    let plist = render_launch_agent_plist(&executable, home)?;

    if let Some(parent) = launch_agent_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create launch agent directory at {}",
                parent.display()
            )
        })?;
    }

    fs::write(&launch_agent_path, plist).with_context(|| {
        format!(
            "failed to write launch agent plist at {}",
            launch_agent_path.display()
        )
    })?;

    Ok(launch_agent_path)
}

fn uninstall_launch_agent_at_home(home: &Path) -> Result<bool> {
    let launch_agent_path = AppConfig::launch_agent_path(home);
    match fs::remove_file(&launch_agent_path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| {
            format!(
                "failed to remove launch agent plist at {}",
                launch_agent_path.display()
            )
        }),
    }
}

fn launch_agent_installed_at_home(home: &Path) -> Result<bool> {
    let launch_agent_path = AppConfig::launch_agent_path(home);
    match fs::metadata(&launch_agent_path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| {
            format!(
                "failed to inspect launch agent plist at {}",
                launch_agent_path.display()
            )
        }),
    }
}

fn render_launch_agent_plist(executable: &Path, home: &Path) -> Result<String> {
    let executable = plist_path_string(executable, "launchd executable path")?;
    let home = plist_path_string(home, "launchd home path")?;

    Ok(format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
            "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
            "<plist version=\"1.0\">\n",
            "  <dict>\n",
            "    <key>Label</key>\n",
            "    <string>{}</string>\n",
            "    <key>ProgramArguments</key>\n",
            "    <array>\n",
            "      <string>{}</string>\n",
            "      <string>{}</string>\n",
            "    </array>\n",
            "    <key>RunAtLoad</key>\n",
            "    <true/>\n",
            "    <key>KeepAlive</key>\n",
            "    <true/>\n",
            "    <key>WorkingDirectory</key>\n",
            "    <string>{}</string>\n",
            "    <key>EnvironmentVariables</key>\n",
            "    <dict>\n",
            "      <key>HOME</key>\n",
            "      <string>{}</string>\n",
            "    </dict>\n",
            "  </dict>\n",
            "</plist>\n"
        ),
        xml_escape(LAUNCHD_AGENT_LABEL),
        xml_escape(executable),
        xml_escape(INTERNAL_DAEMON_SUBCOMMAND),
        xml_escape(home),
        xml_escape(home),
    ))
}

fn plist_path_string<'a>(path: &'a Path, field: &str) -> Result<&'a str> {
    path.to_str()
        .ok_or_else(|| anyhow!("{field} is not valid UTF-8: {}", path.display()))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
        config.capture.event_settle_ms,
        config.storage.max_age_days,
        shutdown_rx.clone(),
    ));
    let mut server_task = tokio::spawn(api::server::serve(
        listener,
        config.clone(),
        home.to_path_buf(),
        shutdown_rx.clone(),
    ));
    let mut extraction_task = tokio::spawn(run_managed_pipeline_task(
        PipelineTaskKind::Extraction,
        config.clone(),
        home.to_path_buf(),
        shutdown_rx.clone(),
    ));
    let mut rolling_task = tokio::spawn(run_managed_pipeline_task(
        PipelineTaskKind::RollingContext,
        config.clone(),
        home.to_path_buf(),
        shutdown_rx.clone(),
    ));
    let mut hourly_task = tokio::spawn(run_managed_pipeline_task(
        PipelineTaskKind::HourlyDigest,
        config.clone(),
        home.to_path_buf(),
        shutdown_rx.clone(),
    ));
    let mut daily_task = tokio::spawn(run_managed_pipeline_task(
        PipelineTaskKind::DailySummary,
        config.clone(),
        home.to_path_buf(),
        shutdown_rx,
    ));

    tokio::select! {
        result = shutdown_signal() => {
            result?;
            info!("shutdown signal received");
            let _ = shutdown_tx.send(true);
            join_named_tasks(vec![
                ("capture loop", capture_task),
                ("api server", server_task),
                ("extraction scheduler", extraction_task),
                ("rolling context scheduler", rolling_task),
                ("hourly digest scheduler", hourly_task),
                ("daily summary scheduler", daily_task),
            ]).await?;
            Ok(())
        }
        result = &mut capture_task => {
            let _ = shutdown_tx.send(true);
            let capture_result = result.context("capture loop task panicked")?;
            join_named_tasks(vec![
                ("api server", server_task),
                ("extraction scheduler", extraction_task),
                ("rolling context scheduler", rolling_task),
                ("hourly digest scheduler", hourly_task),
                ("daily summary scheduler", daily_task),
            ]).await?;
            task_exit_result("capture loop", capture_result)
        }
        result = &mut server_task => {
            let _ = shutdown_tx.send(true);
            let server_result = result.context("api server task panicked")?;
            join_named_tasks(vec![
                ("capture loop", capture_task),
                ("extraction scheduler", extraction_task),
                ("rolling context scheduler", rolling_task),
                ("hourly digest scheduler", hourly_task),
                ("daily summary scheduler", daily_task),
            ]).await?;
            task_exit_result("api server", server_result)
        }
        result = &mut extraction_task => {
            let _ = shutdown_tx.send(true);
            let extraction_result = result.context("extraction scheduler task panicked")?;
            join_named_tasks(vec![
                ("capture loop", capture_task),
                ("api server", server_task),
                ("rolling context scheduler", rolling_task),
                ("hourly digest scheduler", hourly_task),
                ("daily summary scheduler", daily_task),
            ]).await?;
            task_exit_result("extraction scheduler", extraction_result)
        }
        result = &mut rolling_task => {
            let _ = shutdown_tx.send(true);
            let rolling_result = result.context("rolling context scheduler task panicked")?;
            join_named_tasks(vec![
                ("capture loop", capture_task),
                ("api server", server_task),
                ("extraction scheduler", extraction_task),
                ("hourly digest scheduler", hourly_task),
                ("daily summary scheduler", daily_task),
            ]).await?;
            task_exit_result("rolling context scheduler", rolling_result)
        }
        result = &mut hourly_task => {
            let _ = shutdown_tx.send(true);
            let hourly_result = result.context("hourly digest scheduler task panicked")?;
            join_named_tasks(vec![
                ("capture loop", capture_task),
                ("api server", server_task),
                ("extraction scheduler", extraction_task),
                ("rolling context scheduler", rolling_task),
                ("daily summary scheduler", daily_task),
            ]).await?;
            task_exit_result("hourly digest scheduler", hourly_result)
        }
        result = &mut daily_task => {
            let _ = shutdown_tx.send(true);
            let daily_result = result.context("daily summary scheduler task panicked")?;
            join_named_tasks(vec![
                ("capture loop", capture_task),
                ("api server", server_task),
                ("extraction scheduler", extraction_task),
                ("rolling context scheduler", rolling_task),
                ("hourly digest scheduler", hourly_task),
            ]).await?;
            task_exit_result("daily summary scheduler", daily_result)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PipelineTaskKind {
    Extraction,
    RollingContext,
    HourlyDigest,
    DailySummary,
}

impl PipelineTaskKind {
    fn task_name(self) -> &'static str {
        match self {
            Self::Extraction => "extraction scheduler",
            Self::RollingContext => "rolling context scheduler",
            Self::HourlyDigest => "hourly digest scheduler",
            Self::DailySummary => "daily summary scheduler",
        }
    }

    fn automatic_work_label(self) -> &'static str {
        match self {
            Self::Extraction => "automatic extraction",
            Self::RollingContext => "automatic rolling context synthesis",
            Self::HourlyDigest => "automatic hourly digest synthesis",
            Self::DailySummary => "automatic daily summary synthesis",
        }
    }

    fn is_disabled(self, config: &AppConfig) -> bool {
        match self {
            Self::Extraction => !config.extraction.enabled,
            Self::RollingContext | Self::DailySummary => !config.synthesis.enabled,
            Self::HourlyDigest => !config.synthesis.enabled || !config.synthesis.hourly_enabled,
        }
    }

    async fn run(
        self,
        config: AppConfig,
        home: &Path,
        shutdown: watch::Receiver<bool>,
    ) -> Result<()> {
        match self {
            Self::Extraction => scheduler::run_extraction_scheduler(config, home, shutdown).await,
            Self::RollingContext => {
                synthesis::run_rolling_context_scheduler(config, home, shutdown).await
            }
            Self::HourlyDigest => {
                synthesis::run_hourly_digest_scheduler(config, home, shutdown).await
            }
            Self::DailySummary => {
                synthesis::run_daily_summary_scheduler(config, home, shutdown).await
            }
        }
    }
}

async fn run_managed_pipeline_task(
    kind: PipelineTaskKind,
    config: AppConfig,
    home: PathBuf,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    if kind.is_disabled(&config) {
        return wait_for_shutdown(shutdown).await;
    }

    match kind.run(config, &home, shutdown.clone()).await {
        Ok(()) => Ok(()),
        Err(error) if is_nonfatal_pipeline_init_error(&error) => {
            warn!(
                error = %error,
                task = kind.task_name(),
                "{} disabled; capture and API services will continue without {}",
                kind.task_name(),
                kind.automatic_work_label()
            );
            wait_for_shutdown(shutdown).await
        }
        Err(error) => Err(error.context(format!("failed to initialize {}", kind.task_name()))),
    }
}

async fn join_named_tasks(
    tasks: Vec<(&'static str, tokio::task::JoinHandle<Result<()>>)>,
) -> Result<()> {
    for (task_name, task) in tasks {
        task.await
            .with_context(|| format!("{task_name} task panicked"))??;
    }

    Ok(())
}

async fn wait_for_shutdown(mut shutdown: watch::Receiver<bool>) -> Result<()> {
    if *shutdown.borrow() {
        return Ok(());
    }

    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            break;
        }
    }

    Ok(())
}

fn task_exit_result(task_name: &str, result: Result<()>) -> Result<()> {
    match result {
        Ok(()) => Err(anyhow!("{task_name} exited before shutdown signal")),
        Err(error) => Err(error),
    }
}

fn is_nonfatal_pipeline_init_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<ProviderError>(),
            Some(ProviderError::MissingApiKey { .. } | ProviderError::UnsupportedProvider { .. })
        )
    })
}

async fn run_capture_loop(
    mut capture_loop: CaptureLoop,
    idle_interval_secs: u64,
    event_settle_ms: u64,
    retention_days: u32,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let idle_interval = Duration::from_secs(idle_interval_secs);
    let event_settle = Duration::from_millis(event_settle_ms);
    let (event_trigger_tx, mut event_trigger_rx) = mpsc::channel(32);
    let _event_listener_guard = match events::start_capture_trigger_listener(
        event_trigger_tx,
        idle_interval,
    ) {
        events::EventListenerStart::Active(guard) => Some(guard),
        events::EventListenerStart::Disabled { reason } => {
            warn!(reason = %reason, "event-driven capture disabled; falling back to timer-only mode");
            None
        }
    };

    let mut wake_scheduler =
        CaptureWakeScheduler::new(time::Instant::now(), idle_interval, event_settle);
    let mut event_listener_closed = false;
    let mut last_prune_date: Option<NaiveDate> = None;
    loop {
        let timer_deadline = wake_scheduler.timer_deadline();
        let event_deadline = wake_scheduler.event_deadline();
        let disabled_deadline = time::Instant::now() + Duration::from_secs(86_400);
        let timer_deadline = timer_deadline.unwrap_or(disabled_deadline);
        let event_deadline = event_deadline.unwrap_or(disabled_deadline);
        let wake_reason = tokio::select! {
            _ = time::sleep_until(timer_deadline), if wake_scheduler.timer_deadline().is_some() => {
                Some(CaptureWakeReason::TimerFallback)
            }
            _ = time::sleep_until(event_deadline), if wake_scheduler.event_deadline().is_some() => {
                wake_scheduler.take_pending_event_wake()
            }
            maybe_trigger = event_trigger_rx.recv(), if !event_listener_closed => {
                match maybe_trigger {
                    Some(trigger) => wake_scheduler.record_event(trigger, time::Instant::now()),
                    None => {
                        event_listener_closed = true;
                        debug!("event-driven capture listener stopped");
                    }
                }
                continue;
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
                continue;
            }
        };

        match wake_reason {
            Some(CaptureWakeReason::TimerFallback) => {
                debug!("capture triggered by idle fallback timer");
            }
            Some(CaptureWakeReason::Event(trigger)) => {
                debug!(
                    ?trigger,
                    settle_ms = event_settle_ms,
                    "capture triggered by settled event activity"
                );
            }
            None => continue,
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

        wake_scheduler.capture_completed(time::Instant::now());
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
    let launchd_installed = launch_agent_installed_at_home(home)?;
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
        launchd_installed,
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
    excluded_window_title_patterns: Vec<Regex>,
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

fn compile_excluded_window_title_patterns(raw_patterns: &[String]) -> Result<Vec<Regex>> {
    raw_patterns
        .iter()
        .enumerate()
        .map(|(index, pattern)| {
            Regex::new(pattern).with_context(|| {
                format!("invalid capture.excluded_window_titles regex at index {index}: {pattern}")
            })
        })
        .collect()
}

fn is_excluded_window_title(window_title: &str, patterns: &[Regex]) -> bool {
    patterns
        .iter()
        .any(|pattern| pattern.is_match(window_title))
}

impl CaptureLoop {
    fn open(config: AppConfig, home: PathBuf) -> Result<Self> {
        ensure!(
            config.capture.idle_interval_secs > 0,
            "capture idle_interval_secs must be greater than 0"
        );

        let excluded_window_title_patterns =
            compile_excluded_window_title_patterns(&config.capture.excluded_window_titles)?;
        let db_path = config.storage_root(&home).join("screencap.db");
        let db = StorageDb::open_at_path(&db_path)
            .with_context(|| format!("failed to open capture database at {}", db_path.display()))?;

        Ok(Self {
            config,
            home,
            db,
            excluded_window_title_patterns,
        })
    }

    fn capture_once(&mut self) -> Result<CaptureCycleOutcome> {
        self.capture_once_at(Utc::now())
    }

    fn capture_once_at(&mut self, timestamp: DateTime<Utc>) -> Result<CaptureCycleOutcome> {
        let active_window = window::get_active_window().context("failed to fetch active window")?;

        if self
            .config
            .capture
            .excluded_apps
            .contains(&active_window.app_name)
        {
            info!(
                app_name = %active_window.app_name,
                window_title = %active_window.window_title,
                "skipping capture for excluded app"
            );
            return Ok(CaptureCycleOutcome::SkippedExcluded {
                app_name: active_window.app_name,
                window_title: active_window.window_title,
            });
        }

        if is_excluded_window_title(
            &active_window.window_title,
            &self.excluded_window_title_patterns,
        ) {
            info!(
                app_name = %active_window.app_name,
                window_title = %active_window.window_title,
                "Skipping capture: window title matches exclusion pattern"
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

        let display_ids = screenshot::display_ids().context("failed to enumerate display ids")?;
        ensure!(!display_ids.is_empty(), "capture bridge returned no displays");

        let mut screenshot_paths = Vec::with_capacity(display_ids.len());
        let mut captures = Vec::with_capacity(display_ids.len());
        let mut failed_displays = Vec::new();

        for display_id in display_ids {
            let screenshot_path = self.screenshot_path(timestamp, display_id);
            match screenshot::capture_screenshot(
                display_id,
                &screenshot_path,
                self.config.capture.jpeg_quality,
            ) {
                Ok(()) => {
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
                Err(err) => {
                    error!(display_id, error = %err, "failed to capture display screenshot");
                    failed_displays.push((display_id, err));
                }
            }
        }

        if !failed_displays.is_empty() {
            cleanup_files(&screenshot_paths);
            let failed_display_details = failed_displays
                .into_iter()
                .map(|(display_id, err)| format!("{display_id}: {err}"))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(anyhow!(
                "failed to capture one or more displays: {failed_display_details}"
            ));
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

#[cfg(test)]
mod exclusion_pattern_tests {
    use super::{compile_excluded_window_title_patterns, is_excluded_window_title};

    #[test]
    fn excluded_window_title_regex_matches() {
        let patterns = compile_excluded_window_title_patterns(&[
            "^Mock\\s+Window$".to_string(),
            ".*Incognito.*".to_string(),
        ])
        .expect("regex patterns should compile");

        assert!(is_excluded_window_title("Mock Window", &patterns));
        assert!(is_excluded_window_title(
            "Personal Incognito Tab",
            &patterns
        ));
        assert!(!is_excluded_window_title("Regular Browser", &patterns));
    }

    #[test]
    fn invalid_excluded_window_title_regex_returns_error() {
        let err = compile_excluded_window_title_patterns(&["[invalid".to_string()])
            .expect_err("invalid regex should fail compilation");

        assert!(
            err.to_string()
                .contains("invalid capture.excluded_window_titles regex at index 0"),
            "unexpected error: {err}"
        );
    }
}

#[cfg(test)]
mod wake_scheduler_tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::{events, CaptureWakeReason, CaptureWakeScheduler};

    #[test]
    fn event_wake_suppresses_idle_timer_until_settle_deadline() {
        let now = Instant::now();
        let idle_interval = Duration::from_secs(300);
        let settle = Duration::from_millis(500);
        let mut scheduler = CaptureWakeScheduler::new(now, idle_interval, settle);

        assert_eq!(scheduler.timer_deadline(), Some(now));
        scheduler.record_event(
            events::CaptureTrigger::AppSwitch,
            now + Duration::from_secs(1),
        );

        assert_eq!(scheduler.timer_deadline(), None);
        assert_eq!(
            scheduler.event_deadline(),
            Some(now + Duration::from_secs(1) + settle)
        );
    }

    #[test]
    fn later_event_extends_settle_deadline_and_keeps_latest_trigger() {
        let now = Instant::now();
        let settle = Duration::from_millis(500);
        let mut scheduler = CaptureWakeScheduler::new(now, Duration::from_secs(300), settle);

        scheduler.record_event(
            events::CaptureTrigger::AppSwitch,
            now + Duration::from_secs(1),
        );
        scheduler.record_event(
            events::CaptureTrigger::KeyboardBurst,
            now + Duration::from_secs(2),
        );

        assert_eq!(
            scheduler.event_deadline(),
            Some(now + Duration::from_secs(2) + settle)
        );
        assert_eq!(
            scheduler.take_pending_event_wake(),
            Some(CaptureWakeReason::Event(
                events::CaptureTrigger::KeyboardBurst,
            ))
        );
    }

    #[test]
    fn completed_capture_reschedules_idle_timer_and_clears_pending_event() {
        let now = Instant::now();
        let idle_interval = Duration::from_secs(300);
        let settle = Duration::from_millis(500);
        let mut scheduler = CaptureWakeScheduler::new(now, idle_interval, settle);

        scheduler.record_event(
            events::CaptureTrigger::MouseResume,
            now + Duration::from_secs(5),
        );
        scheduler.capture_completed(now + Duration::from_secs(6));

        assert_eq!(
            scheduler.timer_deadline(),
            Some(now + Duration::from_secs(6) + idle_interval)
        );
        assert_eq!(scheduler.event_deadline(), None);
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
        install_launch_agent_at_home, render_launch_agent_plist, status_at_home,
        uninstall_launch_agent_at_home, CaptureCycleOutcome, CaptureLoop, DaemonState, PidRecord,
        INTERNAL_DAEMON_SUBCOMMAND, LAUNCHD_AGENT_LABEL,
    };
    use crate::{capture::screenshot, config::AppConfig};

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

    #[tokio::test]
    async fn startup_components_open_with_defaults_when_config_is_missing() -> Result<()> {
        let home = temp_home_root("missing-config-startup");
        let mut config = test_config(&home);
        config.server.port = 0;

        let capture_loop = CaptureLoop::open(config.clone(), home.clone())?;
        let listener = crate::api::server::bind(&config).await?;
        assert!(listener.local_addr()?.port() > 0);

        drop(listener);
        drop(capture_loop);
        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn startup_components_open_with_defaults_when_config_is_empty() -> Result<()> {
        let home = temp_home_root("empty-config-startup");
        let app_root = home.join(".screencap");
        fs::create_dir_all(&app_root)?;
        fs::write(app_root.join("config.toml"), "")?;

        let mut config = AppConfig::load_from_root_and_home(&app_root, &home)?;
        config.capture.excluded_apps.clear();
        config.capture.excluded_window_titles.clear();
        config.server.port = 0;

        let capture_loop = CaptureLoop::open(config.clone(), home.clone())?;
        let listener = crate::api::server::bind(&config).await?;
        assert!(listener.local_addr()?.port() > 0);

        drop(listener);
        drop(capture_loop);
        fs::remove_dir_all(&home)?;
        Ok(())
    }


    #[test]
    fn invalid_excluded_window_title_regex_fails_capture_loop_open() {
        let home = temp_home_root("invalid-window-title-regex");
        let mut config = test_config(&home);
        config.capture.excluded_window_titles = vec!["[invalid".into()];

        let err = CaptureLoop::open(config, home.clone())
            .expect_err("invalid title regex should fail daemon startup");
        assert!(
            err.to_string()
                .contains("invalid capture.excluded_window_titles regex at index 0"),
            "unexpected error: {err}"
        );

        fs::remove_dir_all(&home).expect("cleanup temp home");
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
    fn excluded_window_title_pattern_is_skipped() -> Result<()> {
        let home = temp_home_root("excluded-window-title");
        let mut config = test_config(&home);
        config.capture.excluded_window_titles = vec!["^Mock\\s+Window$".into()];
        let timestamp = chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();

        let mut capture_loop = CaptureLoop::open(config, home.clone())?;
        let outcome = capture_loop.capture_once_at(timestamp)?;
        assert!(matches!(
            outcome,
            CaptureCycleOutcome::SkippedExcluded { ref window_title, .. }
                if window_title == "Mock Window"
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
    fn persisted_capture_uses_bridge_display_id_in_path_and_record() -> Result<()> {
        screenshot::with_mock_display_ids_for_tests(vec![42], || -> Result<()> {
            let home = temp_home_root("path");
            let config = test_config(&home);
            let timestamp = chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 5, 6).unwrap();
            let expected_path = home
                .join(".screencap")
                .join("screenshots")
                .join("2026")
                .join("04")
                .join("10")
                .join("140506-42.jpg");

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
            assert_eq!(captures[0].display_id, Some(42));
            assert_eq!(captures[0].app_name.as_deref(), Some("MockApp"));
            assert_eq!(captures[0].window_title.as_deref(), Some("Mock Window"));
            assert!(expected_path.exists());
            assert_eq!(captures[0].extraction_status.as_str(), "pending");

            drop(capture_loop);
            fs::remove_dir_all(&home)?;
            Ok(())
        })
    }

    #[test]
    fn multi_display_capture_uses_each_bridge_display_id() -> Result<()> {
        screenshot::with_mock_display_ids_for_tests(vec![42, 84], || -> Result<()> {
            let home = temp_home_root("multi-display");
            let config = test_config(&home);
            let timestamp = chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 5, 6).unwrap();
            let expected_paths = vec![
                home.join(".screencap")
                    .join("screenshots")
                    .join("2026")
                    .join("04")
                    .join("10")
                    .join("140506-42.jpg"),
                home.join(".screencap")
                    .join("screenshots")
                    .join("2026")
                    .join("04")
                    .join("10")
                    .join("140506-84.jpg"),
            ];

            let mut capture_loop = CaptureLoop::open(config, home.clone())?;
            let outcome = capture_loop.capture_once_at(timestamp)?;
            assert!(matches!(
                outcome,
                CaptureCycleOutcome::Captured {
                    capture_count: 2,
                    ..
                }
            ));

            let mut captures = capture_loop.db.get_pending_captures()?;
            captures.sort_by_key(|capture| capture.display_id);
            assert_eq!(captures.len(), 2);
            assert_eq!(
                captures.iter().map(|capture| capture.display_id).collect::<Vec<_>>(),
                vec![Some(42), Some(84)]
            );
            assert_eq!(
                captures
                    .iter()
                    .map(|capture| PathBuf::from(&capture.screenshot_path))
                    .collect::<Vec<_>>(),
                expected_paths
            );
            for path in &expected_paths {
                assert!(path.exists(), "expected screenshot at {}", path.display());
            }

            drop(capture_loop);
            fs::remove_dir_all(&home)?;
            Ok(())
        })
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
    #[test]
    fn render_launch_agent_plist_sets_keepalive_runatload_and_home() -> Result<()> {
        let home = temp_home_root("launchd-render");
        let executable = home.join("Applications").join("screencap");

        let plist = render_launch_agent_plist(&executable, &home)?;

        assert!(plist.contains(LAUNCHD_AGENT_LABEL));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains(INTERNAL_DAEMON_SUBCOMMAND));
        assert!(plist.contains(home.to_str().expect("temp path should be utf-8")));

        Ok(())
    }

    #[test]
    fn install_and_uninstall_launch_agent_updates_status() -> Result<()> {
        let home = temp_home_root("launchd-status");
        let config = test_config(&home);

        let launch_agent_path = install_launch_agent_at_home(&home)?;
        assert!(launch_agent_path.exists());

        let installed = status_at_home(&config, &home)?;
        assert!(installed.launchd_installed);

        let removed = uninstall_launch_agent_at_home(&home)?;
        assert!(removed);
        assert!(!launch_agent_path.exists());

        let uninstalled = status_at_home(&config, &home)?;
        assert!(!uninstalled.launchd_installed);

        fs::remove_dir_all(&home)?;
        Ok(())
    }
}
