use std::{
    collections::BTreeMap,
    env,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{Days, Duration as ChronoDuration, NaiveDate, Utc};
use tokio::fs as tokio_fs;

use crate::{
    config::AppConfig,
    storage::{
        db::StorageDb,
        models::{Capture, CaptureQuery, DailyProjectSummary, Insight, InsightData},
    },
};

const CAPTURE_QUERY_PAGE_SIZE: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportResult {
    pub markdown_path: PathBuf,
    pub screenshots_dir: Option<PathBuf>,
    pub screenshots_copied: usize,
}

pub fn parse_export_date(raw: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .with_context(|| format!("invalid date `{raw}`; expected YYYY-MM-DD"))
}

pub async fn export_daily(
    insight: &Insight,
    output: Option<&Path>,
    config: &AppConfig,
    home: &Path,
) -> Result<Vec<PathBuf>> {
    let date = insight_date(insight)?;
    let markdown = generate_markdown(insight)?;

    let destination = match output {
        Some(path) => path.to_path_buf(),
        None => {
            let root = config.ensure_daily_export_root(home)?;
            root.join(format!("{date}.md"))
        }
    };

    if let Some(parent) = destination.parent() {
        tokio_fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create export directory {}", parent.display()))?;
    }

    tokio_fs::write(&destination, markdown)
        .await
        .with_context(|| {
            format!(
                "failed to write markdown export to {}",
                destination.display()
            )
        })?;

    let mut written_paths = vec![destination.clone()];

    if let Some(vault_root) = config.obsidian_vault_root(home) {
        tokio_fs::create_dir_all(&vault_root)
            .await
            .with_context(|| {
                format!(
                    "failed to create obsidian vault at {}",
                    vault_root.display()
                )
            })?;

        let file_name = destination.file_name().ok_or_else(|| {
            anyhow!(
                "failed to derive export filename from {}",
                destination.display()
            )
        })?;
        let vault_destination = vault_root.join(file_name);

        if vault_destination != destination {
            tokio_fs::copy(&destination, &vault_destination)
                .await
                .with_context(|| {
                    format!(
                        "failed to copy markdown export to obsidian vault {}",
                        vault_destination.display()
                    )
                })?;
            written_paths.push(vault_destination);
        }
    }

    Ok(written_paths)
}

pub async fn run_export(
    date: Option<String>,
    last: Option<String>,
    output: Option<String>,
) -> Result<()> {
    let config = AppConfig::load()?;
    let home = runtime_home_dir()?;
    let db_path = config.storage_root(&home).join("screencap.db");
    let Some(db) = StorageDb::open_existing_at_path(&db_path).with_context(|| {
        format!(
            "failed to open read-only sqlite database at {}",
            db_path.display()
        )
    })?
    else {
        println!("no daily summaries available yet");
        return Ok(());
    };

    let dates = resolve_export_dates(date.as_deref(), last.as_deref(), Utc::now().date_naive())?;
    let output = output.map(PathBuf::from);
    let print_to_stdout = output.is_none() && !config.has_custom_daily_export_path();
    let multiple_dates = dates.len() > 1;

    let mut printed_any = false;
    for export_date in dates {
        let Some(insight) = db.get_latest_daily_insight_for_date(export_date)? else {
            eprintln!("warning: no daily summary available for {export_date}");
            continue;
        };

        if print_to_stdout {
            let markdown = generate_markdown(&insight)?;
            if printed_any {
                println!("\n---\n");
            }
            print!("{markdown}");
            if !markdown.ends_with('\n') {
                println!();
            }
            printed_any = true;
            continue;
        }

        let explicit_output = output_path_for_date(output.as_deref(), export_date, multiple_dates)?;
        let paths = export_daily(&insight, explicit_output.as_deref(), &config, &home).await?;
        for path in paths {
            println!("exported {}", path.display());
        }
    }

    Ok(())
}

fn output_path_for_date(
    base: Option<&Path>,
    date: NaiveDate,
    multiple: bool,
) -> Result<Option<PathBuf>> {
    let Some(base) = base else {
        return Ok(None);
    };

    if !multiple {
        return Ok(Some(base.to_path_buf()));
    }

    if base
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        bail!("--output must be a directory when exporting multiple days");
    }

    Ok(Some(base.join(format!("{date}.md"))))
}

