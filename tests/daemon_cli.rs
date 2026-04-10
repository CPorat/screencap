mod support;

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use chrono::{NaiveDate, Utc};
use screencap::storage::{
    db::StorageDb,
    models::{HourlyProjectSummary, InsightData, InsightType, NewInsight},
};

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_screencap")
}

struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(name: &str) -> Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let home = Self {
            path: std::env::temp_dir().join(format!("screencap-cli-tests-{name}-{unique}")),
        };
        let app_root = home.path.join(".screencap");
        let port = reserve_port()?;

        fs::create_dir_all(&app_root)
            .with_context(|| format!("failed to create test app root at {}", app_root.display()))?;
        fs::write(home.config_path(), format!("[server]\nport = {port}\n"))
            .with_context(|| format!("failed to write test config at {}", app_root.display()))?;

        Ok(home)
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn pid_path(&self) -> PathBuf {
        self.path.join(".screencap").join("screencap.pid")
    }

    fn config_path(&self) -> PathBuf {
        self.path.join(".screencap").join("config.toml")
    }

    fn db_path(&self) -> PathBuf {
        self.path.join(".screencap").join("screencap.db")
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = Command::new(binary_path())
            .arg("stop")
            .env("HOME", &self.path)
            .output();
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn reserve_port() -> Result<u16> {
    let listener =
        TcpListener::bind("127.0.0.1:0").context("failed to reserve a local tcp port")?;
    listener
        .local_addr()
        .map(|address| address.port())
        .context("failed to read reserved tcp port")
}

fn run_cli(home: &Path, args: &[&str]) -> Result<Output> {
    Command::new(binary_path())
        .args(args)
        .env("HOME", home)
        .output()
        .with_context(|| format!("failed to run screencap {:?}", args))
}

fn run_cli_with_env(home: &Path, args: &[&str], envs: &[(&str, &str)]) -> Result<Output> {
    let mut command = Command::new(binary_path());
    command.args(args).env("HOME", home);
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .output()
        .with_context(|| format!("failed to run screencap {:?} with extra env", args))
}

fn spawn_foreground(home: &Path) -> Result<Child> {
    Command::new(binary_path())
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn foreground daemon")
}

fn assert_success(output: &Output, command: &str) {
    assert!(
        output.status.success(),
        "{command} failed: status={:?}, stdout={}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn wait_for(condition: impl Fn() -> Result<bool>, description: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if condition()? {
            return Ok(());
        }

        if Instant::now() >= deadline {
            bail!("timed out waiting for {description}");
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_exit(child: &mut Child) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().context("failed to poll child process")? {
            if status.success() {
                return Ok(());
            }
            bail!("daemon exited unsuccessfully: {status}");
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("timed out waiting for daemon exit");
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn seed_hourly_insights(db_path: &Path, date: NaiveDate) -> Result<()> {
    let mut db = StorageDb::open_at_path(db_path)?;
    for hour in [9_u32, 10, 14] {
        let hour_start = date.and_hms_opt(hour, 0, 0).unwrap().and_utc();
        let hour_end = hour_start + chrono::Duration::hours(1);
        db.insert_insight(&NewInsight {
            insight_type: InsightType::Hourly,
            window_start: hour_start,
            window_end: hour_end,
            data: InsightData::Hourly {
                hour_start,
                hour_end,
                dominant_activity: if hour < 12 {
                    "coding".into()
                } else {
                    "communication".into()
                },
                projects: vec![
                    HourlyProjectSummary {
                        name: Some("screencap".into()),
                        minutes: 42,
                        activities: vec!["debugging auth".into(), "writing tests".into()],
                    },
                    HourlyProjectSummary {
                        name: None,
                        minutes: 18,
                        activities: vec!["Slack conversations".into()],
                    },
                ],
                topics: vec!["JWT".into(), "authentication".into(), "testing".into()],
                people_interacted: vec!["@alice".into()],
                key_moments: vec![
                    "Found the JWT refresh bug and validated the fix".into(),
                    "Shared the outcome with Alice in Slack".into(),
                ],
                focus_score: 0.72,
                narrative: "Productive coding hour. The user traced the JWT refresh path, checked documentation, ran targeted tests, and shared the result in Slack.".into(),
            },
            model_used: Some("mock-synthesis-model".into()),
            tokens_used: Some(300),
            cost_cents: Some(0.42),
        })?;
    }

    Ok(())
}

struct TestServer {
    address: SocketAddr,
    handle: Option<thread::JoinHandle<()>>,
}

impl TestServer {
    fn spawn(status: u16, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener addr");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("read request");
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
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
            handle.join().expect("join server thread");
        }
    }
}

#[test]
fn start_status_stop_manage_background_daemon() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("background")?;
    let start = run_cli(home.path(), &["start"])?;
    assert_success(&start, "start");
    assert!(
        String::from_utf8_lossy(&start.stdout).contains("started daemon pid "),
        "unexpected start output: {}",
        String::from_utf8_lossy(&start.stdout),
    );

    wait_for(
        || Ok(home.pid_path().exists()),
        "pid file after background start",
    )?;

    let status = run_cli(home.path(), &["status"])?;
    assert_success(&status, "status");
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        stdout.contains("state: running"),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("pid: "),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("uptime_secs: "),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("captures_today: "),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("storage_bytes: "),
        "unexpected status output: {stdout}"
    );

    let stop = run_cli(home.path(), &["stop"])?;
    assert_success(&stop, "stop");
    assert!(
        String::from_utf8_lossy(&stop.stdout).contains("stopped daemon pid "),
        "unexpected stop output: {}",
        String::from_utf8_lossy(&stop.stdout),
    );

    wait_for(
        || Ok(!home.pid_path().exists()),
        "pid file removal after stop",
    )?;

    let status = run_cli(home.path(), &["status"])?;
    assert_success(&status, "status after stop");
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        stdout.contains("state: stopped"),
        "unexpected status output: {stdout}"
    );
    assert!(
        stdout.contains("pid: -"),
        "unexpected status output: {stdout}"
    );

    Ok(())
}

