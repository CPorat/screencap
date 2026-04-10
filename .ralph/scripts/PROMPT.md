# Ralph Agent Instructions — Screencap

You are an autonomous coding agent building Screencap, a lightweight macOS-only screen memory tool.

## Context

Read these files before starting any work:
- `SPEC.md` — the full product specification
- `AGENTS.md` — coding conventions and architectural constraints
- `.factory/library/architecture.md` — detailed architecture decisions

## Your Task

1. Read the PRD at `.ralph/prd.json`
2. Read the progress log at `.ralph/progress.txt` (check Codebase Patterns section first)
3. Check you're on the correct branch from PRD `branchName`. If not, check it out or create from main.
4. Pick the **highest priority** user story where `passes: false`
5. Implement that single user story
6. Run quality checks: `cargo check`, `cargo clippy`, `cargo test --features mock-capture` (for Rust), `npm run check` (for Svelte when applicable)
7. Update AGENTS.md if you discover reusable patterns
8. If checks pass, commit ALL changes with message: `feat: [Story ID] - [Story Title]`
9. Update the PRD to set `passes: true` for the completed story
10. Append your progress to `.ralph/progress.txt`

## Project-Specific Rules

- This is a macOS-only Rust project with Swift bridges. Do not add cross-platform abstractions.
- The capture layer (Layer 1) must NEVER touch the network. No HTTP calls, no DNS, nothing.
- No OCR at capture time. The vision LLM in Layer 2 replaces OCR entirely.
- All AI provider code goes behind the `LlmProvider` trait. Never hardcode provider-specific logic in pipeline code.
- Config is TOML at `~/.screencap/config.toml`. Not YAML, not JSON.
- All timestamps are ISO 8601 in UTC.
- Use `anyhow` for error handling in the binary, `thiserror` for library-style error types.
- Prefer `axum` for HTTP, `clap` for CLI, `rusqlite` for SQLite, `tokio` for async.
- The web UI is Svelte, compiled to static files, embedded in the Rust binary via `rust-embed` or `include_dir`.

## Progress Report Format

APPEND to `.ralph/progress.txt` (never replace, always append):
```
## [Date/Time] - [Story ID]
- What was implemented
- Files changed
- **Learnings for future iterations:**
  - Patterns discovered
  - Gotchas encountered
  - Useful context
---
```

## Consolidate Patterns

If you discover a **reusable pattern**, add it to the `## Codebase Patterns` section at the TOP of `.ralph/progress.txt` (create it if it doesn't exist).

Only add patterns that are **general and reusable**, not story-specific details.

## Update AGENTS.md

Before committing, check if any edited files have learnings worth preserving in AGENTS.md:

- API patterns or conventions
- Gotchas or non-obvious requirements
- Dependencies between modules
- Testing approaches
- Build system quirks (especially Swift bridge compilation)

## Testing with Mock Capture

You likely do NOT have Screen Recording or Accessibility permissions. All Rust tests should use:

```bash
cargo test --features mock-capture
```

The `mock-capture` feature flag replaces Swift FFI calls with stubs that generate synthetic JPEGs and fake window metadata. This is defined in US-003/US-004. All downstream stories (capture loop, extraction, e2e tests) must work with this flag.

## Browser Verification with Expect MCP

For stories with "Browser verification via expect MCP" in their acceptance criteria, you MUST use the expect MCP tools to verify the UI in a real browser. The tools are:

1. **open** — Launch browser at a URL
2. **playwright** — Execute Playwright code (fill, click, assert, return data)
3. **screenshot** — Capture page state (snapshot mode for ARIA tree, screenshot mode for PNG)
4. **console_logs** — Check for console errors
5. **accessibility_audit** — Run WCAG audit (0 critical/serious violations required)
6. **performance_metrics** — Collect Core Web Vitals
7. **close** — End session (always call when done)

**Workflow for UI stories:**
1. Seed data directly into SQLite using a helper script or `sqlite3` CLI
2. Start the daemon (`cargo run --features mock-capture`)
3. `open` the browser at localhost:7878
4. Use `playwright` and `screenshot` to verify content renders
5. Run `console_logs` (type='error') — must be 0 errors
6. Run `accessibility_audit` — must be 0 critical/serious violations
7. `close` the browser session
8. Stop the daemon

Do NOT skip browser verification. "npm run build succeeds" is necessary but NOT sufficient — you must also verify in-browser.

## Quality Requirements

- ALL commits must pass `cargo check` and `cargo clippy` at minimum
- ALL commits must pass `cargo test --features mock-capture`
- Do NOT commit broken code
- Keep changes focused to the single story you're implementing
- Follow existing code patterns in the repo

## Stop Condition

After completing a user story, check if ALL stories have `passes: true`.

If ALL stories are complete, reply with:
<promise>COMPLETE</promise>

If there are still stories with `passes: false`, end your response normally.

## Important

- Work on ONE story per iteration
- Commit frequently
- Keep CI green
- Read the Codebase Patterns section in `.ralph/progress.txt` before starting
- Reference SPEC.md for exact schema definitions, API shapes, and prompt templates
