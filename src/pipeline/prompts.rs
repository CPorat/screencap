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
