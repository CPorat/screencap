mod support;

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, Utc};
use reqwest::{header::CONTENT_TYPE, Client};
use screencap::storage::{
    db::StorageDb,
    models::{
        ActivityType, HourlyProjectSummary, Insight, InsightData, InsightType, NewCapture,
        NewExtraction, NewExtractionBatch, NewInsight, Sentiment,
    },
};
use serde::Deserialize;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::time::sleep;
use uuid::Uuid;

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

fn write_config_with_synthesis(
    home: &Path,
    port: u16,
    base_url: &str,
    api_key_env: &str,
    daily_summary_time: &str,
) -> Result<PathBuf> {
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
            "enabled = true\n",
            "provider = \"openai\"\n",
            "model = \"mock-synthesis-model\"\n",
            "api_key_env = \"{}\"\n",
            "base_url = \"{}\"\n",
            "rolling_interval_secs = 1\n",
            "hourly_enabled = false\n",
            "daily_summary_time = \"{}\"\n",
            "daily_export_markdown = false\n",
        ),
        port, api_key_env, base_url, daily_summary_time
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

struct SynthesisProviderServer {
    server: support::StubHttpServer,
}

impl SynthesisProviderServer {
    fn spawn() -> Self {
        let mut served_requests = 0;
        let server = support::StubHttpServer::spawn("synthesis provider", move |request| {
            let prompt = extract_prompt_from_http_request(&request);
            let content = if prompt.contains("You are synthesizing a rolling context summary") {
                let window_start = prompt_metadata_value(&prompt, "window_start")
                    .expect("rolling prompt window_start");
                let window_end = prompt_metadata_value(&prompt, "window_end")
                    .expect("rolling prompt window_end");
                rolling_success_response_json(&window_start, &window_end)
            } else if prompt.contains("You are synthesizing a daily summary") {
                let date =
                    prompt_metadata_value(&prompt, "date").expect("daily prompt requested date");
                daily_success_response_json(&date)
            } else {
                panic!("unexpected synthesis prompt: {prompt}");
            };

            let body = openai_chat_response(&content);
            served_requests += 1;
            let action = if served_requests == 2 {
                support::StubHttpAction {
                    response: support::json_http_response(200, &body),
                    keep_running: false,
                }
            } else {
                support::StubHttpAction {
                    response: support::json_http_response(200, &body),
                    keep_running: true,
                }
            };

            Ok(action)
        });

        Self { server }
    }

    fn base_url(&self) -> String {
        self.server.base_url()
    }
}

fn extract_prompt_from_http_request(request: &str) -> String {
    let body = request
        .split("\r\n\r\n")
        .nth(1)
        .expect("http request body should be present");
    let payload: Value = serde_json::from_str(body).expect("parse openai-compatible request");
    payload["messages"][0]["content"][0]["text"]
        .as_str()
        .expect("prompt content string")
        .to_owned()
}

fn prompt_metadata_value(prompt: &str, key: &str) -> Option<String> {
    let prefix = format!("- {key}: ");
    prompt.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(str::trim)
            .map(ToOwned::to_owned)
    })
}

fn openai_chat_response(content: &str) -> String {
    json!({
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
            "prompt_tokens": 120,
            "completion_tokens": 60,
            "total_tokens": 180
        }
    })
    .to_string()
}

fn rolling_success_response_json(window_start: &str, window_end: &str) -> String {
    json!({
        "type": "rolling",
        "window_start": window_start,
        "window_end": window_end,
        "current_focus": "Debugging the JWT refresh path in Screencap",
        "active_project": "screencap",
        "apps_used": {
            "Chrome": "8 min",
            "Ghostty": "22 min"
        },
        "context_switches": 3,
        "mood": "deep-focus",
        "summary": "Focused on the JWT refresh path across code, docs, and tests."
    })
    .to_string()
}

fn daily_success_response_json(date: &str) -> String {
    json!({
        "type": "daily",
        "date": date,
        "total_active_hours": 7.5,
        "projects": [
            {
                "name": "screencap",
                "total_minutes": 195,
                "activities": ["auth module debugging", "test writing"],
                "key_accomplishments": ["Fixed JWT refresh bug"]
            }
        ],
        "time_allocation": {
            "coding": "3h 15m",
            "communication": "1h 25m"
        },
        "focus_blocks": [
            {
                "start": "09:15",
                "end": "11:45",
                "duration_min": 150,
                "project": "screencap",
                "quality": "deep-focus"
            }
        ],
        "open_threads": [
            "Need to finish the export path",
            "Follow up with Alice on API docs"
        ],
        "narrative": "Productive day focused on screencap. The user made progress on auth, tests, and follow-up communication."
    })
    .to_string()
}

