mod support;

use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use reqwest::{header::CONTENT_TYPE, Client};
use screencap::{
    ai::mock::MockLlmProvider,
    api,
    capture::screenshot::capture_screenshot,
    config::AppConfig,
    pipeline::{
        scheduler::ExtractionScheduler,
        synthesis::{DailySummaryScheduler, HourlyDigestScheduler, RollingContextScheduler},
    },
    storage::{
        db::StorageDb,
        models::{InsightType, NewCapture},
    },
};
use serde_json::Value;
use tempfile::TempDir;
use tokio::{
    sync::watch,
    time::{sleep, Duration, Instant},
};

fn reserve_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("failed to reserve local tcp port")?;
    listener
        .local_addr()
        .map(|address| address.port())
        .context("failed to read reserved tcp port")
}

fn test_config(home: &Path) -> Result<AppConfig> {
    let mut config = AppConfig::default();
    config.server.port = reserve_port()?;
    config.extraction.enabled = true;
    config.extraction.model = "mock-extraction-model".into();
    config.extraction.max_images_per_batch = 10;
    config.synthesis.enabled = true;
    config.synthesis.model = "mock-synthesis-model".into();
    config.synthesis.hourly_enabled = true;
    config.synthesis.daily_export_markdown = false;

    std::fs::create_dir_all(config.storage_root(home)).with_context(|| {
        format!(
            "failed to create storage root at {}",
            config.storage_root(home).display()
        )
    })?;

    Ok(config)
}

fn pipeline_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 10, 15, 0, 0)
        .single()
        .expect("fixed pipeline timestamp should be valid")
}

fn screenshot_path(
    config: &AppConfig,
    home: &Path,
    timestamp: chrono::DateTime<Utc>,
    index: usize,
) -> PathBuf {
    config
        .screenshots_root(home)
        .join(timestamp.format("%Y").to_string())
        .join(timestamp.format("%m").to_string())
        .join(timestamp.format("%d").to_string())
        .join(format!("{}-{index}.jpg", timestamp.format("%H%M%S")))
}

fn seed_capture_batch(
    config: &AppConfig,
    home: &Path,
    count: usize,
    window_end: chrono::DateTime<Utc>,
) -> Result<()> {
    let db_path = config.storage_root(home).join("screencap.db");
    let mut db = StorageDb::open_at_path(&db_path)?;
    let batch_start = window_end - ChronoDuration::minutes(55);

    for index in 0..count {
        let timestamp = batch_start + ChronoDuration::minutes((index as i64) * 5);
        let screenshot_path = screenshot_path(config, home, timestamp, index);
        capture_screenshot(0, &screenshot_path, config.capture.jpeg_quality)?;

        db.insert_capture(&NewCapture {
            timestamp,
            app_name: Some(format!("App {index}")),
            window_title: Some(format!("Window {index}")),
            bundle_id: Some(format!("dev.screencap.app-{index}")),
            display_id: Some((index % 2) as i64),
            screenshot_path: screenshot_path.to_string_lossy().into_owned(),
        })?;
    }

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
            anyhow::bail!("timed out waiting for API server readiness");
        }

        sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
