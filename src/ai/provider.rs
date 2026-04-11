use std::env;

use async_trait::async_trait;
use thiserror::Error;

use crate::config::{AiProvider, ExtractionConfig, SynthesisConfig};

use super::{anthropic::AnthropicClient, openai_compat::OpenAiCompatClient};

pub type ProviderResult<T> = std::result::Result<T, ProviderError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInput {
    pub media_type: String,
    pub data_base64: String,
}

impl ImageInput {
    pub fn jpeg(data_base64: impl Into<String>) -> Self {
        Self::new("image/jpeg", data_base64)
    }

    pub fn new(media_type: impl Into<String>, data_base64: impl Into<String>) -> Self {
        Self {
            media_type: media_type.into(),
            data_base64: data_base64.into(),
        }
    }

    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.media_type, self.data_base64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmResponse {
    pub content: String,
    pub usage: Option<TokenUsage>,
    pub cost_cents: Option<f64>,
}

impl LlmResponse {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            usage: None,
            cost_cents: None,
        }
    }

    pub fn with_usage(content: impl Into<String>, usage: TokenUsage) -> Self {
        Self {
            content: content.into(),
            usage: Some(usage),
            cost_cents: None,
        }
    }

    pub fn with_usage_and_cost(
        content: impl Into<String>,
        usage: TokenUsage,
        cost_cents: f64,
    ) -> Self {
        Self {
            content: content.into(),
            usage: Some(usage),
            cost_cents: Some(cost_cents),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderConfig {
    pub provider: AiProvider,
    pub model: String,
    pub api_key_env: String,
    pub base_url: String,
}

impl From<&ExtractionConfig> for LlmProviderConfig {
    fn from(config: &ExtractionConfig) -> Self {
        Self {
            provider: config.provider,
            model: config.model.clone(),
            api_key_env: config.api_key_env.clone(),
            base_url: config.base_url.clone(),
        }
    }
}

impl From<&SynthesisConfig> for LlmProviderConfig {
    fn from(config: &SynthesisConfig) -> Self {
        Self {
            provider: config.provider,
            model: config.model.clone(),
            api_key_env: config.api_key_env.clone(),
            base_url: config.base_url.clone(),
        }
    }
}

impl LlmProviderConfig {
    pub fn new(
        provider: AiProvider,
        model: impl Into<String>,
        api_key_env: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            model: model.into(),
            api_key_env: api_key_env.into(),
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str, images: &[ImageInput]) -> ProviderResult<LlmResponse>;
    async fn complete_text(&self, prompt: &str) -> ProviderResult<LlmResponse>;
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("AI provider `{provider}` is not implemented yet")]
    UnsupportedProvider { provider: &'static str },

    #[error("AI provider API key environment variable `{env_var}` is not set")]
    MissingApiKey { env_var: String },

    #[error("AI provider authentication failed: {message}")]
    Authentication { message: String },

    #[error("AI provider rate limit exceeded: {message}")]
    RateLimited { message: String },

    #[error("AI provider request failed with status {status}: {message}")]
    RequestFailed { status: u16, message: String },

    #[error("AI provider network request failed: {source}")]
    Network {
        #[source]
        source: reqwest::Error,
    },

    #[error("AI provider returned invalid JSON: {message}")]
    InvalidResponse { message: String, body: String },

    #[error("AI provider response did not include any assistant text")]
    MissingContent,

    #[error("mock AI provider has no queued responses left")]
    MockResponseExhausted,
}

pub fn create_provider(config: &LlmProviderConfig) -> ProviderResult<Box<dyn LlmProvider>> {
    match config.provider {
        AiProvider::Openrouter | AiProvider::Openai | AiProvider::Lmstudio => {
            Ok(Box::new(OpenAiCompatClient::new(config.clone())?))
        }
        AiProvider::Anthropic => Ok(Box::new(AnthropicClient::new(config.clone())?)),
        provider => Err(ProviderError::UnsupportedProvider {
            provider: provider_name(provider),
        }),
    }
}

pub fn load_api_key(env_var: &str) -> ProviderResult<String> {
    env::var(env_var)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ProviderError::MissingApiKey {
            env_var: env_var.to_owned(),
        })
}

pub fn provider_name(provider: AiProvider) -> &'static str {
    match provider {
        AiProvider::Openrouter => "openrouter",
        AiProvider::Openai => "openai",
        AiProvider::Anthropic => "anthropic",
        AiProvider::Google => "google",
        AiProvider::Lmstudio => "lmstudio",
        AiProvider::Ollama => "ollama",
    }
}

pub fn supported_base_url(provider: AiProvider) -> ProviderResult<&'static str> {
    match provider {
        AiProvider::Openrouter => Ok("https://openrouter.ai/api/v1"),
        AiProvider::Openai => Ok("https://api.openai.com/v1"),
        AiProvider::Anthropic => Ok("https://api.anthropic.com"),
        AiProvider::Lmstudio => Ok("http://localhost:1234/v1"),
        provider => Err(ProviderError::UnsupportedProvider {
            provider: provider_name(provider),
        }),
    }
}

pub fn resolved_base_url(config: &LlmProviderConfig) -> ProviderResult<String> {
    let base_url = if config.base_url.trim().is_empty() {
        supported_base_url(config.provider)?.to_owned()
    } else {
        config.base_url.trim().to_owned()
    };

    Ok(base_url.trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_support::EnvGuard;

    #[test]
    fn create_provider_accepts_openrouter_config() {
        let env_var = "SCREENCAP_TEST_OPENROUTER_KEY";
        let _guard = EnvGuard::set(env_var, "token");
        let config = LlmProviderConfig::new(
            AiProvider::Openrouter,
            "google/gemini-2.0-flash",
            env_var,
            "",
        );

        create_provider(&config).expect("create provider");
    }

    #[test]
    fn create_provider_accepts_anthropic_config() {
        let env_var = "SCREENCAP_TEST_ANTHROPIC_PROVIDER_KEY";
        let _guard = EnvGuard::set(env_var, "token");
        let config =
            LlmProviderConfig::new(AiProvider::Anthropic, "claude-sonnet-4-5", env_var, "");

        create_provider(&config).expect("create provider");
    }

    #[test]
    fn resolved_base_url_uses_lm_studio_default() {
        let config = LlmProviderConfig::new(AiProvider::Lmstudio, "llava", "IGNORED", "");

        assert_eq!(
            resolved_base_url(&config).expect("resolve base url"),
            "http://localhost:1234/v1"
        );
    }

    #[test]
    fn resolved_base_url_uses_anthropic_default() {
        let config =
            LlmProviderConfig::new(AiProvider::Anthropic, "claude-sonnet-4-5", "IGNORED", "");

        assert_eq!(
            resolved_base_url(&config).expect("resolve base url"),
            "https://api.anthropic.com"
        );
    }

    #[test]
    fn load_api_key_rejects_missing_values() {
        let env_var = "SCREENCAP_TEST_MISSING_KEY";
        let _guard = EnvGuard::unset(env_var);

        let error = load_api_key(env_var).expect_err("missing key should fail");
        assert!(matches!(error, ProviderError::MissingApiKey { .. }));
    }
}
