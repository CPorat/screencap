use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use tracing::info;

use crate::config::AppConfig;

use super::db::StorageDb;

pub fn run_prune(older_than: String) -> Result<()> {
    let config = AppConfig::load()?;
    let home = runtime_home_dir()?;
    let cutoff = parse_older_than_cutoff(&older_than, Utc::now())?;
    let cutoff_timestamp = format_db_timestamp(cutoff);
    let db_path = config.storage_root(&home).join("screencap.db");

    if !db_path.exists() {
        println!(
            "pruned 0 rows older than {older_than}; reclaimed 0 bytes (database not found at {})",
            db_path.display()
        );
        return Ok(());
    }

    let db = StorageDb::open_at_path(&db_path)
        .with_context(|| format!("failed to open capture database at {}", db_path.display()))?;
    let (rows_deleted, bytes_reclaimed) = db.prune_older_than(&cutoff_timestamp)?;

    println!(
        "pruned {rows_deleted} rows older than {older_than}; reclaimed {bytes_reclaimed} bytes"
    );
    Ok(())
}

pub fn run_startup_prune(config: &AppConfig, home: &Path) -> Result<Option<(usize, u64)>> {
    let max_age_days = config.storage.max_age_days;
    if max_age_days == 0 {
        return Ok(None);
    }

    let cutoff = Utc::now() - ChronoDuration::days(i64::from(max_age_days));
    let cutoff_timestamp = format_db_timestamp(cutoff);
    let db_path = config.storage_root(home).join("screencap.db");

    if !db_path.exists() {
        info!(
            max_age_days,
            "startup prune skipped because database does not exist"
        );
        return Ok(Some((0, 0)));
    }

    let db = StorageDb::open_at_path(&db_path)
        .with_context(|| format!("failed to open capture database at {}", db_path.display()))?;
    let result = db.prune_older_than(&cutoff_timestamp)?;
    info!(
        max_age_days,
        cutoff = %cutoff_timestamp,
        rows_deleted = result.0,
        bytes_reclaimed = result.1,
        "startup prune completed"
    );

    Ok(Some(result))
}

fn parse_older_than_cutoff(raw: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    Ok(now - parse_older_than_window(raw)?)
}

fn parse_older_than_window(raw: &str) -> Result<ChronoDuration> {
    let raw = raw.trim();
    if raw.len() < 2 {
        bail!("older-than window must be a positive duration like `90d`");
    }

    let (amount, unit) = raw.split_at(raw.len() - 1);
    let amount: i64 = amount
        .parse()
        .with_context(|| format!("invalid prune amount `{amount}`"))?;
    if amount <= 0 {
        bail!("older-than window must be greater than 0");
    }

    let seconds_per_unit = match unit.to_ascii_lowercase().as_str() {
        "m" => 60_i64,
        "h" => 60 * 60,
        "d" => 60 * 60 * 24,
        "w" => 60 * 60 * 24 * 7,
        _ => bail!("unsupported older-than unit `{unit}`; use m, h, d, or w"),
    };

    let total_seconds = amount
        .checked_mul(seconds_per_unit)
        .ok_or_else(|| anyhow!("older-than window `{raw}` is too large"))?;
    Ok(ChronoDuration::seconds(total_seconds))
}

fn format_db_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn runtime_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable is not set"))
}
