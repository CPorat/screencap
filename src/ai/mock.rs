use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use super::provider::{ImageInput, LlmProvider, LlmResponse, ProviderError, ProviderResult};

#[derive(Debug, Clone)]
pub struct MockLlmProvider {
    responses: Arc<Mutex<VecDeque<ProviderResult<LlmResponse>>>>,
    calls: Arc<Mutex<Vec<MockCall>>>,
}

impl MockLlmProvider {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::new())),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_responses(
        responses: impl IntoIterator<Item = ProviderResult<LlmResponse>>,
    ) -> Self {
        let provider = Self::new();
        for response in responses {
            provider.push_response(response);
        }
        provider
    }

    pub fn push_response(&self, response: ProviderResult<LlmResponse>) {
        self.responses
            .lock()
            .expect("lock mock responses")
            .push_back(response);
    }

    pub fn push_text_response(&self, content: impl Into<String>) {
        self.push_response(Ok(LlmResponse::new(content)));
    }

    pub fn calls(&self) -> Vec<MockCall> {
        self.calls.lock().expect("lock mock calls").clone()
    }

    fn record_call(&self, prompt: &str, images: &[ImageInput], kind: MockCallKind) {
        self.calls.lock().expect("lock mock calls").push(MockCall {
            prompt: prompt.to_owned(),
            images: images.to_vec(),
            kind,
        });
    }

    fn next_response(&self) -> ProviderResult<LlmResponse> {
        self.responses
            .lock()
            .expect("lock mock responses")
            .pop_front()
            .unwrap_or(Err(ProviderError::MockResponseExhausted))
    }
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockCall {
    pub prompt: String,
    pub images: Vec<ImageInput>,
    pub kind: MockCallKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockCallKind {
    Vision,
    Text,
}

#[async_trait::async_trait]
impl LlmProvider for MockLlmProvider {
    async fn complete(&self, prompt: &str, images: &[ImageInput]) -> ProviderResult<LlmResponse> {
        self.record_call(prompt, images, MockCallKind::Vision);
        self.next_response()
    }

    async fn complete_text(&self, prompt: &str) -> ProviderResult<LlmResponse> {
        self.record_call(prompt, &[], MockCallKind::Text);
        self.next_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::TokenUsage;

    #[tokio::test]
    async fn complete_returns_configured_response() {
        let provider = MockLlmProvider::with_responses([Ok(LlmResponse::with_usage(
            "vision output",
            TokenUsage {
                prompt_tokens: 4,
                completion_tokens: 2,
                total_tokens: 6,
            },
        ))]);

        let response = provider
            .complete("Describe the frame", &[ImageInput::jpeg("ZmFrZQ==")])
            .await
            .expect("complete");

        assert_eq!(response.content, "vision output");
        assert_eq!(provider.calls()[0].kind, MockCallKind::Vision);
    }

    #[tokio::test]
    async fn complete_text_returns_configured_response() {
        let provider = MockLlmProvider::new();
        provider.push_text_response("text output");

        let response = provider
            .complete_text("Summarize this batch")
            .await
            .expect("complete text");

        assert_eq!(response.content, "text output");
        assert_eq!(provider.calls()[0].kind, MockCallKind::Text);
    }
}
