use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Datelike, Days, Duration as ChronoDuration, NaiveDate, Utc};
use clap::{Args, Parser, Subcommand};
use serde::Deserialize;
use screencap::{
    config::AppConfig,
    daemon,
    export::markdown,
    pipeline::synthesis,
    storage::{
        db::StorageDb,
        models::{
            CostBreakdown, ExtractionSearchHit, ExtractionSearchQuery, Insight, InsightData,
            InsightType, ProjectTimeAllocation,
        },
    },
};
use tracing_subscriber::EnvFilter;

const DEFAULT_SEARCH_LIMIT: usize = 20;
const DEFAULT_SEMANTIC_SEARCH_LIMIT: usize = 100;

#[derive(Debug, Parser)]
#[command(name = "screencap", version, about = "Lightweight screen memory for macOS", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Start,
    Stop,
    Status,
    Pause,
    Resume,
    #[command(name = "__daemon-child", hide = true)]
    DaemonChild,
    Now,
    Today,
    Yesterday,
    Week,
    Search(SearchArgs),
    Projects(ProjectsArgs),
    Ask(AskArgs),
    Mcp,
    Config,
    Costs,
    Prune(PruneArgs),
    Export(ExportArgs),
}

#[derive(Debug, Args)]
struct SearchArgs {
    query: String,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    app: Option<String>,
    #[arg(long)]
    last: Option<String>,
}

#[derive(Debug, Args)]
struct AskArgs {
    query: String,
    #[arg(long)]
    last: Option<String>,
}


#[derive(Debug, Args)]
struct ProjectsArgs {
    #[arg(long)]
    last: Option<String>,
}

#[derive(Debug, Args)]
struct PruneArgs {
    #[arg(long = "older-than", default_value = "90d")]
    older_than: String,
}

#[derive(Debug, Args)]
struct ExportArgs {
    #[arg(long)]
    date: Option<String>,
    #[arg(long)]
    last: Option<String>,
    #[arg(long)]
    output: Option<String>,
}
#[derive(Debug, Deserialize)]
struct CapturePausedResponse {
    paused: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(matches!(cli.command, Some(Command::Mcp)));

    match cli.command {
        None | Some(Command::DaemonChild) => {
            let config = AppConfig::load()?;
            daemon::run_foreground(&config).await?;
        }
        Some(Command::Config) => emit_placeholder(
            "config",
            Some(format!(
                "path={}",
                AppConfig::default_config_path()?.display()
            )),
        ),
        Some(Command::Start) => {
            let config = AppConfig::load()?;
            let pid = daemon::start_background(&config).await?;
            println!("started daemon pid {pid}");
        }
        Some(Command::Stop) => {
            let config = AppConfig::load()?;
            let pid = daemon::stop(&config).await?;
            println!("stopped daemon pid {pid}");
        }
        Some(Command::Status) => {
            let config = AppConfig::load()?;
            print_daemon_status(&daemon::status(&config)?);
        }
        Some(Command::Pause) => handle_capture_pause(true).await?,
        Some(Command::Resume) => handle_capture_pause(false).await?,
        Some(Command::Now) => handle_now()?,
        Some(Command::Today) => handle_today().await?,
        Some(Command::Yesterday) => handle_yesterday()?,
        Some(Command::Week) => handle_week()?,
        Some(Command::Search(args)) => handle_search(args)?,
        Some(Command::Projects(args)) => handle_projects(args)?,
        Some(Command::Ask(args)) => handle_ask(args).await?,
        Some(Command::Export(args)) => markdown::run_export(args.date, args.last, args.output).await?,
        Some(Command::Costs) => handle_costs()?,
        Some(Command::Prune(args)) => handle_prune(args)?,
        Some(Command::Mcp) => {
            let config = AppConfig::load()?;
            screencap::mcp::server::run_mcp_server(config).await?;
        }
    }

    Ok(())
}

