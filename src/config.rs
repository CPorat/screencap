//! Configuration parsing (TOML)

use serde::Deserialize;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub capture: CaptureConfig,

    #[serde(default)]
    pub extraction: ExtractionConfig,

    #[serde(default)]
    pub synthesis: SynthesisConfig,

    #[serde(default)]
    pub storage: StorageConfig,

    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub export: ExportConfig,
}

impl Config {
    /// Load configuration from the default path
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path();

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Get the configuration file path
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .map(|p| p.join("screencap").join("config.toml"))
            .unwrap_or_else(|| PathBuf::from(".screencap/config.toml"))
    }

    /// Get the screencap home directory
    pub fn home_dir() -> PathBuf {
        dirs::home_dir()
            .map(|p| p.join(".screencap"))
            .unwrap_or_else(|| PathBuf::from(".screencap"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            capture: CaptureConfig::default(),
            extraction: ExtractionConfig::default(),
            synthesis: SynthesisConfig::default(),
            storage: StorageConfig::default(),
            server: ServerConfig::default(),
            export: ExportConfig::default(),
        }
    }
}

/// Capture layer configuration
#[derive(Debug, Clone, Deserialize)]
pub struct CaptureConfig {
    /// Idle fallback interval in seconds (default: 300 = 5 min)
    #[serde(default = "default_idle_interval")]
    pub idle_interval_secs: u64,

    /// Event settle delay in milliseconds (default: 500)
    #[serde(default = "default_event_settle_ms")]
    pub event_settle_ms: u64,

    /// JPEG quality (default: 75)
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,

    /// Apps to never capture
    #[serde(default)]
    pub excluded_apps: Vec<String>,

    /// Window title patterns to never capture
    #[serde(default)]
    pub excluded_window_titles: Vec<String>,
}

fn default_idle_interval() -> u64 {
    300
}

fn default_event_settle_ms() -> u64 {
    500
}

fn default_jpeg_quality() -> u8 {
    75
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            idle_interval_secs: default_idle_interval(),
            event_settle_ms: default_event_settle_ms(),
            jpeg_quality: default_jpeg_quality(),
            excluded_apps: vec!["1Password".to_string(), "Keychain Access".to_string()],
            excluded_window_titles: vec!["Private Browsing".to_string(), "Incognito".to_string()],
        }
    }
}

/// Extraction layer configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ExtractionConfig {
    /// Enable extraction
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Interval between extraction runs in seconds
    #[serde(default = "default_extraction_interval")]
    pub interval_secs: u64,

    /// AI provider to use
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Model to use
    #[serde(default = "default_extraction_model")]
    pub model: String,

    /// Environment variable name for API key
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,

    /// Base URL override
    #[serde(default)]
    pub base_url: Option<String>,

    /// Maximum images per batch
    #[serde(default = "default_max_images_per_batch")]
    pub max_images_per_batch: usize,
}

fn default_true() -> bool {
    true
}

fn default_extraction_interval() -> u64 {
    600 // 10 min
}

fn default_provider() -> String {
    "openrouter".to_string()
}

fn default_extraction_model() -> String {
    "google/gemini-2.0-flash".to_string()
}

fn default_api_key_env() -> String {
    "OPENROUTER_API_KEY".to_string()
}

fn default_max_images_per_batch() -> usize {
    8
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            interval_secs: default_extraction_interval(),
            provider: default_provider(),
            model: default_extraction_model(),
            api_key_env: default_api_key_env(),
            base_url: None,
            max_images_per_batch: default_max_images_per_batch(),
        }
    }
}

/// Synthesis layer configuration
#[derive(Debug, Clone, Deserialize)]
pub struct SynthesisConfig {
    /// Enable synthesis
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// AI provider to use
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Model to use
    #[serde(default = "default_synthesis_model")]
    pub model: String,

    /// Environment variable name for API key
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,

    /// Base URL override
    #[serde(default)]
    pub base_url: Option<String>,

    /// Rolling context interval in seconds
    #[serde(default = "default_rolling_interval")]
    pub rolling_interval_secs: u64,

    /// Enable hourly digests
    #[serde(default = "default_true")]
    pub hourly_enabled: bool,

    /// Time to generate daily summary (24h format)
    #[serde(default = "default_daily_summary_time")]
    pub daily_summary_time: String,

    /// Write daily summary to markdown
    #[serde(default = "default_true")]
    pub daily_export_markdown: bool,

    /// Path for daily markdown exports
    #[serde(default = "default_daily_export_path")]
    pub daily_export_path: String,
}

fn default_synthesis_model() -> String {
    "anthropic/claude-sonnet-4-20250514".to_string()
}

fn default_rolling_interval() -> u64 {
    1800 // 30 min
}

fn default_daily_summary_time() -> String {
    "18:00".to_string()
}

fn default_daily_export_path() -> String {
    "~/.screencap/daily/".to_string()
}

impl Default for SynthesisConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            provider: default_provider(),
            model: default_synthesis_model(),
            api_key_env: default_api_key_env(),
            base_url: None,
            rolling_interval_secs: default_rolling_interval(),
            hourly_enabled: default_true(),
            daily_summary_time: default_daily_summary_time(),
            daily_export_markdown: default_true(),
            daily_export_path: default_daily_export_path(),
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    /// Storage path
    #[serde(default = "default_storage_path")]
    pub path: String,

    /// Maximum age of captures in days
    #[serde(default = "default_max_age_days")]
    pub max_age_days: u32,
}

fn default_storage_path() -> String {
    "~/.screencap".to_string()
}

fn default_max_age_days() -> u32 {
    90
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: default_storage_path(),
            max_age_days: default_max_age_days(),
        }
    }
}

/// Server configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// HTTP server port
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    7878
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
        }
    }
}

/// Export configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ExportConfig {
    /// Path to Obsidian vault for auto-sync
    #[serde(default)]
    pub obsidian_vault: Option<String>,

    /// Markdown template name or path
    #[serde(default = "default_markdown_template")]
    pub markdown_template: String,
}

fn default_markdown_template() -> String {
    "default".to_string()
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            obsidian_vault: None,
            markdown_template: default_markdown_template(),
        }
    }
}
