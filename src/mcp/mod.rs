use anyhow::Result;

use crate::config::AppConfig;

pub mod server;
pub mod tools;

pub fn run_mcp_server(config: AppConfig) -> Result<()> {
    server::run_stdio_server(config)
}
