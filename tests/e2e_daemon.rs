mod support;

use std::{
    fs,
    io::Read,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{bail, ensure, Context, Result};
use reqwest::{header::CONTENT_TYPE, Client};
use serde::Deserialize;
use tempfile::TempDir;
use tokio::time::sleep;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_screencap")
}

fn reserve_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("failed to reserve local tcp port")?;
    listener
        .local_addr()
        .map(|address| address.port())
        .context("failed to read reserved tcp port")
}

fn write_config(home: &Path, port: u16) -> Result<PathBuf> {
    let app_root = home.join(".screencap");
    fs::create_dir_all(&app_root)
        .with_context(|| format!("failed to create test app root at {}", app_root.display()))?;

    let config_path = app_root.join("config.toml");
    let config = format!(
        concat!(
            "[capture]\n",
            "idle_interval_secs = 60\n",
            "excluded_apps = []\n",
            "excluded_window_titles = []\n\n",
            "[server]\n",
            "port = {}\n\n",
            "[extraction]\n",
            "enabled = false\n\n",
            "[synthesis]\n",
            "enabled = false\n",
        ),
        port
    );

    fs::write(&config_path, config)
        .with_context(|| format!("failed to write config at {}", config_path.display()))?;

    Ok(config_path)
}

struct DaemonProcess {
    child: Option<Child>,
}

impl DaemonProcess {
    fn spawn(home: &Path) -> Result<Self> {
        let child = Command::new(binary_path())
            .env("HOME", home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn screencap daemon")?;

        Ok(Self { child: Some(child) })
    }

    fn pid(&self) -> u32 {
        self.child
            .as_ref()
            .expect("daemon child should be present")
            .id()
    }

    fn wait_for_exit(mut self, timeout: Duration) -> Result<String> {
        let mut child = self.child.take().expect("daemon child should be present");
        let mut stderr = child.stderr.take();
        let deadline = Instant::now() + timeout;

        loop {
            if let Some(status) = child.try_wait().context("failed to poll daemon process")? {
                ensure!(status.success(), "daemon exited unsuccessfully: {status}");
                break;
            }

            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                bail!("timed out waiting for daemon shutdown");
            }

            std::thread::sleep(Duration::from_millis(50));
        }

        let mut stderr_output = String::new();
        if let Some(mut handle) = stderr.take() {
            handle
                .read_to_string(&mut stderr_output)
                .context("failed to read daemon stderr")?;
        }

        Ok(stderr_output)
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, Deserialize)]
struct StatsResponse {
    capture_count: u64,
}

async fn wait_for_server(client: &Client, base_url: &str, timeout: Duration) -> Result<Duration> {
    let started_at = Instant::now();

    loop {
        if let Ok(response) = client.get(format!("{base_url}/api/health")).send().await {
            if response.status().is_success() {
                return Ok(started_at.elapsed());
            }
        }

        if started_at.elapsed() >= timeout {
            bail!(
                "timed out waiting {}s for daemon API readiness",
                timeout.as_secs_f32()
            );
        }

        sleep(Duration::from_millis(50)).await;
    }
}

fn send_sigterm(pid: u32) -> Result<()> {
    let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    if result == 0 {
        return Ok(());
    }

    Err(std::io::Error::last_os_error())
        .with_context(|| format!("failed to send SIGTERM to daemon pid {pid}"))
}

#[tokio::test]
async fn daemon_startup_smoke_test() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let temp = TempDir::new().context("failed to allocate temporary home directory")?;
    let home = temp.path();
    let port = reserve_port()?;
    let config_path = write_config(home, port)?;
    let pid_path = home.join(".screencap").join("screencap.pid");
    assert!(
        config_path.exists(),
        "config should be written before daemon start"
    );

    let daemon = DaemonProcess::spawn(home)?;
    let daemon_pid = daemon.pid();
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();

    let startup_time = wait_for_server(&client, &base_url, Duration::from_secs(3)).await?;
    assert!(
        pid_path.exists(),
        "daemon should create its pid file before reporting healthy"
    );
    assert!(
        startup_time <= Duration::from_secs(3),
        "daemon should become healthy within 3 seconds, took {:?}",
        startup_time
    );

    let health_response = client
        .get(format!("{base_url}/api/health"))
        .send()
        .await?
        .error_for_status()?;
    let health_content_type = health_response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        health_content_type.contains("application/json"),
        "expected JSON health response, got {health_content_type}"
    );
    let health: HealthResponse = health_response.json().await?;
    assert_eq!(health.status, "ok");

    let stats_response = client
        .get(format!("{base_url}/api/stats"))
        .send()
        .await?
        .error_for_status()?;
    let stats: StatsResponse = stats_response.json().await?;
    let _ = stats.capture_count;

    let root_response = client
        .get(format!("{base_url}/"))
        .send()
        .await?
        .error_for_status()?;
    let root_content_type = root_response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        root_content_type.contains("text/html"),
        "expected HTML content type for root endpoint, got {root_content_type}"
    );
    let root_body = root_response.text().await?;
    let root_body_lower = root_body.to_ascii_lowercase();
    assert!(
        root_body_lower.contains("<!doctype html") || root_body_lower.contains("<html"),
        "expected embedded UI shell from root endpoint"
    );

    send_sigterm(daemon_pid)?;
    let stderr_output = daemon.wait_for_exit(Duration::from_secs(5))?;
    assert!(
        !pid_path.exists(),
        "daemon pid file should be removed after clean shutdown"
    );

    let stderr_lower = stderr_output.to_ascii_lowercase();
    assert!(
        !stderr_lower.contains("panicked at") && !stderr_lower.contains("panic"),
        "daemon stderr should not include panic output:\n{stderr_output}"
    );

    Ok(())
}
