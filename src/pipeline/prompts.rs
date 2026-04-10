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