fn init_tracing(force_stderr: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    if force_stderr {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(std::io::stderr)
            .try_init();
        return;
    }

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

fn print_daemon_status(status: &daemon::DaemonStatus) {
    println!("state: {}", status.state.as_str());
    println!(
        "pid: {}",
        status
            .pid
            .map_or_else(|| "-".to_string(), |pid| pid.to_string())
    );
    println!("uptime_secs: {}", status.uptime_secs);
    println!("captures_today: {}", status.captures_today);
    println!("storage_bytes: {}", status.storage_bytes);
}

async fn handle_today() -> Result<()> {
    let (config, home) = load_config_and_home()?;
    let now = Utc::now();

    match get_or_generate_today_summary(&config, &home, now).await? {
        Some(insight) => print_daily_summary(Some("Today"), &insight)?,
        None => println!("no daily summary available for {}", now.date_naive()),
    }

    Ok(())
}

fn handle_now() -> Result<()> {
    let (config, home) = load_config_and_home()?;
    let Some(db) = open_read_db(&config, &home)? else {
        println!("no rolling context is available yet");
        return Ok(());
    };

    match db.get_latest_insight_by_type(InsightType::Rolling)? {
        Some(insight) => print_rolling_insight(&insight)?,
        None => println!("no rolling context is available yet"),
    }

    Ok(())
}

fn handle_yesterday() -> Result<()> {
    let (config, home) = load_config_and_home()?;
    let yesterday = Utc::now()
        .date_naive()
        .pred_opt()
        .ok_or_else(|| anyhow!("failed to compute yesterday's date"))?;
    let Some(db) = open_read_db(&config, &home)? else {
        println!("no daily summary available for {}", yesterday);
        return Ok(());
    };

    match db.get_latest_daily_insight_for_date(yesterday)? {
        Some(insight) => print_daily_summary(Some("Yesterday"), &insight)?,
        None => println!("no daily summary available for {}", yesterday),
    }

    Ok(())
}

fn handle_week() -> Result<()> {
    let (config, home) = load_config_and_home()?;
    let today = Utc::now().date_naive();
    let week_start = today
        .checked_sub_days(Days::new(u64::from(today.weekday().num_days_from_monday())))
        .ok_or_else(|| anyhow!("failed to compute the start of the current week"))?;
    let Some(db) = open_read_db(&config, &home)? else {
        println!("no daily summaries available for {}..{}", week_start, today);
        return Ok(());
    };

    let insights = db.list_daily_insights_in_date_range(week_start, today)?;
    if insights.is_empty() {
        println!("no daily summaries available for {}..{}", week_start, today);
        return Ok(());
    }

    print_week_summaries(week_start, today, &insights)
}

fn handle_search(args: SearchArgs) -> Result<()> {
    let query = args.query.trim();
    if query.is_empty() {
        bail!("search query must not be empty");
    }

    let (config, home) = load_config_and_home()?;
    let Some(db) = open_read_db(&config, &home)? else {
        println!("no search results found for \"{query}\"");
        return Ok(());
    };

    let from = parse_optional_lookback_start(args.last.as_deref(), Utc::now())?;
    let search_query = ExtractionSearchQuery {
        query: query.to_owned(),
        app_name: trim_to_option(args.app),
        project: trim_to_option(args.project),
        from,
        to: None,
        limit: DEFAULT_SEARCH_LIMIT,
    };
    let results = db.search_extractions_filtered(&search_query)?;
    print_search_results(query, &search_query, &results);

    Ok(())
}

async fn handle_ask(args: AskArgs) -> Result<()> {
    let query = args.query.trim();
    if query.is_empty() {
        bail!("ask query must not be empty");
    }

    let (config, home) = load_config_and_home()?;
    let Some(db) = open_read_db(&config, &home)? else {
        println!("no captures are available yet");
        return Ok(());
    };

    let from = parse_optional_lookback_start(args.last.as_deref(), Utc::now())?;
    let candidates = synthesis::semantic_search_candidates(
        &db,
        query,
        from,
        None,
        DEFAULT_SEMANTIC_SEARCH_LIMIT,
    )?;
    let result = synthesis::semantic_search(&config, query, candidates).await?;

    let answer = result.answer.trim();
    if answer.is_empty() {
        println!("(no answer returned)");
    } else {
        println!("{answer}");
    }

    if !result.references.is_empty() {
        println!();
        println!("references:");
        for reference in result.references {
            println!(
                "- #{} {} — {}",
                reference.capture.id,
                format_timestamp(&reference.capture.timestamp),
                reference
                    .extraction
                    .description
                    .as_deref()
                    .unwrap_or("no description available")
            );
        }
    }

    if let Some(tokens_used) = result.tokens_used {
        println!();
        println!("tokens_used: {tokens_used}");
    }
    if let Some(cost_cents) = result.cost_cents {
        println!("cost_cents: {cost_cents:.4}");
    }

    Ok(())
}


fn handle_projects(args: ProjectsArgs) -> Result<()> {
    let (config, home) = load_config_and_home()?;
    let Some(db) = open_read_db(&config, &home)? else {
        println!("no project activity is available yet");
        return Ok(());
    };

    let from = parse_optional_lookback_start(args.last.as_deref(), Utc::now())?;
    let projects = db.list_project_time_allocations(from, None)?;
    print_project_time_allocations(args.last.as_deref(), &projects);

    Ok(())
}

fn handle_costs() -> Result<()> {
    let (config, home) = load_config_and_home()?;
    let Some(db) = open_read_db(&config, &home)? else {
        println!("no reported AI cost is available yet");
        return Ok(());
    };

    let costs = db.summarize_reported_costs()?;
    print_cost_breakdown(&costs);

    Ok(())
}






async fn handle_capture_pause(paused: bool) -> Result<()> {
    let config = AppConfig::load()?;
    let endpoint = if paused { "pause" } else { "resume" };
    let url = format!("http://127.0.0.1:{}/api/{endpoint}", config.server.port);

    let response = reqwest::Client::new()
        .post(&url)
        .send()
        .await
        .with_context(|| format!("failed to call daemon endpoint at {url}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read response body>".to_string());
        bail!("daemon request to {url} failed with status {status}: {body}");
    }

    let state: CapturePausedResponse = response
        .json()
        .await
        .with_context(|| format!("failed to parse daemon response from {url}"))?;
    println!("paused: {}", state.paused);
    Ok(())
}

fn handle_prune(args: PruneArgs) -> Result<()> {
    screencap::storage::prune::run_prune(args.older_than)
}


fn emit_placeholder(command: &str, details: Option<String>) {
    match details {
        Some(details) => println!("{command} is scaffolded but not implemented yet ({details})"),
        None => println!("{command} is scaffolded but not implemented yet"),
    }
}


fn load_config_and_home() -> Result<(AppConfig, PathBuf)> {
    Ok((AppConfig::load()?, runtime_home_dir()?))
}

fn open_read_db(config: &AppConfig, home: &Path) -> Result<Option<StorageDb>> {
    let db_path = config.storage_root(home).join("screencap.db");
    StorageDb::open_existing_at_path(&db_path).with_context(|| {
        format!(
            "failed to open read-only sqlite database at {}",
            db_path.display()
        )
    })
}

async fn get_or_generate_today_summary(
    config: &AppConfig,
    home: &Path,
    now: DateTime<Utc>,
) -> Result<Option<Insight>> {
    let Some(_) = open_read_db(config, home)? else {
        return Ok(None);
    };

    synthesis::get_or_generate_today_summary_at_home(config.clone(), home, now).await
}

fn parse_optional_lookback_start(
    raw: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    raw.map(|value| parse_lookback_window(value).map(|duration| now - duration))
        .transpose()
}

fn parse_lookback_window(raw: &str) -> Result<ChronoDuration> {
    let raw = raw.trim();
    if raw.len() < 2 {
        bail!("lookback window must be a positive duration like `24h` or `7d`");
    }

    let (amount, unit) = raw.split_at(raw.len() - 1);
    let amount: i64 = amount
        .parse()
        .with_context(|| format!("invalid lookback amount `{amount}`"))?;
    if amount <= 0 {
        bail!("lookback window must be greater than 0");
    }

    let seconds_per_unit = match unit.to_ascii_lowercase().as_str() {
        "m" => 60_i64,
        "h" => 60 * 60,
        "d" => 60 * 60 * 24,
        "w" => 60 * 60 * 24 * 7,
        _ => bail!("unsupported lookback unit `{unit}`; use m, h, d, or w"),
    };
    let total_seconds = amount
        .checked_mul(seconds_per_unit)
        .ok_or_else(|| anyhow!("lookback window `{raw}` is too large"))?;

    Ok(ChronoDuration::seconds(total_seconds))
}

fn trim_to_option(raw: Option<String>) -> Option<String> {
    raw.and_then(|value| {
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_owned())
        }
    })
}

