use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::provider::{
    load_api_key, resolved_base_url, ImageInput, LlmProvider, LlmProviderConfig, LlmResponse,
    ProviderError, ProviderResult, TokenUsage,
};

#[derive(Debug, Clone)]
pub struct OpenAiCompatClient {
    http: Client,
    model: String,
    base_url: String,
    api_key: String,
}

impl OpenAiCompatClient {
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
        format!("{}/chat/completions", self.base_url)
    }

    fn build_request(&self, prompt: &str, images: &[ImageInput]) -> ChatCompletionRequest {
        let mut content = Vec::with_capacity(images.len() + 1);
        content.push(ChatCompletionContentPart::Text {
            text: prompt.to_owned(),
        });

        for image in images {
            content.push(ChatCompletionContentPart::ImageUrl {
                image_url: ImageUrlPart {
                    url: image.data_url(),
                    detail: None,
                },
            });
        }

        ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user",
                content,
            }],
        }
    }

    async fn send_request(&self, request: ChatCompletionRequest) -> ProviderResult<LlmResponse> {
        let response = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
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

        let response: ChatCompletionResponse =
            serde_json::from_str(&body).map_err(|error| ProviderError::InvalidResponse {
                message: error.to_string(),
                body,
            })?;

        let content = response
            .first_message_text()
            .ok_or(ProviderError::MissingContent)?;
        let usage = response.usage.map(TokenUsage::from);

        Ok(LlmResponse { content, usage })
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatClient {
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
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: Vec<ChatCompletionContentPart>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ChatCompletionContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrlPart },
}

#[derive(Debug, Serialize)]
struct ImageUrlPart {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'static str>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

impl ChatCompletionResponse {
    fn first_message_text(&self) -> Option<String> {
        let content = self.choices.first()?.message.content.as_ref()?;

        match content {
            AssistantMessageContent::Text(text) => Some(text.clone()),
            AssistantMessageContent::Parts(parts) => {
                let text = parts
                    .iter()
                    .filter_map(|part| match part {
                        AssistantMessageContentPart::Text { text } => Some(text.as_str()),
                        AssistantMessageContentPart::Other => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                (!text.is_empty()).then_some(text)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: AssistantMessage,
}

#[derive(Debug, Deserialize)]
struct AssistantMessage {
    content: Option<AssistantMessageContent>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AssistantMessageContent {
    Text(String),
    Parts(Vec<AssistantMessageContentPart>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AssistantMessageContentPart {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

impl From<Usage> for TokenUsage {
    fn from(value: Usage) -> Self {
        Self {
            prompt_tokens: value.prompt_tokens,
            completion_tokens: value.completion_tokens,
            total_tokens: value.total_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorBody>,
    message: Option<String>,
}

impl ErrorEnvelope {
    fn message(self) -> Option<String> {
        self.error
            .and_then(|error| error.message)
            .or(self.message)
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
    use std::{
        env,
        io::{Read, Write},
        net::{SocketAddr, TcpListener},
        thread,
    };

    use super::*;
    use crate::ai::provider::{LlmProviderConfig, ProviderError};
    use crate::config::AiProvider;

    #[test]
    fn new_uses_openrouter_default_base_url() {
        let env_var = "SCREENCAP_TEST_OPENROUTER_CLIENT_KEY";
        let _guard = EnvGuard::set(env_var, "token");
        let config = LlmProviderConfig::new(
            AiProvider::Openrouter,
            "google/gemini-2.0-flash",
            env_var,
            "",
        );

        let client = OpenAiCompatClient::new(config).expect("create client");
        assert_eq!(client.base_url(), "https://openrouter.ai/api/v1");
    }

    #[test]
    fn new_uses_lm_studio_default_base_url() {
        let env_var = "SCREENCAP_TEST_LMSTUDIO_CLIENT_KEY";
        let _guard = EnvGuard::set(env_var, "token");
        let config = LlmProviderConfig::new(AiProvider::Lmstudio, "llava", env_var, "");

        let client = OpenAiCompatClient::new(config).expect("create client");
        assert_eq!(client.base_url(), "http://localhost:1234/v1");
    }

    #[test]
    fn build_request_embeds_images_as_data_urls() {
        let client = test_client();
        let request =
            client.build_request("Describe this frame", &[ImageInput::jpeg("ZmFrZS1qcGVn")]);

        let payload = serde_json::to_value(request).expect("serialize request");
        let image_url = &payload["messages"][0]["content"][1]["image_url"]["url"];
        assert_eq!(image_url, "data:image/jpeg;base64,ZmFrZS1qcGVn");
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
            r#"{"choices":[{"message":{"content":"hello from model"}}],"usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}}"#,
        );

        let env_var = "SCREENCAP_TEST_COMPLETE_TEXT_KEY";
        let _guard = EnvGuard::set(env_var, "token");
        let client = OpenAiCompatClient::new(LlmProviderConfig::new(
            AiProvider::Openai,
            "gpt-4o-mini",
            env_var,
            server.base_url(),
        ))
        .expect("create client");

        let response = client
            .complete_text("Summarize this")
            .await
            .expect("complete");

        assert_eq!(response.content, "hello from model");
        assert_eq!(
            response.usage,
            Some(TokenUsage {
                prompt_tokens: 7,
                completion_tokens: 3,
                total_tokens: 10,
            })
        );
    }

    fn test_client() -> OpenAiCompatClient {
        OpenAiCompatClient {
            http: Client::new(),
            model: "test-model".into(),
            base_url: "http://localhost:1234/v1".into(),
            api_key: "token".into(),
        }
    }

    struct EnvGuard {
        key: String,
        previous: Option<String>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            static ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
            let lock = ENV_LOCK
                .get_or_init(|| std::sync::Mutex::new(()))
                .lock()
                .expect("lock env");
            let previous = env::var(key).ok();
            env::set_var(key, value);
            Self {
                key: key.to_owned(),
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                env::set_var(&self.key, previous);
            } else {
                env::remove_var(&self.key);
            }
        }
    }

    struct TestServer {
        address: SocketAddr,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn spawn(status: u16, body: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
            let address = listener.local_addr().expect("listener addr");
            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer).expect("read request");
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
            });

            Self {
                address,
                handle: Some(handle),
            }
        }

        fn base_url(&self) -> String {
            format!("http://{}", self.address)
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                handle.join().expect("join server thread");
            }
        }
    }
}
