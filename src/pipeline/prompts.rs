use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{config::AppConfig, storage::models::Extraction};

pub const DEFAULT_EXTRACTION_PROMPT: &str = r#"You are analyzing a batch of sequential screenshots from a user's computer.
For each screenshot, extract structured data. Then provide a batch summary.

Return JSON in this exact format:
{
  "frames": [
    {
      "capture_id": 123,
      "activity_type": "coding" | "browsing" | "communication" | "reading" | "writing" | "design" | "terminal" | "meeting" | "media" | "other",
      "description": "One sentence: what the user is doing in this frame",
      "app_context": "What the app is being used for specifically",
      "project": "Project or repo name if identifiable, null otherwise",
      "topics": ["typescript", "authentication", "JWT"],
      "people": ["@alice in Slack"],
      "key_content": "Most important visible text (code snippet, message, heading, URL)",
      "sentiment": "focused" | "exploring" | "communicating" | "idle" | "context-switching"
    }
  ],
  "batch_summary": {
    "primary_activity": "What the user was mainly doing across this batch",
    "project_context": "Which project(s) they were working on",
    "narrative": "2-3 sentence natural language summary of this time period"
  }
}

Return one frame entry for every attached screenshot in the same order.
Use each provided capture_id exactly as given in the frame metadata below."#;

pub const DEFAULT_ROLLING_PROMPT: &str = r#"You are synthesizing a rolling context summary from structured screenshot extractions.
Your job is to answer: what is the user working on right now?

Return JSON in this exact format:
{
  "type": "rolling",
  "window_start": "2026-04-10T14:00:00Z",
  "window_end": "2026-04-10T14:30:00Z",
  "current_focus": "Debugging JWT token refresh in the screencap auth module",
  "active_project": "screencap",
  "apps_used": {"VS Code": "18 min", "Chrome": "8 min", "Slack": "4 min"},
  "context_switches": 3,
  "mood": "deep-focus",
  "summary": "Focused coding session on the auth module. Looked up JWT refresh token patterns on Stack Overflow, then implemented the fix in VS Code. Brief Slack check."
}

Use the exact window_start and window_end values provided in the request metadata below.
Base the summary only on the extraction batches and frame details from that window.
Return JSON only; do not wrap it in markdown or add commentary."#;

pub const DEFAULT_HOURLY_PROMPT: &str = r#"You are synthesizing an hourly digest from structured screenshot extraction batches.
Your job is to summarize the last completed hour as a durable record for later daily summaries.

Return JSON in this exact format:
{
  "type": "hourly",
  "hour_start": "2026-04-10T14:00:00Z",
  "hour_end": "2026-04-10T15:00:00Z",
  "dominant_activity": "coding",
  "projects": [
    {"name": "screencap", "minutes": 42, "activities": ["debugging auth", "writing tests"]},
    {"name": null, "minutes": 18, "activities": ["Slack conversations", "email triage"]}
  ],
  "topics": ["JWT", "authentication", "Rust FFI", "team standup"],
  "people_interacted": ["@alice", "@bob"],
  "key_moments": [
    "Found the JWT refresh bug — was using wrong expiry field",
    "Discussed deployment timeline with Alice in Slack"
  ],
  "focus_score": 0.72,
  "narrative": "Productive coding hour. First 40 minutes deep in the auth module fixing a JWT refresh token bug. Found the issue and wrote a fix plus tests. Last 20 minutes was communication."
}

Use the exact hour_start and hour_end values provided in the request metadata below.
Base the digest only on the extraction batches and frame details from that window.
Return JSON only; do not wrap it in markdown or add commentary."#;

pub const DEFAULT_DAILY_PROMPT: &str = r#"You are synthesizing a daily summary from previously generated hourly digests.
Your job is to summarize the user's day so far as a durable record for review and export.

