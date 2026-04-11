mod support;

use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use reqwest::{header::CONTENT_TYPE, Client};
use screencap::{
    api,
    config::{AiProvider, AppConfig},
    storage::{
        db::StorageDb,
        models::{
            ActivityType, AppCaptureCount, DailyProjectSummary, Extraction, FocusBlock,
            HourlyProjectSummary, Insight, InsightData, InsightType, NewCapture, NewExtraction,
            NewExtractionBatch, NewInsight, ProjectTimeAllocation, Sentiment, TopicFrequency,
        },
    },
};
use serde::Deserialize;
use tokio::{
    sync::watch,
    time::{sleep, Duration, Instant},
};
use uuid::Uuid;

struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(name: &str) -> Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("screencap-api-tests-{name}-{unique}"));
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create test home at {}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    uptime_secs: u64,
}

#[derive(Debug, Deserialize)]
struct StatsResponse {
    capture_count: u64,
    captures_today: u64,
    storage_bytes: u64,
    uptime_secs: u64,
}

#[derive(Debug, Deserialize)]
struct ApiCapture {
    id: i64,
    screenshot_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CaptureListResponse {
    captures: Vec<ApiCapture>,
    limit: usize,
    offset: usize,
}

#[derive(Debug, Deserialize)]
struct ApiCaptureDetail {
    capture: ApiCapture,
    extraction: Option<Extraction>,
}

#[derive(Debug, Deserialize)]
struct AppsResponse {
    apps: Vec<AppCaptureCount>,
}

#[derive(Debug, Deserialize)]
struct InsightListResponse {
    insights: Vec<Insight>,
}

#[derive(Debug, Deserialize)]
struct ProjectTimeAllocationResponse {
    projects: Vec<ProjectTimeAllocation>,
}

#[derive(Debug, Deserialize)]
struct TopicFrequencyResponse {
    topics: Vec<TopicFrequency>,
}

#[derive(Debug, Deserialize)]
struct ApiSearchHit {
    capture: ApiCapture,
    extraction: Extraction,
    batch_narrative: Option<String>,
    rank: f64,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    results: Vec<ApiSearchHit>,
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct SemanticSearchReference {
    capture: ApiCapture,
    extraction: Extraction,
}

#[derive(Debug, Deserialize)]
struct SemanticSearchResponse {
    answer: String,
    references: Vec<SemanticSearchReference>,
    cost_cents: Option<f64>,
    tokens_used: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AnalyzeResponse {
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    capture_count: u64,
    analysis_query: Option<String>,
    analysis: AnalyzePayload,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AnalyzePayload {
    RollingContext {
        batch_count: usize,
        analyzed_capture_count: usize,
        insight: InsightData,
        tokens_used: Option<u32>,
        cost_cents: Option<f64>,
    },
    QuestionAnswer {
        analyzed_capture_count: usize,
        answer: String,
        references: Vec<SemanticSearchReference>,
        tokens_used: Option<u32>,
        cost_cents: Option<f64>,
    },
}


struct TestServer {
    address: SocketAddr,
    handle: Option<thread::JoinHandle<()>>,
}

impl TestServer {
    fn spawn(status: u16, body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener addr");
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer);
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        Self {
            address,
            handle: Some(handle),
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = TcpStream::connect(self.address);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
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

fn reserve_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("failed to reserve local tcp port")?;
    listener
        .local_addr()
        .map(|address| address.port())
        .context("failed to read reserved tcp port")
}

fn test_config(home: &Path) -> Result<AppConfig> {
    let app_root = home.join(".screencap");
    let mut config = AppConfig::load_from_root_and_home(&app_root, home)?;
    config.server.port = reserve_port()?;
    config.capture.excluded_apps.clear();
    config.capture.excluded_window_titles.clear();
    Ok(config)
}

fn sample_screenshot_path(config: &AppConfig, home: &Path) -> PathBuf {
    let timestamp = Utc::now();
    config
        .screenshots_root(home)
        .join(timestamp.format("%Y").to_string())
        .join(timestamp.format("%m").to_string())
        .join(timestamp.format("%d").to_string())
        .join("api-test.jpg")
}

fn write_screenshot_fixture(path: &Path) -> Result<()> {
    fs::create_dir_all(
        path.parent()
            .expect("screenshot fixture should have a parent directory"),
    )
    .with_context(|| format!("failed to create screenshot parent for {}", path.display()))?;
    fs::write(path, b"test-jpeg")
        .with_context(|| format!("failed to write screenshot fixture at {}", path.display()))?;
    Ok(())
}

fn seed_processed_capture(
    db_path: &Path,
    screenshot_path: &Path,
    timestamp: DateTime<Utc>,
) -> Result<(i64, i64)> {
    let mut db = StorageDb::open_at_path(db_path)?;
    let capture = db.insert_capture(&NewCapture {
        timestamp,
        app_name: Some("Code".into()),
        window_title: Some("REST API tests".into()),
        bundle_id: Some("com.microsoft.VSCode".into()),
        display_id: Some(1),
        screenshot_path: screenshot_path.to_string_lossy().into_owned(),
    })?;

    let batch_id = Uuid::new_v4();
    db.insert_extraction_batch(&NewExtractionBatch {
        id: batch_id,
        batch_start: timestamp - ChronoDuration::minutes(5),
        batch_end: timestamp + ChronoDuration::minutes(5),
        capture_count: 1,
        primary_activity: Some("coding".into()),
        project_context: Some("screencap".into()),
        narrative: Some("Added API search and insight endpoints for the daemon.".into()),
        raw_response: None,
        model_used: Some("mock-extractor".into()),
        tokens_used: Some(144),
        cost_cents: Some(0.32),
    })?;

    let extraction = db.insert_extraction(&NewExtraction {
        capture_id: capture.id,
        batch_id,
        activity_type: Some(ActivityType::Coding),
        description: Some("Implemented search endpoint filters for the API".into()),
        app_context: Some("Editing axum routes and integration tests".into()),
        project: Some("screencap".into()),
        topics: vec!["api".into(), "search".into()],
        people: vec![],
        key_content: Some("GET /api/search?q=filters".into()),
        sentiment: Some(Sentiment::Focused),
    })?;

    Ok((capture.id, extraction.id))
}

async fn start_test_api_server(
    config: &AppConfig,
    home: &Path,
 ) -> Result<(watch::Sender<bool>, tokio::task::JoinHandle<Result<()>>, Client, String)> {
    let listener = api::server::bind(config).await?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(api::server::serve(
        listener,
        config.clone(),
        home.to_path_buf(),
        shutdown_rx,
    ));
    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{}", config.server.port);
    wait_for_server(&client, &base_url).await?;
    Ok((shutdown_tx, server, client, base_url))
}

async fn stop_test_api_server(
    client: Client,
    shutdown_tx: watch::Sender<bool>,
    server: tokio::task::JoinHandle<Result<()>>,
 ) -> Result<()> {
    drop(client);
    shutdown_tx
        .send(true)
        .expect("server shutdown channel should accept signal");
    server.await??;
    Ok(())
}


async fn wait_for_server(client: &Client, base_url: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        if let Ok(response) = client.get(format!("{base_url}/api/health")).send().await {
            if response.status().is_success() {
                return Ok(());
            }
        }

        if Instant::now() >= deadline {
            bail!("timed out waiting for api server readiness");
        }

        sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn api_server_serves_embedded_ui_and_spa_fallback() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("embedded-ui")?;
    let config = test_config(home.path())?;

    let listener = api::server::bind(&config).await?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(api::server::serve(
        listener,
        config.clone(),
        home.path().to_path_buf(),
        shutdown_rx,
    ));

    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{}", config.server.port);
    wait_for_server(&client, &base_url).await?;

    let health_response = client
        .get(format!("{base_url}/api/health"))
        .send()
        .await?
        .error_for_status()?;
    assert_eq!(
        health_response
            .headers()
            .get(CONTENT_TYPE)
            .expect("content type header should exist"),
        "application/json"
    );
    let health: HealthResponse = health_response.json().await?;
    assert_eq!(health.status, "ok");

    let root_response = client.get(format!("{base_url}/")).send().await?;
    assert_eq!(root_response.status(), 200);
    assert_eq!(
        root_response
            .headers()
            .get(CONTENT_TYPE)
            .expect("content type header should exist"),
        "text/html"
    );
    let root_html = root_response.text().await?;
    assert!(root_html.contains("<html"));

    let fallback_response = client.get(format!("{base_url}/timeline")).send().await?;
    assert_eq!(fallback_response.status(), 200);
    assert_eq!(
        fallback_response
            .headers()
            .get(CONTENT_TYPE)
            .expect("content type header should exist"),
        "text/html"
    );
    let fallback_html = fallback_response.text().await?;
    assert!(fallback_html.contains("<html"));
    assert_eq!(fallback_html, root_html);

    drop(client);
    shutdown_tx
        .send(true)
        .expect("server shutdown channel should accept signal");
    server.await??;

    Ok(())
}

#[tokio::test]
async fn api_server_serves_rest_endpoints() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("rest")?;
    let mut config = test_config(home.path())?;
    let semantic_llm = TestServer::spawn(
        200,
        "{\"choices\":[{\"message\":{\"content\":\"{\\\"answer\\\":\\\"You were implementing API search filters.\\\",\\\"capture_ids\\\":[1]}\"}}],\"usage\":{\"prompt_tokens\":81,\"completion_tokens\":23,\"total_tokens\":104,\"cost\":0.19}}".to_owned(),
    );
    let semantic_api_key_env = "SCREENCAP_TEST_API_SEMANTIC_KEY";
    config.synthesis.provider = AiProvider::Openai;
    config.synthesis.model = "mock-synthesis-model".into();
    config.synthesis.api_key_env = semantic_api_key_env.into();
    config.synthesis.base_url = semantic_llm.base_url();
    let _api_key_guard = TestEnvGuard::set(semantic_api_key_env, "token");

    let db_path = config.storage_root(home.path()).join("screencap.db");
    let screenshot_path = sample_screenshot_path(&config, home.path());
    fs::create_dir_all(
        screenshot_path
            .parent()
            .expect("screenshot path should have a parent directory"),
    )
    .with_context(|| {
        format!(
            "failed to create screenshot parent for {}",
            screenshot_path.display()
        )
    })?;
    fs::write(&screenshot_path, b"test-jpeg").with_context(|| {
        format!(
            "failed to write screenshot fixture at {}",
            screenshot_path.display()
        )
    })?;

    let listener = api::server::bind(&config).await?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(api::server::serve(
        listener,
        config.clone(),
        home.path().to_path_buf(),
        shutdown_rx,
    ));

    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{}", config.server.port);
    println!("Server running at {}", base_url);
    wait_for_server(&client, &base_url).await?;

    let health: HealthResponse = client
        .get(format!("{base_url}/api/health"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(health.status, "ok");
    assert!(health.uptime_secs <= 5);

    assert!(
        !db_path.exists(),
        "read-only API requests should not create the database"
    );

    let empty_stats: StatsResponse = client
        .get(format!("{base_url}/api/stats"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(empty_stats.capture_count, 0);
    assert_eq!(empty_stats.captures_today, 0);
    assert!(empty_stats.storage_bytes >= b"test-jpeg".len() as u64);
    assert!(!db_path.exists(), "stats should not create the database");

    let empty_captures: CaptureListResponse = client
        .get(format!("{base_url}/api/captures"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert!(empty_captures.captures.is_empty());
    assert!(
        !db_path.exists(),
        "capture listing should not create the database"
    );

    let empty_apps: AppsResponse = client
        .get(format!("{base_url}/api/apps"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert!(empty_apps.apps.is_empty());
    assert!(
        !db_path.exists(),
        "app listing should not create the database"
    );

    let invalid_query = client
        .get(format!("{base_url}/api/captures?offset=abc"))
        .send()
        .await?;
    assert_eq!(invalid_query.status(), 400);
    let invalid_query: ErrorResponse = invalid_query.json().await?;
    assert_eq!(invalid_query.error, "invalid capture query parameters");

    let timestamp = Utc.with_ymd_and_hms(2026, 4, 10, 14, 5, 0).unwrap();
    let insight_date = timestamp.date_naive();
    let (capture, extraction) = {
        let mut db = StorageDb::open_at_path(&db_path)?;
        let capture = db.insert_capture(&NewCapture {
            timestamp,
            app_name: Some("Code".into()),
            window_title: Some("REST API tests".into()),
            bundle_id: Some("com.microsoft.VSCode".into()),
            display_id: Some(1),
            screenshot_path: screenshot_path.to_string_lossy().into_owned(),
        })?;

        let batch_id = Uuid::new_v4();
        db.insert_extraction_batch(&NewExtractionBatch {
            id: batch_id,
            batch_start: timestamp - ChronoDuration::minutes(5),
            batch_end: timestamp + ChronoDuration::minutes(5),
            capture_count: 1,
            primary_activity: Some("coding".into()),
            project_context: Some("screencap".into()),
            narrative: Some("Added API search and insight endpoints for the daemon.".into()),
            raw_response: None,
            model_used: Some("mock-extractor".into()),
            tokens_used: Some(144),
            cost_cents: Some(0.32),
        })?;
        let extraction = db.insert_extraction(&NewExtraction {
            capture_id: capture.id,
            batch_id,
            activity_type: Some(ActivityType::Coding),
            description: Some("Implemented search endpoint filters for the API".into()),
            app_context: Some("Editing axum routes and integration tests".into()),
            project: Some("screencap".into()),
            topics: vec!["api".into(), "search".into()],
            people: vec![],
            key_content: Some("GET /api/search?q=filters".into()),
            sentiment: Some(Sentiment::Focused),
        })?;

        let rolling_start = timestamp - ChronoDuration::minutes(30);
        let rolling_end = timestamp;
        db.insert_insight(&NewInsight {
            insight_type: InsightType::Rolling,
            window_start: rolling_start,
            window_end: rolling_end,
            data: InsightData::Rolling {
                window_start: rolling_start,
                window_end: rolling_end,
                current_focus: "Implementing the US-013 insight and search endpoints".into(),
                active_project: Some("screencap".into()),
                apps_used: BTreeMap::from([("Code".into(), "30 min".into())]),
                context_switches: 1,
                mood: "focused".into(),
                summary: "Focused API work on insight and search endpoints.".into(),
            },
            model_used: Some("mock-synthesis".into()),
            tokens_used: Some(210),
            cost_cents: Some(0.21),
        })?;

        let hour_start = insight_date.and_hms_opt(14, 0, 0).unwrap().and_utc();
        let hour_end = hour_start + ChronoDuration::hours(1);
        db.insert_insight(&NewInsight {
            insight_type: InsightType::Hourly,
            window_start: hour_start,
            window_end: hour_end,
            data: InsightData::Hourly {
                hour_start,
                hour_end,
                dominant_activity: "coding".into(),
                projects: vec![HourlyProjectSummary {
                    name: Some("screencap".into()),
                    minutes: 55,
                    activities: vec!["api endpoints".into(), "search filters".into()],
                }],
                topics: vec!["api".into(), "search".into()],
                people_interacted: vec![],
                key_moments: vec!["Added the insights and search API endpoints".into()],
                focus_score: 0.83,
                narrative: "Spent the hour building and verifying API insight endpoints.".into(),
            },
            model_used: Some("mock-synthesis".into()),
            tokens_used: Some(320),
            cost_cents: Some(0.43),
        })?;

        let day_start = insight_date.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let day_end = insight_date
            .succ_opt()
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        db.insert_insight(&NewInsight {
            insight_type: InsightType::Daily,
            window_start: day_start,
            window_end: day_end,
            data: InsightData::Daily {
                date: insight_date,
                total_active_hours: 6.5,
                projects: vec![DailyProjectSummary {
                    name: "screencap".into(),
                    total_minutes: 180,
                    activities: vec!["REST API work".into()],
                    key_accomplishments: vec!["Added insights endpoints".into()],
                }],
                time_allocation: BTreeMap::from([("coding".into(), "3h 0m".into())]),
                focus_blocks: vec![FocusBlock {
                    start: "14:00".into(),
                    end: "15:00".into(),
                    duration_min: 60,
                    project: "screencap".into(),
                    quality: "deep-focus".into(),
                }],
                open_threads: vec!["Wire the UI to the new endpoints".into()],
                narrative: "Most of the day went into API endpoint work for screencap.".into(),
            },
            model_used: Some("mock-synthesis".into()),
            tokens_used: Some(410),
            cost_cents: Some(0.58),
        })?;

        (capture, extraction)
    };

    let relative_screenshot_path = screenshot_path
        .strip_prefix(config.screenshots_root(home.path()))
        .expect("fixture screenshot should be under screenshots root")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");

    let stats: StatsResponse = client
        .get(format!("{base_url}/api/stats"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(stats.capture_count, 1);
    let expected_captures_today = if timestamp.date_naive() == Utc::now().date_naive() {
        1
    } else {
        0
    };
    assert_eq!(stats.captures_today, expected_captures_today);
    assert!(stats.storage_bytes >= b"test-jpeg".len() as u64);
    assert!(stats.uptime_secs <= 5);

    let from = (timestamp - ChronoDuration::minutes(1)).to_rfc3339();
    let to = (timestamp + ChronoDuration::minutes(1)).to_rfc3339();
    let captures: CaptureListResponse = client
        .get(format!("{base_url}/api/captures"))
        .query(&[
            ("from", from.as_str()),
            ("to", to.as_str()),
            ("app", "Code"),
            ("limit", "10"),
            ("offset", "0"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(captures.limit, 10);
    assert_eq!(captures.offset, 0);
    assert_eq!(captures.captures.len(), 1);
    assert_eq!(captures.captures[0].id, capture.id);

    assert_eq!(
        captures.captures[0].screenshot_url.as_deref(),
        Some(format!("/api/screenshots/{relative_screenshot_path}").as_str())
    );

    let detail: ApiCaptureDetail = client
        .get(format!("{base_url}/api/captures/{}", capture.id))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(detail.capture.id, capture.id);
    assert_eq!(
        detail.capture.screenshot_url.as_deref(),
        Some(format!("/api/screenshots/{relative_screenshot_path}").as_str())
    );
    assert!(detail.extraction.is_none());

    let apps: AppsResponse = client
        .get(format!("{base_url}/api/apps"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(
        apps.apps,
        vec![AppCaptureCount {
            app_name: "Code".into(),
            capture_count: 1,
        }]
    );

    let insight_date_str = insight_date.to_string();

    let current: Insight = client
        .get(format!("{base_url}/api/insights/current"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    match current.data {
        InsightData::Rolling {
            current_focus,
            active_project,
            ..
        } => {
            assert_eq!(
                current_focus,
                "Implementing the US-013 insight and search endpoints"
            );
            assert_eq!(active_project.as_deref(), Some("screencap"));
        }
        other => panic!("expected rolling insight, got {other:?}"),
    }

    let hourly: InsightListResponse = client
        .get(format!("{base_url}/api/insights/hourly"))
        .query(&[("date", insight_date_str.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(hourly.insights.len(), 1);
    assert_eq!(hourly.insights[0].insight_type, InsightType::Hourly);

    let daily: Insight = client
        .get(format!("{base_url}/api/insights/daily"))
        .query(&[("date", insight_date_str.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(daily.insight_type, InsightType::Daily);

    let daily_range: InsightListResponse = client
        .get(format!("{base_url}/api/insights/daily"))
        .query(&[
            ("from", insight_date_str.as_str()),
            ("to", insight_date_str.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(daily_range.insights.len(), 1);
    assert_eq!(daily_range.insights[0].insight_type, InsightType::Daily);

    let project_allocations: ProjectTimeAllocationResponse = client
        .get(format!("{base_url}/api/insights/projects"))
        .query(&[("from", from.as_str()), ("to", to.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(
        project_allocations.projects,
        vec![ProjectTimeAllocation {
            project: Some("screencap".into()),
            capture_count: 1,
        }]
    );

    let topic_frequencies: TopicFrequencyResponse = client
        .get(format!("{base_url}/api/insights/topics"))
        .query(&[("from", from.as_str()), ("to", to.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(
        topic_frequencies.topics,
        vec![
            TopicFrequency {
                topic: "api".into(),
                capture_count: 1,
            },
            TopicFrequency {
                topic: "search".into(),
                capture_count: 1,
            },
        ]
    );

    let search_results: SearchResponse = client
        .get(format!("{base_url}/api/search"))
        .query(&[
            ("q", "filters"),
            ("app", "Code"),
            ("project", "screencap"),
            ("from", from.as_str()),
            ("to", to.as_str()),
            ("limit", "5"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(search_results.limit, 5);
    assert_eq!(search_results.results.len(), 1);
    assert_eq!(search_results.results[0].capture.id, capture.id);
    assert_eq!(search_results.results[0].extraction.id, extraction.id);
    assert_eq!(
        search_results.results[0].capture.screenshot_url.as_deref(),
        Some(format!("/api/screenshots/{relative_screenshot_path}").as_str())
    );
    assert_eq!(
        search_results.results[0].batch_narrative.as_deref(),
        Some("Added API search and insight endpoints for the daemon.")
    );
    assert!(search_results.results[0].rank.is_finite());
    let semantic_results: SemanticSearchResponse = client
        .get(format!("{base_url}/api/search/semantic"))
        .query(&[
            ("q", "api filters"),
            ("from", from.as_str()),
            ("to", to.as_str()),
            ("limit", "5"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(
        semantic_results.answer,
        "You were implementing API search filters."
    );
    assert_eq!(semantic_results.references.len(), 1);
    assert_eq!(semantic_results.references[0].capture.id, capture.id);
    assert_eq!(semantic_results.references[0].extraction.id, extraction.id);
    assert_eq!(semantic_results.tokens_used, Some(104));
    assert_eq!(semantic_results.cost_cents, Some(0.19));

    let screenshot_response = client
        .get(format!(
            "{base_url}/api/screenshots/{relative_screenshot_path}"
        ))
        .send()
        .await?
        .error_for_status()?;
    assert_eq!(
        screenshot_response
            .headers()
            .get(CONTENT_TYPE)
            .expect("content type header should exist"),
        "image/jpeg"
    );
    let screenshot_bytes = screenshot_response.bytes().await?;
    assert_eq!(screenshot_bytes.as_ref(), b"test-jpeg");

    let screenshot_directory = screenshot_path
        .parent()
        .expect("screenshot path should have a parent directory")
        .strip_prefix(config.screenshots_root(home.path()))
        .expect("screenshot directory should be under screenshots root")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let directory_response = client
        .get(format!("{base_url}/api/screenshots/{screenshot_directory}"))
        .send()
        .await?;
    assert_eq!(directory_response.status(), 404);

    let secret_path = home.path().join("outside-secret.txt");
    fs::write(&secret_path, b"secret").with_context(|| {
        format!(
            "failed to write secret fixture at {}",
            secret_path.display()
        )
    })?;
    let escaped_screenshot_path = screenshot_path
        .parent()
        .expect("screenshot path should have a parent directory")
        .join("escape.jpg");
    symlink(&secret_path, &escaped_screenshot_path).with_context(|| {
        format!(
            "failed to create screenshot escape symlink at {}",
            escaped_screenshot_path.display()
        )
    })?;
    let escaped_relative_path = escaped_screenshot_path
        .strip_prefix(config.screenshots_root(home.path()))
        .expect("escape symlink should be under screenshots root")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let rejected_screenshot = client
        .get(format!(
            "{base_url}/api/screenshots/{escaped_relative_path}"
        ))
        .send()
        .await?;
    println!(
        "rejected_screenshot status: {}",
        rejected_screenshot.status()
    );
    assert_eq!(rejected_screenshot.status(), 404);

    let html_response = client.get(format!("{base_url}/")).send().await?;
    println!("html response status: {}", html_response.status());
    println!("html response headers: {:?}", html_response.headers());
    assert_eq!(html_response.status(), 200);
    assert_eq!(
        html_response
            .headers()
            .get(CONTENT_TYPE)
            .expect("content type header should exist"),
        "text/html"
    );
    let html_text = html_response.text().await?;
    assert!(html_text.contains("<html"));
    assert!(html_text.contains("</html>"));

    drop(client);
    shutdown_tx
        .send(true)
        .expect("server shutdown channel should accept signal");
    server.await??;

    Ok(())
}


#[tokio::test]
async fn api_server_analyzes_time_range_with_summary_payload() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("analyze-summary")?;
    let mut config = test_config(home.path())?;
    let timestamp = Utc.with_ymd_and_hms(2026, 4, 10, 14, 5, 0).unwrap();
    let window_start = timestamp - ChronoDuration::minutes(1);
    let window_end = timestamp + ChronoDuration::minutes(1);
    let provider_body = serde_json::json!({
        "choices": [{
            "message": {
                "content": serde_json::json!({
                    "type": "rolling",
                    "window_start": window_start.to_rfc3339(),
                    "window_end": window_end.to_rfc3339(),
                    "current_focus": "Reviewing API activity for a custom window",
                    "active_project": "screencap",
                    "apps_used": {"Code": "2 min"},
                    "context_switches": 0,
                    "mood": "focused",
                    "summary": "Focused review of API work in the selected time range."
                })
                .to_string()
            }
        }],
        "usage": {
            "prompt_tokens": 81,
            "completion_tokens": 23,
            "total_tokens": 104,
            "cost": 0.27
        }
    })
    .to_string();
    let analysis_llm = TestServer::spawn(200, provider_body);
    let analysis_api_key_env = "SCREENCAP_TEST_ANALYZE_SUMMARY_KEY";
    config.synthesis.provider = AiProvider::Openai;
    config.synthesis.model = "mock-synthesis-model".into();
    config.synthesis.api_key_env = analysis_api_key_env.into();
    config.synthesis.base_url = analysis_llm.base_url();
    let _api_key_guard = TestEnvGuard::set(analysis_api_key_env, "token");

    let db_path = config.storage_root(home.path()).join("screencap.db");
    let screenshot_path = sample_screenshot_path(&config, home.path());
    write_screenshot_fixture(&screenshot_path)?;
    seed_processed_capture(&db_path, &screenshot_path, timestamp)?;

    let (shutdown_tx, server, client, base_url) = start_test_api_server(&config, home.path()).await?;
    let response: AnalyzeResponse = client
        .post(format!("{base_url}/api/analyze"))
        .json(&serde_json::json!({
            "from": window_start.to_rfc3339(),
            "to": window_end.to_rfc3339()
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    assert_eq!(response.window_start, window_start);
    assert_eq!(response.window_end, window_end);
    assert_eq!(response.capture_count, 1);
    assert_eq!(response.analysis_query, None);
    match response.analysis {
        AnalyzePayload::RollingContext {
            batch_count,
            analyzed_capture_count,
            insight,
            tokens_used,
            cost_cents,
        } => {
            assert_eq!(batch_count, 1);
            assert_eq!(analyzed_capture_count, 1);
            assert_eq!(tokens_used, Some(104));
            assert_eq!(cost_cents, Some(0.27));
            match insight {
                InsightData::Rolling {
                    current_focus,
                    active_project,
                    summary,
                    ..
                } => {
                    assert_eq!(current_focus, "Reviewing API activity for a custom window");
                    assert_eq!(active_project.as_deref(), Some("screencap"));
                    assert_eq!(
                        summary,
                        "Focused review of API work in the selected time range."
                    );
                }
                other => panic!("expected rolling insight payload, got {other:?}"),
            }
        }
        other => panic!("expected rolling analysis payload, got {other:?}"),
    }

    stop_test_api_server(client, shutdown_tx, server).await
}

#[tokio::test]
async fn api_server_analyze_accepts_prompt_alias() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("analyze-prompt")?;
    let mut config = test_config(home.path())?;
    let provider_body = serde_json::json!({
        "choices": [{
            "message": {
                "content": serde_json::json!({
                    "answer": "You were implementing API search filters.",
                    "capture_ids": [1]
                })
                .to_string()
            }
        }],
        "usage": {
            "prompt_tokens": 79,
            "completion_tokens": 21,
            "total_tokens": 100,
            "cost": 0.19
        }
    })
    .to_string();
    let analysis_llm = TestServer::spawn(200, provider_body);
    let analysis_api_key_env = "SCREENCAP_TEST_ANALYZE_PROMPT_KEY";
    config.synthesis.provider = AiProvider::Openai;
    config.synthesis.model = "mock-synthesis-model".into();
    config.synthesis.api_key_env = analysis_api_key_env.into();
    config.synthesis.base_url = analysis_llm.base_url();
    let _api_key_guard = TestEnvGuard::set(analysis_api_key_env, "token");

    let timestamp = Utc.with_ymd_and_hms(2026, 4, 10, 14, 5, 0).unwrap();
    let window_start = timestamp - ChronoDuration::minutes(1);
    let window_end = timestamp + ChronoDuration::minutes(1);
    let db_path = config.storage_root(home.path()).join("screencap.db");
    let screenshot_path = sample_screenshot_path(&config, home.path());
    write_screenshot_fixture(&screenshot_path)?;
    let (capture_id, extraction_id) = seed_processed_capture(&db_path, &screenshot_path, timestamp)?;

    let (shutdown_tx, server, client, base_url) = start_test_api_server(&config, home.path()).await?;
    let response: AnalyzeResponse = client
        .post(format!("{base_url}/api/analyze"))
        .json(&serde_json::json!({
            "from": window_start.to_rfc3339(),
            "to": window_end.to_rfc3339(),
            "prompt": "filters"
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    assert_eq!(response.capture_count, 1);
    assert_eq!(response.analysis_query.as_deref(), Some("filters"));
    match response.analysis {
        AnalyzePayload::QuestionAnswer {
            analyzed_capture_count,
            answer,
            references,
            tokens_used,
            cost_cents,
        } => {
            assert_eq!(analyzed_capture_count, 1);
            assert_eq!(answer, "You were implementing API search filters.");
            assert_eq!(tokens_used, Some(100));
            assert_eq!(cost_cents, Some(0.19));
            assert_eq!(references.len(), 1);
            assert_eq!(references[0].capture.id, capture_id);
            assert_eq!(references[0].extraction.id, extraction_id);
        }
        other => panic!("expected question-answer analysis payload, got {other:?}"),
    }

    stop_test_api_server(client, shutdown_tx, server).await
}

#[tokio::test]
async fn api_server_analyze_returns_actionable_errors() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("analyze-errors")?;
    let mut config = test_config(home.path())?;
    let missing_api_key_env = "SCREENCAP_TEST_ANALYZE_ERRORS_KEY";
    config.synthesis.provider = AiProvider::Openai;
    config.synthesis.model = "mock-synthesis-model".into();
    config.synthesis.api_key_env = missing_api_key_env.into();
    config.synthesis.base_url = "http://127.0.0.1:9".into();

    let timestamp = Utc.with_ymd_and_hms(2026, 4, 10, 14, 5, 0).unwrap();
    let window_start = timestamp - ChronoDuration::minutes(1);
    let window_end = timestamp + ChronoDuration::minutes(1);
    let db_path = config.storage_root(home.path()).join("screencap.db");
    let screenshot_path = sample_screenshot_path(&config, home.path());
    write_screenshot_fixture(&screenshot_path)?;
    seed_processed_capture(&db_path, &screenshot_path, timestamp)?;

    let (shutdown_tx, server, client, base_url) = start_test_api_server(&config, home.path()).await?;

    let invalid_range = client
        .post(format!("{base_url}/api/analyze"))
        .json(&serde_json::json!({
            "from": window_end.to_rfc3339(),
            "to": window_start.to_rfc3339()
        }))
        .send()
        .await?;
    assert_eq!(invalid_range.status(), 400);
    let invalid_range: ErrorResponse = invalid_range.json().await?;
    assert_eq!(invalid_range.error, "`from` must be less than or equal to `to`");

    let unavailable = client
        .post(format!("{base_url}/api/analyze"))
        .json(&serde_json::json!({
            "from": window_start.to_rfc3339(),
            "to": window_end.to_rfc3339()
        }))
        .send()
        .await?;
    assert_eq!(unavailable.status(), 503);
    let unavailable: ErrorResponse = unavailable.json().await?;
    assert!(unavailable.error.contains("analysis provider unavailable"));
    assert!(unavailable.error.contains(missing_api_key_env));

    stop_test_api_server(client, shutdown_tx, server).await
}