use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::storage::{
    db::StorageDb,
    models::{ActivityQuery, AppCaptureCount, CaptureDetail, InsightType, SearchHit, SearchQuery},
    screenshots::{read_screenshot_file, relative_screenshot_path},
};

const DEFAULT_SEARCH_LIMIT: usize = 25;
const MAX_SEARCH_LIMIT: usize = 200;
const MAX_ACTIVITY_RESULTS: usize = 500;

#[derive(Debug, Clone)]
pub(crate) struct ToolExecutionContext {
    db_path: PathBuf,
    screenshots_root: PathBuf,
}

impl ToolExecutionContext {
    pub(crate) fn new(db_path: PathBuf, screenshots_root: PathBuf) -> Self {
        Self {
            db_path,
            screenshots_root,
        }
    }

    fn open_db_read_only(&self) -> Result<Option<StorageDb>> {
        StorageDb::open_existing_at_path(&self.db_path).with_context(|| {
            format!(
                "failed to open sqlite database at {}",
                self.db_path.display()
            )
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchScreenHistoryArgs {
    query: String,
    from: Option<String>,
    to: Option<String>,
    app: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecentActivityArgs {
    minutes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotArgs {
    id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DailySummaryArgs {
    date: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectActivityArgs {
    project: String,
    from: Option<String>,
    to: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppUsageArgs {
    from: Option<String>,
    to: Option<String>,
}

pub(crate) fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "get_current_context",
            "description": "Return the latest rolling context describing what the user is doing right now.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "search_screen_history",
            "description": "Search extracted screen history and synthesized insights with optional time-range and app filters.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Full-text search query." },
                    "from": { "type": "string", "format": "date-time", "description": "Inclusive UTC start timestamp." },
                    "to": { "type": "string", "format": "date-time", "description": "Inclusive UTC end timestamp." },
                    "app": { "type": "string", "description": "Exact app name filter." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_SEARCH_LIMIT, "description": "Maximum number of results to return." }
                },
                "required": ["query"],
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_recent_activity",
            "description": "Return structured recent activity for the last N minutes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "minutes": { "type": "integer", "minimum": 1, "description": "How many recent minutes to inspect." }
                },
                "required": ["minutes"],
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_screenshot",
            "description": "Return a base64-encoded JPEG for a stored capture id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "minimum": 1, "description": "Capture id." }
                },
                "required": ["id"],
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_daily_summary",
            "description": "Return the daily summary insight for a UTC date.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "date": { "type": "string", "format": "date", "description": "UTC calendar date in YYYY-MM-DD format." }
                },
                "required": ["date"],
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_project_activity",
            "description": "Return structured activity entries for a named project in a time range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Exact extracted project name." },
                    "from": { "type": "string", "format": "date-time", "description": "Inclusive UTC start timestamp." },
                    "to": { "type": "string", "format": "date-time", "description": "Inclusive UTC end timestamp." }
                },
                "required": ["project"],
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_app_usage",
            "description": "Return capture-count-based app usage for a time range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "format": "date-time", "description": "Inclusive UTC start timestamp." },
                    "to": { "type": "string", "format": "date-time", "description": "Inclusive UTC end timestamp." }
                },
                "additionalProperties": false,
            }
        }),
    ]
}

pub(crate) fn call_tool(
    context: &ToolExecutionContext,
    name: &str,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    match name {
        "get_current_context" => call_get_current_context(context),
        "search_screen_history" => {
            let args = parse_args::<SearchScreenHistoryArgs>(arguments, name)?;
            call_search_screen_history(context, args)
        }
        "get_recent_activity" => {
            let args = parse_args::<RecentActivityArgs>(arguments, name)?;
            call_get_recent_activity(context, args)
        }
        "get_screenshot" => {
            let args = parse_args::<ScreenshotArgs>(arguments, name)?;
            call_get_screenshot(context, args)
        }
        "get_daily_summary" => {
            let args = parse_args::<DailySummaryArgs>(arguments, name)?;
            call_get_daily_summary(context, args)
        }
        "get_project_activity" => {
            let args = parse_args::<ProjectActivityArgs>(arguments, name)?;
            call_get_project_activity(context, args)
        }
        "get_app_usage" => {
            let args = parse_args::<AppUsageArgs>(arguments, name)?;
            call_get_app_usage(context, args)
        }
        _ => bail!("unknown tool `{name}`"),
    }
}