Return JSON in this exact format:
{
  "type": "daily",
  "date": "2026-04-10",
  "total_active_hours": 7.5,
  "projects": [
    {
      "name": "screencap",
      "total_minutes": 195,
      "activities": ["auth module debugging", "capture pipeline", "test writing"],
      "key_accomplishments": ["Fixed JWT refresh bug", "Added multi-monitor support"]
    },
    {
      "name": "admin",
      "total_minutes": 85,
      "activities": ["email", "Slack", "standup meeting"],
      "key_accomplishments": ["Aligned on Q2 deployment timeline with team"]
    }
  ],
  "time_allocation": {
    "coding": "3h 15m",
    "communication": "1h 25m",
    "browsing_research": "1h 10m",
    "design": "0h 45m",
    "meetings": "0h 30m",
    "other": "0h 25m"
  },
  "focus_blocks": [
    {"start": "09:15", "end": "11:45", "duration_min": 150, "project": "screencap", "quality": "deep-focus"},
    {"start": "14:00", "end": "15:30", "duration_min": 90, "project": "screencap", "quality": "moderate-focus"}
  ],
  "open_threads": [
    "Need to finish the multi-monitor edge case for ultrawide displays",
    "Alice asked about the API auth docs — haven't responded yet"
  ],
  "narrative": "Productive day focused on screencap. Two solid focus blocks: morning session on the capture pipeline and afternoon on auth. Communication was light and several follow-ups remain open."
}

Use the exact date provided in the request metadata below.
Base the summary only on the hourly digests from that date.
Return JSON only; do not wrap it in markdown or add commentary."#;

pub const DEFAULT_SEMANTIC_SEARCH_PROMPT: &str = r#"You are an assistant that answers user questions about recent computer activity.
Use ONLY the provided extraction records as evidence.
If the answer is not supported by the records, say you do not know.

Return JSON in this exact format:
{
  "answer": "Direct answer grounded in the provided records",
  "capture_ids": [123, 456]
}

Rules:
- `capture_ids` must contain only capture IDs from the provided extraction records.
- Rank `capture_ids` from most relevant to least relevant.
- Do not include IDs that are not present in the provided records.
- Return JSON only; no markdown or commentary."#;

pub const DEFAULT_PROMPT_FILES: [(&str, &str); 5] = [
    ("extraction.txt", DEFAULT_EXTRACTION_PROMPT),
    ("rolling.txt", DEFAULT_ROLLING_PROMPT),
    ("hourly.txt", DEFAULT_HOURLY_PROMPT),
    ("daily.txt", DEFAULT_DAILY_PROMPT),
    ("semantic_search.txt", DEFAULT_SEMANTIC_SEARCH_PROMPT),
];

pub fn load_extraction_prompt_template() -> String {
    load_prompt_template("extraction.txt", DEFAULT_EXTRACTION_PROMPT)
}

pub fn load_rolling_prompt_template() -> String {
    load_prompt_template("rolling.txt", DEFAULT_ROLLING_PROMPT)
}

pub fn load_hourly_prompt_template() -> String {
    load_prompt_template("hourly.txt", DEFAULT_HOURLY_PROMPT)
}

pub fn load_daily_prompt_template() -> String {
    load_prompt_template("daily.txt", DEFAULT_DAILY_PROMPT)
}

pub fn load_semantic_search_prompt_template() -> String {
    load_prompt_template("semantic_search.txt", DEFAULT_SEMANTIC_SEARCH_PROMPT)
}

