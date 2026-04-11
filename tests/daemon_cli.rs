mod support;

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use screencap::storage::{
    db::StorageDb,
    models::{
        ActivityType, HourlyProjectSummary, InsightData, InsightType, NewCapture, NewExtraction,
        NewExtractionBatch, NewInsight, Sentiment,
    },
};
use uuid::Uuid;

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

    fn launch_agent_path(&self) -> PathBuf {
        self.path
            .join("Library")
            .join("LaunchAgents")
            .join("dev.screencap.daemon.plist")
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

fn seed_rolling_insight(db_path: &Path) -> Result<()> {
    let mut db = StorageDb::open_at_path(db_path)?;
    let window_end = Utc::now();
    let window_start = window_end - ChronoDuration::minutes(30);

    db.insert_insight(&NewInsight {
        insight_type: InsightType::Rolling,
        window_start,
        window_end,
        data: InsightData::Rolling {
            window_start,
            window_end,
            current_focus: "Implementing CLI read commands".into(),
            active_project: Some("screencap".into()),
            apps_used: std::collections::BTreeMap::from([("Code".into(), "28 min".into())]),
            context_switches: 1,
            mood: "focused".into(),
            summary: "Focused API work on CLI read commands.".into(),
        },
        model_used: Some("mock-synthesis-model".into()),
        tokens_used: Some(180),
        cost_cents: Some(0.21),
    })?;

    Ok(())
}

fn seed_search_data(db_path: &Path, now: chrono::DateTime<Utc>) -> Result<()> {
    let mut db = StorageDb::open_at_path(db_path)?;
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
        screencap::storage::models::ExtractionStatus::Processed,
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
        screencap::storage::models::ExtractionStatus::Processed,
        Some(filtered_extraction.id),
    )?;

    Ok(())
}

fn seed_cost_data(db_path: &Path, now: chrono::DateTime<Utc>) -> Result<()> {
    let mut db = StorageDb::open_at_path(db_path)?;
    let batch_end = now - ChronoDuration::minutes(30);
    let rolling_end = now;
    let rolling_start = rolling_end - ChronoDuration::minutes(30);

    db.insert_extraction_batch(&NewExtractionBatch {
        id: Uuid::new_v4(),
        batch_start: batch_end - ChronoDuration::minutes(10),
        batch_end,
        capture_count: 2,
        primary_activity: Some("coding".into()),
        project_context: Some("screencap".into()),
        narrative: Some("CLI read command extraction batch".into()),
        raw_response: None,
        model_used: Some("mock-vision-model".into()),
        tokens_used: Some(90),
        cost_cents: Some(0.30),
    })?;

    db.insert_insight(&NewInsight {
        insight_type: InsightType::Rolling,
        window_start: rolling_start,
        window_end: rolling_end,
        data: InsightData::Rolling {
            window_start: rolling_start,
            window_end: rolling_end,
            current_focus: "Summarizing AI spend".into(),
            active_project: Some("screencap".into()),
            apps_used: std::collections::BTreeMap::from([("Code".into(), "30 min".into())]),
            context_switches: 0,
            mood: "focused".into(),
            summary: "Verified reported AI cost totals.".into(),
        },
        model_used: Some("mock-synthesis-model".into()),
        tokens_used: Some(250),
        cost_cents: Some(1.25),
    })?;

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
        let _ = TcpStream::connect(self.address);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
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
fn start_install_and_stop_uninstall_manage_launch_agent() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("launchd-install")?;
    let launch_agent_path = home.launch_agent_path();

    let start = run_cli(home.path(), &["start", "--install"])?;
    assert_success(&start, "start --install");
    assert!(
        launch_agent_path.exists(),
        "launch agent plist should be created"
    );

    let plist = fs::read_to_string(&launch_agent_path)?;
    assert!(plist.contains("<key>RunAtLoad</key>"));
    assert!(plist.contains("<key>KeepAlive</key>"));
    assert!(plist.contains("__daemon-child"));

    wait_for(
        || Ok(home.pid_path().exists()),
        "pid file after launch-agent background start",
    )?;

    let status = run_cli(home.path(), &["status"])?;
    assert_success(&status, "status after start --install");
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        stdout.contains("launchd_installed: true"),
        "unexpected status output: {stdout}"
    );

    let stop = run_cli(home.path(), &["stop", "--uninstall"])?;
    assert_success(&stop, "stop --uninstall");

    wait_for(
        || Ok(!home.pid_path().exists()),
        "pid file removal after stop --uninstall",
    )?;
    assert!(
        !launch_agent_path.exists(),
        "launch agent plist should be removed by stop --uninstall"
    );

    let status = run_cli(home.path(), &["status"])?;
    assert_success(&status, "status after stop --uninstall");
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        stdout.contains("launchd_installed: false"),
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
fn now_today_and_search_return_helpful_messages_on_empty_database() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("empty-read")?;
    let _db = StorageDb::open_at_path(home.db_path())?;

    let now = run_cli(home.path(), &["now"])?;
    assert_success(&now, "now");
    assert!(
        String::from_utf8_lossy(&now.stdout).contains("no rolling context is available yet"),
        "unexpected now output: {}",
        String::from_utf8_lossy(&now.stdout),
    );

    let today = run_cli(home.path(), &["today"])?;
    assert_success(&today, "today");
    assert!(
        String::from_utf8_lossy(&today.stdout).contains("no daily summary available for "),
        "unexpected today output: {}",
        String::from_utf8_lossy(&today.stdout),
    );

    let search = run_cli(home.path(), &["search", "jwt"])?;
    assert_success(&search, "search");
    assert!(
        String::from_utf8_lossy(&search.stdout).contains("no search results found for \"jwt\""),
        "unexpected search output: {}",
        String::from_utf8_lossy(&search.stdout),
    );

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
    assert!(first_stdout.contains(&format!("Today ({today})")));
    assert!(first_stdout.contains("summary: Productive day focused on screencap."));
    assert!(first_stdout.contains("active time: 7h 30m"));

    drop(server);

    let second = run_cli(home.path(), &["today"])?;
    assert_success(&second, "today second run");
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(second_stdout.contains(&format!("Today ({today})")));
    assert!(second_stdout.contains("summary: Productive day focused on screencap."));

    Ok(())
}