fn resolve_export_dates(
    date: Option<&str>,
    last: Option<&str>,
    today: NaiveDate,
) -> Result<Vec<NaiveDate>> {
    match (date, last) {
        (Some(_), Some(_)) => bail!("--date and --last cannot be used together"),
        (Some(raw_date), None) => Ok(vec![parse_export_date(raw_date)?]),
        (None, Some(raw_last)) => {
            let days = parse_last_days(raw_last)?;
            let mut dates = Vec::with_capacity(days as usize);
            for offset in (0..days).rev() {
                let date = today.checked_sub_days(Days::new(offset)).ok_or_else(|| {
                    anyhow!("failed to compute export date for --last {raw_last}")
                })?;
                dates.push(date);
            }
            Ok(dates)
        }
        (None, None) => Ok(vec![today]),
    }
}

fn parse_last_days(raw: &str) -> Result<u64> {
    let trimmed = raw.trim();
    if trimmed.len() < 2 {
        bail!("--last must be a day window like `7d`");
    }

    let (amount, unit) = trimmed.split_at(trimmed.len() - 1);
    if unit.to_ascii_lowercase() != "d" {
        bail!("--last only supports day windows like `7d`");
    }

    let days: u64 = amount
        .parse()
        .with_context(|| format!("invalid day count in --last `{raw}`"))?;
    if days == 0 {
        bail!("--last must be greater than 0");
    }

    Ok(days)
}

fn runtime_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable is not set"))
}

pub fn export_daily_markdown(
    db: &StorageDb,
    date: NaiveDate,
    out_dir: &Path,
    include_screenshots: bool,
) -> Result<ExportResult> {
    let insight = db
        .get_latest_daily_insight_for_date(date)?
        .ok_or_else(|| anyhow!("no daily summary available for {date}"))?;

    let captures = list_captures_for_day(db, date)?;

    fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create export directory {}", out_dir.display()))?;

    let (screenshot_links, screenshots_dir, screenshots_copied) = if include_screenshots {
        let (links, dir) = copy_screenshots(&captures, out_dir)?;
        let copied = links.len();
        (links, Some(dir), copied)
    } else {
        (BTreeMap::new(), None, 0)
    };

    let markdown = generate_markdown_with_captures(&insight, &captures, &screenshot_links)?;
    let markdown_path = out_dir.join(format!("{date}.md"));
    fs::write(&markdown_path, markdown).with_context(|| {
        format!(
            "failed to write markdown export to {}",
            markdown_path.display()
        )
    })?;

    Ok(ExportResult {
        markdown_path,
        screenshots_dir,
        screenshots_copied,
    })
}

pub fn generate_markdown(insight: &Insight) -> Result<String> {
    generate_markdown_with_captures(insight, &[], &BTreeMap::new())
}

