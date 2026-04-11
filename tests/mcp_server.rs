mod support;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use assert_cmd::cargo::CommandCargoExt;
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use screencap::storage::{
    db::StorageDb,
    models::{
        ActivityType, ExtractionStatus, InsightData, InsightType, NewCapture, NewExtraction,
        NewExtractionBatch, NewInsight, Sentiment,
    },
};
use serde_json::{json, Value};
use uuid::Uuid;

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

    fn app_root(&self) -> PathBuf {
        self.path.join(".screencap")
    }

    fn config_path(&self) -> PathBuf {
        self.app_root().join("config.toml")
    }

    fn db_path(&self) -> PathBuf {
        self.app_root().join("screencap.db")
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

struct TestServer {
    address: std::net::SocketAddr,
    handle: Option<thread::JoinHandle<()>>,
}

impl TestServer {
    fn spawn(status: u16, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener addr");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 8192];
            let _ = stream.read(&mut buffer).expect("read request");
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        });

        Self {
            address,
            handle: Some(handle),
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn spawn_mcp_server_with_env(
    home: &Path,
    envs: &[(&str, &str)],
) -> Result<(ChildGuard, mpsc::Receiver<Result<Value, String>>)> {
    let mut command = Command::cargo_bin("screencap")?;
    command
        .arg("mcp")
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, value) in envs {
        command.env(key, value);
    }

    let mut child = command.spawn().context("failed to spawn screencap mcp")?;

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

fn spawn_mcp_server(home: &Path) -> Result<(ChildGuard, mpsc::Receiver<Result<Value, String>>)> {
    spawn_mcp_server_with_env(home, &[])
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

fn initialize_session(
    child: &mut ChildGuard,
    rx: &mpsc::Receiver<Result<Value, String>>,
) -> Result<()> {
    send_request(
        child,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {},
        }),
    )?;
    let initialize_response = recv_response(rx, Duration::from_secs(3))?;
    assert_eq!(
        initialize_response.get("jsonrpc"),
        Some(&Value::String("2.0".into()))
    );
    assert_eq!(initialize_response.get("id"), Some(&json!(1)));

    send_request(
        child,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {},
        }),
    )?;

    Ok(())
}

fn call_tool(
    child: &mut ChildGuard,
    rx: &mpsc::Receiver<Result<Value, String>>,
    id: i64,
    name: &str,
    arguments: Value,
) -> Result<Value> {
    send_request(
        child,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments,
            },
        }),
    )?;

    recv_response(rx, Duration::from_secs(3))
}

fn parse_tool_payload(response: &Value) -> Result<Value> {
    let payload = response["result"]["content"][0]["text"]
        .as_str()
        .context("tool result should include text content")?;
    serde_json::from_str(payload).context("tool text should be valid JSON")
}

fn write_synthesis_config(home: &TestHome, api_key_env: &str, base_url: &str) -> Result<()> {
    let app_root = home.app_root();
    fs::create_dir_all(&app_root)
        .with_context(|| format!("failed to create app root at {}", app_root.display()))?;
    fs::write(
        home.config_path(),
        format!(
            concat!(
                "[synthesis]\n",
                "provider = \"openai\"\n",
                "model = \"mock-synthesis-model\"\n",
                "api_key_env = \"{}\"\n",
                "base_url = \"{}\"\n"
            ),
            api_key_env, base_url
        ),
    )
    .with_context(|| format!("failed to write config at {}", home.config_path().display()))
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

fn seed_search_data(home: &TestHome) -> Result<()> {
    let app_root = home.app_root();
    fs::create_dir_all(&app_root)
        .with_context(|| format!("failed to create app root at {}", app_root.display()))?;
    let mut db = StorageDb::open_at_path(home.db_path())?;
    let now = Utc.with_ymd_and_hms(2026, 4, 11, 12, 0, 0).unwrap();

    let batch = db.insert_extraction_batch(&NewExtractionBatch {
        id: Uuid::new_v4(),
        batch_start: now - ChronoDuration::hours(1),
        batch_end: now - ChronoDuration::minutes(20),
        capture_count: 2,
        primary_activity: Some("coding".into()),
        project_context: Some("screencap".into()),
        narrative: Some("Debugged a JWT refresh token bug in the CLI read path".into()),
        raw_response: None,
        model_used: Some("mock-vision-model".into()),
        tokens_used: Some(90),
        cost_cents: Some(0.30),
    })?;

    let matching_capture = db.insert_capture(&NewCapture {
        timestamp: now - ChronoDuration::minutes(30),
        app_name: Some("Code".into()),
        window_title: Some("auth.rs".into()),
        bundle_id: Some("com.microsoft.VSCode".into()),
        display_id: Some(1),
        screenshot_path: "screenshots/search-match.jpg".into(),
    })?;
    let filtered_capture = db.insert_capture(&NewCapture {
        timestamp: now - ChronoDuration::minutes(25),
        app_name: Some("Safari".into()),
        window_title: Some("Docs".into()),
        bundle_id: Some("com.apple.Safari".into()),
        display_id: Some(1),
        screenshot_path: "screenshots/search-filtered.jpg".into(),
    })?;

    let matching_extraction = db.insert_extraction(&NewExtraction {
        capture_id: matching_capture.id,
        batch_id: batch.id,
        activity_type: Some(ActivityType::Coding),
        description: Some("JWT refresh token bug hunt".into()),
        app_context: Some("Editing the CLI read path in Rust".into()),
        project: Some("screencap".into()),
        topics: vec!["jwt".into(), "auth".into()],
        people: vec![],
        key_content: Some("refresh_token_expires_at".into()),
        sentiment: Some(Sentiment::Focused),
    })?;
    db.update_capture_status(
        matching_capture.id,
        ExtractionStatus::Processed,
        Some(matching_extraction.id),
    )?;

    let filtered_extraction = db.insert_extraction(&NewExtraction {
        capture_id: filtered_capture.id,
        batch_id: batch.id,
        activity_type: Some(ActivityType::Browsing),
        description: Some("Read unrelated payroll docs".into()),
        app_context: Some("Reviewing backoffice docs".into()),
        project: Some("backoffice".into()),
        topics: vec!["finance".into()],
        people: vec![],
        key_content: Some("benefits renewal".into()),
        sentiment: Some(Sentiment::Exploring),
    })?;
    db.update_capture_status(
        filtered_capture.id,
        ExtractionStatus::Processed,
        Some(filtered_extraction.id),
    )?;

    Ok(())
}

#[test]
fn mcp_server_handles_initialize_tools_list_and_current_context() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("initialize-tools-current-context")?;
    seed_rolling_context(home.path())?;

    let (mut child, rx) = spawn_mcp_server(home.path())?;
    initialize_session(&mut child, &rx)?;

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
    assert_eq!(tools.len(), 8);

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
        "ask_about_activity",
    ]);
    assert_eq!(tool_names, expected_names);

    let tool_call_response = call_tool(&mut child, &rx, 3, "get_current_context", json!({}))?;
    assert_eq!(tool_call_response.get("id"), Some(&json!(3)));
    assert_eq!(tool_call_response["result"]["isError"], Value::Bool(false));

    let parsed = parse_tool_payload(&tool_call_response)?;
    assert_eq!(parsed["available"], Value::Bool(true));
    assert_eq!(parsed["context"]["insight_type"], "rolling");
    assert_eq!(
        parsed["context"]["data"]["current_focus"],
        "Debugging the MCP transport"
    );
    assert_eq!(parsed["context"]["data"]["active_project"], "screencap");

    Ok(())
}