fn call_get_current_context(context: &ToolExecutionContext) -> Result<Value> {
    let Some(db) = context.open_db_read_only()? else {
        return tool_json_result(json!({
            "available": false,
            "context": Value::Null,
            "message": "No rolling context is available yet."
        }));
    };

    let insight = db.get_latest_insight_by_type(InsightType::Rolling)?;
    tool_json_result(json!({
        "available": insight.is_some(),
        "context": insight,
    }))
}

fn call_search_screen_history(
    context: &ToolExecutionContext,
    args: SearchScreenHistoryArgs,
) -> Result<Value> {
    let query = required_non_empty(args.query, "query", "search_screen_history")?;
    let from = parse_optional_timestamp(args.from.as_deref(), "from", "search_screen_history")?;
    let to = parse_optional_timestamp(args.to.as_deref(), "to", "search_screen_history")?;
    validate_time_range(from.as_ref(), to.as_ref(), "search_screen_history")?;
    let app = optional_trimmed(args.app);
    let limit = bounded_limit(args.limit, DEFAULT_SEARCH_LIMIT, MAX_SEARCH_LIMIT, "limit")?;

    let Some(db) = context.open_db_read_only()? else {
        return tool_json_result(json!({
            "query": query,
            "from": from,
            "to": to,
            "app": app,
            "limit": limit,
            "result_source": "history",
            "count": 0,
            "results": Vec::<SearchHit>::new(),
        }));
    };

    let results = db.search_history_filtered(&SearchQuery {
        query: query.clone(),
        app_name: app.clone(),
        project: None,
        activity_type: None,
        from,
        to,
        limit,
    })?;

    tool_json_result(json!({
        "query": query,
        "from": from,
        "to": to,
        "app": app,
        "limit": limit,
        "result_source": "history",
        "count": results.len(),
        "results": results,
    }))
}

fn call_get_recent_activity(
    context: &ToolExecutionContext,
    args: RecentActivityArgs,
) -> Result<Value> {
    if args.minutes == 0 {
        bail!("get_recent_activity requires `minutes` to be greater than 0");
    }

    let minutes = i64::try_from(args.minutes).context("recent activity minutes exceed i64")?;
    let window_end = Utc::now();
    let window_start = window_end - ChronoDuration::minutes(minutes);
    let (activities, truncated) = load_activity_window(
        context,
        ActivityQuery {
            from: Some(window_start),
            to: Some(window_end),
            app_name: None,
            project: None,
            limit: MAX_ACTIVITY_RESULTS + 1,
        },
    )?;

    tool_json_result(json!({
        "window_start": window_start,
        "window_end": window_end,
        "minutes": args.minutes,
        "count": activities.len(),
        "truncated": truncated,
        "activities": activities,
    }))
}

fn call_get_screenshot(context: &ToolExecutionContext, args: ScreenshotArgs) -> Result<Value> {
    if args.id <= 0 {
        bail!("get_screenshot requires a positive `id`");
    }

    let Some(db) = context.open_db_read_only()? else {
        return tool_json_result(json!({
            "found": false,
            "capture_id": args.id,
            "message": "No captures are available yet.",
        }));
    };

    let Some(detail) = db.get_capture_detail(args.id)? else {
        return tool_json_result(json!({
            "found": false,
            "capture_id": args.id,
            "message": format!("No capture exists for id {}.", args.id),
        }));
    };

    let relative_path =
        relative_screenshot_path(&context.screenshots_root, &detail.capture.screenshot_path)
            .ok_or_else(|| {
                anyhow!(
                    "capture {} stores an invalid screenshot path",
                    detail.capture.id
                )
            })?;
    let bytes =
        read_screenshot_file(&context.screenshots_root, &relative_path).with_context(|| {
            format!(
                "failed to read screenshot bytes for capture {} from {}",
                detail.capture.id,
                relative_path.display()
            )
        })?;

    tool_json_result(json!({
        "found": true,
        "capture": detail.capture,
        "mime_type": "image/jpeg",
        "data_base64": BASE64_STANDARD.encode(bytes),
    }))
}

