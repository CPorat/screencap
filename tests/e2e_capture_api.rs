mod support;

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use reqwest::Client;
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
            "idle_interval_secs = 1\n",
            "excluded_apps = []\n",
            "excluded_window_titles = []\n\n",
            "[server]\n",
            "port = {}\n",
        ),
        port
    );

    fs::write(&config_path, config)
        .with_context(|| format!("failed to write config at {}", config_path.display()))?;

    Ok(config_path)
}

struct ForegroundDaemon {
    child: Child,
}

impl ForegroundDaemon {
    fn spawn(home: &Path) -> Result<Self> {
        let child = Command::new(binary_path())
            .env("HOME", home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to spawn screencap daemon")?;

        Ok(Self { child })
    }
}

impl Drop for ForegroundDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug, Deserialize)]
struct ApiCapture {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct CaptureListResponse {
    captures: Vec<ApiCapture>,
}

async fn wait_for_server(client: &Client, base_url: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        if let Ok(response) = client.get(format!("{base_url}/api/health")).send().await {
            if response.status().is_success() {
                return Ok(());
            }
        }

        if Instant::now() >= deadline {
            bail!("timed out waiting for API server to become healthy");
        }

        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_captures(client: &Client, base_url: &str) -> Result<CaptureListResponse> {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        if let Ok(response) = client.get(format!("{base_url}/api/captures")).send().await {
            if response.status().is_success() {
                let captures: CaptureListResponse = response
                    .json()
                    .await
                    .context("failed to decode captures response")?;

                if !captures.captures.is_empty() {
                    return Ok(captures);
                }
            }
        }

        if Instant::now() >= deadline {
            bail!("timed out waiting for capture loop to publish captures");
        }

        sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn capture_loop_populates_api_captures() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let temp = TempDir::new().context("failed to allocate temporary home directory")?;
    let home = temp.path();

    let port = reserve_port()?;
    let config_path = write_config(home, port)?;
    assert!(config_path.exists(), "config should be written before start");

    let _daemon = ForegroundDaemon::spawn(home)?;

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();

    wait_for_server(&client, &base_url).await?;
    sleep(Duration::from_secs(4)).await;

    let captures = wait_for_captures(&client, &base_url).await?;
    assert!(
        !captures.captures.is_empty(),
        "expected at least one capture from capture loop"
    );
    assert!(captures.captures[0].id > 0, "expected persisted capture id");

    Ok(())
}
