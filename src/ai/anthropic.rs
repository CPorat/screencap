use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::provider::{
    load_api_key, resolved_base_url, ImageInput, LlmProvider, LlmProviderConfig, LlmResponse,
    ProviderError, ProviderResult, TokenUsage,
};

const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 4096;

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    http: Client,
    model: String,
    base_url: String,
    api_key: String,
}

impl AnthropicClient {
    pub fn new(config: LlmProviderConfig) -> ProviderResult<Self> {
        let api_key = load_api_key(&config.api_key_env)?;
        let base_url = resolved_base_url(&config)?;

        Ok(Self {
            http: Client::builder()
                .build()
                .map_err(|source| ProviderError::Network { source })?,
            model: config.model,
            base_url,
            api_key,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/messages", self.base_url)
    }

    fn build_request(&self, prompt: &str, images: &[ImageInput]) -> MessagesRequest {
        let mut content = Vec::with_capacity(images.len() + 1);

        for image in images {
            content.push(InputContentBlock::Image {
                source: Base64ImageSource::from(image),
            });
        }

        content.push(InputContentBlock::Text {
            text: prompt.to_owned(),
        });

        MessagesRequest {
            model: self.model.clone(),
            max_tokens: DEFAULT_MAX_TOKENS,
            messages: vec![InputMessage {
                role: "user",
                content,
            }],
        }
    }

    async fn send_request(&self, request: MessagesRequest) -> ProviderResult<LlmResponse> {
        let response = self
            .http
            .post(self.endpoint())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .json(&request)
            .send()
            .await
            .map_err(|source| ProviderError::Network { source })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|source| ProviderError::Network { source })?;

        if !status.is_success() {
            return Err(classify_error(status, &body));
        }

        let response: MessagesResponse =
            serde_json::from_str(&body).map_err(|error| ProviderError::InvalidResponse {
                message: error.to_string(),
                body,
            })?;

        let content = response
            .text_content()
            .ok_or(ProviderError::MissingContent)?;

        Ok(LlmResponse {
            content,
            usage: response.usage.map(TokenUsage::from),
            cost_cents: None,
        })
    }
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicClient {
    async fn complete(&self, prompt: &str, images: &[ImageInput]) -> ProviderResult<LlmResponse> {
        self.send_request(self.build_request(prompt, images)).await
    }

    async fn complete_text(&self, prompt: &str) -> ProviderResult<LlmResponse> {
        self.send_request(self.build_request(prompt, &[])).await
    }
}

fn classify_error(status: StatusCode, body: &str) -> ProviderError {
    let message = serde_json::from_str::<ErrorEnvelope>(body)
        .ok()
        .and_then(|envelope| envelope.message())
        .unwrap_or_else(|| body.trim().to_owned());

    match status {
        StatusCode::UNAUTHORIZED => ProviderError::Authentication { message },
        StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimited { message },
        _ => ProviderError::RequestFailed {
            status: status.as_u16(),
            message,
        },
    }
}

#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<InputMessage>,
}

#[derive(Debug, Serialize)]
struct InputMessage {
    role: &'static str,
    content: Vec<InputContentBlock>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InputContentBlock {
    Text { text: String },
    Image { source: Base64ImageSource },
}

#[derive(Debug, Serialize)]
struct Base64ImageSource {
    #[serde(rename = "type")]
    source_type: &'static str,
    media_type: String,
    data: String,
}

impl From<&ImageInput> for Base64ImageSource {
    fn from(value: &ImageInput) -> Self {
        Self {
            source_type: "base64",
            media_type: value.media_type.clone(),
            data: value.data_base64.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<OutputContentBlock>,
    usage: Option<Usage>,
}

impl MessagesResponse {
    fn text_content(&self) -> Option<String> {
        let text = self
            .content
            .iter()
            .filter(|block| block.block_type == "text")
            .filter_map(|block| block.text.as_deref())
            .collect::<Vec<_>>()
            .join("");

        (!text.is_empty()).then_some(text)
    }
}

#[derive(Debug, Deserialize)]
struct OutputContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
}

impl From<Usage> for TokenUsage {
    fn from(value: Usage) -> Self {
        Self {
            prompt_tokens: value.input_tokens,
            completion_tokens: value.output_tokens,
            total_tokens: value.input_tokens + value.output_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorBody>,
}

impl ErrorEnvelope {
    fn message(self) -> Option<String> {
        self.error
            .and_then(|error| error.message)
            .map(|message| message.trim().to_owned())
            .filter(|message| !message.is_empty())
    }
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_support::{EnvGuard, TestServer};
    use crate::config::AiProvider;

    #[test]
    fn new_uses_anthropic_default_base_url() {
        let env_var = "SCREENCAP_TEST_ANTHROPIC_CLIENT_KEY";
        let _guard = EnvGuard::set(env_var, "token");
        let config =
            LlmProviderConfig::new(AiProvider::Anthropic, "claude-sonnet-4-5", env_var, "");

        let client = AnthropicClient::new(config).expect("create client");
        assert_eq!(client.base_url(), "https://api.anthropic.com");
        assert_eq!(client.model(), "claude-sonnet-4-5");
    }

    #[test]
    fn build_request_embeds_images_as_base64_blocks() {
        let client = test_client();
        let request =
            client.build_request("Describe this frame", &[ImageInput::jpeg("ZmFrZS1qcGVn")]);

        let payload = serde_json::to_value(request).expect("serialize request");
        let image = &payload["messages"][0]["content"][0];
        assert_eq!(image["type"], "image");
        assert_eq!(image["source"]["type"], "base64");
        assert_eq!(image["source"]["media_type"], "image/jpeg");
        assert_eq!(image["source"]["data"], "ZmFrZS1qcGVn");
        assert_eq!(payload["messages"][0]["content"][1]["type"], "text");
        assert_eq!(
            payload["messages"][0]["content"][1]["text"],
            "Describe this frame"
        );
        assert_eq!(payload["max_tokens"], DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn classify_rate_limits_and_auth_failures() {
        let auth = classify_error(
            StatusCode::UNAUTHORIZED,
            r#"{"error":{"message":"bad key"}}"#,
        );
        assert!(matches!(auth, ProviderError::Authentication { .. }));

        let rate_limit = classify_error(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"slow down"}}"#,
        );
        assert!(matches!(rate_limit, ProviderError::RateLimited { .. }));
    }

    #[tokio::test]
    async fn complete_text_parses_usage_and_content() {
        let server = TestServer::spawn(
            200,
            r#"{"content":[{"type":"text","text":"hello from claude"}],"usage":{"input_tokens":7,"output_tokens":3}}"#,
        );

        let env_var = "SCREENCAP_TEST_ANTHROPIC_COMPLETE_TEXT_KEY";
        let _guard = EnvGuard::set(env_var, "token");
        let client = AnthropicClient::new(LlmProviderConfig::new(
            AiProvider::Anthropic,
            "claude-sonnet-4-5",
            env_var,
            server.base_url(),
        ))
        .expect("create client");

        let response = client
            .complete_text("Summarize this")
            .await
            .expect("complete");

        assert_eq!(response.content, "hello from claude");
        assert_eq!(
            response.usage,
            Some(TokenUsage {
                prompt_tokens: 7,
                completion_tokens: 3,
                total_tokens: 10,
            })
        );
        assert_eq!(response.cost_cents, None);
    }

    fn test_client() -> AnthropicClient {
        AnthropicClient {
            http: Client::new(),
            model: "test-model".into(),
            base_url: "https://api.anthropic.com".into(),
            api_key: "token".into(),
        }
    }
}
