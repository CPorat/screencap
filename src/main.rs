//! Screencap CLI entrypoint

use clap::{Parser, Subcommand};
use screencap::Config;

/// Lightweight screen memory for macOS
#[derive(Parser)]
#[command(name = "screencap")]
#[command(about = "Lightweight screen memory for macOS", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start capture daemon (foreground)
    Run,

    /// Start as background daemon
    Start,

    /// Stop daemon
    Stop,

    /// Show daemon status, pipeline health, and cost so far today
    Status,

    /// Show current rolling context
    Now,

    /// Show today's summary (or generate if not yet run)
    Today,

    /// Show yesterday's daily summary
    Yesterday,

    /// Show this week's summaries
    Week,

    /// Full-text search across extractions and insights
    Search {
        /// Search query
        query: String,

        /// Filter by project
        #[arg(long)]
        project: Option<String>,

        /// Filter by app
        #[arg(long)]
        app: Option<String>,

        /// Time range (e.g., "24h", "7d")
        #[arg(long)]
        last: Option<String>,
    },

    /// Semantic/AI search over activity
    Ask {
        /// Question about your activity
        query: String,
    },

    /// List projects with time totals
    Projects {
        /// Time range (e.g., "7d")
        #[arg(long)]
        last: Option<String>,
    },

    /// Export day(s) as markdown
    Export {
        /// Specific date to export (YYYY-MM-DD)
        #[arg(long)]
        date: Option<String>,

        /// Export last N days
        #[arg(long)]
        last: Option<String>,

        /// Output format
        #[arg(long, default_value = "md")]
        format: String,

        /// Output file path
        #[arg(long)]
        output: Option<String>,
    },

    /// Start MCP server (stdio)
    Mcp,

    /// Open config in $EDITOR
    Config,

    /// Show AI API cost breakdown
    Costs,

    /// Delete old captures
    Prune {
        /// Delete captures older than this (e.g., "90d")
        #[arg(long)]
        older_than: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load config if it exists
    let _config = Config::load().ok();

    match cli.command {
        None => {
            // Default: run in foreground
            println!("Starting screencap daemon in foreground...");
            println!("HTTP server will be available at http://localhost:7878");
            // TODO: implement daemon run
            Ok(())
        }
        Some(Commands::Run) => {
            println!("Starting screencap daemon in foreground...");
            // TODO: implement daemon run
            Ok(())
        }
        Some(Commands::Start) => {
            println!("Starting screencap daemon in background...");
            // TODO: implement background daemon start
            Ok(())
        }
        Some(Commands::Stop) => {
            println!("Stopping screencap daemon...");
            // TODO: implement daemon stop
            Ok(())
        }
        Some(Commands::Status) => {
            println!("Daemon status: not implemented yet");
            // TODO: implement status check
            Ok(())
        }
        Some(Commands::Now) => {
            println!("Current context: not implemented yet");
            // TODO: implement rolling context display
            Ok(())
        }
        Some(Commands::Today) => {
            println!("Today's summary: not implemented yet");
            // TODO: implement today's summary
            Ok(())
        }
        Some(Commands::Yesterday) => {
            println!("Yesterday's summary: not implemented yet");
            // TODO: implement yesterday's summary
            Ok(())
        }
        Some(Commands::Week) => {
            println!("Week's summaries: not implemented yet");
            // TODO: implement week's summaries
            Ok(())
        }
        Some(Commands::Search {
            query,
            project,
            app,
            last,
        }) => {
            println!("Searching for: {}", query);
            if let Some(p) = project {
                println!("  Project filter: {}", p);
            }
            if let Some(a) = app {
                println!("  App filter: {}", a);
            }
            if let Some(l) = last {
                println!("  Time range: last {}", l);
            }
            // TODO: implement search
            println!("Search: not implemented yet");
            Ok(())
        }
        Some(Commands::Ask { query }) => {
            println!("Asking: {}", query);
            // TODO: implement semantic search
            println!("Semantic search: not implemented yet");
            Ok(())
        }
        Some(Commands::Projects { last }) => {
            if let Some(l) = last {
                println!("Projects (last {}):", l);
            } else {
                println!("Projects:");
            }
            // TODO: implement projects list
            println!("Projects: not implemented yet");
            Ok(())
        }
        Some(Commands::Export {
            date,
            last,
            format,
            output,
        }) => {
            if let Some(d) = date {
                println!("Exporting date: {}", d);
            } else if let Some(l) = last {
                println!("Exporting last: {}", l);
            }
            println!("Format: {}", format);
            if let Some(o) = output {
                println!("Output: {}", o);
            }
            // TODO: implement export
            println!("Export: not implemented yet");
            Ok(())
        }
        Some(Commands::Mcp) => {
            println!("Starting MCP server...");
            // TODO: implement MCP server
            println!("MCP server: not implemented yet");
            Ok(())
        }
        Some(Commands::Config) => {
            let config_path = dirs::config_dir()
                .map(|p| p.join("screencap").join("config.toml"))
                .unwrap_or_else(|| std::path::PathBuf::from("~/.screencap/config.toml"));
            println!("Config path: {}", config_path.display());
            // TODO: open in editor
            Ok(())
        }
        Some(Commands::Costs) => {
            println!("AI API cost breakdown: not implemented yet");
            // TODO: implement costs display
            Ok(())
        }
        Some(Commands::Prune { older_than }) => {
            println!("Pruning captures older than: {}", older_than);
            // TODO: implement prune
            println!("Prune: not implemented yet");
            Ok(())
        }
    }
}
