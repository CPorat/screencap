use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use serde_json::json;

use super::provider::{ImageInput, LlmProvider, LlmResponse, ProviderResult};

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

    fn next_response(&self, prompt: &str, kind: MockCallKind) -> ProviderResult<LlmResponse> {
        if let Some(response) = self
            .responses
            .lock()
            .expect("lock mock responses")
            .pop_front()
        {
            return response;
        }

        Ok(LlmResponse::new(Self::default_response_for(prompt, kind)))
    }

    fn default_response_for(prompt: &str, kind: MockCallKind) -> String {
        match kind {
            MockCallKind::Vision => default_extraction_response(prompt),
            MockCallKind::Text => default_text_response(prompt),
        }
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
        self.next_response(prompt, MockCallKind::Vision)
    }

    async fn complete_text(&self, prompt: &str) -> ProviderResult<LlmResponse> {
        self.record_call(prompt, &[], MockCallKind::Text);
        self.next_response(prompt, MockCallKind::Text)
    }
}

fn default_extraction_response(prompt: &str) -> String {
    let capture_ids = extract_capture_ids(prompt);
    let capture_ids = if capture_ids.is_empty() {
        vec![1]
    } else {
        capture_ids
    };

    let frames = capture_ids
        .into_iter()
        .map(|capture_id| {
            json!({
                "capture_id": capture_id,
                "activity_type": "coding",
                "description": format!("Mock extraction for capture {capture_id}"),
                "app_context": "Mock screencap extraction context",
                "project": "screencap",
                "topics": ["mock", "pipeline"],
                "people": [],
                "key_content": format!("mock-key-content-{capture_id}"),
                "sentiment": "focused"
            })
        })
        .collect::<Vec<_>>();

    json!({
        "frames": frames,
        "batch_summary": {
            "primary_activity": "Mock extraction pipeline",
            "project_context": "screencap",
            "narrative": "Mock provider generated extraction output."
        }
    })
    .to_string()
}

fn default_text_response(prompt: &str) -> String {
    if let Some(date) = extract_prompt_value(prompt, "- date:") {
        return json!({
            "type": "daily",
            "date": date,
            "total_active_hours": 1.5,
            "projects": [
                {
                    "name": "screencap",
                    "total_minutes": 90,
                    "activities": ["mock synthesis"],
                    "key_accomplishments": ["Generated daily summary"]
                }
            ],
            "time_allocation": {"coding": "1h 30m"},
            "focus_blocks": [
                {
                    "start": "09:00",
                    "end": "10:30",
                    "duration_min": 90,
                    "project": "screencap",
                    "quality": "focused"
                }
            ],
            "open_threads": ["none"],
            "narrative": "Mock daily summary narrative."
        })
        .to_string();
    }

    if let (Some(hour_start), Some(hour_end)) = (
        extract_prompt_value(prompt, "- hour_start:"),
        extract_prompt_value(prompt, "- hour_end:"),
    ) {
        return json!({
            "type": "hourly",
            "hour_start": hour_start,
            "hour_end": hour_end,
            "dominant_activity": "coding",
            "projects": [
                {
                    "name": "screencap",
                    "minutes": 45,
                    "activities": ["mock synthesis"]
                }
            ],
            "topics": ["mock", "pipeline"],
            "people_interacted": [],
            "key_moments": ["Mock hourly summary generated"],
            "focus_score": 0.75,
            "narrative": "Mock hourly digest narrative."
        })
        .to_string();
    }

    if let (Some(window_start), Some(window_end)) = (
        extract_prompt_value(prompt, "- window_start:"),
        extract_prompt_value(prompt, "- window_end:"),
    ) {
        return json!({
            "type": "rolling",
            "window_start": window_start,
            "window_end": window_end,
            "current_focus": "Mock rolling focus",
            "active_project": "screencap",
            "apps_used": {"Ghostty": "5 min"},
            "context_switches": 1,
            "mood": "focused",
            "summary": "Mock rolling summary narrative."
        })
        .to_string();
    }

    json!({
        "answer": "Mock provider default response",
        "capture_ids": []
    })
    .to_string()
}

fn extract_capture_ids(prompt: &str) -> Vec<i64> {
    prompt
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("- capture_id:")
                .map(str::trim)
                .and_then(|raw| raw.parse::<i64>().ok())
        })
        .collect()
}

fn extract_prompt_value<'a>(prompt: &'a str, key: &str) -> Option<&'a str> {
    prompt.lines().find_map(|line| {
        line.trim()
            .strip_prefix(key)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
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

    #[tokio::test]
    async fn complete_generates_extraction_json_when_queue_is_empty() {
        let provider = MockLlmProvider::new();
        let response = provider
            .complete(
                "Frame metadata:\n- capture_id: 41\n- capture_id: 42",
                &[ImageInput::jpeg("ZmFrZQ==")],
            )
            .await
            .expect("mock fallback response");

        assert!(response.content.contains("\"capture_id\":41"));
        assert!(response.content.contains("\"capture_id\":42"));
    }

    #[tokio::test]
    async fn complete_text_generates_daily_json_when_prompt_has_date() {
        let provider = MockLlmProvider::new();
        let response = provider
            .complete_text(
                "Requested date:\n- date: 2026-04-10\n- window_start: 2026-04-10T00:00:00Z",
            )
            .await
            .expect("mock fallback response");

        assert!(response.content.contains("\"type\":\"daily\""));
        assert!(response.content.contains("\"date\":\"2026-04-10\""));
    }
}
