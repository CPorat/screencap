use std::{env, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde_json::{json, Map, Value};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    config::AppConfig,
    storage::{db::StorageDb, metrics},
};

const PROTOCOL_VERSION: &str = "2024-11-05";

pub async fn run_mcp_server(config: AppConfig) -> Result<()> {
    let home = runtime_home_dir()?;
    let server = McpServer::new(config.storage_root(&home).join("screencap.db"));

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin).lines();
    let mut writer = stdout;

    while let Some(line) = reader
        .next_line()
        .await
        .context("failed to read MCP request from stdin")?
    {
        let payload = line.trim();
        if payload.is_empty() {
            continue;
        }

        if let Some(response) = server.handle_payload(payload) {
            write_response(&mut writer, &response).await?;
        }
    }

    Ok(())
}

pub fn run_stdio_server(config: AppConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .context("failed to build tokio runtime for MCP server")?;

    runtime.block_on(run_mcp_server(config))
}

#[derive(Debug)]
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug)]
struct McpServer {
    db_path: PathBuf,
}

impl McpServer {
    fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn handle_payload(&self, payload: &str) -> Option<Value> {
        let request = match serde_json::from_str::<Value>(payload) {
            Ok(request) => request,
            Err(error) => {
                return Some(error_response(
                    Value::Null,
                    -32700,
                    format!("parse error: {error}"),
                ));
            }
        };

        self.handle_request(&request)
    }

    fn handle_request(&self, request: &Value) -> Option<Value> {
        let request_object = match request.as_object() {
            Some(request_object) => request_object,
            None => {
                return Some(error_response(
                    Value::Null,
                    -32600,
                    "request must be a JSON object",
                ));
            }
        };

        let method = match request_object.get("method").and_then(Value::as_str) {
            Some(method) => method,
            None => {
                let id = request_object.get("id").cloned().unwrap_or(Value::Null);
                return Some(error_response(id, -32600, "missing JSON-RPC method"));
            }
        };

        if method == "notifications/initialized" {
            return None;
        }

        let id = match request_object.get("id").cloned() {
            Some(id) => id,
            None => return None,
        };

        let params = request_object.get("params");

        let result = match method {
            "initialize" => Ok(self.handle_initialize()),
            "tools/list" => Ok(self.handle_tools_list()),
            "tools/call" => match params.and_then(Value::as_object) {
                Some(params) => self.handle_tools_call(params),
                None => Err(RpcError::new(-32602, "tools/call requires params object")),
            },
            _ => Err(RpcError::new(
                -32601,
                format!("method `{method}` not found"),
            )),
        };

        match result {
            Ok(result) => Some(success_response(id, result)),
            Err(error) => Some(error_response(id, error.code, error.message)),
        }
    }

    fn handle_initialize(&self) -> Value {
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "serverInfo": {
                "name": "screencap",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "tools": {}
            }
        })
    }

    fn handle_tools_list(&self) -> Value {
        json!({
            "tools": [
                {
                    "name": "screencap.search",
                    "description": "Search extracted screen activity by query text.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" }
                        },
                        "required": ["query"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "screencap.stats",
                    "description": "Return aggregate capture statistics from local storage.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }
                }
            ]
        })
    }

    fn handle_tools_call(&self, params: &Map<String, Value>) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(-32602, "tools/call requires a tool name"))?;

        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(args)) => args.clone(),
            Some(_) => {
                return Err(RpcError::new(
                    -32602,
                    "tools/call arguments must be a JSON object",
                ));
            }
        };

        let output = match name {
            "screencap.search" => self.call_search(&arguments),
            "screencap.stats" => self.call_stats(),
            _ => Err(anyhow!("unknown tool `{name}`")),
        }
        .map_err(|error| RpcError::new(-32000, error.to_string()))?;

        tool_text_result(output).map_err(|error| RpcError::new(-32000, error.to_string()))
    }

    fn call_search(&self, arguments: &Map<String, Value>) -> Result<Value> {
        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .ok_or_else(|| anyhow!("screencap.search requires a non-empty `query`"))?;

        let Some(db) = self.open_db_read_only()? else {
            return Ok(json!({
                "query": query,
                "count": 0,
                "results": []
            }));
        };

        let hits = db
            .search_extractions(query)?
            .into_iter()
            .map(|hit| {
                json!({
                    "capture": hit.capture,
                    "extraction": hit.extraction,
                    "batch_narrative": hit.batch_narrative,
                    "rank": hit.rank,
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "query": query,
            "count": hits.len(),
            "results": hits,
        }))
    }

    fn call_stats(&self) -> Result<Value> {
        let Some(db) = self.open_db_read_only()? else {
            return Ok(json!({
                "capture_count": 0,
                "captures_today": 0,
            }));
        };

        let capture_count = db.count_captures()?;
        let captures_today = metrics::count_captures_today(&db, Utc::now())?;

        Ok(json!({
            "capture_count": capture_count,
            "captures_today": captures_today,
        }))
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

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
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

fn tool_text_result(value: Value) -> Result<Value> {
    let text = serde_json::to_string(&value).context("failed to serialize tool output")?;
    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": text,
            }
        ]
    }))
}

