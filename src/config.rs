use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_DAILY_EXPORT_PATH: &str = "~/.screencap/daily/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub capture: CaptureConfig,
    pub extraction: ExtractionConfig,
    pub synthesis: SynthesisConfig,
    pub storage: StorageConfig,
    pub server: ServerConfig,
    pub export: ExportConfig,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let home = home_dir()?;
        let root = default_app_root(&home);
        Self::load_from_root_and_home(&root, &home)
    }

    pub fn default_config_path() -> Result<PathBuf> {
        let home = home_dir()?;
        Ok(default_app_root(&home).join("config.toml"))
    }

    pub fn pid_file_path(home: &Path) -> PathBuf {
        default_app_root(home).join("screencap.pid")
    }

    pub fn load_from_root_and_home(root: impl AsRef<Path>, home: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let home = home.as_ref();

        fs::create_dir_all(root)
            .with_context(|| format!("failed to create application root at {}", root.display()))?;

        let config_path = root.join("config.toml");
        let config = if config_path.exists() {
            let raw = fs::read_to_string(&config_path).with_context(|| {
                format!("failed to read config file at {}", config_path.display())
            })?;

            toml::from_str::<Self>(&raw).with_context(|| {
                format!("failed to parse TOML config at {}", config_path.display())
            })?
        } else {
            Self::default()
        };

        config.ensure_runtime_dirs(home)?;
        Ok(config)
    }

    pub fn storage_root(&self, home: &Path) -> PathBuf {
        expand_home_path(&self.storage.path, home)
    }

    pub fn screenshots_root(&self, home: &Path) -> PathBuf {
        self.storage_root(home).join("screenshots")
    }

    pub fn daily_export_root(&self, home: &Path) -> PathBuf {
        expand_home_path(&self.synthesis.daily_export_path, home)
    }

    pub fn obsidian_vault_root(&self, home: &Path) -> Option<PathBuf> {
        let raw = self.export.obsidian_vault.trim();
        if raw.is_empty() {
            None
        } else {
            Some(expand_home_path(raw, home))
        }
    }

    pub fn ensure_daily_export_root(&self, home: &Path) -> Result<PathBuf> {
        let root = self.daily_export_root(home);
        fs::create_dir_all(&root)
            .with_context(|| format!("failed to create runtime directory at {}", root.display()))?;
        Ok(root)
    }

    pub fn has_custom_daily_export_path(&self) -> bool {
        normalize_path_for_compare(&self.synthesis.daily_export_path)
            != normalize_path_for_compare(DEFAULT_DAILY_EXPORT_PATH)
    }

    fn ensure_runtime_dirs(&self, home: &Path) -> Result<()> {
        let paths = [self.storage_root(home), self.screenshots_root(home)];

        for path in paths {
            fs::create_dir_all(&path).with_context(|| {
                format!("failed to create runtime directory at {}", path.display())
            })?;
        }

        self.ensure_daily_export_root(home)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    #[default]
    Openrouter,
    Openai,
    Anthropic,
    Google,
    Lmstudio,
    Ollama,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    pub idle_interval_secs: u64,
    pub event_settle_ms: u64,
    pub jpeg_quality: u8,
    pub excluded_apps: Vec<String>,
    pub excluded_window_titles: Vec<String>,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            idle_interval_secs: 300,
            event_settle_ms: 500,
            jpeg_quality: 75,
            excluded_apps: vec!["1Password".into(), "Keychain Access".into()],
            excluded_window_titles: vec!["Private Browsing".into(), "Incognito".into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtractionConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub provider: AiProvider,
    pub model: String,
    pub api_key_env: String,
    pub base_url: String,
    pub max_images_per_batch: u32,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 600,
            provider: AiProvider::Openrouter,
            model: "google/gemini-2.0-flash".into(),
            api_key_env: "OPENROUTER_API_KEY".into(),
            base_url: String::new(),
            max_images_per_batch: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthesisConfig {
    pub enabled: bool,
    pub provider: AiProvider,
    pub model: String,
    pub api_key_env: String,
    pub base_url: String,
    pub rolling_interval_secs: u64,
    pub hourly_enabled: bool,
    pub daily_summary_time: String,
    pub daily_export_markdown: bool,
    pub daily_export_path: String,
}

impl Default for SynthesisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: AiProvider::Openrouter,
            model: "anthropic/claude-sonnet-4-20250514".into(),
            api_key_env: "OPENROUTER_API_KEY".into(),
            base_url: String::new(),
            rolling_interval_secs: 1800,
            hourly_enabled: true,
            daily_summary_time: "23:50".into(),
            daily_export_markdown: true,
            daily_export_path: "~/.screencap/daily/".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub path: String,
    pub max_age_days: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: "~/.screencap".into(),
            max_age_days: 90,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { port: 7878 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportConfig {
    pub obsidian_vault: String,
    pub markdown_template: String,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            obsidian_vault: String::new(),
            markdown_template: "default".into(),
        }
    }
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable is not set"))
}

fn default_app_root(home: &Path) -> PathBuf {
    home.join(".screencap")
}

fn expand_home_path(raw: &str, home: &Path) -> PathBuf {
    if raw == "~" {
        home.to_path_buf()
    } else if let Some(stripped) = raw.strip_prefix("~/") {
        home.join(stripped)
    } else {
        PathBuf::from(raw)
    }
}

fn normalize_path_for_compare(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return "~".to_string();
    }

    trimmed.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn temp_home_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        env::temp_dir().join(format!("screencap-config-tests-{name}-{unique}"))
    }

    #[test]
    fn load_defaults_when_config_is_missing() {
        let home = temp_home_root("missing");
        let app_root = home.join(".screencap");

        let config = AppConfig::load_from_root_and_home(&app_root, &home).expect("load defaults");

        assert_eq!(config, AppConfig::default());
        assert!(app_root.exists());
        assert!(config.screenshots_root(&home).exists());
        assert!(config.daily_export_root(&home).exists());
        assert!(!config.has_custom_daily_export_path());
        assert!(config.obsidian_vault_root(&home).is_none());

        fs::remove_dir_all(&home).expect("cleanup temp home");
    }

    #[test]
    fn parse_valid_config_toml() {
        let home = temp_home_root("valid");
        let app_root = home.join(".screencap");
        fs::create_dir_all(&app_root).expect("create app root");

        let config_toml = r#"
[capture]
idle_interval_secs = 120
jpeg_quality = 65
excluded_apps = ["Secrets"]
excluded_window_titles = ["Hidden"]

[extraction]
enabled = true
interval_secs = 1200
provider = "lmstudio"
model = "llava-v1"
api_key_env = "LM_STUDIO_KEY"
base_url = "http://localhost:1234/v1"
max_images_per_batch = 4

[synthesis]
enabled = false
provider = "openai"
model = "gpt-4o"
api_key_env = "OPENAI_API_KEY"
base_url = "https://api.openai.com/v1"
rolling_interval_secs = 900
hourly_enabled = false
daily_summary_time = "20:30"
daily_export_markdown = false
daily_export_path = "~/Exports/Screencap"

[storage]
path = "~/Library/Application Support/ScreencapTest"
max_age_days = 30

[server]
port = 9000

[export]
obsidian_vault = "~/Notes"
markdown_template = "compact"
"#;
        fs::write(app_root.join("config.toml"), config_toml).expect("write config");

        let config =
            AppConfig::load_from_root_and_home(&app_root, &home).expect("load configured app");

        assert_eq!(config.capture.idle_interval_secs, 120);
        assert_eq!(config.capture.event_settle_ms, 500);
        assert_eq!(config.capture.excluded_apps, vec!["Secrets"]);
        assert_eq!(config.extraction.provider, AiProvider::Lmstudio);
        assert_eq!(config.extraction.max_images_per_batch, 4);
        assert_eq!(config.synthesis.provider, AiProvider::Openai);
        assert_eq!(config.synthesis.daily_summary_time, "20:30");
        assert_eq!(config.storage.max_age_days, 30);
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.export.markdown_template, "compact");
        assert_eq!(
            config.storage_root(&home),
            home.join("Library/Application Support/ScreencapTest")
        );
        assert!(config.screenshots_root(&home).exists());
        assert!(config.daily_export_root(&home).exists());
        assert!(config.has_custom_daily_export_path());
        assert_eq!(config.obsidian_vault_root(&home), Some(home.join("Notes")));

        fs::remove_dir_all(&home).expect("cleanup temp home");
    }

    #[test]
    fn invalid_toml_returns_error() {
        let home = temp_home_root("invalid");
        let app_root = home.join(".screencap");
        fs::create_dir_all(&app_root).expect("create app root");
        fs::write(
            app_root.join("config.toml"),
            "[capture\nidle_interval_secs = 300",
        )
        .expect("write invalid config");

        let error =
            AppConfig::load_from_root_and_home(&app_root, &home).expect_err("expected parse error");

        assert!(error.to_string().contains("failed to parse TOML config"));

        fs::remove_dir_all(&home).expect("cleanup temp home");
    }
}
