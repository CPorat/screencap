#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "openai>=1.0.0",
# ]
# ///

from __future__ import annotations

import argparse
import base64
import json
import os
import sqlite3
import sys
from pathlib import Path
from typing import Any

from openai import OpenAI

DEFAULT_MODEL = "google/gemini-2.5-flash"
OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1"


class ModelResponseError(Exception):
    pass


def default_root() -> Path:
    return Path.home() / "Pictures" / "Screencap"


def load_env_file(path: Path = Path(".env")) -> None:
    if not path.exists():
        return

    for raw_line in path.read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        if not key or key in os.environ:
            continue
        os.environ[key] = clean_env_value(value.strip())


def clean_env_value(value: str) -> str:
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        return value[1:-1]
    return value


def init_db(db_path: Path) -> None:
    db_path.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(db_path) as conn:
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS captures (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              captured_at TEXT NOT NULL,
              screenshot_path TEXT NOT NULL,
              status TEXT NOT NULL DEFAULT 'pending',
              error TEXT,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            """
        )
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS extractions (
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
            )
            """
        )
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


def migrate_legacy_captures(db_path: Path) -> int:
    with sqlite3.connect(db_path) as conn:
        conn.row_factory = sqlite3.Row
        rows = list(
            conn.execute(
                """
                SELECT
                  captures.id,
                  captures.captured_at,
                  captures.screenshot_path,
                  captures.status,
                  captures.error,
                  e.id AS extraction_id,
                  e.model,
                  e.activity_type,
                  e.description,
                  e.app_context,
                  e.project,
                  e.topics_json,
                  e.people_json,
                  e.key_content,
                  e.visible_text_json,
                  e.raw_json
                FROM captures
                LEFT JOIN extractions e ON e.id = (
                  SELECT e2.id
                  FROM extractions e2
                  WHERE e2.capture_id = captures.id
                  ORDER BY e2.id DESC
                  LIMIT 1
                )
                WHERE captures.status IN ('pending', 'processed')
                ORDER BY captures.captured_at ASC, captures.id ASC
                """
            )
        )
        migrated = 0
        for row in rows:
            path = Path(row["screenshot_path"])
            if not path.is_file():
                conn.execute(
                    "UPDATE captures SET status = 'failed', error = ? WHERE id = ?",
                    ("screenshot file is missing", row["id"]),
                )
                continue

            has_extraction = row["extraction_id"] is not None
            status = "processed" if has_extraction else "pending"
            cursor = conn.execute(
                """
                INSERT INTO capture_moments (captured_at, status, error)
                VALUES (?, ?, NULL)
                """,
                (row["captured_at"], status),
            )
            moment_id = int(cursor.lastrowid)
            conn.execute(
                """
                INSERT INTO capture_images (
                  moment_id,
                  display_number,
                  screenshot_path,
                  width,
                  height
                ) VALUES (?, 1, ?, NULL, NULL)
                """,
                (moment_id, row["screenshot_path"]),
            )
            if has_extraction:
                conn.execute(
                    """
                    INSERT INTO moment_extractions (
                      moment_id,
                      model,
                      activity_type,
                      description,
                      app_context,
                      project,
                      topics_json,
                      people_json,
                      key_content,
                      visible_text_json,
                      displays_json,
                      raw_json
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '[]', ?)
                    """,
                    (
                        moment_id,
                        row["model"],
                        row["activity_type"],
                        row["description"],
                        row["app_context"],
                        row["project"],
                        row["topics_json"] or "[]",
                        row["people_json"] or "[]",
                        row["key_content"],
                        row["visible_text_json"] or "[]",
                        row["raw_json"] or "{}",
                    ),
                )
            conn.execute(
                "UPDATE captures SET status = 'migrated', error = NULL WHERE id = ?",
                (row["id"],),
            )
            migrated += 1
        return migrated


