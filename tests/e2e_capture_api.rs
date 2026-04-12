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
use serde_json::json;
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
    write_test_config(home, port, None)
}

fn write_config_with_extraction(
    home: &Path,
    port: u16,
    base_url: &str,
    api_key_env: &str,
) -> Result<PathBuf> {
    write_test_config(home, port, Some((base_url, api_key_env)))
}

fn write_test_config(home: &Path, port: u16, extraction: Option<(&str, &str)>) -> Result<PathBuf> {
    let app_root = home.join(".screencap");
    fs::create_dir_all(&app_root)
        .with_context(|| format!("failed to create test app root at {}", app_root.display()))?;

    let config_path = app_root.join("config.toml");
    let mut config = format!(
        concat!(
            "[capture]\n",
            "idle_interval_secs = 1\n",
            "excluded_apps = []\n",
            "excluded_window_titles = []\n\n",
            "[server]\n",
            "port = {}\n\n",
            "[synthesis]\n",
            "enabled = false\n",
        ),
        port
    );

    if let Some((base_url, api_key_env)) = extraction {
        config.push_str(&format!(
            concat!(
                "\n[extraction]\n",
                "enabled = true\n",
                "interval_secs = 1\n",
                "provider = \"openai\"\n",
                "model = \"gpt-4o-mini\"\n",
                "api_key_env = \"{}\"\n",
                "base_url = \"{}\"\n",
                "max_images_per_batch = 1\n",
            ),
            api_key_env, base_url
        ));
    }

    fs::write(&config_path, config)
        .with_context(|| format!("failed to write config at {}", config_path.display()))?;

    Ok(config_path)
}

struct ForegroundDaemon {
    child: Child,
}

impl ForegroundDaemon {
    fn spawn(home: &Path) -> Result<Self> {
        Self::spawn_with_env(home, &[])
    }

    fn spawn_with_env(home: &Path, envs: &[(&str, &str)]) -> Result<Self> {
        let mut command = Command::new(binary_path());
        command
            .env("HOME", home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        for (key, value) in envs {
            command.env(key, value);
        }

        let child = command
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

struct VisionProviderServer {
    server: support::StubHttpServer,
}

impl VisionProviderServer {
    fn spawn() -> Self {
        let server = support::StubHttpServer::spawn("vision provider", |_request| {
            let content = json!({
                "frames": [
                    {
                        "capture_id": 1,
                        "activity_type": "coding",
                        "description": "Processed by daemon scheduler",
                        "app_context": "Mock extraction context",
                        "project": "screencap",
                        "topics": ["daemon", "extraction"],
                        "people": [],
                        "key_content": "scheduler success",
                        "sentiment": "focused"
                    }
                ],
                "batch_summary": {
                    "primary_activity": "coding",
                    "project_context": "screencap",
                    "narrative": "Automatic extraction succeeded."
                }
            })
            .to_string();
            let body = json!({
                "id": "chatcmpl-test",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": content
                        },
                        "finish_reason": "stop"
                    }
                ],
                "usage": {
                    "prompt_tokens": 12,
                    "completion_tokens": 8,
                    "total_tokens": 20
                }
            })
            .to_string();

            Ok(support::StubHttpAction {
                response: support::json_http_response(200, &body),
                keep_running: false,
            })
        });

        Self { server }
    }

    fn base_url(&self) -> String {
        self.server.base_url()
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

#[derive(Debug, Deserialize)]
struct CaptureDetailResponse {
    capture: ApiCaptureState,
    extraction: Option<ApiExtraction>,
}

#[derive(Debug, Deserialize)]
struct ApiCaptureState {
    id: i64,
    extraction_status: String,
}

#[derive(Debug, Deserialize)]
struct ApiExtraction {
    capture_id: i64,
    description: Option<String>,
    project: Option<String>,
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

async fn wait_for_processed_capture(
    client: &Client,
    base_url: &str,
) -> Result<CaptureDetailResponse> {
    let deadline = Instant::now() + Duration::from_secs(15);

    loop {
        if let Ok(response) = client.get(format!("{base_url}/api/captures")).send().await {
            if response.status().is_success() {
                let captures: CaptureListResponse = response
                    .json()
                    .await
                    .context("failed to decode captures list while waiting for processing")?;

                for capture in captures.captures {
                    let detail_response = client
                        .get(format!("{base_url}/api/captures/{}", capture.id))
                        .send()
                        .await
                        .with_context(|| {
                            format!("failed to fetch capture detail {}", capture.id)
                        })?;

                    if !detail_response.status().is_success() {
                        continue;
                    }

                    let detail: CaptureDetailResponse =
                        detail_response.json().await.with_context(|| {
                            format!("failed to decode capture detail {}", capture.id)
                        })?;

                    if detail.capture.extraction_status == "processed" {
                        return Ok(detail);
                    }
                }
            }
        }

        if Instant::now() >= deadline {
            bail!("timed out waiting for extraction scheduler to process a capture");
        }

        sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
#[ignore = "requires macOS Screen Recording permission; run with cargo test --ignored on a permissioned machine"]
async fn capture_loop_populates_api_captures() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let temp = TempDir::new().context("failed to allocate temporary home directory")?;
    let home = temp.path();

    let port = reserve_port()?;
    let config_path = write_config(home, port)?;
    assert!(
        config_path.exists(),
        "config should be written before start"
    );

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

#[tokio::test]
#[ignore = "requires macOS Screen Recording permission; run with cargo test --ignored on a permissioned machine"]
async fn daemon_processes_pending_captures_with_extraction_scheduler() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let temp = TempDir::new().context("failed to allocate temporary home directory")?;
    let home = temp.path();

    let port = reserve_port()?;
    let provider = VisionProviderServer::spawn();
    let api_key_env = "SCREENCAP_TEST_OPENAI_KEY";
    let config_path = write_config_with_extraction(home, port, &provider.base_url(), api_key_env)?;
    assert!(
        config_path.exists(),
        "config with extraction should be written before start"
    );

    let _daemon = ForegroundDaemon::spawn_with_env(home, &[(api_key_env, "test-token")])?;

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();

    wait_for_server(&client, &base_url).await?;
    let detail = wait_for_processed_capture(&client, &base_url).await?;

    assert_eq!(detail.capture.extraction_status, "processed");
    let extraction = detail
        .extraction
        .context("processed capture should include extraction detail")?;
    assert_eq!(extraction.capture_id, detail.capture.id);
    assert_eq!(
        extraction.description.as_deref(),
        Some("Processed by daemon scheduler")
    );
    assert_eq!(extraction.project.as_deref(), Some("screencap"));

    Ok(())
}
