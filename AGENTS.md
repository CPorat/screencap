# Screencap Lite - Agent Guidelines

This branch is a radical simplicity prototype.

## Project Shape

- macOS-only.
- Four uv single-file Python scripts:
  - `capture.py` captures screenshots.
  - `process.py` optionally processes screenshots with OpenRouter.
  - `summarize.py` rolls processed moments into daily, weekly, and monthly summaries.
  - `check.py` sends lightweight health notifications.
- Images are normal files on disk.
- SQLite stores metadata and extraction text only.
- launchd plists live in `launchd/` and are templates for this checkout path.

## Hard Boundaries

- Do not add Rust.
- Do not add Swift.
- Do not add a menu bar app.
- Do not add a web server, REST API, MCP server, plugin system, daemon framework, or agent framework.
- Do not store screenshot images as SQLite blobs.
- Do not add OCR at capture time.
- Do not hardcode API keys.
- Do not commit `.env` or any local runtime database/screenshots.

## Implementation Style

- Keep scripts readable and boring.
- Prefer Python standard library unless a dependency removes real complexity.
- `capture.py` must remain independent from AI processing.
- `process.py` and `summarize.py` may depend on the `openai` package through uv inline script metadata.
- All timestamps are UTC ISO 8601.
- The capture engine is macOS `/usr/sbin/screencapture`.
- Keep capture and network processing separate; capture must work without OpenRouter credentials.
- Missing OpenRouter credentials should be a logged no-op, not a capture blocker.

## Storage

Default root:

```text
~/Pictures/Screencap/
```

Screenshots:

```text
~/Pictures/Screencap/YYYY/MM/DD/YYYYMMDD-HHMMSSZ/display-N.jpg
```

Database:

```text
~/Pictures/Screencap/screencap.db
```

## Runtime Cadence

- Capture: every 5 minutes.
- Processing: every 10 minutes, up to 10 pending moments.
- Health check: every 5 minutes, alerts if latest capture is older than 15 minutes.
- Daily summary: 00:15 local time.
- Weekly summary: Monday 00:25 local time.
- Monthly summary: day 1 at 00:35 local time.