def pending_moments(db_path: Path, limit: int) -> list[sqlite3.Row]:
    with sqlite3.connect(db_path) as conn:
        conn.row_factory = sqlite3.Row
        return list(
            conn.execute(
                """
                SELECT id, captured_at
                FROM capture_moments
                WHERE status IN ('pending', 'partial')
                  AND NOT EXISTS (
                    SELECT 1
                    FROM moment_extractions
                    WHERE moment_extractions.moment_id = capture_moments.id
                  )
                ORDER BY captured_at ASC, id ASC
                LIMIT ?
                """,
                (limit,),
            )
        )


def moment_images(db_path: Path, moment_id: int) -> list[sqlite3.Row]:
    with sqlite3.connect(db_path) as conn:
        conn.row_factory = sqlite3.Row
        return list(
            conn.execute(
                """
                SELECT display_number, screenshot_path, width, height
                FROM capture_images
                WHERE moment_id = ?
                ORDER BY display_number ASC
                """,
                (moment_id,),
            )
        )


def mark_moment(db_path: Path, moment_id: int, status: str, error: str | None = None) -> None:
    with sqlite3.connect(db_path) as conn:
        conn.execute(
            "UPDATE capture_moments SET status = ?, error = ? WHERE id = ?",
            (status, error, moment_id),
        )