fn print_rolling_insight(insight: &Insight) -> Result<()> {
    let InsightData::Rolling {
        window_start,
        window_end,
        current_focus,
        active_project,
        apps_used,
        context_switches,
        mood,
        summary,
    } = &insight.data
    else {
        bail!("latest rolling insight stored an unexpected payload shape");
    };

    println!("Current context");
    println!(
        "window: {} -> {}",
        format_timestamp(window_start),
        format_timestamp(window_end)
    );
    println!("focus: {current_focus}");
    if let Some(project) = active_project.as_deref() {
        println!("project: {project}");
    }
    println!("mood: {mood}");
    println!("context switches: {context_switches}");
    if !apps_used.is_empty() {
        println!("apps:");
        for (app, duration) in apps_used {
            println!("- {app}: {duration}");
        }
    }
    println!("summary: {summary}");

    Ok(())
}

fn print_daily_summary(title: Option<&str>, insight: &Insight) -> Result<()> {
    let InsightData::Daily {
        date,
        total_active_hours,
        projects,
        time_allocation,
        focus_blocks,
        open_threads,
        narrative,
    } = &insight.data
    else {
        bail!("daily summary stored an unexpected payload shape");
    };

    match title {
        Some(title) => println!("{title} ({date})"),
        None => println!("{date}"),
    }
    println!("summary: {narrative}");
    println!("active time: {}", format_hours(*total_active_hours));

    if !projects.is_empty() {
        println!("projects:");
        for project in projects {
            println!(
                "- {}: {}",
                project.name,
                format_duration_minutes(u64::from(project.total_minutes))
            );
            if !project.activities.is_empty() {
                println!("  activities: {}", project.activities.join(", "));
            }
            if !project.key_accomplishments.is_empty() {
                println!(
                    "  accomplishments: {}",
                    project.key_accomplishments.join(", ")
                );
            }
        }
    }

    if !time_allocation.is_empty() {
        println!("time allocation:");
        for (activity, duration) in time_allocation {
            println!("- {activity}: {duration}");
        }
    }

    if !focus_blocks.is_empty() {
        println!("focus blocks:");
        for block in focus_blocks {
            println!(
                "- {}-{}: {} ({}, {})",
                block.start,
                block.end,
                block.project,
                block.quality,
                format_duration_minutes(u64::from(block.duration_min))
            );
        }
    }

    if !open_threads.is_empty() {
        println!("open threads:");
        for thread in open_threads {
            println!("- {thread}");
        }
    }

    Ok(())
}

