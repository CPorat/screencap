#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# ///

from __future__ import annotations

import argparse
import json
import sqlite3
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


def default_root() -> Path:
    return Path.home() / "Pictures" / "Screencap"


def now_utc() -> datetime:
    return datetime.now(timezone.utc)


def parse_utc(raw: str) -> datetime:
    return datetime.fromisoformat(raw.replace("Z", "+00:00"))


def state_path(root: Path) -> Path:
    return root / ".healthcheck.json"


def read_state(root: Path) -> dict[str, str]:
    path = state_path(root)
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text())
    except Exception:
        return {}


def write_state(root: Path, state: dict[str, str]) -> None:
    root.mkdir(parents=True, exist_ok=True)
    state_path(root).write_text(json.dumps(state, indent=2, sort_keys=True) + "\n")


def latest_capture(db_path: Path) -> tuple[str, str | None] | None:
    if not db_path.exists():
        return None
    try:
        with sqlite3.connect(db_path) as conn:
            row = conn.execute(
                """
                SELECT captured_at, error
                FROM captures
                ORDER BY captured_at DESC, id DESC
                LIMIT 1
                """
            ).fetchone()
    except sqlite3.OperationalError as error:
        if "no such table" in str(error):
            return None
        raise
    if row is None:
        return None
    return str(row[0]), row[1]


def latest_moment(db_path: Path) -> tuple[str, str | None] | None:
    if not db_path.exists():
        return None
    try:
        with sqlite3.connect(db_path) as conn:
            row = conn.execute(
                """
                SELECT captured_at, error
                FROM capture_moments
                ORDER BY captured_at DESC, id DESC
                LIMIT 1
                """
            ).fetchone()
    except sqlite3.OperationalError as error:
        if "no such table" in str(error):
            return latest_capture(db_path)
        raise
    if row is None:
        return latest_capture(db_path)
    return str(row[0]), row[1]


def recent_error_count(db_path: Path, limit: int = 5) -> int:
    if not db_path.exists():
        return 0
    try:
        with sqlite3.connect(db_path) as conn:
            row = conn.execute(
                """
                SELECT COUNT(*)
                FROM (
                  SELECT error
                  FROM captures
                  ORDER BY captured_at DESC, id DESC
                  LIMIT ?
                )
                WHERE error IS NOT NULL
                """,
                (limit,),
            ).fetchone()
    except sqlite3.OperationalError as error:
        if "no such table" in str(error):
            return 0
        raise
    return int(row[0] or 0)


def recent_moment_error_count(db_path: Path, limit: int = 5) -> int:
    if not db_path.exists():
        return 0
    try:
        with sqlite3.connect(db_path) as conn:
            row = conn.execute(
                """
                SELECT COUNT(*)
                FROM (
                  SELECT error, status
                  FROM capture_moments
                  ORDER BY captured_at DESC, id DESC
                  LIMIT ?
                )
                WHERE error IS NOT NULL OR status = 'failed'
                """,
                (limit,),
            ).fetchone()
    except sqlite3.OperationalError as error:
        if "no such table" in str(error):
            return recent_error_count(db_path, limit)
        raise
    return int(row[0] or 0)


def notify(title: str, message: str) -> None:
    subprocess.run(
        [
            "/usr/bin/osascript",
            "-e",
            f'display notification "{escape_applescript(message)}" with title "{escape_applescript(title)}"',
        ],
        check=False,
    )


def escape_applescript(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', '\\"')


def should_alert(root: Path, cooldown_minutes: int, reason: str, now: datetime) -> bool:
    state = read_state(root)
    last_reason = state.get("last_reason")
    raw_last_alert = state.get("last_alert_at")
    if raw_last_alert:
        try:
            last_alert = parse_utc(raw_last_alert)
            elapsed_minutes = (now - last_alert).total_seconds() / 60
            if last_reason == reason and elapsed_minutes < cooldown_minutes:
                return False
        except ValueError:
            pass

    state["last_reason"] = reason
    state["last_alert_at"] = now.replace(microsecond=0).isoformat().replace("+00:00", "Z")
    write_state(root, state)
    return True


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Check Screencap Lite health.")
    parser.add_argument(
        "--root",
        type=Path,
        default=default_root(),
        help="Storage root. Defaults to ~/Pictures/Screencap.",
    )
    parser.add_argument(
        "--max-age-minutes",
        type=int,
        default=15,
        help="Alert if the latest capture is older than this. Defaults to 15.",
    )
    parser.add_argument(
        "--cooldown-minutes",
        type=int,
        default=30,
        help="Minimum minutes between repeated alerts for the same issue. Defaults to 30.",
    )
    parser.add_argument(
        "--notify",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Send macOS notification on failure. Use --no-notify for status-only checks.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.max_age_minutes < 1 or args.cooldown_minutes < 1:
        print("--max-age-minutes and --cooldown-minutes must be positive", file=sys.stderr)
        return 2

    root = args.root.expanduser()
    db_path = root / "screencap.db"
    now = now_utc()
    latest = latest_moment(db_path)

    if latest is None:
        print("no captures found yet")
        return 0

    captured_at_raw, latest_error = latest
    try:
        captured_at = parse_utc(captured_at_raw)
    except ValueError:
        reason = "invalid latest capture timestamp"
        message = f"Screencap Lite has an invalid latest timestamp: {captured_at_raw}"
        if args.notify and should_alert(root, args.cooldown_minutes, reason, now):
            notify("Screencap Lite", message)
        print(message, file=sys.stderr)
        return 1

    age_minutes = (now - captured_at).total_seconds() / 60
    if age_minutes > args.max_age_minutes:
        reason = "stale capture"
        message = (
            f"No screenshot captured in {age_minutes:.1f} minutes "
            f"(latest: {captured_at_raw})."
        )
        if args.notify and should_alert(root, args.cooldown_minutes, reason, now):
            notify("Screencap Lite", message)
        print(message, file=sys.stderr)
        return 1

    errors = recent_moment_error_count(db_path)
    if latest_error or errors >= 3:
        reason = "recent capture errors"
        message = f"Screencap Lite has recent capture errors. Latest error: {latest_error or 'see database'}"
        if args.notify and should_alert(root, args.cooldown_minutes, reason, now):
            notify("Screencap Lite", message)
        print(message, file=sys.stderr)
        return 1

    print(f"ok latest_capture={captured_at_raw} age_minutes={age_minutes:.1f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
