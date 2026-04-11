use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::provider::{
    resolved_base_url, ImageInput, LlmProvider, LlmProviderConfig, LlmResponse, ProviderError,
    ProviderResult, TokenUsage,
};

#[derive(Debug, Clone)]
pub struct OllamaClient {
    http: Client,
    model: String,
    base_url: String,
}

impl OllamaClient {
    pub fn new(config: LlmProviderConfig) -> ProviderResult<Self> {
        let base_url = resolved_base_url(&config)?;

        Ok(Self {
            http: Client::builder()
                .build()
                .map_err(|source| ProviderError::Network { source })?,
            model: config.model,
            base_url,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn endpoint(&self) -> String {
        format!("{}/api/chat", self.base_url)
    }

    fn build_request(&self, prompt: &str, images: &[ImageInput]) -> ChatRequest {
        ChatRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user",
                content: prompt.to_owned(),
                images: images
                    .iter()
                    .map(|image| image.data_base64.clone())
                    .collect(),
            }],
            stream: false,
        }
    }

    async fn send_request(&self, request: ChatRequest) -> ProviderResult<LlmResponse> {
        let response = self
            .http
            .post(self.endpoint())
            .json(&request)
            .send()
            .await
            .map_err(|source| classify_network_error(source, &self.base_url))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|source| classify_network_error(source, &self.base_url))?;

        if !status.is_success() {
            return Err(classify_error(status, &body));
        }

        let response: ChatResponse =
            serde_json::from_str(&body).map_err(|error| ProviderError::InvalidResponse {
                message: error.to_string(),
                body,
            })?;

        let usage = response.token_usage();
        let content = response
            .message
            .content
            .filter(|content| !content.trim().is_empty())
            .ok_or(ProviderError::MissingContent)?;

        Ok(LlmResponse {
            content,
            usage,
            cost_cents: None,
        })
    }
}

#[async_trait::async_trait]
impl LlmProvider for OllamaClient {
    async fn complete(&self, prompt: &str, images: &[ImageInput]) -> ProviderResult<LlmResponse> {
        self.send_request(self.build_request(prompt, images)).await
    }

    async fn complete_text(&self, prompt: &str) -> ProviderResult<LlmResponse> {
        self.send_request(self.build_request(prompt, &[])).await
    }
}

fn classify_network_error(source: reqwest::Error, base_url: &str) -> ProviderError {
    if source.is_connect() {
        return ProviderError::RequestFailed {
            status: 503,
            message: format!(
                "could not reach Ollama at {}. Start the local Ollama daemon and try again",
                base_url
            ),
        };
    }

    ProviderError::Network { source }
}

fn classify_error(status: StatusCode, body: &str) -> ProviderError {
    let message = serde_json::from_str::<ErrorEnvelope>(body)
        .ok()
        .and_then(ErrorEnvelope::message)
        .unwrap_or_else(|| body.trim().to_owned());

    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            ProviderError::Authentication { message }
        }
        StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimited { message },
        _ => ProviderError::RequestFailed {
            status: status.as_u16(),
            message,
        },
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatResponseMessage,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
}

impl ChatResponse {
    fn token_usage(&self) -> Option<TokenUsage> {
        let prompt_tokens = self.prompt_eval_count?;
        let completion_tokens = self.eval_count?;

        Some(TokenUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: Option<String>,
}

impl ErrorEnvelope {
    fn message(self) -> Option<String> {
        self.error
            .map(|message| message.trim().to_owned())
            .filter(|message| !message.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;
    use crate::ai::{
        provider::{LlmProviderConfig, ProviderError},
        test_support::TestServer,
    };
    use crate::config::AiProvider;

    #[test]
    fn new_uses_ollama_default_base_url() {
        let config = LlmProviderConfig::new(AiProvider::Ollama, "llava", "IGNORED", "");

        let client = OllamaClient::new(config).expect("create client");
        assert_eq!(client.base_url(), "http://localhost:11434");
        assert_eq!(client.model(), "llava");
    }

    #[test]
    fn build_request_embeds_images_as_base64_payloads() {
        let client = test_client();
        let request = client.build_request(
            "Describe this frame",
            &[
                ImageInput::jpeg("ZmFrZS1qcGVn"),
                ImageInput::jpeg("c2Vjb25kLWpwZWc="),
            ],
        );

        let payload = serde_json::to_value(request).expect("serialize request");
        assert_eq!(payload["stream"], false);
        assert_eq!(payload["messages"][0]["content"], "Describe this frame");
        assert_eq!(payload["messages"][0]["images"][0], "ZmFrZS1qcGVn");
        assert_eq!(payload["messages"][0]["images"][1], "c2Vjb25kLWpwZWc=");
    }

    #[tokio::test]
    async fn complete_text_parses_usage_and_content() {
        let server = TestServer::spawn(
            200,
            r#"{"message":{"content":"hello from ollama"},"prompt_eval_count":9,"eval_count":4,"done":true}"#,
        );
        let client = OllamaClient::new(LlmProviderConfig::new(
            AiProvider::Ollama,
            "llama3.2",
            "IGNORED",
            server.base_url(),
        ))
        .expect("create client");

        let response = client
            .complete_text("Summarize this")
            .await
            .expect("complete");

        assert_eq!(response.content, "hello from ollama");
        assert_eq!(
            response.usage,
            Some(TokenUsage {
                prompt_tokens: 9,
                completion_tokens: 4,
                total_tokens: 13,
            })
        );
        assert_eq!(response.cost_cents, None);
    }

    #[tokio::test]
    async fn complete_text_reports_connect_failures_with_ollama_hint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let address = listener.local_addr().expect("listener addr");
        drop(listener);

        let client = OllamaClient::new(LlmProviderConfig::new(
            AiProvider::Ollama,
            "llama3.2",
            "IGNORED",
            format!("http://{}", address),
        ))
        .expect("create client");

        let error = client
            .complete_text("Summarize this")
            .await
            .expect_err("connect failure should error");

        match error {
            ProviderError::RequestFailed { status, message } => {
                assert_eq!(status, 503);
                assert!(message.contains("could not reach Ollama"));
                assert!(message.contains(&format!("http://{}", address)));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    fn test_client() -> OllamaClient {
        OllamaClient {
            http: Client::new(),
            model: "test-model".into(),
            base_url: "http://localhost:11434".into(),
        }
    }
}
