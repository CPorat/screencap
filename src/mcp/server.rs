use std::{
    collections::BTreeMap,
    env,
    io::{self, BufRead, BufReader, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::{
    config::AppConfig,
    storage::{
        db::StorageDb,
        models::{Capture, Extraction, ExtractionSearchQuery, InsightType},
    },
};

const PROTOCOL_VERSION: &str = "2024-11-05";
const DEFAULT_SEARCH_LIMIT: usize = 20;
const MAX_SEARCH_LIMIT: usize = 200;

pub fn run_stdio_server(config: AppConfig) -> Result<()> {
    let home = runtime_home_dir()?;
    let server = McpServer::new(config, home);

    let stdin = io::stdin();
    let stdout = io::stdout();
    server.serve(BufReader::new(stdin.lock()), stdout.lock())
}

struct McpServer {
    storage_root: PathBuf,
    db_path: PathBuf,
}

#[derive(Debug)]
struct RpcError {
    id: Value,
    code: i64,
    message: String,
}

#[derive(Debug, Serialize)]
struct AppUsage {
    app_name: String,
    capture_count: u64,
}

#[derive(Debug, Serialize)]
struct SearchHitOutput {
    capture: Capture,
    extraction: Extraction,
    batch_narrative: Option<String>,
    rank: f64,
}

impl RpcError {
    fn new(id: Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            id,
            code,
            message: message.into(),
        }
    }
}

impl McpServer {
    fn new(config: AppConfig, home: PathBuf) -> Self {
        let storage_root = config.storage_root(&home);
        let db_path = storage_root.join("screencap.db");
        Self {
            storage_root,
            db_path,
        }
    }

    fn serve<R: BufRead, W: Write>(&self, mut reader: R, mut writer: W) -> Result<()> {
        let mut line = String::new();

        while let Some(payload) = read_next_message(&mut reader, &mut line)? {
            let request = match serde_json::from_str::<Value>(&payload) {
                Ok(request) => request,
                Err(error) => {
                    let response = error_response(Value::Null, -32700, format!("parse error: {error}"));
                    write_response(&mut writer, &response)?;
                    continue;
                }
            };

            match self.process_request(&request) {
                Ok(Some(response)) => write_response(&mut writer, &response)?,
                Ok(None) => {}
                Err(error) => {
                    let response = error_response(error.id, error.code, error.message);
                    write_response(&mut writer, &response)?;
                }
            }
        }

        Ok(())
    }

    fn process_request(&self, request: &Value) -> Result<Option<Value>, RpcError> {
        let Some(request_object) = request.as_object() else {
            return Err(RpcError::new(Value::Null, -32600, "request must be a JSON object"));
        };

        let id = request_object.get("id").cloned().unwrap_or(Value::Null);
        let method = request_object
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(id.clone(), -32600, "missing JSON-RPC method"))?;

        if method == "notifications/initialized" {
            return Ok(None);
        }

        if id.is_null() {
            return Ok(None);
        }

        let params = request_object.get("params");

        let result = match method {
            "initialize" => self.handle_initialize(),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => {
                let params = params
                    .and_then(Value::as_object)
                    .ok_or_else(|| RpcError::new(id.clone(), -32602, "tools/call requires params object"))?;
                self.handle_tools_call(id.clone(), params)
            }
            _ => Err(RpcError::new(
                id.clone(),
                -32601,
                format!("method `{method}` not found"),
            )),
        }?;

        Ok(Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        })))
    }

    fn handle_initialize(&self) -> Result<Value, RpcError> {
        Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "screencap",
                "version": env!("CARGO_PKG_VERSION"),
            }
        }))
    }

    fn handle_tools_list(&self) -> Result<Value, RpcError> {
        Ok(json!({ "tools": tool_definitions() }))
    }

    fn handle_tools_call(&self, request_id: Value, params: &Map<String, Value>) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(request_id.clone(), -32602, "tools/call requires a tool name"))?;

        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(object)) => object.clone(),
            Some(_) => {
                return Err(RpcError::new(
                    request_id.clone(),
                    -32602,
                    "tools/call arguments must be a JSON object",
                ))
            }
        };

        let output = match name {
            "get_current_context" => self.tool_get_current_context(),
            "search_screen_history" => self.tool_search_screen_history(&arguments),
            "get_recent_activity" => self.tool_get_recent_activity(&arguments),
            "get_screenshot" => self.tool_get_screenshot(&arguments),
            "get_daily_summary" => self.tool_get_daily_summary(&arguments),
            "get_project_activity" => self.tool_get_project_activity(&arguments),
            "get_app_usage" => self.tool_get_app_usage(&arguments),
            _ => return Err(RpcError::new(request_id.clone(), -32601, format!("unknown tool `{name}`"))),
        }
        .map_err(|error| RpcError::new(request_id.clone(), -32000, error.to_string()))?;

        wrap_tool_result(output)
            .map_err(|error| RpcError::new(request_id, -32000, error.to_string()))
    }

    fn tool_get_current_context(&self) -> Result<Value> {
        let Some(db) = self.open_db()? else {
            return Ok(json!({ "insight": Value::Null }));
        };

        let insight = db.get_latest_insight_by_type(InsightType::Rolling)?;
        Ok(json!({ "insight": insight }))
    }

    fn tool_search_screen_history(&self, args: &Map<String, Value>) -> Result<Value> {
        let query = required_string(args, "query")?;
        if query.trim().is_empty() {
            bail!("search query must not be empty");
        }

        let from = optional_datetime(args, "from")?;
        let to = optional_datetime(args, "to")?;
        let app = optional_non_empty_string(args, "app")?;
        let limit = optional_usize(args, "limit")?.unwrap_or(DEFAULT_SEARCH_LIMIT).min(MAX_SEARCH_LIMIT);

        let Some(db) = self.open_db()? else {
            return Ok(json!({ "results": [] }));
        };

        let query = ExtractionSearchQuery {
            query,
            app_name: app,
            project: None,
            from,
            to,
            limit,
        };
        let results = db
            .search_extractions_filtered(&query)?
            .into_iter()
            .map(|hit| SearchHitOutput {
                capture: hit.capture,
                extraction: hit.extraction,
                batch_narrative: hit.batch_narrative,
                rank: hit.rank,
            })
            .collect::<Vec<_>>();

        Ok(json!({ "results": results }))
    }

    fn tool_get_recent_activity(&self, args: &Map<String, Value>) -> Result<Value> {
        let minutes = required_positive_i64(args, "minutes")?;
        let end = Utc::now();
        let start = end - chrono::Duration::minutes(minutes);

        let captures = match self.open_db()? {
            Some(db) => db.get_captures_by_timerange(start, end)?,
            None => Vec::new(),
        };

        Ok(json!({
            "from": start,
            "to": end,
            "captures": captures,
        }))
    }

    fn tool_get_screenshot(&self, args: &Map<String, Value>) -> Result<Value> {
        let capture_id = required_i64(args, "id")?;

        let Some(db) = self.open_db()? else {
            return Ok(json!({ "screenshot": Value::Null }));
        };

        let capture = db
            .get_capture_detail(capture_id)?
            .map(|detail| detail.capture);

        let Some(capture) = capture else {
            return Ok(json!({ "screenshot": Value::Null }));
        };

        let resolved_path = self.resolve_screenshot_path(&capture)?;
        let exists = resolved_path.exists();

        Ok(json!({
            "screenshot": {
                "id": capture.id,
                "timestamp": capture.timestamp,
                "app_name": capture.app_name,
                "window_title": capture.window_title,
                "path": resolved_path,
                "exists": exists,
            }
        }))
    }

    fn tool_get_daily_summary(&self, args: &Map<String, Value>) -> Result<Value> {
        let date = required_date(args, "date")?;

        let summary = match self.open_db()? {
            Some(db) => db.get_latest_daily_insight_for_date(date)?,
            None => None,
        };

        Ok(json!({
            "date": date,
            "summary": summary,
        }))
    }

    fn tool_get_project_activity(&self, args: &Map<String, Value>) -> Result<Value> {
        let project = optional_non_empty_string(args, "project")?;
        let from = optional_datetime(args, "from")?;
        let to = optional_datetime(args, "to")?;

        let Some(db) = self.open_db()? else {
            return Ok(json!({ "project": project, "allocations": [] }));
        };

        let mut allocations = db.list_project_time_allocations(from, to)?;
        if let Some(project_name) = project.as_deref() {
            allocations.retain(|allocation| allocation.project.as_deref() == Some(project_name));
        }

        Ok(json!({
            "project": project,
            "allocations": allocations,
        }))
    }

    fn tool_get_app_usage(&self, args: &Map<String, Value>) -> Result<Value> {
        let to = optional_datetime(args, "to")?.unwrap_or_else(Utc::now);
        let from = optional_datetime(args, "from")?.unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("unix epoch must be representable"));

        if from > to {
            bail!("`from` must be before or equal to `to`");
        }

        let Some(db) = self.open_db()? else {
            return Ok(json!({ "from": from, "to": to, "apps": [] }));
        };

        let captures = db.get_captures_by_timerange(from, to)?;
        let mut counts = BTreeMap::<String, u64>::new();
        for capture in captures {
            let Some(app_name) = capture.app_name else {
                continue;
            };
            let app_name = app_name.trim();
            if app_name.is_empty() {
                continue;
            }
            *counts.entry(app_name.to_owned()).or_default() += 1;
        }

        let mut apps: Vec<AppUsage> = counts
            .into_iter()
            .map(|(app_name, capture_count)| AppUsage {
                app_name,
                capture_count,
            })
            .collect();
        apps.sort_by(|left, right| {
            right
                .capture_count
                .cmp(&left.capture_count)
                .then_with(|| left.app_name.cmp(&right.app_name))
        });

        Ok(json!({ "from": from, "to": to, "apps": apps }))
    }

    fn open_db(&self) -> Result<Option<StorageDb>> {
        StorageDb::open_existing_at_path(&self.db_path).with_context(|| {
            format!(
                "failed to open sqlite database at {}",
                self.db_path.display()
            )
        })
    }

    fn resolve_screenshot_path(&self, capture: &Capture) -> Result<PathBuf> {
        let storage_root = normalize_path(&self.storage_root);
        let raw_path = Path::new(&capture.screenshot_path);
        let candidate = if raw_path.is_absolute() {
            raw_path.to_path_buf()
        } else {
            storage_root.join(raw_path)
        };
        let normalized = normalize_path(&candidate);

        if !normalized.starts_with(&storage_root) {
            bail!(
                "capture {} resolved screenshot path outside storage root",
                capture.id
            );
        }

        Ok(normalized)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

fn wrap_tool_result(payload: Value) -> Result<Value> {
    let text = serde_json::to_string(&payload).context("failed to serialize MCP tool output")?;
    Ok(json!({
        "content": [{
            "type": "text",
            "text": text,
        }],
        "structuredContent": payload,
        "isError": false,
    }))
}

fn read_next_message<R: BufRead>(reader: &mut R, line_buffer: &mut String) -> Result<Option<String>> {
    loop {
        line_buffer.clear();
        let read = reader.read_line(line_buffer)?;
        if read == 0 {
            return Ok(None);
        }

        let line = line_buffer.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }

        if is_content_length_header(line) {
            let length = parse_content_length(line)?;

            loop {
                line_buffer.clear();
                let header_bytes = reader.read_line(line_buffer)?;
                if header_bytes == 0 {
                    bail!("unexpected EOF while reading MCP headers");
                }

                if line_buffer == "\n" || line_buffer == "\r\n" {
                    break;
                }
            }

            let mut body = vec![0_u8; length];
            reader.read_exact(&mut body)?;
            let payload = String::from_utf8(body).context("MCP payload is not valid UTF-8")?;
            return Ok(Some(payload));
        }

        return Ok(Some(line.to_owned()));
    }
}

