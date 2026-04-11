mod support;

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
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
struct HealthResponse {
    status: String,
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

#[tokio::test]
async fn daemon_serves_embedded_ui_and_health_api() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let temp = TempDir::new().context("failed to allocate temporary home directory")?;
    let home = temp.path();

    let port = reserve_port()?;
    let config_path = write_config(home, port)?;
    assert!(
        config_path.exists(),
        "config should be written before daemon start"
    );

    let _daemon = ForegroundDaemon::spawn(home)?;
    sleep(Duration::from_secs(2)).await;

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    wait_for_server(&client, &base_url).await?;

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
        "expected JSON content type for health endpoint, got {health_content_type}"
    );
    let health: HealthResponse = health_response.json().await?;
    assert_eq!(
        health.status, "ok",
        "health endpoint should report ok status"
    );

    Ok(())
}