fn print_week_summaries(
    week_start: NaiveDate,
    today: NaiveDate,
    insights: &[Insight],
) -> Result<()> {
    println!("Week to date ({}..{})", week_start, today);
    for (index, insight) in insights.iter().enumerate() {
        println!();
        print_daily_summary(None, insight)?;
        if index + 1 < insights.len() {
            println!();
        }
    }

    Ok(())
}

fn print_search_results(
    query: &str,
    search_query: &ExtractionSearchQuery,
    results: &[ExtractionSearchHit],
) {
    if results.is_empty() {
        println!("no search results found for \"{query}\"");
        return;
    }

    println!("Search results for \"{query}\"");
    if let Some(project) = search_query.project.as_deref() {
        println!("project filter: {project}");
    }
    if let Some(app_name) = search_query.app_name.as_deref() {
        println!("app filter: {app_name}");
    }
    if let Some(from) = search_query.from.as_ref() {
        println!("from: {}", format_timestamp(from));
    }

    for (index, hit) in results.iter().enumerate() {
        println!();
        println!(
            "{}. {} — {}",
            index + 1,
            format_timestamp(&hit.capture.timestamp),
            hit.extraction
                .description
                .as_deref()
                .unwrap_or("no description available")
        );
        if let Some(app_name) = hit.capture.app_name.as_deref() {
            println!("   app: {app_name}");
        }
        if let Some(window_title) = hit.capture.window_title.as_deref() {
            println!("   window: {window_title}");
        }
        if let Some(project) = hit.extraction.project.as_deref() {
            println!("   project: {project}");
        }
        if !hit.extraction.topics.is_empty() {
            println!("   topics: {}", hit.extraction.topics.join(", "));
        }
        if let Some(key_content) = hit.extraction.key_content.as_deref() {
            println!("   key content: {key_content}");
        }
        if let Some(batch_narrative) = hit.batch_narrative.as_deref() {
            println!("   batch: {batch_narrative}");
        }
    }
}