fn is_content_length_header(line: &str) -> bool {
    line.len() >= "Content-Length:".len()
        && line[.."Content-Length:".len()].eq_ignore_ascii_case("Content-Length:")
}

fn parse_content_length(line: &str) -> Result<usize> {
    let (_, value) = line
        .split_once(':')
        .ok_or_else(|| anyhow!("invalid Content-Length header"))?;
    let length = value
        .trim()
        .parse::<usize>()
        .context("Content-Length must be a positive integer")?;
    Ok(length)
}

fn write_response(writer: &mut impl Write, response: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, response).context("failed to write MCP response")?;
    writer
        .write_all(b"\n")
        .context("failed to terminate MCP response")?;
    writer.flush().context("failed to flush MCP response")?;
    Ok(())
}

fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}

fn required_string(args: &Map<String, Value>, key: &str) -> Result<String> {
    let value = args
        .get(key)
        .ok_or_else(|| anyhow!("missing required argument `{key}`"))?;

    let value = value
        .as_str()
        .ok_or_else(|| anyhow!("argument `{key}` must be a string"))?
        .trim();

    if value.is_empty() {
        bail!("argument `{key}` must not be empty");
    }

    Ok(value.to_owned())
}

fn optional_non_empty_string(args: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };

    if value.is_null() {
        return Ok(None);
    }

    let value = value
        .as_str()
        .ok_or_else(|| anyhow!("argument `{key}` must be a string"))?
        .trim();

    if value.is_empty() {
        return Ok(None);
    }

    Ok(Some(value.to_owned()))
}