pub fn generate_markdown_with_captures(
    insight: &Insight,
    captures: &[Capture],
    screenshot_links: &BTreeMap<i64, String>,
) -> Result<String> {
    let InsightData::Daily {
        date,
        total_active_hours,
        projects,
        time_allocation,
        focus_blocks,
        open_threads,
        narrative,
    } = &insight.data
    else {
        bail!("markdown export only supports daily insights");
    };

    let mut markdown = String::with_capacity(4096);

    writeln!(&mut markdown, "# Screencap: {}", date)?;
    writeln!(&mut markdown)?;

    writeln!(&mut markdown, "## Summary")?;
    writeln!(&mut markdown, "{narrative}")?;
    writeln!(&mut markdown)?;

    let total_minutes = hours_to_minutes(*total_active_hours)?;
    writeln!(
        &mut markdown,
        "## Time: {} active",
        format_duration_minutes(total_minutes)
    )?;
    writeln!(&mut markdown)?;

    writeln!(&mut markdown, "### By Project")?;
    if projects.is_empty() {
        writeln!(&mut markdown, "- None")?;
    } else {
        for project in projects {
            write!(
                &mut markdown,
                "- **{}** — {}",
                project.name,
                format_duration_minutes(u64::from(project.total_minutes))
            )?;
            if !project.activities.is_empty() {
                write!(&mut markdown, " ({})", project.activities.join(", "))?;
            }
            writeln!(&mut markdown)?;
        }
    }
    writeln!(&mut markdown)?;

    writeln!(&mut markdown, "### By Activity")?;
    let ordered_activity_rows = ordered_activity_rows(time_allocation);
    if ordered_activity_rows.is_empty() {
        writeln!(&mut markdown, "- None")?;
    } else {
        for (activity, duration) in ordered_activity_rows {
            writeln!(
                &mut markdown,
                "- {}: {}",
                format_activity_label(activity),
                duration
            )?;
        }
    }
    writeln!(&mut markdown)?;

    writeln!(&mut markdown, "## Focus Blocks")?;
    if focus_blocks.is_empty() {
        writeln!(&mut markdown, "- None")?;
    } else {
        for block in focus_blocks {
            writeln!(
                &mut markdown,
                "- {}-{} ({}) — {}, {}",
                block.start,
                block.end,
                format_duration_minutes(u64::from(block.duration_min)),
                block.project,
                block.quality.replace('-', " "),
            )?;
        }
    }
    writeln!(&mut markdown)?;

    writeln!(&mut markdown, "## Key Moments")?;
    let key_moments = collect_key_moments(projects);
    if key_moments.is_empty() {
        writeln!(&mut markdown, "- None")?;
    } else {
        for moment in key_moments {
            writeln!(&mut markdown, "- {moment}")?;
        }
    }
    writeln!(&mut markdown)?;

    writeln!(&mut markdown, "## Open Threads")?;
    if open_threads.is_empty() {
        writeln!(&mut markdown, "- None")?;
    } else {
        for thread in open_threads {
            writeln!(&mut markdown, "- {thread}")?;
        }
    }
    writeln!(&mut markdown)?;

    if !captures.is_empty() {
        writeln!(&mut markdown, "## Raw Timeline")?;
        for capture in captures {
            write!(&mut markdown, "- {} UTC", capture.timestamp.format("%H:%M"))?;

            if let Some(app_name) = capture
                .app_name
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                write!(&mut markdown, " — {app_name}")?;
            }

            if let Some(window_title) = capture
                .window_title
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                write!(&mut markdown, " — {window_title}")?;
            }

            if let Some(relative_path) = screenshot_links.get(&capture.id) {
                write!(&mut markdown, " ([screenshot]({relative_path}))")?;
            }

            writeln!(&mut markdown)?;
        }
    }

    Ok(markdown)
}

fn insight_date(insight: &Insight) -> Result<NaiveDate> {
    match &insight.data {
        InsightData::Daily { date, .. } => Ok(*date),
        _ => bail!("markdown export only supports daily insights"),
    }
}

fn list_captures_for_day(db: &StorageDb, date: NaiveDate) -> Result<Vec<Capture>> {
    let start = date
        .and_hms_opt(0, 0, 0)
        .expect("midnight should be representable")
        .and_utc();
    let end = date
        .succ_opt()
        .expect("successor date should be representable")
        .and_hms_opt(0, 0, 0)
        .expect("midnight should be representable")
        .and_utc()
        - ChronoDuration::nanoseconds(1);

    let mut captures = Vec::new();
    let mut offset = 0;

    loop {
        let page = db.list_captures(&CaptureQuery {
            from: Some(start),
            to: Some(end),
            app_name: None,
            limit: CAPTURE_QUERY_PAGE_SIZE,
            offset,
        })?;

        if page.is_empty() {
            break;
        }

        offset += page.len();
        let page_len = page.len();
        captures.extend(page);

        if page_len < CAPTURE_QUERY_PAGE_SIZE {
            break;
        }
    }

    Ok(captures)
}

fn copy_screenshots(
    captures: &[Capture],
    out_dir: &Path,
) -> Result<(BTreeMap<i64, String>, PathBuf)> {
    let screenshots_dir = out_dir.join("screenshots");
    fs::create_dir_all(&screenshots_dir).with_context(|| {
        format!(
            "failed to create screenshots export directory {}",
            screenshots_dir.display()
        )
    })?;

    let mut links = BTreeMap::new();
    for capture in captures {
        let source = Path::new(&capture.screenshot_path);
        if !source.exists() {
            bail!(
                "screenshot for capture {} does not exist at {}",
                capture.id,
                source.display()
            );
        }

        let file_name = format!("capture-{}.jpg", capture.id);
        let destination = screenshots_dir.join(&file_name);
        fs::copy(source, &destination).with_context(|| {
            format!(
                "failed to copy screenshot for capture {} from {} to {}",
                capture.id,
                source.display(),
                destination.display()
            )
        })?;

        let relative = Path::new("screenshots")
            .join(file_name)
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        links.insert(capture.id, relative);
    }

    Ok((links, screenshots_dir))
}