#[ignore = "requires macOS Screen Recording permission; run with cargo test --ignored on a permissioned machine"]
async fn full_pipeline_persists_and_serves_processed_results() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let temp = TempDir::new().context("failed to allocate temporary home directory")?;
    let home = temp.path();
    let config = test_config(home)?;
    let window_end = pipeline_now();
    let db_path = config.storage_root(home).join("screencap.db");

    seed_capture_batch(&config, home, 10, window_end)?;

    let extraction_provider = Arc::new(MockLlmProvider::new());
    let mut extraction_scheduler =
        ExtractionScheduler::with_provider(config.clone(), home, extraction_provider)?;
    let extraction_report = extraction_scheduler.run_once().await?;
    assert_eq!(extraction_report.processed_batches, 1);
    assert_eq!(extraction_report.processed_captures, 10);
    assert_eq!(extraction_report.failed_batches, 0);
    assert_eq!(extraction_report.failed_captures, 0);
    drop(extraction_scheduler);

    {
        let db = StorageDb::open_at_path(&db_path)?;
        let processed_count: i64 = db.connection().query_row(
            "SELECT COUNT(*) FROM captures WHERE extraction_status = 'processed'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(processed_count, 10);

        let extraction_count: i64 =
            db.connection()
                .query_row("SELECT COUNT(*) FROM extractions", [], |row| row.get(0))?;
        assert_eq!(extraction_count, 10);

        let search_index_count: i64 =
            db.connection()
                .query_row("SELECT COUNT(*) FROM search_index", [], |row| row.get(0))?;
        assert_eq!(search_index_count, 10);
        assert!(!db.search_extractions("pipeline")?.is_empty());
    }

    let synthesis_provider = Arc::new(MockLlmProvider::new());

    let mut rolling_scheduler =
        RollingContextScheduler::with_provider(config.clone(), home, synthesis_provider.clone())?;
    let rolling = rolling_scheduler
        .run_once_at(window_end)
        .await?
        .context("rolling context should be created")?;
    assert_eq!(rolling.insight_type, InsightType::Rolling);
    drop(rolling_scheduler);

    let mut hourly_scheduler =
        HourlyDigestScheduler::with_provider(config.clone(), home, synthesis_provider.clone())?;
    let hourly = hourly_scheduler
        .run_once_at(window_end)
        .await?
        .context("hourly digest should be created")?;
    assert_eq!(hourly.insight_type, InsightType::Hourly);
    drop(hourly_scheduler);

    let mut daily_scheduler =
        DailySummaryScheduler::with_provider(config.clone(), home, synthesis_provider)?;
    let daily = daily_scheduler
        .run_once_at(window_end)
        .await?
        .context("daily summary should be created")?;
    assert_eq!(daily.insight_type, InsightType::Daily);
    drop(daily_scheduler);

    {
        let db = StorageDb::open_at_path(&db_path)?;
        assert!(db
            .get_latest_insight_by_type(InsightType::Rolling)?
            .is_some());
        assert!(db
            .get_latest_insight_by_type(InsightType::Hourly)?
            .is_some());
        assert!(db
            .get_latest_daily_insight_for_date(window_end.date_naive())?
            .is_some());
    }

    let listener = api::server::bind(&config).await?;
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

    let health_response = client
        .get(format!("{base_url}/api/health"))
        .send()
        .await?
        .error_for_status()?;
    assert_eq!(
        health_response
            .headers()
            .get(CONTENT_TYPE)
            .context("health response should include content type")?,
        "application/json"
    );
    let health: Value = health_response.json().await?;
    assert_eq!(health["status"].as_str(), Some("ok"));

    let captures: Value = client
        .get(format!("{base_url}/api/captures"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let capture_items = captures["captures"]
        .as_array()
        .context("captures response should contain an array")?;
    assert_eq!(capture_items.len(), 10);

    let search_results: Value = client
        .get(format!("{base_url}/api/search"))
        .query(&[("q", "pipeline")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let search_items = search_results["results"]
        .as_array()
        .context("search response should contain a results array")?;
    assert!(!search_items.is_empty());

    let current: Value = client
        .get(format!("{base_url}/api/insights/current"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(current["insight_type"].as_str(), Some("rolling"));

    let date = window_end.date_naive().to_string();
    let daily_response: Value = client
        .get(format!("{base_url}/api/insights/daily"))
        .query(&[("date", date.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(daily_response["insight_type"].as_str(), Some("daily"));

    let root_response = client.get(format!("{base_url}/")).send().await?;
    assert_eq!(root_response.status(), 200);
    assert_eq!(
        root_response
            .headers()
            .get(CONTENT_TYPE)
            .context("root response should include content type")?,
        "text/html"
    );
    let root_html = root_response.text().await?;
    assert!(root_html.contains("<html"));

    let screenshot_url = capture_items[0]["screenshot_url"]
        .as_str()
        .context("capture should include screenshot url")?;
    let screenshot_response = client
        .get(format!("{base_url}{screenshot_url}"))
        .send()
        .await?;
    assert_eq!(screenshot_response.status(), 200);
    assert_eq!(
        screenshot_response
            .headers()
            .get(CONTENT_TYPE)
            .context("screenshot response should include content type")?,
        "image/jpeg"
    );
    let screenshot_bytes = screenshot_response.bytes().await?;
    assert!(screenshot_bytes.starts_with(&[0xFF, 0xD8]));

    drop(client);
    shutdown_tx
        .send(true)
        .expect("server shutdown channel should accept signal");
    server.await??;

    Ok(())
}
