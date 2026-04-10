//! Screencap - Lightweight screen memory for macOS
//!
//! This is a macOS-only screen memory tool that captures screenshots,
//! extracts structured context via vision LLMs, and synthesizes
//! rolling/hourly/daily insights.

pub mod api;
pub mod capture;
pub mod config;
pub mod daemon;
pub mod export;
pub mod mcp;
pub mod pipeline;
pub mod storage;

// Re-export commonly used types
pub use config::Config;

#[cfg(test)]
mod tests;
