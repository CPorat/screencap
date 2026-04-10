//! Daily summary to markdown export

use chrono::NaiveDate;

/// Export a daily summary to markdown
pub fn export_daily(_date: NaiveDate, _output_path: &std::path::Path) -> anyhow::Result<()> {
    // TODO: Implement markdown export
    anyhow::bail!("Markdown export not yet implemented")
}
