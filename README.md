# Screencap

Screencap is a lightweight, macOS-only screen memory tool written in Rust with Swift bridges for native Apple APIs. It captures screenshots locally, extracts structured context with a vision LLM, synthesizes rolling/hourly/daily summaries, and exposes the results through a CLI, REST API, embedded web UI, and MCP server.

The capture layer is offline-only: it writes screenshots plus window metadata to local storage and never touches the network.

## What it includes

- Continuous screenshot capture with timer and event-driven triggers
- Swift bridges for ScreenCaptureKit, frontmost window metadata, and native event hooks
- SQLite storage with FTS search
- Extraction pipeline for batch screenshot understanding
- Rolling, hourly, and daily synthesis pipelines
- Local REST API on `127.0.0.1:7878`
- Embedded Svelte web UI
- MCP stdio server via `screencap mcp`
- Menu bar app under `menubar/`

## Requirements

- macOS
- Rust toolchain
- Xcode Command Line Tools / Swift toolchain
- Node.js and npm for building the embedded web UI

## Build instructions

### Debug build

```bash
cargo build
```

`build.rs` compiles the Swift bridge and builds the embedded web UI from `web/`.

### Release build

```bash
cargo build --release
```

### Web-only checks

```bash
cd web
npm run check
npm run build
```

### Rust quality checks

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --features mock-capture
cd web && npm run check
```

Use `--features mock-capture` as the default CI path when Screen Recording or Accessibility permissions are unavailable. The mock path generates synthetic JPEGs and fake window metadata so the integration tests still exercise the real pipeline shape.

The native-permission capture e2e tests in `tests/e2e_capture_api.rs` are marked ignored by default and can be run on a permissioned machine with:

```bash
cargo test --ignored
```

### Skipping web rebuilds during local Rust iteration

If you already have a valid `web/dist/` and want Rust builds to reuse it:

```bash
SCREENCAP_WEB_DEV=1 cargo build
```

## Runtime layout

By default Screencap stores data under `~/.screencap/`:

- `config.toml` — main configuration file
- `screencap.db` — SQLite database
- `screenshots/YYYY/MM/DD/` — captured JPEGs
- `daily/` — markdown daily exports
- `prompts/` — disk-backed prompt templates seeded on first run
- `screencap.pid` — daemon lifecycle file

## Configuration guide

Screencap reads `~/.screencap/config.toml`. If the file does not exist, the app uses built-in defaults and creates the runtime directories on first run.

Example configuration:

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
provider = "openrouter"
model = "google/gemini-2.5-flash"
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

Notes:

- `storage.path` is the runtime root; screenshots and the database live under it.
- `provider` supports `openrouter`, `openai`, `anthropic`, `google`, `lmstudio`, and `ollama`.
- `api_key_env` names the environment variable to read; Screencap does not hardcode secrets.
- Prompt templates are seeded to `~/.screencap/prompts/` once and can be edited in place afterward.

## Running Screencap

### Foreground daemon

```bash
cargo run
```

Running without a subcommand starts the daemon in the foreground.

### Background daemon

```bash
cargo run -- start
cargo run -- status
cargo run -- stop
```

### LaunchAgent install/uninstall

```bash
cargo run -- start --install
cargo run -- stop --uninstall
```

## CLI usage examples

These commands are implemented by the current CLI surface:

```bash
cargo run -- now
cargo run -- today
cargo run -- yesterday
cargo run -- week
cargo run -- search "jwt refresh" --project screencap --last 7d
cargo run -- projects --last 7d
cargo run -- ask "What was I working on this afternoon?" --last 4h
cargo run -- costs
cargo run -- prune --older-than 30d
cargo run -- export --date 2026-04-11 --output ~/Desktop/2026-04-11.md
cargo run -- export --last 7d --output ~/Desktop/screencap-exports/
cargo run -- pause
cargo run -- resume
cargo run -- mcp
```

To see the installed CLI help text:

```bash
cargo run -- --help
```

## Web UI

Once the daemon is running, open:

- `http://127.0.0.1:7878/` — Timeline
- `http://127.0.0.1:7878/insights`
- `http://127.0.0.1:7878/search`
- `http://127.0.0.1:7878/settings`
- `http://127.0.0.1:7878/stats`

The Rust server embeds `web/dist/` and falls back to `index.html` for non-API routes so direct navigation to those pages works.

## REST API

The daemon exposes a localhost API on the configured port. Common endpoints include:

- `GET /api/health`
- `GET /api/stats`
- `GET /api/captures`
- `GET /api/captures/:id`
- `GET /api/search`
- `GET /api/insights/current`
- `GET /api/insights/daily?date=YYYY-MM-DD`

## MCP server

Screencap can also run as an MCP stdio server:

```bash
cargo run -- mcp
```

## Development notes

- This project is macOS-only by design.
- The capture layer must remain network-free.
- The embedded frontend is built from `web/` into `web/dist/` and then served by the Rust binary.
- The Swift bridge is compiled from `swift/Sources/` by `build.rs`.