fn call_get_daily_summary(context: &ToolExecutionContext, args: DailySummaryArgs) -> Result<Value> {
    let date = parse_required_date(&args.date, "date", "get_daily_summary")?;

    let Some(db) = context.open_db_read_only()? else {
        return tool_json_result(json!({
            "date": date,
            "available": false,
            "summary": Value::Null,
            "message": format!("No daily summary is available for {}.", date),
        }));
    };

    let summary = db.get_latest_daily_insight_for_date(date)?;
    tool_json_result(json!({
        "date": date,
        "available": summary.is_some(),
        "summary": summary,
    }))
}

fn call_get_project_activity(
    context: &ToolExecutionContext,
    args: ProjectActivityArgs,
) -> Result<Value> {
    let project = required_non_empty(args.project, "project", "get_project_activity")?;
    let from = parse_optional_timestamp(args.from.as_deref(), "from", "get_project_activity")?;
    let to = parse_optional_timestamp(args.to.as_deref(), "to", "get_project_activity")?;
    validate_time_range(from.as_ref(), to.as_ref(), "get_project_activity")?;

    let (activities, truncated) = load_activity_window(
        context,
        ActivityQuery {
            from,
            to,
            app_name: None,
            project: Some(project.clone()),
            limit: MAX_ACTIVITY_RESULTS + 1,
        },
    )?;

    tool_json_result(json!({
        "project": project,
        "from": from,
        "to": to,
        "count": activities.len(),
        "truncated": truncated,
        "activities": activities,
    }))
}

fn call_get_app_usage(context: &ToolExecutionContext, args: AppUsageArgs) -> Result<Value> {
    let from = parse_optional_timestamp(args.from.as_deref(), "from", "get_app_usage")?;
    let to = parse_optional_timestamp(args.to.as_deref(), "to", "get_app_usage")?;
    validate_time_range(from.as_ref(), to.as_ref(), "get_app_usage")?;

    let Some(db) = context.open_db_read_only()? else {
        return tool_json_result(json!({
            "from": from,
            "to": to,
            "usage_basis": "capture_count",
            "apps": Vec::<AppCaptureCount>::new(),
        }));
    };

    let apps = db.list_app_capture_counts_in_range(from, to)?;
    tool_json_result(json!({
        "from": from,
        "to": to,
        "usage_basis": "capture_count",
        "apps": apps,
    }))
}

fn load_activity_window(
    context: &ToolExecutionContext,
    mut query: ActivityQuery,
) -> Result<(Vec<CaptureDetail>, bool)> {
    let Some(db) = context.open_db_read_only()? else {
        return Ok((Vec::new(), false));
    };

    query.limit = query.limit.max(1);
    let mut activities = db.list_activity_details(&query)?;
    let truncated = activities.len() > MAX_ACTIVITY_RESULTS;
    if truncated {
        activities.truncate(MAX_ACTIVITY_RESULTS);
    }
    Ok((activities, truncated))
}

fn parse_args<T>(arguments: &Map<String, Value>, tool_name: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(Value::Object(arguments.clone()))
        .with_context(|| format!("{tool_name} received invalid arguments"))
}

fn required_non_empty(raw: String, field: &str, tool_name: &str) -> Result<String> {
    let value = raw.trim();
    if value.is_empty() {
        bail!("{tool_name} requires a non-empty `{field}`");
    }

    Ok(value.to_owned())
}

