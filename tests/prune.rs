mod support;

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use rusqlite::params;
use screencap::storage::{
    db::StorageDb,
    models::{
        ActivityType, ExtractionStatus, InsightData, InsightType, NewCapture, NewExtraction,
        NewExtractionBatch, NewInsight, Sentiment,
    },
};
use uuid::Uuid;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_screencap")
}

struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(name: &str, max_age_days: u32) -> Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("screencap-prune-tests-{name}-{unique}"));
        let app_root = path.join(".screencap");
        fs::create_dir_all(&app_root)
            .with_context(|| format!("failed to create test app root at {}", app_root.display()))?;

        let port = reserve_port()?;
        fs::write(
            app_root.join("config.toml"),
            format!(
                "[server]\nport = {port}\n\n[storage]\nmax_age_days = {max_age_days}\n"
            ),
        )
        .with_context(|| format!("failed to write test config at {}", app_root.display()))?;

        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn db_path(&self) -> PathBuf {
        self.path.join(".screencap").join("screencap.db")
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
    let listener = TcpListener::bind("127.0.0.1:0").context("failed to reserve local tcp port")?;
    listener
        .local_addr()
        .map(|address| address.port())
        .context("failed to read reserved local tcp port")
}

fn run_cli(home: &Path, args: &[&str]) -> Result<Output> {
    Command::new(binary_path())
        .args(args)
        .env("HOME", home)
        .output()
        .with_context(|| format!("failed to run screencap {:?}", args))
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

fn seed_prune_fixture(home: &TestHome) -> Result<Vec<PathBuf>> {
    let mut db = StorageDb::open_at_path(home.db_path())?;
    let now = Utc::now();
    let old_time = now - ChronoDuration::days(120);
    let new_time = now - ChronoDuration::hours(12);

    let old_batch = db.insert_extraction_batch(&NewExtractionBatch {
        id: Uuid::new_v4(),
        batch_start: old_time - ChronoDuration::minutes(10),
        batch_end: old_time,
        capture_count: 5,
        primary_activity: Some("coding".into()),
        project_context: Some("screencap".into()),
        narrative: Some("old batch".into()),
        raw_response: None,
        model_used: None,
        tokens_used: None,
        cost_cents: None,
    })?;

    let mut old_files = Vec::with_capacity(5);
    for index in 0..5 {
        let relative = PathBuf::from(format!("screenshots/2025/01/01/old-{index}.jpg"));
        let absolute = home.path().join(".screencap").join(&relative);
        fs::create_dir_all(
            absolute
                .parent()
                .expect("old screenshot parent should exist"),
        )?;
        fs::write(&absolute, vec![index as u8; 256])?;
        old_files.push(absolute);

        let capture = db.insert_capture(&NewCapture {
            timestamp: old_time + ChronoDuration::minutes(index as i64),
            app_name: Some("Code".into()),
            window_title: Some(format!("old-{index}")),
            bundle_id: Some("com.microsoft.VSCode".into()),
            display_id: Some(1),
            screenshot_path: relative.to_string_lossy().into_owned(),
        })?;

        let extraction = db.insert_extraction(&NewExtraction {
            capture_id: capture.id,
            batch_id: old_batch.id,
            activity_type: Some(ActivityType::Coding),
            description: Some(format!("old extraction {index}")),
            app_context: Some("old context".into()),
            project: Some("screencap".into()),
            topics: vec!["prune".into()],
            people: vec![],
            key_content: Some("old key content".into()),
            sentiment: Some(Sentiment::Focused),
        })?;

        db.update_capture_status(capture.id, ExtractionStatus::Processed, Some(extraction.id))?;
    }

    let new_relative = PathBuf::from("screenshots/2026/01/01/new.jpg");
    let new_absolute = home.path().join(".screencap").join(&new_relative);
    fs::create_dir_all(new_absolute.parent().expect("new screenshot parent should exist"))?;
    fs::write(new_absolute, vec![1_u8; 128])?;
    db.insert_capture(&NewCapture {
        timestamp: new_time,
        app_name: Some("Code".into()),
        window_title: Some("new-capture".into()),
        bundle_id: Some("com.microsoft.VSCode".into()),
        display_id: Some(1),
        screenshot_path: new_relative.to_string_lossy().into_owned(),
    })?;

    let old_daily_start = old_time.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let old_daily_end = old_time
        .date_naive()
        .and_hms_opt(23, 59, 59)
        .unwrap()
        .and_utc();
    db.insert_insight(&NewInsight {
        insight_type: InsightType::Daily,
        window_start: old_daily_start,
        window_end: old_daily_end,
        data: InsightData::Daily {
            date: old_time.date_naive(),
            total_active_hours: 1.0,
            projects: vec![],
            time_allocation: std::collections::BTreeMap::new(),
            focus_blocks: vec![],
            open_threads: vec![],
            narrative: "old daily insight".into(),
        },
        model_used: None,
        tokens_used: None,
        cost_cents: None,
    })?;

    db.insert_insight(&NewInsight {
        insight_type: InsightType::Hourly,
        window_start: old_time,
        window_end: old_time + ChronoDuration::hours(1),
        data: InsightData::Hourly {
            hour_start: old_time,
            hour_end: old_time + ChronoDuration::hours(1),
            dominant_activity: "coding".into(),
            projects: vec![],
            topics: vec![],
            people_interacted: vec![],
            key_moments: vec![],
            focus_score: 0.4,
            narrative: "old hourly insight".into(),
        },
        model_used: None,
        tokens_used: None,
        cost_cents: None,
    })?;

    Ok(old_files)
}

#[test]
fn prune_command_deletes_old_rows_and_files() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("cli", 90)?;
    let old_files = seed_prune_fixture(&home)?;

    let output = run_cli(home.path(), &["prune", "--older-than", "90d"])?;
    assert_success(&output, "prune");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pruned"), "unexpected prune output: {stdout}");

    for old_file in &old_files {
        assert!(
            !old_file.exists(),
            "old screenshot should be deleted at {}",
            old_file.display()
        );
    }

    let db = StorageDb::open_at_path(home.db_path())?;
    let cutoff = (Utc::now() - ChronoDuration::days(90)).to_rfc3339();

    let old_capture_count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM captures WHERE timestamp < ?1",
        params![&cutoff],
        |row| row.get(0),
    )?;
    assert_eq!(old_capture_count, 0);

    let new_capture_count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM captures WHERE window_title = 'new-capture'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(new_capture_count, 1);

    let extraction_count: i64 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM extractions", [], |row| row.get(0))?;
    assert_eq!(extraction_count, 0);

    let batch_count: i64 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM extraction_batches", [], |row| row.get(0))?;
    assert_eq!(batch_count, 0);

    let old_non_daily_count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM insights WHERE type != 'daily' AND window_end < ?1",
        params![&cutoff],
        |row| row.get(0),
    )?;
    assert_eq!(old_non_daily_count, 0);

    let old_daily_count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM insights WHERE type = 'daily' AND window_end < ?1",
        params![&cutoff],
        |row| row.get(0),
    )?;
    assert_eq!(old_daily_count, 1);

    Ok(())
}


#[test]
fn daemon_startup_auto_prunes_when_max_age_configured() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("startup", 30)?;
    let old_files = seed_prune_fixture(&home)?;

    let start = run_cli(home.path(), &["start"])?;
    assert_success(&start, "start");

    let stop = run_cli(home.path(), &["stop"])?;
    assert_success(&stop, "stop");

    for old_file in &old_files {
        assert!(
            !old_file.exists(),
            "old screenshot should be deleted at {}",
            old_file.display()
        );
    }

    let db = StorageDb::open_at_path(home.db_path())?;
    let cutoff = (Utc::now() - ChronoDuration::days(30)).to_rfc3339();

    let old_capture_count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM captures WHERE timestamp < ?1",
        params![&cutoff],
        |row| row.get(0),
    )?;
    assert_eq!(old_capture_count, 0);

    let new_capture_count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM captures WHERE window_title = 'new-capture'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(new_capture_count, 1);

    let old_daily_count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM insights WHERE type = 'daily' AND window_end < ?1",
        params![&cutoff],
        |row| row.get(0),
    )?;
    assert_eq!(old_daily_count, 1);

    Ok(())
}