fn ordered_activity_rows<'a>(
    allocation: &'a std::collections::BTreeMap<String, String>,
) -> Vec<(&'a str, &'a str)> {
    const CANONICAL_ORDER: [&str; 6] = [
        "coding",
        "communication",
        "browsing_research",
        "design",
        "meetings",
        "other",
    ];

    let mut rows = Vec::with_capacity(allocation.len());
    for key in CANONICAL_ORDER {
        if let Some(duration) = allocation.get(key) {
            rows.push((key, duration.as_str()));
        }
    }

    for (key, duration) in allocation {
        if CANONICAL_ORDER.contains(&key.as_str()) {
            continue;
        }
        rows.push((key.as_str(), duration.as_str()));
    }

    rows
}

fn collect_key_moments(projects: &[DailyProjectSummary]) -> Vec<String> {
    let mut moments = Vec::new();
    for project in projects {
        for accomplishment in &project.key_accomplishments {
            if accomplishment.trim().is_empty() || moments.contains(accomplishment) {
                continue;
            }
            moments.push(accomplishment.clone());
        }
    }

    moments
}

fn format_activity_label(raw: &str) -> String {
    match raw {
        "coding" => "Coding".to_string(),
        "communication" => "Communication".to_string(),
        "browsing_research" => "Research/Browsing".to_string(),
        "design" => "Design".to_string(),
        "meetings" => "Meetings".to_string(),
        "other" => "Other".to_string(),
        other => title_case(other),
    }
}

fn title_case(raw: &str) -> String {
    raw.split(['_', '-', ' '])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut title = first.to_uppercase().to_string();
                    title.push_str(chars.as_str());
                    title
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn hours_to_minutes(hours: f64) -> Result<u64> {
    if !hours.is_finite() || hours < 0.0 {
        bail!("daily summary total_active_hours must be a non-negative finite value");
    }

    Ok((hours * 60.0).round() as u64)
}

fn format_duration_minutes(total_minutes: u64) -> String {
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    match (hours, minutes) {
        (0, minutes) => format!("{minutes}m"),
        (hours, 0) => format!("{hours}h"),
        (hours, minutes) => format!("{hours}h {minutes}m"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{TimeZone, Utc};

    use crate::storage::models::{DailyProjectSummary, FocusBlock, InsightType};

    use super::*;

    #[test]
    fn generate_markdown_matches_spec_sections() {
        let insight = sample_daily_insight();
        let markdown = generate_markdown(&insight).expect("generate markdown");

        assert!(markdown.contains("# Screencap: 2026-04-10"));
        assert!(markdown.contains("## Summary"));
        assert!(markdown.contains("## Time: 7h 30m active"));
        assert!(markdown.contains("### By Project"));
        assert!(markdown.contains("### By Activity"));
        assert!(markdown.contains("## Focus Blocks"));
        assert!(markdown.contains("## Key Moments"));
        assert!(markdown.contains("## Open Threads"));
        assert!(!markdown.contains("## Raw Timeline"));
    }

    fn sample_daily_insight() -> Insight {
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        Insight {
            id: 42,
            insight_type: InsightType::Daily,
            window_start: Utc.with_ymd_and_hms(2026, 4, 10, 0, 0, 0).unwrap(),
            window_end: Utc.with_ymd_and_hms(2026, 4, 10, 18, 0, 0).unwrap(),
            data: InsightData::Daily {
                date,
                total_active_hours: 7.5,
                projects: vec![DailyProjectSummary {
                    name: "screencap".into(),
                    total_minutes: 195,
                    activities: vec!["capture pipeline".into(), "auth fixes".into()],
                    key_accomplishments: vec!["Fixed JWT refresh bug".into()],
                }],
                time_allocation: BTreeMap::from([
                    ("coding".into(), "3h 15m".into()),
                    ("communication".into(), "1h 25m".into()),
                ]),
                focus_blocks: vec![FocusBlock {
                    start: "09:15".into(),
                    end: "11:45".into(),
                    duration_min: 150,
                    project: "screencap".into(),
                    quality: "deep-focus".into(),
                }],
                open_threads: vec!["Respond to API auth docs request".into()],
                narrative: "Productive day focused on screencap.".into(),
            },
            narrative: "Productive day focused on screencap.".into(),
            model_used: Some("mock-model".into()),
            tokens_used: Some(440),
            cost_cents: Some(0.61),
            created_at: Utc::now(),
        }
    }
}
