use std::{fs, io::ErrorKind, path::Path};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};

use super::db::StorageDb;

pub fn count_captures_today(db: &StorageDb, now: DateTime<Utc>) -> Result<u64> {
    let day_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight should be representable")
        .and_utc();
    let day_end = day_start + Duration::days(1);

    db.count_captures_in_window(day_start, day_end)
}

pub fn directory_size(path: &Path) -> Result<u64> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(0),
        Err(err) => return Err(err).with_context(|| format!("failed to stat {}", path.display())),
    };

    if metadata.file_type().is_symlink() {
        return Ok(0);
    }

    if metadata.is_file() {
        return Ok(metadata.len());
    }

    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut total = 0_u64;
    for entry in fs::read_dir(path)
        .with_context(|| format!("failed to read directory {}", path.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to enumerate directory {}", path.display()))?;
        total = total
            .checked_add(directory_size(&entry.path())?)
            .ok_or_else(|| anyhow!("storage usage overflowed u64"))?;
    }

    Ok(total)
}