#[test]
fn now_prints_latest_rolling_context_summary() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("now")?;
    seed_rolling_insight(&home.db_path())?;

    let output = run_cli(home.path(), &["now"])?;
    assert_success(&output, "now");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Current context"));
    assert!(stdout.contains("Focused API work on CLI read commands."));
    assert!(stdout.contains("project: screencap"));

    Ok(())
}

#[test]
fn search_returns_matching_extractions() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("search")?;
    let now = Utc::now();
    seed_search_data(&home.db_path(), now)?;

    let output = run_cli(
        home.path(),
        &[
            "search",
            "JWT",
            "--project",
            "screencap",
            "--app",
            "Code",
            "--last",
            "24h",
        ],
    )?;
    assert_success(&output, "search");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Search results for \"JWT\""));
    assert!(stdout.contains("JWT refresh token bug hunt"));
    assert!(stdout.contains("app filter: Code"));
    assert!(stdout.contains("project filter: screencap"));
    assert!(!stdout.contains("Read unrelated payroll docs"));

    Ok(())
}

#[test]
fn ask_returns_semantic_answer_with_references() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("ask")?;
    let now = Utc::now();
    seed_search_data(&home.db_path(), now)?;

    let env_var = "SCREENCAP_TEST_ASK_API_KEY";
    let server = TestServer::spawn(
        200,
        "{\"choices\":[{\"message\":{\"content\":\"{\\\"answer\\\":\\\"You were fixing a JWT refresh token bug in the CLI path.\\\",\\\"capture_ids\\\":[1]}\"}}],\"usage\":{\"prompt_tokens\":90,\"completion_tokens\":30,\"total_tokens\":120,\"cost\":0.22}}",
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
                "base_url = \"{}\"\n"
            ),
            reserve_port()?,
            env_var,
            server.base_url(),
        ),
    )?;

    let output = run_cli_with_env(
        home.path(),
        &["ask", "jwt refresh", "--last", "24h"],
        &[(env_var, "token")],
    )?;
    assert_success(&output, "ask");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("You were fixing a JWT refresh token bug in the CLI path."));
    assert!(stdout.contains("references:"));
    assert!(stdout.contains("tokens_used: 120"));
    assert!(stdout.contains("cost_cents: 0.2200"));

    Ok(())
}

#[test]
fn projects_show_capture_based_allocations() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("projects")?;
    seed_search_data(&home.db_path(), Utc::now())?;

    let output = run_cli(home.path(), &["projects", "--last", "24h"])?;
    assert_success(&output, "projects");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Project allocation (capture-based)"));
    assert!(stdout.contains("total captures: 2"));
    assert!(stdout.contains("backoffice: 1 capture (50.0%)"));
    assert!(stdout.contains("screencap: 1 capture (50.0%)"));

    Ok(())
}

#[test]
fn costs_show_total_and_stage_breakdown() -> Result<()> {
    let _lock = support::IntegrationTestLock::acquire()?;
    let home = TestHome::new("costs")?;
    let now = Utc::now();
    seed_cost_data(&home.db_path(), now)?;

    let output = run_cli(home.path(), &["costs"])?;
    assert_success(&output, "costs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Reported AI cost"));
    assert!(stdout.contains("total: 1.55¢ ($0.0155) across 340 tokens"));
    assert!(stdout.contains("- extraction: 0.30¢ ($0.0030) across 90 tokens"));
    assert!(stdout.contains("- synthesis: 1.25¢ ($0.0125) across 250 tokens"));
    assert!(stdout.contains("by day:"));

    Ok(())
}
