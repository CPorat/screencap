# screencap

## Project description

`screencap` is a lightweight screen-memory tool for macOS.

Based on the current codebase and spec, it captures screenshots and active-window metadata, stores data locally in SQLite, runs AI extraction/synthesis pipelines, and exposes results through CLI commands (plus MCP mode).

## Build instructions

### Prerequisites

- Rust toolchain (Cargo, rustc)
- macOS development tools (`xcrun`, `swiftc`) for the Swift bridge used by capture on macOS
- Node.js + npm for embedding the full web UI at build time (when npm is missing, the build embeds a placeholder UI)

### Build

```bash
cargo build
cargo build --release
```

### Verify locally

```bash
cargo check
cargo clippy -- -D warnings
cargo test
```

## Configuration guide

Runtime state is stored under `~/.screencap` by default.

- Config file path: `~/.screencap/config.toml`
- If `config.toml` is missing, built-in defaults are used.
- Paths that start with `~/` are expanded to your home directory.

### Default config template

```toml
[capture]
idle_interval_secs = 300
event_settle_ms = 500
jpeg_quality = 75
excluded_apps = ["1Password", "Keychain Access"]
excluded_window_titles = []

[extraction]
enabled = true
interval_secs = 600
provider = "openrouter" # openrouter | openai | anthropic | google | lmstudio | ollama
model = "google/gemini-2.0-flash"
api_key_env = "OPENROUTER_API_KEY"
base_url = ""
max_images_per_batch = 8

[synthesis]
enabled = true
provider = "openrouter"
model = "anthropic/claude-sonnet-4-20250514"
api_key_env = "OPENROUTER_API_KEY"
base_url = ""
rolling_interval_secs = 1800
hourly_enabled = true
daily_summary_time = "23:50"
daily_export_markdown = true
daily_export_path = "~/.screencap/daily/"

[storage]
path = "~/.screencap"
max_age_days = 90

[server]
port = 7878

[export]
obsidian_vault = ""
markdown_template = "default"
```

## CLI usage examples

Run `screencap` with no subcommand to start the daemon in the foreground.

```bash
# Daemon lifecycle
screencap start
screencap status
screencap stop

# Temporarily pause/resume capture
screencap pause
screencap resume

# Context and summaries
screencap now
screencap today
screencap yesterday
screencap week

# Text search with optional filters
screencap search "jwt refresh"
screencap search "jwt" --project screencap --app Code --last 24h

# Semantic Q&A over captured history
screencap ask "what was I doing this morning?" --last 8h

# Project/cost reporting
screencap projects --last 7d
screencap costs

# Retention cleanup
screencap prune --older-than 90d

# Export daily summaries
screencap export --date 2026-04-10
screencap export --last 7d --output ./exports

# Start MCP server (stdio transport)
screencap mcp

# Print config path (command is scaffolded)
screencap config
```

`--last` accepts positive durations with units: `m`, `h`, `d`, `w` (for example `30m`, `24h`, `7d`).