fn required_i64(args: &Map<String, Value>, key: &str) -> Result<i64> {
    let value = args
        .get(key)
        .ok_or_else(|| anyhow!("missing required argument `{key}`"))?;

    value
        .as_i64()
        .ok_or_else(|| anyhow!("argument `{key}` must be an integer"))
}

fn required_positive_i64(args: &Map<String, Value>, key: &str) -> Result<i64> {
    let value = required_i64(args, key)?;
    if value <= 0 {
        bail!("argument `{key}` must be greater than 0");
    }
    Ok(value)
}

fn optional_usize(args: &Map<String, Value>, key: &str) -> Result<Option<usize>> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };

    if value.is_null() {
        return Ok(None);
    }

    let value = value
        .as_u64()
        .ok_or_else(|| anyhow!("argument `{key}` must be a non-negative integer"))?;

    let value = usize::try_from(value)
        .with_context(|| format!("argument `{key}` exceeds supported range"))?;

    Ok(Some(value))
}

fn optional_datetime(args: &Map<String, Value>, key: &str) -> Result<Option<DateTime<Utc>>> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };

    if value.is_null() {
        return Ok(None);
    }

    let raw = value
        .as_str()
        .ok_or_else(|| anyhow!("argument `{key}` must be an RFC3339 timestamp string"))?;
    let parsed = DateTime::parse_from_rfc3339(raw)
        .with_context(|| format!("argument `{key}` is not a valid RFC3339 timestamp"))?
        .with_timezone(&Utc);
    Ok(Some(parsed))
}

