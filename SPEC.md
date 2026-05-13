# Screencap Lite Specification

## Goal

Screencap Lite is a radical, macOS-only screen memory prototype. It captures periodic screenshots with the built-in macOS `screencapture` command, stores images on disk, indexes metadata in SQLite, and optionally uses OpenRouter to extract and summarize activity.

The main design goal is operational simplicity: no app shell, no daemon framework, no Rust, no Swift, no web UI, no REST API, no MCP server, and no bundled database or worker stack beyond SQLite and launchd.

## Non-Goals

- No menu bar app.
- No dock app.
- No native Swift/ScreenCaptureKit integration.
- No Rust daemon.
- No browser UI.
- No OCR at capture time.
- No screenshot blobs in SQLite.
- No audio, keylogging, or video recording.
- No cross-platform support.

## Runtime Files

Repository files:

- `capture.py`: captures one workspace moment.
- `process.py`: sends pending moments to OpenRouter and stores structured extractions.
- `summarize.py`: rolls extractions into daily, weekly, and monthly summaries.
- `check.py`: checks capture freshness and optionally sends a macOS notification.
- `launchd/*.plist`: user LaunchAgent templates for recurring capture, processing, summaries, and health checks.

Runtime storage:

```text
~/Pictures/Screencap/
  screencap.db
  YYYY/MM/DD/YYYYMMDD-HHMMSSZ/display-1.jpg
  YYYY/MM/DD/YYYYMMDD-HHMMSSZ/display-2.jpg
```

## Capture

`capture.py` uses `/usr/sbin/screencapture` directly:

```bash
/usr/sbin/screencapture -x -t jpg -D <display-number> <path>
```

It probes display numbers from `1` through `--max-displays`, defaulting to `5`. Each successful run creates one `capture_moments` row and one `capture_images` row per captured display. A multi-monitor setup is treated as one workspace moment; display-specific files are evidence for that moment.

Capture never calls the network and does not require OpenRouter credentials.

## Processing

`process.py` reads pending or partial moments, loads the images for each moment, and sends all displays from one timestamp together to OpenRouter. The prompt asks for a workspace-level interpretation first and display-level evidence second.

Default model resolution:

```text
OPENROUTER_MODEL -> google/gemini-2.5-flash
```

Missing `OPENROUTER_API_KEY` is a logged no-op so capture-only operation stays quiet and reliable.

## Summaries

`summarize.py` reads structured extraction text, not raw images.

Summary periods:

- `day`: reads moment extractions.
- `week`: prefers daily summaries, falls back to moment extractions.
- `month`: prefers weekly summaries, then daily summaries, then moment extractions.

Default model resolution:

```text
OPENROUTER_SUMMARY_MODEL -> OPENROUTER_MODEL -> google/gemini-2.5-flash
```

Summaries are upserted by `(period, period_start, period_end)`, so rerunning a day/week/month corrects previously incomplete summaries after more moments have been processed.

## Health Check

`check.py` reads the latest capture moment and exits nonzero if:

- the latest capture timestamp is invalid;
- the latest capture is older than the configured threshold;
- recent capture moments contain repeated capture errors.

Default LaunchAgent threshold is 15 minutes. Notifications use `/usr/bin/osascript`.

## launchd Cadence

- Capture: every 300 seconds, with `RunAtLoad`.
- Processing: every 600 seconds, with `RunAtLoad`.
- Health check: every 300 seconds, with `RunAtLoad`.
- Daily summary: 00:15 local time.
- Weekly summary: Monday 00:25 local time.
- Monthly summary: day 1 at 00:35 local time.

## Database

Current tables:

- `capture_moments`: one row per timestamped workspace capture.
- `capture_images`: one row per display image belonging to a moment.
- `moment_extractions`: one row per processed workspace moment.
- `summaries`: daily, weekly, and monthly rollups.

Legacy-compatible tables:

- `captures`
- `extractions`

`process.py` migrates legacy pending/processed `captures` into moment tables so old prototype data can still be processed or summarized.

## Failure Behavior

- Capture failure writes a failed `capture_moments` row when possible and exits nonzero.
- Missing OpenRouter credentials are no-op for `process.py` and `summarize.py`.
- Transient OpenRouter/API failures leave moments pending for retry.
- Deterministic bad model responses, such as invalid JSON or wrong `moment_id`, mark that moment failed so the queue can continue.
- Same-second manual captures use a suffix to avoid overwriting image files.
