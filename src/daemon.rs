use anyhow::Result;
use tracing::info;

use crate::config::AppConfig;

pub async fn run_foreground(config: &AppConfig) -> Result<()> {
    info!(
        idle_interval_secs = config.capture.idle_interval_secs,
        extraction_interval_secs = config.extraction.interval_secs,
        "daemon skeleton initialized"
    );

    Ok(())
}
