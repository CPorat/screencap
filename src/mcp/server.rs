use std::{env, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Map, Value};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::config::AppConfig;

use super::tools::{self, ToolExecutionContext};

const PROTOCOL_VERSION: &str = "2024-11-05";

pub async fn run_mcp_server(config: AppConfig) -> Result<()> {
    let home = runtime_home_dir()?;
    let server = McpServer::new(
        config.storage_root(&home).join("screencap.db"),
        config.screenshots_root(&home),
    );

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
    tools: ToolExecutionContext,
}

impl McpServer {
    fn new(db_path: PathBuf, screenshots_root: PathBuf) -> Self {
        Self {
            tools: ToolExecutionContext::new(db_path, screenshots_root),
        }
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

        let id = request_object.get("id").cloned()?;
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
                "tools": {
                    "listChanged": false,
                }
            }
        })
    }

    fn handle_tools_list(&self) -> Value {
        json!({
            "tools": tools::tool_definitions(),
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

        tools::call_tool(&self.tools, name, &arguments)
            .map_err(|error| RpcError::new(-32000, error.to_string()))
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
        env,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use serde_json::{json, Value};

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
        let root = temp_path("initialize");
        let server = McpServer::new(root.join("screencap.db"), root.join("screenshots"));

        let response = server
            .handle_payload(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .expect("initialize should return response");

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(
            response["result"]["capabilities"],
            json!({ "tools": { "listChanged": false } })
        );
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let root = temp_path("parse-error");
        let server = McpServer::new(root.join("screencap.db"), root.join("screenshots"));

        let response = server
            .handle_payload("not-json")
            .expect("invalid payload should return response");

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], Value::Null);
        assert_eq!(response["error"]["code"], -32700);
    }

    #[test]
    fn tools_list_exposes_story_tool_names() {
        let root = temp_path("tools-list");
        let server = McpServer::new(root.join("screencap.db"), root.join("screenshots"));

        let response = server
            .handle_payload(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#)
            .expect("tools/list should return response");
        let tools = response["result"]["tools"]
            .as_array()
            .expect("tools/list response should include tools");
        let names = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "get_current_context",
                "search_screen_history",
                "get_recent_activity",
                "get_screenshot",
                "get_daily_summary",
                "get_project_activity",
                "get_app_usage",
            ]
        );
    }
}
