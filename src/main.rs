use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use screencap::{config::AppConfig, daemon};
use tracing_subscriber::EnvFilter;

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
    #[command(name = "__daemon-child", hide = true)]
    DaemonChild,
    Now,
    Today,
    Search(SearchArgs),
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
    last: Option<String>,
}

#[derive(Debug, Args)]
struct PruneArgs {
    #[arg(long, default_value = "90d")]
    older_than: String,
}

#[derive(Debug, Args)]
struct ExportArgs {
    #[arg(long)]
    date: Option<String>,
    #[arg(long)]
    last: Option<String>,
    #[arg(long, default_value = "md")]
    format: String,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
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
        Some(command) => {
            let _config = AppConfig::load()?;
            handle_scaffolded_command(command);
        }
    }

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

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

fn emit_placeholder(command: &str, details: Option<String>) {
    match details {
        Some(details) => println!("{command} is scaffolded but not implemented yet ({details})"),
        None => println!("{command} is scaffolded but not implemented yet"),
    }
}

fn handle_scaffolded_command(command: Command) {
    match command {
        Command::Now => emit_placeholder("now", None),
        Command::Today => emit_placeholder("today", None),
        Command::Search(args) => emit_placeholder(
            "search",
            Some(format!(
                "query={:?}, project={:?}, last={:?}",
                args.query, args.project, args.last
            )),
        ),
        Command::Mcp => emit_placeholder("mcp", None),
        Command::Costs => emit_placeholder("costs", None),
        Command::Prune(args) => {
            emit_placeholder("prune", Some(format!("older_than={}", args.older_than)))
        }
        Command::Export(args) => emit_placeholder(
            "export",
            Some(format!(
                "date={:?}, last={:?}, format={}, output={:?}",
                args.date, args.last, args.format, args.output
            )),
        ),
        Command::Start
        | Command::Stop
        | Command::Status
        | Command::DaemonChild
        | Command::Config => unreachable!("handled before scaffolding dispatch"),
    }
}