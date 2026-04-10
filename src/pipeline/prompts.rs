pub const EXTRACTION_PROMPT_TEMPLATE: &str = r#"You are analyzing a batch of sequential screenshots from a user's computer.
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

pub const ROLLING_CONTEXT_PROMPT_TEMPLATE: &str = r#"You are synthesizing a rolling context summary from structured screenshot extractions.
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

pub const HOURLY_DIGEST_PROMPT_TEMPLATE: &str = r#"You are synthesizing an hourly digest from structured screenshot extraction batches.
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

pub const DAILY_SUMMARY_PROMPT_TEMPLATE: &str = r#"You are synthesizing a daily summary from previously generated hourly digests.
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