fn optional_trimmed(raw: Option<String>) -> Option<String> {
    raw.and_then(|value| {
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_owned())
        }
    })
}

fn bounded_limit(value: Option<usize>, default: usize, max: usize, field: &str) -> Result<usize> {
    match value {
        Some(0) => bail!("`{field}` must be greater than 0"),
        Some(value) => Ok(value.min(max)),
        None => Ok(default),
    }
}

fn parse_optional_timestamp(
    raw: Option<&str>,
    field: &str,
    tool_name: &str,
) -> Result<Option<DateTime<Utc>>> {
    raw.map(|value| parse_timestamp(value, field, tool_name))
        .transpose()
}

fn parse_timestamp(raw: &str, field: &str, tool_name: &str) -> Result<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .with_context(|| format!("{tool_name} expected `{field}` to be an RFC3339 timestamp"))
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn parse_required_date(raw: &str, field: &str, tool_name: &str) -> Result<NaiveDate> {
    let value = raw.trim();
    if value.is_empty() {
        bail!("{tool_name} requires a non-empty `{field}`");
    }

    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("{tool_name} expected `{field}` to be YYYY-MM-DD"))
}

fn validate_time_range(
    from: Option<&DateTime<Utc>>,
    to: Option<&DateTime<Utc>>,
    tool_name: &str,
) -> Result<()> {
    if let (Some(from), Some(to)) = (from, to) {
        if from > to {
            bail!("{tool_name} requires `from` to be less than or equal to `to`");
        }
    }

    Ok(())
}