fn seed_recent_extractions(db_path: &Path, window_end: DateTime<Utc>) -> Result<()> {
    let mut db = StorageDb::open_at_path(db_path)?;
    let captures = db.insert_captures(&[
        NewCapture {
            timestamp: window_end - ChronoDuration::minutes(24),
            app_name: Some("Ghostty".into()),
            window_title: Some("src/auth.rs".into()),
            bundle_id: Some("com.mitchellh.ghostty".into()),
            display_id: Some(1),
            screenshot_path: "/tmp/capture-1.jpg".into(),
        },
        NewCapture {
            timestamp: window_end - ChronoDuration::minutes(18),
            app_name: Some("Chrome".into()),
            window_title: Some("JWT refresh tokens - docs".into()),
            bundle_id: Some("com.google.Chrome".into()),
            display_id: Some(1),
            screenshot_path: "/tmp/capture-2.jpg".into(),
        },
        NewCapture {
            timestamp: window_end - ChronoDuration::minutes(7),
            app_name: Some("Ghostty".into()),
            window_title: Some("cargo test".into()),
            bundle_id: Some("com.mitchellh.ghostty".into()),
            display_id: Some(2),
            screenshot_path: "/tmp/capture-3.jpg".into(),
        },
    ])?;

    let first_batch_id = Uuid::new_v4();
    db.persist_extraction_batch(
        &NewExtractionBatch {
            id: first_batch_id,
            batch_start: captures[0].timestamp,
            batch_end: captures[1].timestamp,
            capture_count: 2,
            primary_activity: Some("coding".into()),
            project_context: Some("screencap auth".into()),
            narrative: Some("Investigated the JWT refresh path and compared it with docs.".into()),
            raw_response: Some("{}".into()),
            model_used: Some("mock-vision-model".into()),
            tokens_used: Some(120),
            cost_cents: None,
        },
        &[
            NewExtraction {
                capture_id: captures[0].id,
                batch_id: first_batch_id,
                activity_type: Some(ActivityType::Coding),
                description: Some("Tracing the auth refresh logic in Rust".into()),
                app_context: Some("Editing the screencap auth module".into()),
                project: Some("screencap".into()),
                topics: vec!["jwt".into(), "auth".into()],
                people: vec![],
                key_content: Some("fn refresh_session".into()),
                sentiment: Some(Sentiment::Focused),
            },
            NewExtraction {
                capture_id: captures[1].id,
                batch_id: first_batch_id,
                activity_type: Some(ActivityType::Browsing),
                description: Some("Reading refresh token documentation".into()),
                app_context: Some("Comparing implementation against reference docs".into()),
                project: Some("screencap".into()),
                topics: vec!["jwt".into(), "refresh tokens".into()],
                people: vec![],
                key_content: Some("refresh token rotation".into()),
                sentiment: Some(Sentiment::Exploring),
            },
        ],
    )?;

    let second_batch_id = Uuid::new_v4();
    db.persist_extraction_batch(
        &NewExtractionBatch {
            id: second_batch_id,
            batch_start: captures[2].timestamp,
            batch_end: captures[2].timestamp,
            capture_count: 1,
            primary_activity: Some("terminal".into()),
            project_context: Some("screencap auth".into()),
            narrative: Some("Ran targeted tests after the JWT refresh fix.".into()),
            raw_response: Some("{}".into()),
            model_used: Some("mock-vision-model".into()),
            tokens_used: Some(90),
            cost_cents: None,
        },
        &[NewExtraction {
            capture_id: captures[2].id,
            batch_id: second_batch_id,
            activity_type: Some(ActivityType::Terminal),
            description: Some("Running cargo test for the auth module".into()),
            app_context: Some("Validating the JWT refresh fix".into()),
            project: Some("screencap".into()),
            topics: vec!["jwt".into(), "tests".into()],
            people: vec![],
            key_content: Some("cargo test auth_refresh".into()),
            sentiment: Some(Sentiment::Focused),
        }],
    )?;

    Ok(())
}

fn sample_hourly_new_insight(date: NaiveDate, start_hour: u32) -> NewInsight {
    let hour_start = date
        .and_hms_opt(start_hour, 0, 0)
        .expect("valid hourly insight start")
        .and_utc();
    let hour_end = hour_start + ChronoDuration::hours(1);
    NewInsight {
        insight_type: InsightType::Hourly,
        window_start: hour_start,
        window_end: hour_end,
        data: InsightData::Hourly {
            hour_start,
            hour_end,
            dominant_activity: "coding".into(),
            projects: vec![HourlyProjectSummary {
                name: Some("screencap".into()),
                minutes: 42,
                activities: vec!["debugging auth".into(), "writing tests".into()],
            }],
            topics: vec!["JWT".into(), "authentication".into(), "testing".into()],
            people_interacted: vec!["@alice".into()],
            key_moments: vec!["Found the JWT refresh bug and validated the fix".into()],
            focus_score: 0.72,
            narrative: "Productive coding hour. The user traced the JWT refresh path, checked documentation, ran targeted tests, and shared the result in Slack.".into(),
        },
        model_used: Some("mock-synthesis-model".into()),
        tokens_used: Some(300),
        cost_cents: Some(0.42),
    }
}