pub fn build_semantic_search_prompt(query: &str, extractions: &[Extraction]) -> String {
    let semantic_prompt = load_semantic_search_prompt_template();
    let mut prompt = String::with_capacity(semantic_prompt.len() + extractions.len() * 320);
    prompt.push_str(&semantic_prompt);
    prompt.push_str("\n\nUser query:\n");
    prompt.push_str(query.trim());
    prompt.push_str("\n\nExtraction records:\n");

    if extractions.is_empty() {
        prompt.push_str("- none\n");
    }

    for extraction in extractions {
        prompt.push_str(&format!(
            concat!(
                "- capture_id: {capture_id}\n",
                "  extraction_id: {extraction_id}\n",
                "  activity_type: {activity_type}\n",
                "  description: {description}\n",
                "  app_context: {app_context}\n",
                "  project: {project}\n",
                "  topics: {topics}\n",
                "  people: {people}\n",
                "  key_content: {key_content}\n",
                "  sentiment: {sentiment}\n"
            ),
            capture_id = extraction.capture_id,
            extraction_id = extraction.id,
            activity_type = extraction
                .activity_type
                .map(|value| value.as_str())
                .unwrap_or("unknown"),
            description = extraction.description.as_deref().unwrap_or(""),
            app_context = extraction.app_context.as_deref().unwrap_or(""),
            project = extraction.project.as_deref().unwrap_or("null"),
            topics = format_string_list(&extraction.topics),
            people = format_string_list(&extraction.people),
            key_content = extraction.key_content.as_deref().unwrap_or(""),
            sentiment = extraction.sentiment.map(|value| value.as_str()).unwrap_or("unknown"),
        ));
    }

    prompt
}

fn load_prompt_template(file_name: &str, fallback: &str) -> String {
    let Ok(home) = AppConfig::home_dir() else {
        return fallback.to_owned();
    };

    load_prompt_template_from_home(&home, file_name, fallback)
}

fn load_prompt_template_from_home(home: &Path, file_name: &str, fallback: &str) -> String {
    let path: PathBuf = AppConfig::prompts_dir(home).join(file_name);
    fs::read_to_string(path).unwrap_or_else(|_| fallback.to_owned())
}

fn format_string_list(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use super::*;

    fn temp_home_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("screencap-prompts-tests-{name}-{unique}"))
    }

    #[test]
    fn build_semantic_search_prompt_includes_query_and_extractions() {
        let prompt = build_semantic_search_prompt(
            "what changed in auth?",
            &[Extraction {
                id: 7,
                capture_id: 42,
                batch_id: Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
                activity_type: Some(crate::storage::models::ActivityType::Coding),
                description: Some("Debugged refresh token flow".into()),
                app_context: Some("Editing src/auth.rs".into()),
                project: Some("screencap".into()),
                topics: vec!["jwt".into(), "auth".into()],
                people: vec!["@alice".into()],
                key_content: Some("refresh_session".into()),
                sentiment: Some(crate::storage::models::Sentiment::Focused),
                created_at: Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap(),
            }],
        );

        assert!(prompt.contains("what changed in auth?"));
        assert!(prompt.contains("capture_id: 42"));
        assert!(prompt.contains("description: Debugged refresh token flow"));
        assert!(prompt.contains("topics: [\"jwt\",\"auth\"]"));
    }

    #[test]
    fn load_prompt_template_from_home_reads_custom_prompt() {
        let home = temp_home_root("custom");
        let prompts_dir = AppConfig::prompts_dir(&home);
        fs::create_dir_all(&prompts_dir).expect("create prompts dir");
        let custom_prompt = "custom semantic prompt";
        fs::write(prompts_dir.join("semantic_search.txt"), custom_prompt).expect("write prompt");

        let loaded = load_prompt_template_from_home(
            &home,
            "semantic_search.txt",
            DEFAULT_SEMANTIC_SEARCH_PROMPT,
        );

        assert_eq!(loaded, custom_prompt);
        fs::remove_dir_all(&home).expect("cleanup temp home");
    }

    #[test]
    fn load_prompt_template_from_home_falls_back_to_default() {
        let home = temp_home_root("fallback");

        let loaded = load_prompt_template_from_home(
            &home,
            "semantic_search.txt",
            DEFAULT_SEMANTIC_SEARCH_PROMPT,
        );

        assert_eq!(loaded, DEFAULT_SEMANTIC_SEARCH_PROMPT);
    }
}
