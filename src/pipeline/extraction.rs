use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};

use crate::{
    capture::window::WindowInfo,
    storage::models::{ActivityType, Capture, Sentiment},
};

use super::{json::extract_json_payload, prompts::load_extraction_prompt_template};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionResult {
    pub frames: Vec<ExtractedFrame>,
    pub batch_summary: ExtractionBatchSummary,
}

impl ExtractionResult {
    fn validate(&self) -> Result<()> {
        if self.frames.is_empty() {
            bail!("extraction response contained no frames")
        }

        let mut capture_ids = BTreeSet::new();
        for frame in &self.frames {
            if !capture_ids.insert(frame.capture_id) {
                bail!(
                    "extraction response contains duplicate capture_id {}",
                    frame.capture_id
                );
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedFrame {
    pub capture_id: i64,
    pub activity_type: ActivityType,
    pub description: String,
    pub app_context: String,
    pub project: Option<String>,
    pub topics: Vec<String>,
    pub people: Vec<String>,
    pub key_content: String,
    pub sentiment: Sentiment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionBatchSummary {
    pub primary_activity: String,
    pub project_context: String,
    pub narrative: String,
}

pub fn build_extraction_prompt(captures: &[Capture], window_metadata: &[WindowInfo]) -> String {
    let extraction_prompt = load_extraction_prompt_template();
    let mut prompt = String::with_capacity(extraction_prompt.len() + captures.len() * 256);
    prompt.push_str(&extraction_prompt);
    prompt.push_str("\n\nFrame metadata:\n");

    for (index, capture) in captures.iter().enumerate() {
        let metadata = PromptMetadata::from_sources(capture, window_metadata.get(index));
        let timestamp = capture.timestamp.to_rfc3339_opts(SecondsFormat::Secs, true);
        let display_id = capture
            .display_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_owned());

        prompt.push_str(&format!(
            concat!(
                "- capture_id: {capture_id}\n",
                "  timestamp_utc: {timestamp}\n",
                "  app_name: {app_name}\n",
                "  window_title: {window_title}\n",
                "  bundle_id: {bundle_id}\n",
                "  display_id: {display_id}\n"
            ),
            capture_id = capture.id,
            timestamp = timestamp,
            app_name = metadata.app_name,
            window_title = metadata.window_title,
            bundle_id = metadata.bundle_id,
            display_id = display_id,
        ));
    }

    if captures.is_empty() {
        prompt.push_str("- none\n");
    }

    prompt.push_str(
        "\nUse the attached screenshots and the metadata above together. Return JSON only; do not wrap it in markdown or add commentary.",
    );

    prompt
}

pub fn parse_extraction_response(json_str: &str) -> Result<ExtractionResult> {
    let payload = extract_json_payload(json_str);
    let parsed = serde_json::from_str::<ExtractionResult>(payload)
        .context("failed to parse extraction response JSON")?;
    parsed.validate()?;
    Ok(parsed)
}

struct PromptMetadata<'a> {
    app_name: &'a str,
    window_title: &'a str,
    bundle_id: &'a str,
}

impl<'a> PromptMetadata<'a> {
    fn from_sources(capture: &'a Capture, window: Option<&'a WindowInfo>) -> Self {
        Self {
            app_name: window
                .map(|value| value.app_name.as_str())
                .or(capture.app_name.as_deref())
                .unwrap_or(""),
            window_title: window
                .map(|value| value.window_title.as_str())
                .or(capture.window_title.as_deref())
                .unwrap_or(""),
            bundle_id: window
                .map(|value| value.bundle_id.as_str())
                .or(capture.bundle_id.as_deref())
                .unwrap_or(""),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{
        build_extraction_prompt, parse_extraction_response, ExtractedFrame, ExtractionBatchSummary,
        ExtractionResult,
    };
    use crate::{
        capture::window::WindowInfo,
        storage::models::{ActivityType, Capture, ExtractionStatus, Sentiment},
    };

    #[test]
    fn build_extraction_prompt_includes_capture_metadata() {
        let captures = vec![
            sample_capture(
                101,
                0,
                Some("Ghostty"),
                Some("cargo test"),
                Some("com.mitchellh.ghostty"),
            ),
            sample_capture(
                102,
                1,
                Some("Safari"),
                Some("SPEC.md"),
                Some("com.apple.Safari"),
            ),
            sample_capture(
                103,
                2,
                Some("Slack"),
                Some("Alice DM"),
                Some("com.tinyspeck.slackmacgap"),
            ),
        ];
        let metadata = vec![
            WindowInfo {
                app_name: "Ghostty".into(),
                window_title: "cargo test --features mock-capture".into(),
                bundle_id: "com.mitchellh.ghostty".into(),
            },
            WindowInfo {
                app_name: "Safari".into(),
                window_title: "Screencap SPEC.md".into(),
                bundle_id: "com.apple.Safari".into(),
            },
            WindowInfo {
                app_name: "Slack".into(),
                window_title: "Alice DM".into(),
                bundle_id: "com.tinyspeck.slackmacgap".into(),
            },
        ];

        let prompt = build_extraction_prompt(&captures, &metadata);

        assert!(prompt.contains("capture_id: 101"));
        assert!(prompt.contains("timestamp_utc: 2026-04-10T14:01:00Z"));
        assert!(prompt.contains("app_name: Ghostty"));
        assert!(prompt.contains("window_title: cargo test --features mock-capture"));
        assert!(prompt.contains("bundle_id: com.apple.Safari"));
        assert!(prompt.contains("display_id: 2"));
    }

    #[test]
    fn parse_extraction_response_parses_golden_json() {
        let json = r#"{
          "frames": [
            {
              "capture_id": 101,
              "activity_type": "coding",
              "description": "Editing the extraction pipeline in Rust.",
              "app_context": "Ghostty is running cargo test for the screencap repo.",
              "project": "screencap",
              "topics": ["rust", "testing", "llm"],
              "people": [],
              "key_content": "cargo test --features mock-capture",
              "sentiment": "focused"
            },
            {
              "capture_id": 102,
              "activity_type": "reading",
              "description": "Reviewing the product spec for extraction output requirements.",
              "app_context": "Safari is open to the Screencap spec.",
              "project": "screencap",
              "topics": ["product-spec", "extraction"],
              "people": ["@alice in Slack"],
              "key_content": "Return JSON in this exact format",
              "sentiment": "exploring"
            }
          ],
          "batch_summary": {
            "primary_activity": "Implementing and validating the extraction pipeline.",
            "project_context": "Working on Screencap extraction prompt and parser code.",
            "narrative": "The user alternated between Rust code and the product spec to implement the extraction pipeline. They were focused on matching the JSON schema and testing it end to end."
          }
        }"#;

        let parsed = parse_extraction_response(json).expect("parse extraction response");

        assert_eq!(
            parsed,
            ExtractionResult {
                frames: vec![
                    ExtractedFrame {
                        capture_id: 101,
                        activity_type: ActivityType::Coding,
                        description: "Editing the extraction pipeline in Rust.".into(),
                        app_context: "Ghostty is running cargo test for the screencap repo.".into(),
                        project: Some("screencap".into()),
                        topics: vec!["rust".into(), "testing".into(), "llm".into()],
                        people: vec![],
                        key_content: "cargo test --features mock-capture".into(),
                        sentiment: Sentiment::Focused,
                    },
                    ExtractedFrame {
                        capture_id: 102,
                        activity_type: ActivityType::Reading,
                        description: "Reviewing the product spec for extraction output requirements.".into(),
                        app_context: "Safari is open to the Screencap spec.".into(),
                        project: Some("screencap".into()),
                        topics: vec!["product-spec".into(), "extraction".into()],
                        people: vec!["@alice in Slack".into()],
                        key_content: "Return JSON in this exact format".into(),
                        sentiment: Sentiment::Exploring,
                    },
                ],
                batch_summary: ExtractionBatchSummary {
                    primary_activity: "Implementing and validating the extraction pipeline.".into(),
                    project_context: "Working on Screencap extraction prompt and parser code.".into(),
                    narrative: "The user alternated between Rust code and the product spec to implement the extraction pipeline. They were focused on matching the JSON schema and testing it end to end.".into(),
                },
            }
        );
    }

    #[test]
    fn parse_extraction_response_rejects_duplicate_capture_ids() {
        let json = r#"{
          "frames": [
            {
              "capture_id": 101,
              "activity_type": "coding",
              "description": "Frame one.",
              "app_context": "VS Code",
              "project": "screencap",
              "topics": [],
              "people": [],
              "key_content": "fn build_extraction_prompt",
              "sentiment": "focused"
            },
            {
              "capture_id": 101,
              "activity_type": "reading",
              "description": "Frame two.",
              "app_context": "Safari",
              "project": null,
              "topics": [],
              "people": [],
              "key_content": "SPEC.md",
              "sentiment": "exploring"
            }
          ],
          "batch_summary": {
            "primary_activity": "Debugging parser behavior.",
            "project_context": "Screencap",
            "narrative": "A duplicate capture_id slipped into the model response."
          }
        }"#;

        let error = parse_extraction_response(json).expect_err("duplicate capture ids should fail");

        assert!(error.to_string().contains("duplicate capture_id 101"));
    }

    #[test]
    fn parse_extraction_response_rejects_malformed_json() {
        let error = parse_extraction_response("```json\n{\"frames\":[}\n```")
            .expect_err("malformed json should fail");

        assert!(error
            .to_string()
            .contains("failed to parse extraction response JSON"));
    }

    fn sample_capture(
        id: i64,
        display_id: i64,
        app_name: Option<&str>,
        window_title: Option<&str>,
        bundle_id: Option<&str>,
    ) -> Capture {
        let timestamp = Utc
            .with_ymd_and_hms(2026, 4, 10, 14, id as u32 - 100, 0)
            .single()
            .expect("valid timestamp");

        Capture {
            id,
            timestamp,
            app_name: app_name.map(str::to_owned),
            window_title: window_title.map(str::to_owned),
            bundle_id: bundle_id.map(str::to_owned),
            display_id: Some(display_id),
            screenshot_path: format!("screenshots/2026/04/10/140{:02}-{display_id}.jpg", id - 100),
            extraction_status: ExtractionStatus::Pending,
            extraction_id: None,
            created_at: Utc
                .with_ymd_and_hms(2026, 4, 10, 14, id as u32 - 100, 5)
                .single()
                .expect("valid created_at"),
        }
    }
}