fn required_date(args: &Map<String, Value>, key: &str) -> Result<NaiveDate> {
    let raw = required_string(args, key)?;
    NaiveDate::parse_from_str(&raw, "%Y-%m-%d")
        .with_context(|| format!("argument `{key}` must be in YYYY-MM-DD format"))
}

fn runtime_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable is not set"))
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "get_current_context",
            "description": "Get the latest rolling context insight.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "search_screen_history",
            "description": "Search historical screen extraction data.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "from": { "type": "string", "format": "date-time" },
                    "to": { "type": "string", "format": "date-time" },
                    "app": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1 }
                },
                "required": ["query"],
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_recent_activity",
            "description": "Return captures observed in the most recent N minutes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "minutes": { "type": "integer", "minimum": 1 }
                },
                "required": ["minutes"],
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_screenshot",
            "description": "Return metadata for a screenshot by capture id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "minimum": 1 }
                },
                "required": ["id"],
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_daily_summary",
            "description": "Get the daily insight summary for a date.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "date": { "type": "string", "format": "date" }
                },
                "required": ["date"],
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_project_activity",
            "description": "Return project capture activity within an optional time range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string" },
                    "from": { "type": "string", "format": "date-time" },
                    "to": { "type": "string", "format": "date-time" }
                },
                "additionalProperties": false,
            }
        }),
        json!({
            "name": "get_app_usage",
            "description": "Aggregate capture counts by app name in a time range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "format": "date-time" },
                    "to": { "type": "string", "format": "date-time" }
                },
                "additionalProperties": false,
            }
        }),
    ]
}