#[test]
fn mcp_server_handles_ask_about_activity_happy_path() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("ask-about-activity-success")?;
    seed_search_data(&home)?;

    let env_var = "SCREENCAP_TEST_MCP_ASK_API_KEY";
    let provider = TestServer::spawn(
        200,
        "{\"choices\":[{\"message\":{\"content\":\"{\\\"answer\\\":\\\"You were fixing a JWT refresh token bug in the CLI path.\\\",\\\"capture_ids\\\":[1]}\"}}],\"usage\":{\"prompt_tokens\":90,\"completion_tokens\":30,\"total_tokens\":120,\"cost\":0.22}}",
    );
    write_synthesis_config(&home, env_var, &provider.base_url())?;

    let (mut child, rx) = spawn_mcp_server_with_env(home.path(), &[(env_var, "token")])?;
    initialize_session(&mut child, &rx)?;

    let response = call_tool(
        &mut child,
        &rx,
        2,
        "ask_about_activity",
        json!({
            "question": "jwt refresh",
            "from": "2026-04-11T11:00:00Z",
            "to": "2026-04-11T12:00:00Z"
        }),
    )?;

    assert_eq!(response.get("id"), Some(&json!(2)));
    assert_eq!(response["result"]["isError"], Value::Bool(false));

    let payload = parse_tool_payload(&response)?;
    assert_eq!(payload["question"], "jwt refresh");
    assert_eq!(payload["analyzed_capture_count"], 2);
    assert_eq!(
        payload["answer"],
        "You were fixing a JWT refresh token bug in the CLI path."
    );
    assert_eq!(payload["tokens_used"], 120);
    assert_eq!(payload["cost_cents"], 0.22);
    assert_eq!(payload["references"][0]["capture_id"], 1);
    assert_eq!(
        payload["references"][0]["description"],
        "JWT refresh token bug hunt"
    );

    Ok(())
}

#[test]
fn mcp_server_reports_ask_about_activity_provider_errors_without_crashing_transport() -> Result<()>
{
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("ask-about-activity-missing-provider")?;
    seed_search_data(&home)?;

    let missing_env = "SCREENCAP_TEST_MCP_MISSING_API_KEY";
    write_synthesis_config(&home, missing_env, "http://127.0.0.1:9")?;

    let (mut child, rx) = spawn_mcp_server(home.path())?;
    initialize_session(&mut child, &rx)?;

    let response = call_tool(
        &mut child,
        &rx,
        2,
        "ask_about_activity",
        json!({
            "question": "jwt refresh",
            "from": "2026-04-11T11:00:00Z",
            "to": "2026-04-11T12:00:00Z"
        }),
    )?;

    assert_eq!(response.get("id"), Some(&json!(2)));
    assert_eq!(response["result"]["isError"], Value::Bool(true));

    let payload = parse_tool_payload(&response)?;
    assert_eq!(payload["tool"], "ask_about_activity");
    assert_eq!(payload["error"]["code"], "activity_analysis_failed");
    assert!(payload["error"]["message"]
        .as_str()
        .context("error message should be text")?
        .contains(missing_env));

    send_request(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/list",
            "params": {},
        }),
    )?;
    let post_error_response = recv_response(&rx, Duration::from_secs(3))?;
    assert_eq!(post_error_response.get("id"), Some(&json!(3)));
    assert!(post_error_response.get("result").is_some());

    Ok(())
}
