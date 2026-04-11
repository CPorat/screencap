mod support;

use std::{
    collections::BTreeSet,
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

fn spawn_mcp_server(home: &Path) -> Result<(Child, mpsc::Receiver<Result<Value, String>>)> {
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

    Ok((child, rx))
}

fn send_request(child: &mut Child, request: Value) -> Result<()> {
    let stdin = child
        .stdin
        .as_mut()
        .context("mcp stdin pipe is unavailable")?;
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

#[test]
fn mcp_server_handles_initialize_and_tools_list() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("initialize-tools-list")?;

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
    assert!(initialize_response["result"]["capabilities"]["tools"].is_object());

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
    assert_eq!(tools.len(), 2);

    let tool_names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let expected_names = BTreeSet::from(["screencap.search", "screencap.stats"]);
    assert_eq!(tool_names, expected_names);

    let _ = child.kill();
    let _ = child.wait();

    Ok(())
}
