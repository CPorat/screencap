# Screencap Lite

Radically simple macOS screenshot capture.

This prototype has no app, no daemon framework, no Rust, no Swift, no web UI, and no bundled AI pipeline. It is four `uv` single-file Python scripts:

- `capture.py` calls the built-in macOS `/usr/sbin/screencapture` command, writes one JPG per connected display into a timestamped workspace-moment folder, and records metadata in SQLite.
- `process.py` optionally sends each pending workspace moment to OpenRouter and writes structured extraction rows.
- `summarize.py` rolls processed moments into daily, weekly, and monthly summaries.
- `check.py` sends a native macOS notification if capture appears stale.

## Requirements

- macOS
- [`uv`](https://docs.astral.sh/uv/)
- Screen Recording permission for the process that runs `screencapture`
- Optional: `OPENROUTER_API_KEY` for AI processing

`/usr/sbin/screencapture` ships with macOS. It is not part of this repo.

## Quick Start

```bash
uv run capture.py
uv run check.py --no-notify
```

Optional AI processing:

```bash
cat > .env <<'EOF'
OPENROUTER_API_KEY=...
OPENROUTER_MODEL=google/gemini-2.5-flash
EOF

uv run process.py
uv run summarize.py --period day --date 2026-05-12
```

`.env` is local runtime configuration and should not be committed.

## Capture Once

```bash
uv run capture.py
```

Screenshots are written to:

```text
~/Pictures/Screencap/YYYY/MM/DD/YYYYMMDD-HHMMSSZ/display-1.jpg
~/Pictures/Screencap/YYYY/MM/DD/YYYYMMDD-HHMMSSZ/display-2.jpg
```

Metadata is indexed in:

```text
~/Pictures/Screencap/screencap.db
```

Use a custom root for testing:

```bash
uv run capture.py --root /tmp/screencap-test
```

## Process Pending Screenshots

Set OpenRouter configuration in `.env`:

```bash
OPENROUTER_API_KEY=...
OPENROUTER_MODEL=google/gemini-2.5-flash
```

```bash
uv run process.py
```

If `OPENROUTER_API_KEY` is missing, processing exits as a logged no-op. Capture still works.

Defaults:

- model: `OPENROUTER_MODEL` or `google/gemini-2.5-flash`
- batch size: `1` workspace moment manually, `10` moments from launchd
- limit: maximum moments processed per run

Options:

```bash
export OPENROUTER_API_KEY="..."
export OPENROUTER_MODEL="google/gemini-2.5-flash"
uv run process.py --model google/gemini-2.5-flash --batch-size 5 --limit 5
uv run process.py --root /tmp/screencap-test
```

`capture.py` probes display numbers with `screencapture -D` up to a configurable maximum. The default is 5:

```bash
uv run capture.py --max-displays 5
```

If fewer displays are connected, it stops when macOS reports there are no more displays.

## Summarize Processed Activity

Summaries use extracted text, not screenshots. The default model is:

```text
OPENROUTER_SUMMARY_MODEL -> OPENROUTER_MODEL -> google/gemini-2.5-flash
```

Using the same model as extraction is fine for now. If summary cost or latency starts to matter, set `OPENROUTER_SUMMARY_MODEL` to a cheaper text model without changing capture or extraction.

Manual runs:

```bash
uv run summarize.py --period day
uv run summarize.py --period week
uv run summarize.py --period month
```

If `OPENROUTER_API_KEY` is missing, summaries exit as a logged no-op.

By default each command summarizes the previous completed period using `America/New_York` boundaries. Override with:

```bash
uv run summarize.py --period day --date 2026-05-12
uv run summarize.py --period day --timezone America/New_York
```

## Check Health

```bash
uv run check.py
```

Defaults:

- alert if the latest capture is older than 15 minutes
- wait 30 minutes before repeating the same alert
- use macOS notifications through `/usr/bin/osascript`

Status-only check:

```bash
uv run check.py --no-notify
```

If the Mac sleeps, launchd timers do not run while it is asleep. On wake, the checker may see an old latest capture, but it will notify at most once per cooldown window. It does not alert before the first capture exists.

## SQLite Schema

Images stay on disk. SQLite stores only metadata and extracted text.

```sql
CREATE TABLE captures (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  captured_at TEXT NOT NULL,
  screenshot_path TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  error TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE capture_moments (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  captured_at TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  error TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE capture_images (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  moment_id INTEGER NOT NULL REFERENCES capture_moments(id),
  display_number INTEGER NOT NULL,
  screenshot_path TEXT NOT NULL,
  width INTEGER,
  height INTEGER,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE moment_extractions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  moment_id INTEGER NOT NULL REFERENCES capture_moments(id),
  model TEXT NOT NULL,
  activity_type TEXT,
  description TEXT NOT NULL,
  app_context TEXT,
  project TEXT,
  topics_json TEXT NOT NULL DEFAULT '[]',
  people_json TEXT NOT NULL DEFAULT '[]',
  key_content TEXT,
  visible_text_json TEXT NOT NULL DEFAULT '[]',
  displays_json TEXT NOT NULL DEFAULT '[]',
  raw_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE summaries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  period TEXT NOT NULL,
  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,
  model TEXT NOT NULL,
  headline TEXT NOT NULL,
  narrative TEXT NOT NULL,
  projects_json TEXT NOT NULL DEFAULT '[]',
  topics_json TEXT NOT NULL DEFAULT '[]',
  people_json TEXT NOT NULL DEFAULT '[]',
  timeline_json TEXT NOT NULL DEFAULT '[]',
  open_threads_json TEXT NOT NULL DEFAULT '[]',
  raw_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(period, period_start, period_end)
);

CREATE TABLE extractions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  capture_id INTEGER NOT NULL REFERENCES captures(id),
  model TEXT NOT NULL,
  activity_type TEXT,
  description TEXT NOT NULL,
  app_context TEXT,
  project TEXT,
  topics_json TEXT NOT NULL DEFAULT '[]',
  people_json TEXT NOT NULL DEFAULT '[]',
  key_content TEXT,
  visible_text_json TEXT NOT NULL DEFAULT '[]',
  raw_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

`captures` and `extractions` are legacy-compatible tables from the first prototype. New multi-display capture uses `capture_moments`, `capture_images`, and `moment_extractions`.

## launchd

The LaunchAgent templates use this checkout path:

```text
/Users/chrisporat/Development/personal/screencap
```

Install:

```bash
mkdir -p ~/Library/LaunchAgents ~/Library/Logs/screencap-lite
cp launchd/com.chrisporat.screencap-capture.plist ~/Library/LaunchAgents/
cp launchd/com.chrisporat.screencap-process.plist ~/Library/LaunchAgents/
cp launchd/com.chrisporat.screencap-summary-daily.plist ~/Library/LaunchAgents/
cp launchd/com.chrisporat.screencap-summary-weekly.plist ~/Library/LaunchAgents/
cp launchd/com.chrisporat.screencap-summary-monthly.plist ~/Library/LaunchAgents/
cp launchd/com.chrisporat.screencap-check.plist ~/Library/LaunchAgents/
launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-capture.plist
launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-process.plist
launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-summary-daily.plist
launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-summary-weekly.plist
launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-summary-monthly.plist
launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-check.plist
```

For processing and summaries, launchd uses `.env` from this checkout through the Python scripts.

```bash
OPENROUTER_API_KEY=...
OPENROUTER_MODEL=google/gemini-2.5-flash
# Optional:
OPENROUTER_SUMMARY_MODEL=google/gemini-2.5-flash
```

Unload:

```bash
launchctl bootout "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-capture.plist
launchctl bootout "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-process.plist
launchctl bootout "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-summary-daily.plist
launchctl bootout "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-summary-weekly.plist
launchctl bootout "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-summary-monthly.plist
launchctl bootout "gui/$(id -u)" ~/Library/LaunchAgents/com.chrisporat.screencap-check.plist
```

Logs:

```bash
tail -f ~/Library/Logs/screencap-lite/capture.stderr.log
tail -f ~/Library/Logs/screencap-lite/process.stderr.log
tail -f ~/Library/Logs/screencap-lite/summary-daily.stderr.log
tail -f ~/Library/Logs/screencap-lite/check.stderr.log
```

Cadence is controlled by `StartInterval` or `StartCalendarInterval` in each plist. Capture, processing, and health also use `RunAtLoad` so they run once immediately after install.

```text
capture: 300 seconds
process: 600 seconds
check: 300 seconds
daily summary: 00:15 local time
weekly summary: Monday 00:25 local time
monthly summary: day 1 at 00:35 local time
```

Check launchd status:

```bash
launchctl list | grep screencap
```

Check recent screenshots:

```bash
find ~/Pictures/Screencap -name '*.jpg' -mmin -15
```

Check latest database row:

```bash
sqlite3 ~/Pictures/Screencap/screencap.db \
  "select id, captured_at, status from capture_moments order by id desc limit 10;"
```

## Permissions

If screenshots fail or are blank, grant Screen Recording permission in:

```text
System Settings -> Privacy & Security -> Screen & System Audio Recording
```

The permission may attach to Terminal, Python, uv, or the launchd context depending on how the script is run. Validate manually with `uv run capture.py` before installing launchd.
