mod support;

use std::{
    fs,
    net::TcpListener,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use reqwest::{header::CONTENT_TYPE, Client};
use screencap::{
    api,
    config::AppConfig,
    storage::{
        db::StorageDb,
        models::{AppCaptureCount, Extraction, NewCapture},
    },
};
use serde::Deserialize;
use tokio::{
    sync::watch,
    time::{sleep, Duration, Instant},
};

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
struct ErrorResponse {
    error: String,
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
async fn api_server_serves_rest_endpoints() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("rest")?;
    let config = test_config(home.path())?;
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

    let timestamp = Utc::now();
    let capture = {
        let mut db = StorageDb::open_at_path(&db_path)?;
        db.insert_capture(&NewCapture {
            timestamp,
            app_name: Some("Code".into()),
            window_title: Some("REST API tests".into()),
            bundle_id: Some("com.microsoft.VSCode".into()),
            display_id: Some(1),
            screenshot_path: screenshot_path.to_string_lossy().into_owned(),
        })?
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
    assert_eq!(rejected_screenshot.status(), 404);

    shutdown_tx
        .send(true)
        .expect("server shutdown channel should accept signal");
    server.await??;

    Ok(())
}
