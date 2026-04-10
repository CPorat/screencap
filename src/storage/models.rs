use std::{collections::BTreeMap, fmt, str::FromStr};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractionStatus {
    Pending,
    Processed,
    Failed,
}

impl ExtractionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processed => "processed",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for ExtractionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ExtractionStatus {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "processed" => Ok(Self::Processed),
            "failed" => Ok(Self::Failed),
            other => Err(anyhow!("unsupported extraction status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

impl ActivityType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Browsing => "browsing",
            Self::Communication => "communication",
            Self::Reading => "reading",
            Self::Writing => "writing",
            Self::Design => "design",
            Self::Terminal => "terminal",
            Self::Meeting => "meeting",
            Self::Media => "media",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for ActivityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ActivityType {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "coding" => Ok(Self::Coding),
            "browsing" => Ok(Self::Browsing),
            "communication" => Ok(Self::Communication),
            "reading" => Ok(Self::Reading),
            "writing" => Ok(Self::Writing),
            "design" => Ok(Self::Design),
            "terminal" => Ok(Self::Terminal),
            "meeting" => Ok(Self::Meeting),
            "media" => Ok(Self::Media),
            "other" => Ok(Self::Other),
            other => Err(anyhow!("unsupported activity type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sentiment {
    Focused,
    Exploring,
    Communicating,
    Idle,
    ContextSwitching,
}

impl Sentiment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focused => "focused",
            Self::Exploring => "exploring",
            Self::Communicating => "communicating",
            Self::Idle => "idle",
            Self::ContextSwitching => "context-switching",
        }
    }
}

impl fmt::Display for Sentiment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Sentiment {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "focused" => Ok(Self::Focused),
            "exploring" => Ok(Self::Exploring),
            "communicating" => Ok(Self::Communicating),
            "idle" => Ok(Self::Idle),
            "context-switching" => Ok(Self::ContextSwitching),
            other => Err(anyhow!("unsupported sentiment: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InsightType {
    Rolling,
    Hourly,
    Daily,
}

impl InsightType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rolling => "rolling",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
        }
    }
}

impl fmt::Display for InsightType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for InsightType {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "rolling" => Ok(Self::Rolling),
            "hourly" => Ok(Self::Hourly),
            "daily" => Ok(Self::Daily),
            other => Err(anyhow!("unsupported insight type: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub bundle_id: Option<String>,
    pub display_id: Option<i64>,
    pub screenshot_path: String,
    pub extraction_status: ExtractionStatus,
    pub extraction_id: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewCapture {
    pub timestamp: DateTime<Utc>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub bundle_id: Option<String>,
    pub display_id: Option<i64>,
    pub screenshot_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extraction {
    pub id: i64,
    pub capture_id: i64,
    pub batch_id: Uuid,
    pub activity_type: Option<ActivityType>,
    pub description: Option<String>,
    pub app_context: Option<String>,
    pub project: Option<String>,
    pub topics: Vec<String>,
    pub people: Vec<String>,
    pub key_content: Option<String>,
    pub sentiment: Option<Sentiment>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewExtraction {
    pub capture_id: i64,
    pub batch_id: Uuid,
    pub activity_type: Option<ActivityType>,
    pub description: Option<String>,
    pub app_context: Option<String>,
    pub project: Option<String>,
    pub topics: Vec<String>,
    pub people: Vec<String>,
    pub key_content: Option<String>,
    pub sentiment: Option<Sentiment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionBatch {
    pub id: Uuid,
    pub batch_start: DateTime<Utc>,
    pub batch_end: DateTime<Utc>,
    pub capture_count: i64,
    pub primary_activity: Option<String>,
    pub project_context: Option<String>,
    pub narrative: Option<String>,
    pub raw_response: Option<String>,
    pub model_used: Option<String>,
    pub tokens_used: Option<i64>,
    pub cost_cents: Option<f64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewExtractionBatch {
    pub id: Uuid,
    pub batch_start: DateTime<Utc>,
    pub batch_end: DateTime<Utc>,
    pub capture_count: i64,
    pub primary_activity: Option<String>,
    pub project_context: Option<String>,
    pub narrative: Option<String>,
    pub raw_response: Option<String>,
    pub model_used: Option<String>,
    pub tokens_used: Option<i64>,
    pub cost_cents: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Insight {
    pub id: i64,
    pub insight_type: InsightType,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub data: InsightData,
    pub narrative: String,
    pub model_used: Option<String>,
    pub tokens_used: Option<i64>,
    pub cost_cents: Option<f64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewInsight {
    pub insight_type: InsightType,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub data: InsightData,
    pub model_used: Option<String>,
    pub tokens_used: Option<i64>,
    pub cost_cents: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InsightData {
    Rolling {
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        current_focus: String,
        active_project: Option<String>,
        apps_used: BTreeMap<String, String>,
        context_switches: u32,
        mood: String,
        summary: String,
    },
    Hourly {
        hour_start: DateTime<Utc>,
        hour_end: DateTime<Utc>,
        dominant_activity: String,
        projects: Vec<HourlyProjectSummary>,
        topics: Vec<String>,
        people_interacted: Vec<String>,
        key_moments: Vec<String>,
        focus_score: f64,
        narrative: String,
    },
    Daily {
        date: NaiveDate,
        total_active_hours: f64,
        projects: Vec<DailyProjectSummary>,
        time_allocation: BTreeMap<String, String>,
        focus_blocks: Vec<FocusBlock>,
        open_threads: Vec<String>,
        narrative: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HourlyProjectSummary {
    pub name: Option<String>,
    pub minutes: u32,
    pub activities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyProjectSummary {
    pub name: String,
    pub total_minutes: u32,
    pub activities: Vec<String>,
    pub key_accomplishments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusBlock {
    pub start: String,
    pub end: String,
    pub duration_min: u32,
    pub project: String,
    pub quality: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureDetail {
    pub capture: Capture,
    pub extraction: Option<Extraction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCaptureCount {
    pub app_name: String,
    pub capture_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub app_name: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionSearchHit {
    pub extraction: Extraction,
    pub batch_narrative: Option<String>,
    pub rank: f64,
}

impl InsightData {
    pub fn insight_type(&self) -> InsightType {
        match self {
            Self::Rolling { .. } => InsightType::Rolling,
            Self::Hourly { .. } => InsightType::Hourly,
            Self::Daily { .. } => InsightType::Daily,
        }
    }

    pub fn narrative_text(&self) -> &str {
        match self {
            Self::Rolling { summary, .. } => summary,
            Self::Hourly { narrative, .. } => narrative,
            Self::Daily { narrative, .. } => narrative,
        }
    }
}

pub(crate) fn format_db_timestamp(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

pub(crate) fn parse_db_timestamp(raw: &str) -> Result<DateTime<Utc>> {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(raw) {
        return Ok(timestamp.with_timezone(&Utc));
    }

    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .map(|timestamp| timestamp.and_utc())
        .with_context(|| format!("failed to parse database timestamp `{raw}`"))
}

pub(crate) fn encode_string_list(values: &[String]) -> Result<String> {
    serde_json::to_string(values).context("failed to encode string list as JSON")
}

pub(crate) fn decode_string_list(raw: Option<String>) -> Result<Vec<String>> {
    match raw {
        None => Ok(Vec::new()),
        Some(value) if value.trim().is_empty() => Ok(Vec::new()),
        Some(value) => serde_json::from_str(&value)
            .with_context(|| format!("failed to parse JSON string list `{value}`")),
    }
}