fn tool_json_result(value: Value) -> Result<Value> {
    let text = serde_json::to_string(&value).context("failed to serialize tool output")?;
    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": text,
            }
        ],
        "isError": false,
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use chrono::TimeZone;
    use uuid::Uuid;

    use crate::storage::models::{
        ActivityType, ExtractionStatus, InsightData, NewCapture, NewExtraction, NewInsight,
    };

    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        env::temp_dir().join(format!("screencap-mcp-tools-tests-{name}-{unique}"))
    }

    #[test]
    fn get_current_context_returns_latest_rolling_insight() {
        let root = temp_path("rolling-context");
        fs::create_dir_all(&root).expect("create temp directory");
        let db_path = root.join("screencap.db");
        let screenshots_root = root.join("screenshots");
        fs::create_dir_all(&screenshots_root).expect("create screenshots root");

        {
            let mut db = StorageDb::open_at_path(&db_path).expect("open sqlite db");
            let window_start = Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
            let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 14, 30, 0).unwrap();
            db.insert_insight(&NewInsight {
                insight_type: InsightType::Rolling,
                window_start,
                window_end,
                data: InsightData::Rolling {
                    window_start,
                    window_end,
                    current_focus: "Debugging the MCP transport".into(),
                    active_project: Some("screencap".into()),
                    apps_used: [("Terminal".to_string(), "20 min".to_string())]
                        .into_iter()
                        .collect(),
                    context_switches: 1,
                    mood: "focused".into(),
                    summary: "Working through the MCP tool surface.".into(),
                },
                model_used: Some("mock-model".into()),
                tokens_used: Some(21),
                cost_cents: Some(0.1),
            })
            .expect("insert rolling insight");
        }

        let response = call_tool(
            &ToolExecutionContext::new(db_path, screenshots_root),
            "get_current_context",
            &Map::new(),
        )
        .expect("tool call should succeed");

        let payload: Value = serde_json::from_str(
            response["content"][0]["text"]
                .as_str()
                .expect("tool payload should be text"),
        )
        .expect("tool payload should be valid JSON");

        assert_eq!(payload["available"], Value::Bool(true));
        assert_eq!(
            payload["context"]["data"]["current_focus"],
            "Debugging the MCP transport"
        );
    }

    #[test]
    fn get_screenshot_returns_base64_payload() {
        let root = temp_path("screenshot");
        let screenshots_root = root.join("screenshots");
        fs::create_dir_all(&screenshots_root).expect("create screenshots root");
        let screenshot_path = screenshots_root.join("2026/04/10/140000-0.jpg");
        fs::create_dir_all(screenshot_path.parent().unwrap()).expect("create screenshot parent");
        fs::write(&screenshot_path, b"fake-jpeg").expect("write screenshot fixture");
        let db_path = root.join("screencap.db");

        let capture_id = {
            let mut db = StorageDb::open_at_path(&db_path).expect("open sqlite db");
            let capture = db
                .insert_capture(&NewCapture {
                    timestamp: Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap(),
                    app_name: Some("Terminal".into()),
                    window_title: Some("fixture".into()),
                    bundle_id: Some("com.apple.Terminal".into()),
                    display_id: Some(0),
                    screenshot_path: screenshot_path.to_string_lossy().into_owned(),
                })
                .expect("insert capture");
            capture.id
        };

        let response = call_tool(
            &ToolExecutionContext::new(db_path, screenshots_root),
            "get_screenshot",
            &Map::from_iter([(String::from("id"), json!(capture_id))]),
        )
        .expect("tool call should succeed");

        let payload: Value = serde_json::from_str(
            response["content"][0]["text"]
                .as_str()
                .expect("tool payload should be text"),
        )
        .expect("tool payload should be valid JSON");

        assert_eq!(payload["found"], Value::Bool(true));
        assert_eq!(payload["mime_type"], "image/jpeg");
        assert_eq!(payload["data_base64"], BASE64_STANDARD.encode("fake-jpeg"));
    }

    #[test]
    fn get_project_activity_filters_by_project() {
        let root = temp_path("project-activity");
        fs::create_dir_all(&root).expect("create temp directory");
        let db_path = root.join("screencap.db");
        let screenshots_root = root.join("screenshots");
        fs::create_dir_all(&screenshots_root).expect("create screenshots root");

        {
            let mut db = StorageDb::open_at_path(&db_path).expect("open sqlite db");
            let batch_id = Uuid::new_v4();
            let capture = db
                .insert_capture(&NewCapture {
                    timestamp: Utc.with_ymd_and_hms(2026, 4, 10, 15, 0, 0).unwrap(),
                    app_name: Some("Code".into()),
                    window_title: Some("screencap".into()),
                    bundle_id: Some("com.microsoft.VSCode".into()),
                    display_id: Some(0),
                    screenshot_path: root
                        .join("screenshots/fixture.jpg")
                        .to_string_lossy()
                        .into_owned(),
                })
                .expect("insert capture");
            let extraction = db
                .insert_extraction(&NewExtraction {
                    capture_id: capture.id,
                    batch_id,
                    activity_type: Some(ActivityType::Coding),
                    description: Some("Implementing MCP project activity".into()),
                    app_context: Some("Rust source".into()),
                    project: Some("screencap".into()),
                    topics: vec!["mcp".into()],
                    people: Vec::new(),
                    key_content: Some("get_project_activity".into()),
                    sentiment: None,
                })
                .expect("insert extraction");
            db.update_capture_status(capture.id, ExtractionStatus::Processed, Some(extraction.id))
                .expect("update status");
        }

        let response = call_tool(
            &ToolExecutionContext::new(db_path, screenshots_root),
            "get_project_activity",
            &Map::from_iter([(String::from("project"), json!("screencap"))]),
        )
        .expect("tool call should succeed");

        let payload: Value = serde_json::from_str(
            response["content"][0]["text"]
                .as_str()
                .expect("tool payload should be text"),
        )
        .expect("tool payload should be valid JSON");

        assert_eq!(payload["project"], "screencap");
        assert_eq!(payload["count"], 1);
        assert_eq!(
            payload["activities"][0]["extraction"]["project"],
            "screencap"
        );
    }
}