def insert_moment_extraction(
    db_path: Path,
    *,
    moment_id: int,
    model: str,
    result: dict[str, Any],
    raw_json: str,
) -> None:
    with sqlite3.connect(db_path) as conn:
        conn.execute(
            """
            INSERT INTO moment_extractions (
              moment_id,
              model,
              activity_type,
              description,
              app_context,
              project,
              topics_json,
              people_json,
              key_content,
              visible_text_json,
              displays_json,
              raw_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                moment_id,
                model,
                result.get("activity_type"),
                result["description"],
                result.get("app_context"),
                result.get("project"),
                json.dumps(result.get("topics", [])),
                json.dumps(result.get("people", [])),
                result.get("key_content"),
                json.dumps(result.get("visible_text", [])),
                json.dumps(result.get("displays", [])),
                raw_json,
            ),
        )


def image_data_url(path: Path) -> str:
    data = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:image/jpeg;base64,{data}"


def moment_prompt(moment: sqlite3.Row, images: list[sqlite3.Row]) -> str:
    image_lines = "\n".join(
        (
            f"- image_index: {index}, display_number: {row['display_number']}, "
            f"size: {row['width'] or 'unknown'}x{row['height'] or 'unknown'}, "
            f"path: {row['screenshot_path']}"
        )
        for index, row in enumerate(images, start=1)
    )
    return f"""
You are analyzing one workspace moment from Chris's computer.
All attached images were captured at the same timestamp from different displays.
Analyze them together as one full multi-monitor workspace, not as unrelated screenshots.
The main output should answer what Chris was working on across the whole setup.
Use display-level details only as supporting evidence and for debugging/inspection.

Return JSON only in this exact shape:
{{
  "moment_id": {moment['id']},
  "activity_type": "coding | browsing | communication | reading | writing | design | terminal | meeting | media | other",
  "description": "3-6 sentences describing what Chris was working on across the whole workspace",
  "app_context": "Specific app-level context and task, or null",
  "project": "Project or repo name if identifiable, otherwise null",
  "topics": ["topic"],
  "people": ["person or handle"],
  "key_content": "Dense summary of readable artifacts, files, URLs, commands, headings, or messages across all displays",
  "visible_text": ["verbatim visible text snippet"],
  "displays": [
    {{
      "display_number": 1,
      "description": "Brief evidence from this display that supports the workspace-level interpretation",
      "visible_text": ["verbatim text from this display"]
    }}
  ]
}}

Rules:
- Use moment_id {moment['id']} exactly.
- Include one displays entry per attached image, but keep it secondary to the workspace-level fields.
- Use the display_number values from the metadata.
- Do not invent details that are not visible.
- Prefer concrete labels, filenames, URLs, command text, headings, and error messages.

Moment metadata:
- moment_id: {moment['id']}
- captured_at: {moment['captured_at']}

Images:
{image_lines}
""".strip()


def parse_json_response(text: str) -> dict[str, Any]:
    raw = text.strip()
    if raw.startswith("```"):
        lines = raw.splitlines()
        if lines and lines[0].startswith("```"):
            lines = lines[1:]
        if lines and lines[-1].strip() == "```":
            lines = lines[:-1]
        raw = "\n".join(lines).strip()
    try:
        return json.loads(raw)
    except json.JSONDecodeError as error:
        raise ModelResponseError(f"model returned invalid JSON: {error}") from error


def analyze_moment(
    client: OpenAI,
    *,
    model: str,
    moment: sqlite3.Row,
    images: list[sqlite3.Row],
) -> tuple[str, dict[str, Any]]:
    content: list[dict[str, Any]] = [{"type": "text", "text": moment_prompt(moment, images)}]
    for row in images:
        content.append(
            {
                "type": "image_url",
                "image_url": {"url": image_data_url(Path(row["screenshot_path"]))},
            }
        )

    response = client.chat.completions.create(
        model=model,
        messages=[{"role": "user", "content": content}],
        temperature=0,
    )
    text = response.choices[0].message.content or ""
    return text, parse_json_response(text)


def process_moments(db_path: Path, moments: list[sqlite3.Row], model: str) -> int:
    api_key = os.environ.get("OPENROUTER_API_KEY")
    if not api_key:
        print("OPENROUTER_API_KEY is not set", file=sys.stderr)
        return 0

    client = OpenAI(api_key=api_key, base_url=OPENROUTER_BASE_URL)
    processed = 0

    for moment in moments:
        moment_id = int(moment["id"])
        images = moment_images(db_path, moment_id)
        available = [row for row in images if Path(row["screenshot_path"]).is_file()]
        if not available:
            mark_moment(db_path, moment_id, "failed", "moment has no screenshot files")
            continue

        try:
            raw_text, parsed = analyze_moment(client, model=model, moment=moment, images=available)
        except ModelResponseError as error:
            mark_moment(db_path, moment_id, "failed", str(error))
            print(f"moment {moment_id} processing failed: {error}", file=sys.stderr)
            continue
        except Exception as error:
            print(f"moment {moment_id} processing failed: {error}", file=sys.stderr)
            continue

        if int(parsed.get("moment_id", -1)) != moment_id:
            message = f"model response used the wrong moment_id: {parsed.get('moment_id')}"
            mark_moment(db_path, moment_id, "failed", message)
            print(f"moment {moment_id} {message}", file=sys.stderr)
            continue

        try:
            insert_moment_extraction(
                db_path,
                moment_id=moment_id,
                model=model,
                result=parsed,
                raw_json=raw_text,
            )
            mark_moment(db_path, moment_id, "processed")
            processed += 1
        except Exception as error:
            mark_moment(db_path, moment_id, "failed", f"failed to store extraction: {error}")

    print(f"processed {processed} moment(s)")
    return 0 if processed else 1


def parse_args() -> argparse.Namespace:
    load_env_file()
    parser = argparse.ArgumentParser(description="Process pending screenshots with OpenRouter.")
    parser.add_argument(
        "--root",
        type=Path,
        default=default_root(),
        help="Storage root. Defaults to ~/Pictures/Screencap.",
    )
    parser.add_argument(
        "--model",
        default=os.environ.get("OPENROUTER_MODEL", DEFAULT_MODEL),
        help="OpenRouter model. Defaults to OPENROUTER_MODEL or google/gemini-2.5-flash.",
    )
    parser.add_argument("--batch-size", type=int, default=1)
    parser.add_argument("--limit", type=int, default=5)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.batch_size < 1 or args.limit < 1:
        print("--batch-size and --limit must be positive", file=sys.stderr)
        return 2

    root = args.root.expanduser()
    db_path = root / "screencap.db"
    init_db(db_path)
    migrated = migrate_legacy_captures(db_path)
    if migrated:
        print(f"migrated {migrated} legacy capture(s) to moments")

    moments = pending_moments(db_path, min(args.batch_size, args.limit))
    if not moments:
        print("no pending moments")
        return 0

    return process_moments(db_path, moments, args.model)


if __name__ == "__main__":
    raise SystemExit(main())
