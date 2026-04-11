mod support;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use screencap::storage::{
    db::StorageDb,
    models::{DailyProjectSummary, FocusBlock, InsightData, InsightType, NewInsight},
};
use support::IntegrationTestLock;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_screencap")
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

fn temp_home(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let home = std::env::temp_dir().join(format!("screencap-export-cli-tests-{label}-{unique}"));
    fs::create_dir_all(home.join(".screencap")).expect("create test .screencap dir");
    home
}

fn seed_daily_insight(db_path: &Path, date: NaiveDate) -> Result<()> {
    let mut db = StorageDb::open_at_path(db_path)?;
    let day_start = date
        .and_hms_opt(0, 0, 0)
        .expect("midnight should be representable")
        .and_utc();
    let day_end = day_start + ChronoDuration::hours(18);

    db.insert_insight(&NewInsight {
        insight_type: InsightType::Daily,
        window_start: day_start,
        window_end: day_end,
        data: InsightData::Daily {
            date,
            total_active_hours: 7.5,
            projects: vec![DailyProjectSummary {
                name: "screencap".into(),
                total_minutes: 195,
                activities: vec!["auth module debugging".into(), "test writing".into()],
                key_accomplishments: vec!["Fixed JWT refresh bug".into()],
            }],
            time_allocation: BTreeMap::from([("coding".into(), "3h 15m".into())]),
            focus_blocks: vec![FocusBlock {
                start: "09:15".into(),
                end: "11:45".into(),
                duration_min: 150,
                project: "screencap".into(),
                quality: "deep-focus".into(),
            }],
            open_threads: vec!["Need to finish the export path".into()],
            narrative: "Productive day focused on screencap.".into(),
        },
        model_used: Some("mock-synthesis-model".into()),
        tokens_used: Some(440),
        cost_cents: Some(0.61),
    })?;

    Ok(())
}

#[test]
fn export_command_writes_markdown_to_output_file() -> Result<()> {
    let _lock = IntegrationTestLock::acquire()?;
    let home = temp_home("single-date");
    let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
    let db_path = home.join(".screencap/screencap.db");
    seed_daily_insight(&db_path, date)?;

    let output_path = home.join("exports/daily.md");
    let output_path_string = output_path.to_string_lossy().into_owned();
    let cli_output = run_cli(
        &home,
        &[
            "export",
            "--date",
            "2026-04-10",
            "--output",
            &output_path_string,
        ],
    )?;
    assert_success(&cli_output, "screencap export --date");

    let markdown = fs::read_to_string(&output_path)
        .with_context(|| format!("failed to read markdown at {}", output_path.display()))?;
    assert!(markdown.contains("# Screencap: 2026-04-10"));
    assert!(markdown.contains("## Summary"));
    assert!(markdown.contains("### By Project"));
    assert!(markdown.contains("### By Activity"));
    assert!(markdown.contains("## Focus Blocks"));
    assert!(markdown.contains("## Key Moments"));
    assert!(markdown.contains("## Open Threads"));

    fs::remove_dir_all(&home)?;
    Ok(())
}

#[test]
fn export_last_range_writes_one_file_per_day_when_output_is_directory() -> Result<()> {
    let _lock = IntegrationTestLock::acquire()?;
    let home = temp_home("last-range");
    let today = Utc::now().date_naive();
    let yesterday = today.pred_opt().expect("yesterday should exist");
    let db_path = home.join(".screencap/screencap.db");
    seed_daily_insight(&db_path, yesterday)?;
    seed_daily_insight(&db_path, today)?;

    let output_dir = home.join("range-exports");
    let output_dir_string = output_dir.to_string_lossy().into_owned();
    let cli_output = run_cli(
        &home,
        &["export", "--last", "2d", "--output", &output_dir_string],
    )?;
    assert_success(&cli_output, "screencap export --last");

    let yesterday_file = output_dir.join(format!("{yesterday}.md"));
    let today_file = output_dir.join(format!("{today}.md"));
    assert!(yesterday_file.exists());
    assert!(today_file.exists());

    fs::remove_dir_all(&home)?;
    Ok(())
}
