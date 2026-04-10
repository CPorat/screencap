//! MCP tool definitions

/// MCP tool: get current context
pub fn tool_get_current_context() -> &'static str {
    r#"{"name": "get_current_context", "description": "What is the user doing right now?"}"#
}

/// MCP tool: search screen history
pub fn tool_search_screen_history() -> &'static str {
    r#"{"name": "search_screen_history", "description": "Search across extractions and insights with filters"}"#
}

/// MCP tool: get recent activity
pub fn tool_get_recent_activity() -> &'static str {
    r#"{"name": "get_recent_activity", "description": "Get last N minutes of structured activity data"}"#
}

/// MCP tool: get screenshot
pub fn tool_get_screenshot() -> &'static str {
    r#"{"name": "get_screenshot", "description": "Retrieve a specific screenshot by ID (returns base64)"}"#
}

/// MCP tool: get daily summary
pub fn tool_get_daily_summary() -> &'static str {
    r#"{"name": "get_daily_summary", "description": "Get the daily summary for a given date"}"#
}

/// MCP tool: get project activity
pub fn tool_get_project_activity() -> &'static str {
    r#"{"name": "get_project_activity", "description": "Get all activity for a named project in a time range"}"#
}

/// MCP tool: get app usage
pub fn tool_get_app_usage() -> &'static str {
    r#"{"name": "get_app_usage", "description": "Time spent in each app over a period"}"#
}

/// MCP tool: ask about activity
pub fn tool_ask_about_activity() -> &'static str {
    r#"{"name": "ask_about_activity", "description": "Free-form question about activity"}"#
}
