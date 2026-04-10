# Screencap — Agent Guidelines

## Project Overview

Screencap is a lightweight, macOS-only screen memory tool written in Rust with Swift bridges for native macOS APIs. It captures screenshots, extracts structured context via vision LLMs, and synthesizes rolling/hourly/daily insights. See `SPEC.md` for the full architecture.

## Architecture

Three-layer pipeline:

1. **Capture** (Rust + Swift bridge) — continuous, offline, no network. ScreenCaptureKit for screenshots, NSWorkspace for window metadata. No OCR.
2. **Extraction** (vision LLM) — batches unprocessed screenshots every 10 min, sends to a vision model, returns structured JSON (activity type, project, topics, people, key content).
3. **Synthesis** (text LLM) — reads extractions and produces rolling context (30 min), hourly digests, and daily summaries.

## Tech Stack

- **Language**: Rust (daemon, API, CLI, MCP server)
- **Swift**: thin C-callable bridge for ScreenCaptureKit, NSWorkspace, and any other macOS-only APIs
- **Database**: SQLite with FTS5 full-text search
- **HTTP**: axum for the REST API on localhost:7878
- **CLI**: clap
- **Web UI**: Svelte or Preact, compiled to static files embedded in the Rust binary
- **Menu bar**: standalone Swift app (~200 lines)
- **AI providers**: OpenRouter (default), OpenAI, Anthropic, Google, LM Studio, Ollama — all behind a unified `LlmProvider` trait

## Key Conventions

- macOS only. Lean into Apple APIs aggressively. No cross-platform abstractions.
- The capture layer must never touch the network. All network calls happen in the extraction and synthesis pipelines.
- AI provider code uses a trait (`LlmProvider`) with `complete(prompt, images?)` and `complete_text(prompt)`. The `openai_compat` module handles OpenAI, OpenRouter, and LM Studio since they share the same API format.
- OpenAI-compatible provider implementations should resolve provider-specific default base URLs in one helper and return response text plus optional token-usage metadata; local backends that omit usage must stay `None` instead of inventing zero-cost data.
- When an OpenAI-compatible backend returns usage cost metadata (for example OpenRouter’s `usage.cost`), pass it through to persisted `cost_cents`; providers that omit cost should leave it `None` rather than guessing.
- Config lives in `~/.screencap/config.toml`. Use TOML, not YAML or JSON.
- Config code should expose helpers that accept explicit root/home paths for tests, and create runtime directories from the resolved config values on load.
- Screenshots stored as JPEGs in `~/.screencap/screenshots/YYYY/MM/DD/`.
- Swift bridge build integration should compile sources from `swift/Sources/` via `build.rs`, keep the ABI C-callable, and keep `mock-capture` fallbacks in Rust so tests can emit real JPEGs without macOS permissions.
- When the Swift bridge spans multiple source files, have `build.rs` invoke `swiftc -emit-library -static` over the full `swift/Sources/` set; `-emit-object -o <single-file>` breaks as soon as a second Swift source is added.
- When deriving the frontmost window title from `CGWindowListCopyWindowInfo`, treat the first layer-0 window for the frontmost app PID as authoritative; if that window has no title, return an empty string instead of scanning later windows from the same app.
- When one capture cycle spans multiple displays, write all screenshots for that cycle first and persist their `captures` rows in one SQLite transaction; if any display capture or DB write fails, delete that cycle’s new JPEGs so disk state cannot drift from the database.
- All timestamps are ISO 8601 in UTC.
- Structured data from LLMs is parsed into typed Rust structs, never stored as untyped blobs (except `raw_response` for debugging).
- When full-text search content spans multiple tables, keep a dedicated FTS table keyed by the canonical row id and update it from storage helpers; do not use an external-content FTS table tied to only one source table if some indexed fields come from joins.
- Daemon lifecycle should keep its PID file under `~/.screencap/`, store both `pid` and `started_at`, and have `start`/`stop`/`status` heal stale PID files by checking process liveness before trusting on-disk state.
- REST API read endpoints should open SQLite in read-only mode when possible and return empty/404 results without creating `screencap.db`; GET traffic must not mutate runtime state.
- When serving screenshots over HTTP, accept only sanitized paths relative to the screenshots root and walk them with `openat(..., O_NOFOLLOW)` so traversal or symlink escapes cannot leave the screenshot tree.
- LLM JSON parsers should accept optional markdown code fences but still validate structural invariants like unique capture IDs before persistence, so downstream schedulers can trust parsed model output.
- Extraction scheduler writes should persist a whole batch in one SQLite transaction: insert the `extraction_batches` row, insert all frame `extractions`, link each `captures.extraction_id`, then rebuild the shared FTS rows; if parsing fails after the LLM returns, still store `raw_response` in `extraction_batches` before marking those captures failed.
- When synthesis prompts need both extraction batch summaries and frame-level evidence, expose a typed storage helper that groups `extraction_batches` with their joined `captures`/`extractions`; keep cross-table SQL in `StorageDb`, not in pipeline prompt builders.



## Code Style

- Rust: follow standard `rustfmt` and `clippy` conventions. Prefer `anyhow` for error handling in the binary, `thiserror` for library-style error types.
- Swift bridge code should be minimal — just the native API calls exposed as C-callable functions. Keep logic in Rust.
- No comments that merely narrate what code does. Comments explain *why*, not *what*.
- Prefer small, focused modules. The project structure in `SPEC.md` reflects the intended module boundaries.

## Testing

- Unit tests for storage layer (SQLite operations, FTS queries).
- Unit tests for extraction/synthesis prompt construction and response parsing.
- Integration tests for the API endpoints.
- The capture layer is hard to unit test (requires screen recording permission); test it manually or with integration tests that mock the Swift bridge.

## What NOT to Do

- Do not add audio capture, keylogging, or continuous video recording.
- Do not add a plugin/pipe system or agent framework.
- Do not use Electron, Tauri, or any bundled browser runtime.
- Do not add cross-platform support. This is macOS only.
- Do not run OCR at capture time — the vision LLM in Layer 2 replaces OCR entirely.
- Do not send data to the network in the capture layer, ever.
- Do not hardcode API keys. Always read from environment variables referenced in config.
