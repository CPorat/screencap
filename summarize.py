#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "openai>=1.0.0",
# ]
# ///

from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
from datetime import date, datetime, time, timedelta, timezone
from pathlib import Path
from typing import Any
from zoneinfo import ZoneInfo

from openai import OpenAI

DEFAULT_MODEL = "google/gemini-2.5-flash"
OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1"


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


def iso_utc(value: datetime) -> str:
    return value.astimezone(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def init_db(db_path: Path) -> None:
    db_path.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(db_path) as conn:
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS summaries (
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
            )
            """
        )


def period_bounds(period: str, target: date | None, tz: ZoneInfo) -> tuple[datetime, datetime]:
    now = datetime.now(tz)
    if period == "day":
        day = target or (now.date() - timedelta(days=1))
        start = datetime.combine(day, time.min, tzinfo=tz)
        return start, start + timedelta(days=1)

    if period == "week":
        if target:
            week_start_day = target - timedelta(days=target.weekday())
        else:
            this_week_start = now.date() - timedelta(days=now.date().weekday())
            week_start_day = this_week_start - timedelta(days=7)
        start = datetime.combine(week_start_day, time.min, tzinfo=tz)
        return start, start + timedelta(days=7)

    if period == "month":
        if target:
            month_start_day = target.replace(day=1)
        else:
            this_month_start = now.date().replace(day=1)
            month_start_day = (this_month_start - timedelta(days=1)).replace(day=1)
        start = datetime.combine(month_start_day, time.min, tzinfo=tz)
        if month_start_day.month == 12:
            next_month = month_start_day.replace(year=month_start_day.year + 1, month=1)
        else:
            next_month = month_start_day.replace(month=month_start_day.month + 1)
        return start, datetime.combine(next_month, time.min, tzinfo=tz)

    raise ValueError(f"unsupported period: {period}")


def rows_for_period(db_path: Path, start_utc: str, end_utc: str) -> list[sqlite3.Row]:
    try:
        with sqlite3.connect(db_path) as conn:
            conn.row_factory = sqlite3.Row
            return list(
                conn.execute(
                    """
                    SELECT
                      cm.id AS moment_id,
                      cm.captured_at,
                      me.activity_type,
                      me.description,
                      me.app_context,
                      me.project,
                      me.topics_json,
                      me.people_json,
                      me.key_content,
                      me.visible_text_json
                    FROM moment_extractions me
                    JOIN capture_moments cm ON cm.id = me.moment_id
                    JOIN (
                      SELECT moment_id, MAX(id) AS latest_extraction_id
                      FROM moment_extractions
                      GROUP BY moment_id
                    ) latest ON latest.latest_extraction_id = me.id
                    WHERE cm.captured_at >= ? AND cm.captured_at < ?
                    ORDER BY cm.captured_at ASC, cm.id ASC
                    """,
                    (start_utc, end_utc),
                )
            )
    except sqlite3.OperationalError as error:
        if "no such table" in str(error):
            return []
        raise


def summaries_for_period(db_path: Path, source_period: str, start_utc: str, end_utc: str) -> list[sqlite3.Row]:
    try:
        with sqlite3.connect(db_path) as conn:
            conn.row_factory = sqlite3.Row
            return list(
                conn.execute(
                    """
                    SELECT
                      period,
                      period_start,
                      period_end,
                      headline,
                      narrative,
                      projects_json,
                      topics_json,
                      people_json,
                      timeline_json,
                      open_threads_json
                    FROM summaries
                    WHERE period = ?
                      AND period_start >= ?
                      AND period_start < ?
                    ORDER BY period_start ASC
                    """,
                    (source_period, start_utc, end_utc),
                )
            )
    except sqlite3.OperationalError as error:
        if "no such table" in str(error):
            return []
        raise


def decode_json(raw: str | None) -> Any:
    if not raw:
        return []
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return []


def moment_source_items(rows: list[sqlite3.Row], max_items: int) -> list[dict[str, Any]]:
    items = []
    for row in rows[-max_items:]:
        items.append(
            {
                "moment_id": row["moment_id"],
                "captured_at": row["captured_at"],
                "activity_type": row["activity_type"],
                "description": row["description"],
                "app_context": row["app_context"],
                "project": row["project"],
                "topics": decode_json(row["topics_json"]),
                "people": decode_json(row["people_json"]),
                "key_content": row["key_content"],
                "visible_text": decode_json(row["visible_text_json"])[:8],
            }
        )
    return items


def summary_source_items(rows: list[sqlite3.Row]) -> list[dict[str, Any]]:
    items = []
    for row in rows:
        items.append(
            {
                "period": row["period"],
                "period_start": row["period_start"],
                "period_end": row["period_end"],
                "headline": row["headline"],
                "narrative": row["narrative"],
                "projects": decode_json(row["projects_json"]),
                "topics": decode_json(row["topics_json"]),
                "people": decode_json(row["people_json"]),
                "timeline": decode_json(row["timeline_json"]),
                "open_threads": decode_json(row["open_threads_json"]),
            }
        )
    return items


def source_items(
    db_path: Path,
    period: str,
    start_utc: str,
    end_utc: str,
    max_moments: int,
) -> tuple[str, list[dict[str, Any]]]:
    if period == "week":
        rows = summaries_for_period(db_path, "day", start_utc, end_utc)
        if rows:
            return "daily_summaries", summary_source_items(rows)
    elif period == "month":
        rows = summaries_for_period(db_path, "week", start_utc, end_utc)
        if rows:
            return "weekly_summaries", summary_source_items(rows)
        rows = summaries_for_period(db_path, "day", start_utc, end_utc)
        if rows:
            return "daily_summaries", summary_source_items(rows)

    rows = rows_for_period(db_path, start_utc, end_utc)
    return "moment_extractions", moment_source_items(rows, max_moments)


def summary_prompt(
    *,
    period: str,
    period_start: str,
    period_end: str,
    source_kind: str,
    items: list[dict[str, Any]],
) -> str:
    return f"""
You are summarizing Chris's computer activity.
The input is structured screen-memory data, not complete ground truth.
Prefer concrete work, projects, files, apps, topics, and unresolved threads.
Do not invent details beyond the input.

Return JSON only in this exact shape:
{{
  "period": "{period}",
  "period_start": "{period_start}",
  "period_end": "{period_end}",
  "headline": "one sentence summary of the period",
  "narrative": "5-10 sentences describing the main work and context",
  "projects": [
    {{"name": "project or repo", "summary": "what happened", "evidence": ["specific evidence"]}}
  ],
  "topics": ["topic"],
  "people": ["person or handle"],
  "timeline": [
    {{"time_range": "rough time range", "summary": "what was happening"}}
  ],
  "open_threads": ["unresolved follow-up or question visible in the work"],
  "confidence": "high | medium | low"
}}

Source kind: {source_kind}
Source items:
{json.dumps(items, indent=2)}
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
    return json.loads(raw)


def analyze_summary(client: OpenAI, model: str, prompt: str) -> tuple[str, dict[str, Any]]:
    response = client.chat.completions.create(
        model=model,
        messages=[{"role": "user", "content": prompt}],
        temperature=0,
    )
    text = response.choices[0].message.content or ""
    return text, parse_json_response(text)


def upsert_summary(
    db_path: Path,
    *,
    period: str,
    period_start: str,
    period_end: str,
    model: str,
    result: dict[str, Any],
    raw_json: str,
) -> None:
    with sqlite3.connect(db_path) as conn:
        conn.execute(
            """
            INSERT INTO summaries (
              period,
              period_start,
              period_end,
              model,
              headline,
              narrative,
              projects_json,
              topics_json,
              people_json,
              timeline_json,
              open_threads_json,
              raw_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(period, period_start, period_end) DO UPDATE SET
              model = excluded.model,
              headline = excluded.headline,
              narrative = excluded.narrative,
              projects_json = excluded.projects_json,
              topics_json = excluded.topics_json,
              people_json = excluded.people_json,
              timeline_json = excluded.timeline_json,
              open_threads_json = excluded.open_threads_json,
              raw_json = excluded.raw_json,
              created_at = CURRENT_TIMESTAMP
            """,
            (
                period,
                period_start,
                period_end,
                model,
                result["headline"],
                result["narrative"],
                json.dumps(result.get("projects", [])),
                json.dumps(result.get("topics", [])),
                json.dumps(result.get("people", [])),
                json.dumps(result.get("timeline", [])),
                json.dumps(result.get("open_threads", [])),
                raw_json,
            ),
        )


def parse_args() -> argparse.Namespace:
    load_env_file()
    parser = argparse.ArgumentParser(description="Summarize processed Screencap moments.")
    parser.add_argument(
        "--root",
        type=Path,
        default=default_root(),
        help="Storage root. Defaults to ~/Pictures/Screencap.",
    )
    parser.add_argument("--period", choices=["day", "week", "month"], required=True)
    parser.add_argument(
        "--date",
        type=date.fromisoformat,
        help="Local date inside the period to summarize. Defaults to the previous completed period.",
    )
    parser.add_argument(
        "--timezone",
        default=os.environ.get("SCREENCAP_TIMEZONE", "America/New_York"),
        help="IANA timezone for day/week/month boundaries. Defaults to SCREENCAP_TIMEZONE or America/New_York.",
    )
    parser.add_argument(
        "--model",
        default=os.environ.get(
            "OPENROUTER_SUMMARY_MODEL",
            os.environ.get("OPENROUTER_MODEL", DEFAULT_MODEL),
        ),
        help="OpenRouter model. Defaults to OPENROUTER_SUMMARY_MODEL, OPENROUTER_MODEL, or google/gemini-2.5-flash.",
    )
    parser.add_argument(
        "--max-moments",
        type=int,
        default=500,
        help="Maximum raw moment extractions to send when summary rollups are unavailable.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.max_moments < 1:
        print("--max-moments must be positive", file=sys.stderr)
        return 2

    try:
        tz = ZoneInfo(args.timezone)
    except Exception:
        print(f"invalid timezone: {args.timezone}", file=sys.stderr)
        return 2

    root = args.root.expanduser()
    db_path = root / "screencap.db"
    init_db(db_path)

    start_local, end_local = period_bounds(args.period, args.date, tz)
    start_utc = iso_utc(start_local)
    end_utc = iso_utc(end_local)
    source_kind, items = source_items(db_path, args.period, start_utc, end_utc, args.max_moments)

    if not items:
        print(f"no source rows for {args.period} {start_utc} to {end_utc}")
        return 0

    api_key = os.environ.get("OPENROUTER_API_KEY")
    if not api_key:
        print("OPENROUTER_API_KEY is not set", file=sys.stderr)
        return 0

    prompt = summary_prompt(
        period=args.period,
        period_start=start_utc,
        period_end=end_utc,
        source_kind=source_kind,
        items=items,
    )

    client = OpenAI(api_key=api_key, base_url=OPENROUTER_BASE_URL)
    try:
        raw_text, parsed = analyze_summary(client, args.model, prompt)
    except Exception as error:
        print(f"summary failed: {error}", file=sys.stderr)
        return 1

    if parsed.get("period") != args.period:
        print("summary response used the wrong period", file=sys.stderr)
        return 1

    upsert_summary(
        db_path,
        period=args.period,
        period_start=start_utc,
        period_end=end_utc,
        model=args.model,
        result=parsed,
        raw_json=raw_text,
    )
    print(f"summarized period={args.period} source={source_kind} items={len(items)} start={start_utc}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
