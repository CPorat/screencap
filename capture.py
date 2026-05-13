#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# ///

from __future__ import annotations

import argparse
import sqlite3
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import NamedTuple

SCREENCAPTURE = "/usr/sbin/screencapture"
SIPS = "/usr/bin/sips"


class CapturedDisplay(NamedTuple):
    display_number: int
    path: Path
    width: int | None
    height: int | None


class DisplayCaptureError(Exception):
    def __init__(self, display_number: int, stderr: str):
        self.display_number = display_number
        self.stderr = stderr.strip()
        super().__init__(f"display {display_number}: {self.stderr or 'capture failed'}")


def default_root() -> Path:
    return Path.home() / "Pictures" / "Screencap"


def iso_utc(now: datetime) -> str:
    return now.replace(microsecond=0).isoformat().replace("+00:00", "Z")


def init_db(db_path: Path) -> None:
    db_path.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(db_path) as conn:
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS capture_moments (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              captured_at TEXT NOT NULL,
              status TEXT NOT NULL DEFAULT 'pending',
              error TEXT,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            """
        )
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS capture_images (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              moment_id INTEGER NOT NULL REFERENCES capture_moments(id),
              display_number INTEGER NOT NULL,
              screenshot_path TEXT NOT NULL,
              width INTEGER,
              height INTEGER,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            """
        )
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS moment_extractions (
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
            )
            """
        )


def capture_display(display_number: int, output_path: Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [SCREENCAPTURE, "-x", "-t", "jpg", "-D", str(display_number), str(output_path)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        raise DisplayCaptureError(display_number, result.stderr)


def image_size(path: Path) -> tuple[int | None, int | None]:
    try:
        result = subprocess.run(
            [SIPS, "-g", "pixelWidth", "-g", "pixelHeight", str(path)],
            check=True,
            capture_output=True,
            text=True,
        )
    except Exception:
        return None, None

    width = None
    height = None
    for line in result.stdout.splitlines():
        stripped = line.strip()
        if stripped.startswith("pixelWidth:"):
            width = int(stripped.split(":", 1)[1].strip())
        elif stripped.startswith("pixelHeight:"):
            height = int(stripped.split(":", 1)[1].strip())
    return width, height


def unique_moment_dir(base_dir: Path) -> Path:
    if not base_dir.exists():
        return base_dir
    for suffix in range(2, 100):
        candidate = base_dir.with_name(f"{base_dir.name}-{suffix}")
        if not candidate.exists():
            return candidate
    raise RuntimeError(f"could not find unique capture directory for {base_dir}")


def insert_moment(
    db_path: Path,
    captured_at: str,
    displays: list[CapturedDisplay],
    status: str,
    error: str | None = None,
) -> int:
    with sqlite3.connect(db_path) as conn:
        cursor = conn.execute(
            """
            INSERT INTO capture_moments (captured_at, status, error)
            VALUES (?, ?, ?)
            """,
            (captured_at, status, error),
        )
        moment_id = int(cursor.lastrowid)
        conn.executemany(
            """
            INSERT INTO capture_images (
              moment_id,
              display_number,
              screenshot_path,
              width,
              height
            ) VALUES (?, ?, ?, ?, ?)
            """,
            [
                (moment_id, display.display_number, str(display.path), display.width, display.height)
                for display in displays
            ],
        )
        return moment_id


def capture_moment(moment_dir: Path, max_displays: int) -> tuple[list[CapturedDisplay], list[str]]:
    displays: list[CapturedDisplay] = []
    errors: list[str] = []
    for display_number in range(1, max_displays + 1):
        output_path = moment_dir / f"display-{display_number}.jpg"
        try:
            capture_display(display_number, output_path)
        except DisplayCaptureError as error:
            if "Invalid display specified" in error.stderr:
                break
            errors.append(str(error))
            continue
        width, height = image_size(output_path)
        displays.append(CapturedDisplay(display_number, output_path, width, height))

    if not displays:
        detail = "; ".join(errors) if errors else "no displays captured"
        raise RuntimeError(detail)

    return displays, errors


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Capture one macOS workspace moment.")
    parser.add_argument(
        "--root",
        type=Path,
        default=default_root(),
        help="Storage root. Defaults to ~/Pictures/Screencap.",
    )
    parser.add_argument(
        "--max-displays",
        type=int,
        default=5,
        help="Maximum display numbers to probe with screencapture -D. Defaults to 5.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.max_displays < 1:
        print("--max-displays must be positive", file=sys.stderr)
        return 2

    root = args.root.expanduser()
    now = datetime.now(timezone.utc)
    captured_at = iso_utc(now)
    day_dir = root / now.strftime("%Y") / now.strftime("%m") / now.strftime("%d")
    moment_dir = unique_moment_dir(day_dir / now.strftime("%Y%m%d-%H%M%SZ"))
    db_path = root / "screencap.db"

    try:
        init_db(db_path)
        displays, errors = capture_moment(moment_dir, args.max_displays)
        status = "partial" if errors else "pending"
        moment_id = insert_moment(
            db_path,
            captured_at,
            displays,
            status,
            "; ".join(errors) if errors else None,
        )
    except Exception as error:
        message = f"capture failed: {error}"
        try:
            insert_moment(db_path, captured_at, [], "failed", message)
        except Exception as db_error:
            print(f"failed to record capture error: {db_error}", file=sys.stderr)
        print(message, file=sys.stderr)
        return 1

    print(f"captured moment_id={moment_id} status={status} displays={len(displays)} path={moment_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
