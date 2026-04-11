use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::provider::{
    load_api_key, resolved_base_url, ImageInput, LlmProvider, LlmProviderConfig, LlmResponse,
    ProviderError, ProviderResult, TokenUsage,
};

#[derive(Debug, Clone)]
pub struct GoogleClient {
    http: Client,
    model: String,
    base_url: String,
    api_key: String,
}

impl GoogleClient {
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
        format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url,
            self.request_model_name()
        )
    }

    fn request_model_name(&self) -> &str {
        self.model.trim_start_matches("models/")
    }

    fn build_request(&self, prompt: &str, images: &[ImageInput]) -> GenerateContentRequest {
        let mut parts = Vec::with_capacity(images.len() + 1);

        for image in images {
            parts.push(GenerateContentPart::InlineData {
                inline_data: InlineDataPart::from(image),
            });
        }

        parts.push(GenerateContentPart::Text {
            text: prompt.to_owned(),
        });

        GenerateContentRequest {
            contents: vec![GenerateContentContent { parts }],
        }
    }

    async fn send_request(&self, request: GenerateContentRequest) -> ProviderResult<LlmResponse> {
        let response = self
            .http
            .post(self.endpoint())
            .header("x-goog-api-key", &self.api_key)
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

        let response: GenerateContentResponse =
            serde_json::from_str(&body).map_err(|error| ProviderError::InvalidResponse {
                message: error.to_string(),
                body,
            })?;

        Ok(LlmResponse {
            content: response
                .text_content()
                .ok_or(ProviderError::MissingContent)?,
            usage: response
                .usage_metadata
                .and_then(UsageMetadata::into_token_usage),
            cost_cents: None,
        })
    }
}

#[async_trait::async_trait]
impl LlmProvider for GoogleClient {
    async fn complete(&self, prompt: &str, images: &[ImageInput]) -> ProviderResult<LlmResponse> {
        self.send_request(self.build_request(prompt, images)).await
    }

    async fn complete_text(&self, prompt: &str) -> ProviderResult<LlmResponse> {
        self.send_request(self.build_request(prompt, &[])).await
    }
}

fn classify_error(status: StatusCode, body: &str) -> ProviderError {
    let envelope = serde_json::from_str::<ErrorEnvelope>(body).ok();
    let message = envelope
        .as_ref()
        .and_then(ErrorEnvelope::message)
        .unwrap_or_else(|| body.trim().to_owned());
    let api_status = envelope.as_ref().and_then(ErrorEnvelope::status);

    match (status, api_status.as_deref()) {
        (StatusCode::TOO_MANY_REQUESTS, _) | (_, Some("RESOURCE_EXHAUSTED")) => {
            ProviderError::RateLimited { message }
        }
        (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN, _)
        | (_, Some("UNAUTHENTICATED" | "PERMISSION_DENIED")) => {
            ProviderError::Authentication { message }
        }
        _ => ProviderError::RequestFailed {
            status: status.as_u16(),
            message,
        },
    }
}

#[derive(Debug, Serialize)]
struct GenerateContentRequest {
    contents: Vec<GenerateContentContent>,
}

#[derive(Debug, Serialize)]
struct GenerateContentContent {
    parts: Vec<GenerateContentPart>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum GenerateContentPart {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inline_data")]
        inline_data: InlineDataPart,
    },
}

#[derive(Debug, Serialize)]
struct InlineDataPart {
    #[serde(rename = "mime_type")]
    mime_type: String,
    data: String,
}

