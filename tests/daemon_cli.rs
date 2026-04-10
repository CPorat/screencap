mod support;

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_screencap")
}

struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(name: &str) -> Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let home = Self {
            path: std::env::temp_dir().join(format!("screencap-cli-tests-{name}-{unique}")),
        };
        let app_root = home.path.join(".screencap");
        let port = reserve_port()?;

        fs::create_dir_all(&app_root)
            .with_context(|| format!("failed to create test app root at {}", app_root.display()))?;
        fs::write(
            app_root.join("config.toml"),
            format!("[server]\nport = {port}\n"),
        )
        .with_context(|| format!("failed to write test config at {}", app_root.display()))?;

        Ok(home)
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn pid_path(&self) -> PathBuf {
        self.path.join(".screencap").join("screencap.pid")
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = Command::new(binary_path())
            .arg("stop")
            .env("HOME", &self.path)
            .output();
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn reserve_port() -> Result<u16> {
    let listener =
        TcpListener::bind("127.0.0.1:0").context("failed to reserve a local tcp port")?;
    listener
        .local_addr()
        .map(|address| address.port())
        .context("failed to read reserved tcp port")
}

fn run_cli(home: &Path, args: &[&str]) -> Result<Output> {
    Command::new(binary_path())
        .args(args)
        .env("HOME", home)
        .output()
        .with_context(|| format!("failed to run screencap {:?}", args))
}

fn spawn_foreground(home: &Path) -> Result<Child> {
    Command::new(binary_path())
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn foreground daemon")
}

fn assert_success(output: &Output, command: &str) {
    assert!(
        output.status.success(),
        "{command} failed: status={:?}, stdout={}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn wait_for(condition: impl Fn() -> Result<bool>, description: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if condition()? {
            return Ok(());
        }

        if Instant::now() >= deadline {
            bail!("timed out waiting for {description}");
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_exit(child: &mut Child) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().context("failed to poll child process")? {
            if status.success() {
                return Ok(());
            }
            bail!("daemon exited unsuccessfully: {status}");
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("timed out waiting for daemon exit");
        }

        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn start_status_stop_manage_background_daemon() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("background")?;
    let start = run_cli(home.path(), &["start"])?;
    assert_success(&start, "start");
    assert!(
        String::from_utf8_lossy(&start.stdout).contains("started daemon pid "),
        "unexpected start output: {}",
        String::from_utf8_lossy(&start.stdout),
    );

    wait_for(
        || Ok(home.pid_path().exists()),
        "pid file after background start",
    )?;

    let status = run_cli(home.path(), &["status"])?;
    assert_success(&status, "status");
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        stdout.contains("state: running"),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("pid: "),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("uptime_secs: "),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("captures_today: "),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("storage_bytes: "),
        "unexpected status output: {stdout}"
    );

    let stop = run_cli(home.path(), &["stop"])?;
    assert_success(&stop, "stop");
    assert!(
        String::from_utf8_lossy(&stop.stdout).contains("stopped daemon pid "),
        "unexpected stop output: {}",
        String::from_utf8_lossy(&stop.stdout),
    );

    wait_for(
        || Ok(!home.pid_path().exists()),
        "pid file removal after stop",
    )?;

    let status = run_cli(home.path(), &["status"])?;
    assert_success(&status, "status after stop");
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        stdout.contains("state: stopped"),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("pid: -"),
        "unexpected status output: {stdout}"
    );

    Ok(())
}

#[test]
fn foreground_daemon_can_be_stopped_via_cli() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("foreground")?;
    let mut child = spawn_foreground(home.path())?;

    wait_for(
        || Ok(home.pid_path().exists()),
        "pid file after foreground start",
    )?;

    let status = run_cli(home.path(), &["status"])?;
    assert_success(&status, "status while foreground daemon runs");
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("state: running"),
        "unexpected status output: {}",
        String::from_utf8_lossy(&status.stdout),
    );

    let stop = run_cli(home.path(), &["stop"])?;
    assert_success(&stop, "stop foreground daemon");

    wait_for_exit(&mut child)?;
    wait_for(
        || Ok(!home.pid_path().exists()),
        "pid file removal after foreground stop",
    )?;

    Ok(())
}
