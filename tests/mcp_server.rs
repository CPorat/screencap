mod support;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use assert_cmd::cargo::CommandCargoExt;
use chrono::{TimeZone, Utc};
use screencap::storage::{
    db::StorageDb,
    models::{InsightData, InsightType, NewInsight},
};
use serde_json::{json, Value};

struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(name: &str) -> Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("screencap-mcp-tests-{name}-{unique}"));
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create test home at {}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    fn stdin_mut(&mut self) -> Result<&mut std::process::ChildStdin> {
        self.child
            .stdin
            .as_mut()
            .context("mcp stdin pipe is unavailable")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_mcp_server(home: &Path) -> Result<(ChildGuard, mpsc::Receiver<Result<Value, String>>)> {
    let mut child = Command::cargo_bin("screencap")?
        .arg("mcp")
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn screencap mcp")?;

    let stdout = child.stdout.take().context("failed to take mcp stdout")?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) if line.trim().is_empty() => continue,
                Ok(line) => {
                    let parsed = serde_json::from_str::<Value>(&line).map_err(|error| {
                        format!("failed to parse stdout as JSON `{line}`: {error}")
                    });
                    let _ = tx.send(parsed);
                }
                Err(error) => {
                    let _ = tx.send(Err(format!("failed to read stdout line: {error}")));
                    break;
                }
            }
        }
    });

    Ok((ChildGuard { child }, rx))
}

fn send_request(child: &mut ChildGuard, request: Value) -> Result<()> {
    let stdin = child.stdin_mut()?;
    writeln!(stdin, "{}", serde_json::to_string(&request)?).context("failed to write request")?;
    stdin.flush().context("failed to flush request")?;
    Ok(())
}

fn recv_response(rx: &mpsc::Receiver<Result<Value, String>>, timeout: Duration) -> Result<Value> {
    match rx.recv_timeout(timeout) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => bail!(error),
        Err(_) => bail!("timed out waiting for MCP response"),
    }
}

fn seed_rolling_context(home: &Path) -> Result<()> {
    let storage_root = home.join(".screencap");
    fs::create_dir_all(&storage_root).with_context(|| {
        format!(
            "failed to create storage root at {}",
            storage_root.display()
        )
    })?;
    let db_path = storage_root.join("screencap.db");
    let mut db = StorageDb::open_at_path(&db_path)
        .with_context(|| format!("failed to open sqlite database at {}", db_path.display()))?;

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
            apps_used: BTreeMap::from([("Terminal".to_string(), "18 min".to_string())]),
            context_switches: 2,
            mood: "focused".into(),
            summary: "Implementing the full MCP tool surface and validating stdio responses."
                .into(),
        },
        model_used: Some("mock-model".into()),
        tokens_used: Some(64),
        cost_cents: Some(0.42),
    })
    .context("failed to seed rolling insight")?;

    Ok(())
}

#[test]
fn mcp_server_handles_initialize_tools_list_and_current_context() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("initialize-tools-current-context")?;
    seed_rolling_context(home.path())?;

    let (mut child, rx) = spawn_mcp_server(home.path())?;

    send_request(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {},
        }),
    )?;

    let initialize_response = recv_response(&rx, Duration::from_secs(3))?;
    assert_eq!(
        initialize_response.get("jsonrpc"),
        Some(&Value::String("2.0".into()))
    );
    assert_eq!(initialize_response.get("id"), Some(&json!(1)));
    assert_eq!(
        initialize_response["result"]["capabilities"],
        json!({ "tools": { "listChanged": false } })
    );

    send_request(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {},
        }),
    )?;

    send_request(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {},
        }),
    )?;

    let tools_list_response = recv_response(&rx, Duration::from_secs(3))?;
    assert_eq!(tools_list_response.get("id"), Some(&json!(2)));

    let tools = tools_list_response["result"]["tools"]
        .as_array()
        .context("tools/list response missing tools array")?;
    assert_eq!(tools.len(), 7);

    let tool_names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let expected_names = BTreeSet::from([
        "get_current_context",
        "search_screen_history",
        "get_recent_activity",
        "get_screenshot",
        "get_daily_summary",
        "get_project_activity",
        "get_app_usage",
    ]);
    assert_eq!(tool_names, expected_names);

    send_request(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "get_current_context",
                "arguments": {},
            },
        }),
    )?;

    let tool_call_response = recv_response(&rx, Duration::from_secs(3))?;
    assert_eq!(tool_call_response.get("id"), Some(&json!(3)));
    assert_eq!(tool_call_response["result"]["isError"], Value::Bool(false));

    let payload = tool_call_response["result"]["content"][0]["text"]
        .as_str()
        .context("tool result should include text content")?;
    let parsed: Value = serde_json::from_str(payload).context("tool text should be valid JSON")?;

    assert_eq!(parsed["available"], Value::Bool(true));
    assert_eq!(parsed["context"]["insight_type"], "rolling");
    assert_eq!(
        parsed["context"]["data"]["current_focus"],
        "Debugging the MCP transport"
    );
    assert_eq!(parsed["context"]["data"]["active_project"], "screencap");

    Ok(())
}