fn seed_hourly_insights(db_path: &Path, date: NaiveDate) -> Result<()> {
    let mut db = StorageDb::open_at_path(db_path)?;
    db.insert_insight(&sample_hourly_new_insight(date, 9))?;
    db.insert_insight(&sample_hourly_new_insight(date, 10))?;
    db.insert_insight(&sample_hourly_new_insight(date, 14))?;
    Ok(())
}

struct TestEnvGuard {
    key: String,
    previous: Option<String>,
}

impl TestEnvGuard {
    fn set(key: &str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self {
            key: key.to_owned(),
            previous,
        }
    }
}

impl Drop for TestEnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_var(&self.key, previous);
        } else {
            std::env::remove_var(&self.key);
        }
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
async fn wait_for_generated_rolling_insight(db_path: &Path) -> Result<Insight> {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        let db = StorageDb::open_at_path(db_path)?;
        if let Some(insight) = db.get_latest_insight_by_type(InsightType::Rolling)? {
            return Ok(insight);
        }

        if Instant::now() >= deadline {
            let db = StorageDb::open_at_path(db_path)?;
            let insight_types: Vec<String> = db
                .connection()
                .prepare("SELECT type FROM insights ORDER BY created_at ASC")?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            bail!(
                "timed out waiting for generated rolling insight; existing insight types: {:?}",
                insight_types
            );
        }

        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_generated_daily_insight(db_path: &Path, date: NaiveDate) -> Result<Insight> {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        let db = StorageDb::open_at_path(db_path)?;
        if let Some(insight) = db.get_latest_daily_insight_for_date(date)? {
            return Ok(insight);
        }

        if Instant::now() >= deadline {
            bail!("timed out waiting for generated daily insight for {date}");
        }

        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_json_success(client: &Client, url: &str) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        if let Ok(response) = client.get(url).send().await {
            if response.status().is_success() {
                return response
                    .json::<Value>()
                    .await
                    .with_context(|| format!("failed to decode JSON from {url}"));
            }
        }

        if Instant::now() >= deadline {
            bail!("timed out waiting for JSON response from {url}");
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

#[tokio::test]
async fn daemon_runs_rolling_and_daily_synthesis_schedulers() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let temp = TempDir::new().context("failed to allocate temporary home directory")?;
    let home = temp.path();
    let db_path = home.join(".screencap").join("screencap.db");
    let today = Utc::now().date_naive();
    let window_end = Utc::now();
    let provider = SynthesisProviderServer::spawn();
    let api_key_env = "SCREENCAP_TEST_SYNTHESIS_KEY";
    let port = reserve_port()?;
    let daily_summary_time = window_end.format("%H:%M").to_string();
    let _api_key_guard = TestEnvGuard::set(api_key_env, "token");

    let config_path = write_config_with_synthesis(
        home,
        port,
        &provider.base_url(),
        api_key_env,
        &daily_summary_time,
    )?;
    assert!(
        config_path.exists(),
        "config should be written before daemon start"
    );

    seed_recent_extractions(&db_path, window_end)?;
    seed_hourly_insights(&db_path, today)?;

    let _daemon = ForegroundDaemon::spawn(home)?;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    wait_for_server(&client, &base_url).await?;

    let rolling = wait_for_generated_rolling_insight(&db_path).await?;
    assert_eq!(rolling.insight_type, InsightType::Rolling);
    assert_eq!(rolling.model_used.as_deref(), Some("mock-synthesis-model"));
    let InsightData::Rolling { current_focus, .. } = &rolling.data else {
        unreachable!("expected rolling insight payload");
    };
    assert_eq!(current_focus, "Debugging the JWT refresh path in Screencap");

    let today_string = today.to_string();
    let daily = wait_for_generated_daily_insight(&db_path, today).await?;
    assert_eq!(daily.insight_type, InsightType::Daily);
    assert_eq!(daily.model_used.as_deref(), Some("mock-synthesis-model"));
    let InsightData::Daily { date, projects, .. } = &daily.data else {
        unreachable!("expected daily insight payload");
    };
    assert_eq!(date.to_string(), today_string);
    assert_eq!(
        projects[0].key_accomplishments.first().map(String::as_str),
        Some("Fixed JWT refresh bug")
    );

    let rolling_api =
        wait_for_json_success(&client, &format!("{base_url}/api/insights/current")).await?;
    assert_eq!(rolling_api["insight_type"].as_str(), Some("rolling"));
    assert_eq!(
        rolling_api["model_used"].as_str(),
        Some("mock-synthesis-model")
    );

    let daily_api = wait_for_json_success(
        &client,
        &format!("{base_url}/api/insights/daily?date={today}"),
    )
    .await?;
    assert_eq!(daily_api["insight_type"].as_str(), Some("daily"));
    assert_eq!(
        daily_api["model_used"].as_str(),
        Some("mock-synthesis-model")
    );

    Ok(())
}
