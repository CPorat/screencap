//! Rust structs matching database schema

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Raw capture record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capture {
    pub id: Option<i64>,
    pub timestamp: DateTime<Utc>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub bundle_id: Option<String>,
    pub display_id: Option<i32>,
    pub screenshot_path: String,
    pub extraction_status: String,
    pub extraction_id: Option<i64>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Structured extraction record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extraction {
    pub id: Option<i64>,
    pub capture_id: i64,
    pub batch_id: String,
    pub activity_type: Option<String>,
    pub description: Option<String>,
    pub app_context: Option<String>,
    pub project: Option<String>,
    pub topics: Option<Vec<String>>,
    pub people: Option<Vec<String>>,
    pub key_content: Option<String>,
    pub sentiment: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Extraction batch record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionBatch {
    pub id: String,
    pub batch_start: DateTime<Utc>,
    pub batch_end: DateTime<Utc>,
    pub capture_count: Option<i32>,
    pub primary_activity: Option<String>,
    pub project_context: Option<String>,
    pub narrative: Option<String>,
    pub raw_response: Option<String>,
    pub model_used: Option<String>,
    pub tokens_used: Option<i64>,
    pub cost_cents: Option<f64>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Insight record (rolling, hourly, or daily)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub id: Option<i64>,
    pub insight_type: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub data: String,
    pub narrative: Option<String>,
    pub model_used: Option<String>,
    pub tokens_used: Option<i64>,
    pub cost_cents: Option<f64>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Activity type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActivityType {
    Coding,
    Browsing,
    Communication,
    Reading,
    Writing,
    Design,
    Terminal,
    Meeting,
    Media,
    Other,
}

/// Sentiment enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Sentiment {
    Focused,
    Exploring,
    Communicating,
    Idle,
    ContextSwitching,
}