#[test]
fn foreground_daemon_can_be_stopped_via_cli() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("foreground")?;
    let mut child = spawn_foreground(home.path())?;

    wait_for(
        || Ok(home.pid_path().exists()),
        "pid file after foreground start",
    )?;

    let status = run_cli(home.path(), &["status"])?;
    assert_success(&status, "status while foreground daemon runs");
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("state: running"),
        "unexpected status output: {}",
        String::from_utf8_lossy(&status.stdout),
    );

    let stop = run_cli(home.path(), &["stop"])?;
    assert_success(&stop, "stop foreground daemon");

    wait_for_exit(&mut child)?;
    wait_for(
        || Ok(!home.pid_path().exists()),
        "pid file removal after foreground stop",
    )?;

    Ok(())
}

#[test]
fn today_generates_summary_once_and_reuses_stored_daily_insight() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("today")?;
    let today = Utc::now().date_naive();
    seed_hourly_insights(&home.db_path(), today)?;

    let env_var = "SCREENCAP_TEST_TODAY_API_KEY";
    let server = TestServer::spawn(
        200,
        Box::leak(
            format!(
                concat!(
                    "{{",
                    "\"choices\":[{{\"message\":{{\"content\":\"{{\\\"type\\\":\\\"daily\\\",\\\"date\\\":\\\"{}\\\",\\\"total_active_hours\\\":7.5,\\\"projects\\\":[{{\\\"name\\\":\\\"screencap\\\",\\\"total_minutes\\\":195,\\\"activities\\\":[\\\"auth module debugging\\\",\\\"test writing\\\"],\\\"key_accomplishments\\\":[\\\"Fixed JWT refresh bug\\\"]}}],\\\"time_allocation\\\":{{\\\"coding\\\":\\\"3h 15m\\\"}},\\\"focus_blocks\\\":[{{\\\"start\\\":\\\"09:15\\\",\\\"end\\\":\\\"11:45\\\",\\\"duration_min\\\":150,\\\"project\\\":\\\"screencap\\\",\\\"quality\\\":\\\"deep-focus\\\"}}],\\\"open_threads\\\":[\\\"Need to finish the export path\\\"],\\\"narrative\\\":\\\"Productive day focused on screencap.\\\"}}\"}}}}],",
                    "\"usage\":{{\"prompt_tokens\":320,\"completion_tokens\":120,\"total_tokens\":440}}",
                    "}}"
                ),
                today
            )
            .into_boxed_str(),
        ),
    );

    fs::write(
        home.config_path(),
        format!(
            concat!(
                "[server]\nport = {}\n\n",
                "[synthesis]\n",
                "provider = \"openai\"\n",
                "model = \"mock-synthesis-model\"\n",
                "api_key_env = \"{}\"\n",
                "base_url = \"{}\"\n",
                "daily_summary_time = \"18:00\"\n"
            ),
            reserve_port()?,
            env_var,
            server.base_url(),
        ),
    )?;

    let first = run_cli_with_env(home.path(), &["today"], &[(env_var, "token")])?;
    assert_success(&first, "today first run");
    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("\"type\": \"daily\""));
    assert!(first_stdout.contains("\"Productive day focused on screencap.\""));

    drop(server);

    let second = run_cli(home.path(), &["today"])?;
    assert_success(&second, "today second run");
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(second_stdout.contains("\"type\": \"daily\""));
    assert!(second_stdout.contains("\"Productive day focused on screencap.\""));

    Ok(())
}