fn print_project_time_allocations(last: Option<&str>, projects: &[ProjectTimeAllocation]) {
    if projects.is_empty() {
        println!("no project activity is available yet");
        return;
    }

    let total_captures: u64 = projects.iter().map(|project| project.capture_count).sum();
    println!("Project allocation (capture-based)");
    if let Some(last) = last {
        println!("range: last {last}");
    }
    println!("total captures: {total_captures}");
    for project in projects {
        let share = if total_captures == 0 {
            0.0
        } else {
            (project.capture_count as f64 / total_captures as f64) * 100.0
        };
        let capture_label = if project.capture_count == 1 {
            "capture"
        } else {
            "captures"
        };
        println!(
            "- {}: {} {capture_label} ({share:.1}%)",
            project.project.as_deref().unwrap_or("(unattributed)"),
            project.capture_count
        );
    }
}

fn print_cost_breakdown(costs: &CostBreakdown) {
    if costs.total.tokens_used == 0
        && costs.total.reported_cost_cents == 0.0
        && costs.by_day.is_empty()
    {
        println!("no reported AI cost is available yet");
        return;
    }

    println!("Reported AI cost");
    println!(
        "total: {} across {} tokens",
        format_cost_cents(costs.total.reported_cost_cents),
        costs.total.tokens_used
    );
    println!("by stage:");
    println!(
        "- extraction: {} across {} tokens",
        format_cost_cents(costs.extraction.reported_cost_cents),
        costs.extraction.tokens_used
    );
    println!(
        "- synthesis: {} across {} tokens",
        format_cost_cents(costs.synthesis.reported_cost_cents),
        costs.synthesis.tokens_used
    );
    if !costs.by_day.is_empty() {
        println!("by day:");
        for day in &costs.by_day {
            println!(
                "- {}: {} across {} tokens",
                day.date,
                format_cost_cents(day.reported_cost_cents),
                day.tokens_used
            );
        }
    }
}

fn format_timestamp(timestamp: &DateTime<Utc>) -> String {
    timestamp.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn format_hours(hours: f64) -> String {
    if !hours.is_finite() || hours < 0.0 {
        return format!("{hours:.1}h");
    }

    format_duration_minutes((hours * 60.0).round() as u64)
}

fn format_duration_minutes(total_minutes: u64) -> String {
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    match (hours, minutes) {
        (0, minutes) => format!("{minutes}m"),
        (hours, 0) => format!("{hours}h"),
        (hours, minutes) => format!("{hours}h {minutes}m"),
    }
}

fn format_cost_cents(reported_cost_cents: f64) -> String {
    format!(
        "{reported_cost_cents:.2}¢ (${:.4})",
        reported_cost_cents / 100.0
    )
}

fn runtime_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable is not set"))
}