async fn write_response(writer: &mut io::Stdout, response: &Value) -> Result<()> {
    let serialized = serde_json::to_string(response).context("failed to serialize MCP response")?;
    writer
        .write_all(serialized.as_bytes())
        .await
        .context("failed to write MCP response")?;
    writer
        .write_all(b"\n")
        .await
        .context("failed to terminate MCP response")?;
    writer
        .flush()
        .await
        .context("failed to flush MCP response")?;
    Ok(())
}

fn runtime_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable is not set"))
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use crate::storage::{
        db::StorageDb,
        models::{NewCapture, NewExtraction},
    };

    use super::McpServer;

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        env::temp_dir().join(format!("screencap-mcp-tests-{name}-{unique}"))
    }

    #[test]
    fn initialize_returns_server_capabilities() {
        let server = McpServer::new(temp_path("initialize").join("screencap.db"));

        let response = server
            .handle_payload(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .expect("initialize should return response");

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(response["result"]["capabilities"], json!({ "tools": {} }));
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let server = McpServer::new(temp_path("parse-error").join("screencap.db"));

        let response = server
            .handle_payload("not-json")
            .expect("invalid payload should return response");

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], serde_json::Value::Null);
        assert_eq!(response["error"]["code"], -32700);
    }

    #[test]
    fn tools_call_search_returns_text_response() {
        let root = temp_path("search-tool");
        fs::create_dir_all(&root).expect("create temp directory");
        let db_path = root.join("screencap.db");

        {
            let mut db = StorageDb::open_at_path(&db_path).expect("open sqlite db");
            let capture = db
                .insert_capture(&NewCapture {
                    timestamp: Utc::now(),
                    app_name: Some("Terminal".to_string()),
                    window_title: Some("mcp test".to_string()),
                    bundle_id: Some("com.apple.Terminal".to_string()),
                    display_id: Some(1),
                    screenshot_path: "screenshots/test.jpg".to_string(),
                })
                .expect("insert capture");

            db.insert_extraction(&NewExtraction {
                capture_id: capture.id,
                batch_id: Uuid::new_v4(),
                activity_type: None,
                description: Some("debugging json rpc mcp server".to_string()),
                app_context: None,
                project: None,
                topics: vec!["mcp".to_string()],
                people: Vec::new(),
                key_content: Some("json-rpc search".to_string()),
                sentiment: None,
            })
            .expect("insert extraction");
        }

        let server = McpServer::new(db_path);
        let response = server
            .handle_payload(
                r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"screencap.search","arguments":{"query":"debugging"}}}"#,
            )
            .expect("search call should return response");

        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool response should be text");
        let parsed: serde_json::Value =
            serde_json::from_str(text).expect("tool response text should be JSON");

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 7);
        assert_eq!(parsed["query"], "debugging");
        assert_eq!(parsed["count"], 1);
    }
}