impl From<&ImageInput> for InlineDataPart {
    fn from(value: &ImageInput) -> Self {
        Self {
            mime_type: value.media_type.clone(),
            data: value.data_base64.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GenerateContentResponse {
    candidates: Vec<Candidate>,
    #[serde(rename = "usageMetadata", alias = "usage_metadata")]
    usage_metadata: Option<UsageMetadata>,
}

impl GenerateContentResponse {
    fn text_content(&self) -> Option<String> {
        let text = self
            .candidates
            .first()?
            .content
            .as_ref()?
            .parts
            .iter()
            .filter_map(|part| part.text.as_deref())
            .collect::<Vec<_>>()
            .join("");

        (!text.is_empty()).then_some(text)
    }
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
    parts: Vec<CandidatePart>,
}

#[derive(Debug, Deserialize)]
struct CandidatePart {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsageMetadata {
    #[serde(rename = "promptTokenCount", alias = "prompt_token_count")]
    prompt_token_count: Option<u64>,
    #[serde(rename = "candidatesTokenCount", alias = "candidates_token_count")]
    candidates_token_count: Option<u64>,
    #[serde(rename = "totalTokenCount", alias = "total_token_count")]
    total_token_count: Option<u64>,
}

impl UsageMetadata {
    fn into_token_usage(self) -> Option<TokenUsage> {
        let prompt_tokens = match (
            self.prompt_token_count,
            self.candidates_token_count,
            self.total_token_count,
        ) {
            (Some(prompt_tokens), _, _) => prompt_tokens,
            (None, Some(completion_tokens), Some(total_tokens)) => {
                total_tokens.checked_sub(completion_tokens)?
            }
            _ => return None,
        };

        let completion_tokens = match (self.candidates_token_count, self.total_token_count) {
            (Some(completion_tokens), _) => completion_tokens,
            (None, Some(total_tokens)) => total_tokens.checked_sub(prompt_tokens)?,
            _ => return None,
        };

        let total_tokens = self
            .total_token_count
            .unwrap_or(prompt_tokens + completion_tokens);

        Some(TokenUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorBody>,
}

impl ErrorEnvelope {
    fn message(&self) -> Option<String> {
        self.error
            .as_ref()
            .and_then(|error| error.message.as_deref())
            .map(str::trim)
            .filter(|message| !message.is_empty())
            .map(str::to_owned)
    }

    fn status(&self) -> Option<String> {
        self.error
            .as_ref()
            .and_then(|error| error.status.as_deref())
            .map(str::trim)
            .filter(|status| !status.is_empty())
            .map(str::to_owned)
    }
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
    status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_support::{EnvGuard, TestServer};
    use crate::config::AiProvider;

    #[test]
    fn new_uses_google_default_base_url() {
        let env_var = "SCREENCAP_TEST_GOOGLE_CLIENT_KEY";
        let _guard = EnvGuard::set(env_var, "token");
        let config = LlmProviderConfig::new(AiProvider::Google, "gemini-2.5-flash", env_var, "");

        let client = GoogleClient::new(config).expect("create client");
        assert_eq!(
            client.base_url(),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(client.model(), "gemini-2.5-flash");
    }

    #[test]
    fn endpoint_accepts_models_prefixed_model_names() {
        let client = test_client_with_model("models/gemini-2.5-flash");

        assert_eq!(
            client.endpoint(),
            "http://127.0.0.1:1/v1beta/models/gemini-2.5-flash:generateContent"
        );
    }

    #[test]
    fn build_request_embeds_images_as_inline_data_parts() {
        let client = test_client();
        let request =
            client.build_request("Describe this frame", &[ImageInput::jpeg("ZmFrZS1qcGVn")]);

        let payload = serde_json::to_value(request).expect("serialize request");
        let inline_data = &payload["contents"][0]["parts"][0]["inline_data"];
        assert_eq!(inline_data["mime_type"], "image/jpeg");
        assert_eq!(inline_data["data"], "ZmFrZS1qcGVn");
        assert_eq!(
            payload["contents"][0]["parts"][1]["text"],
            "Describe this frame"
        );
    }

    #[test]
    fn classify_rate_limits_and_auth_failures() {
        let auth = classify_error(
            StatusCode::FORBIDDEN,
            r#"{"error":{"message":"bad key","status":"PERMISSION_DENIED"}}"#,
        );
        assert!(matches!(auth, ProviderError::Authentication { .. }));

        let rate_limit = classify_error(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"slow down","status":"RESOURCE_EXHAUSTED"}}"#,
        );
        assert!(matches!(rate_limit, ProviderError::RateLimited { .. }));
    }

    #[test]
    fn usage_metadata_derives_missing_total_tokens() {
        let usage = UsageMetadata {
            prompt_token_count: Some(7),
            candidates_token_count: Some(3),
            total_token_count: None,
        };

        assert_eq!(
            usage.into_token_usage(),
            Some(TokenUsage {
                prompt_tokens: 7,
                completion_tokens: 3,
                total_tokens: 10,
            })
        );
    }

    #[tokio::test]
    async fn complete_text_parses_usage_and_content() {
        let server = TestServer::spawn(
            200,
            r#"{"candidates":[{"content":{"parts":[{"text":"hello from gemini"}]}}],"usageMetadata":{"promptTokenCount":7,"candidatesTokenCount":3,"totalTokenCount":10}}"#,
        );

        let env_var = "SCREENCAP_TEST_GOOGLE_COMPLETE_TEXT_KEY";
        let _guard = EnvGuard::set(env_var, "token");
        let client = GoogleClient::new(LlmProviderConfig::new(
            AiProvider::Google,
            "gemini-2.5-flash",
            env_var,
            server.base_url(),
        ))
        .expect("create client");

        let response = client
            .complete_text("Summarize this")
            .await
            .expect("complete");

        assert_eq!(response.content, "hello from gemini");
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

    fn test_client() -> GoogleClient {
        test_client_with_model("gemini-2.5-flash")
    }

    fn test_client_with_model(model: &str) -> GoogleClient {
        GoogleClient {
            http: Client::builder().build().expect("build http client"),
            model: model.to_owned(),
            base_url: "http://127.0.0.1:1".to_owned(),
            api_key: "token".to_owned(),
        }
    }
}
