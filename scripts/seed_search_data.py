#!/usr/bin/env python3
from __future__ import annotations

import json
import sqlite3
import uuid
from datetime import datetime, timedelta, timezone
from pathlib import Path

DB_PATH = Path.home() / ".screencap" / "screencap.db"


def iso(value: datetime) -> str:
    return value.replace(microsecond=0).isoformat().replace("+00:00", "Z")


def ensure_schema(conn: sqlite3.Connection) -> None:
    conn.executescript(
        """
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS captures (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            app_name TEXT,
            window_title TEXT,
            bundle_id TEXT,
            display_id INTEGER,
            screenshot_path TEXT NOT NULL,
            extraction_status TEXT NOT NULL DEFAULT 'pending',
            extraction_id INTEGER REFERENCES extractions(id),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );

        CREATE TABLE IF NOT EXISTS extraction_batches (
            id TEXT PRIMARY KEY,
            batch_start TEXT NOT NULL,
            batch_end TEXT NOT NULL,
            capture_count INTEGER,
            primary_activity TEXT,
            project_context TEXT,
            narrative TEXT,
            raw_response TEXT,
            model_used TEXT,
            tokens_used INTEGER,
            cost_cents REAL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );

        CREATE TABLE IF NOT EXISTS extractions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            capture_id INTEGER NOT NULL REFERENCES captures(id),
            batch_id TEXT NOT NULL,
            activity_type TEXT,
            description TEXT,
            app_context TEXT,
            project TEXT,
            topics TEXT,
            people TEXT,
            key_content TEXT,
            sentiment TEXT,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );
        """
    )

    existing_columns = {
        row[1]
        for row in conn.execute("PRAGMA table_info(search_index)").fetchall()
    }
    if existing_columns and "extraction_id" not in existing_columns:
        conn.execute("DROP TABLE search_index")

    conn.execute(
        """
        CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
            extraction_id UNINDEXED,
            description,
            key_content,
            narrative,
            project,
            topics
        )
        """
    )

def seed(conn: sqlite3.Connection) -> int:
    now = datetime.now(timezone.utc)
    rows = [
        {
            "offset_minutes": 0,
            "app_name": "VS Code",
            "window_title": "automation.py — infra scripts",
            "bundle_id": "com.microsoft.VSCode",
            "project": "infra-automation",
            "topics": ["python", "automation", "deployment"],
            "description": "Writing Python script to automate deployment checks",
            "key_content": "python script validates staging config and rollout flags",
            "app_context": "editing a deployment helper script",
            "activity_type": "coding",
            "narrative": "Focused coding session improving automation reliability.",
            "screenshot_path": "seed://us018/python-automation.png",
        },
        {
            "offset_minutes": 7,
            "app_name": "Slack",
            "window_title": "#incident-response",
            "bundle_id": "com.tinyspeck.slackmacgap",
            "project": "incident-response",
            "topics": ["slack", "alerts", "triage"],
            "description": "Reading Slack message about a production alert",
            "key_content": "on-call thread requesting immediate error triage",
            "app_context": "reviewing teammate updates and status",
            "activity_type": "communication",
            "narrative": "Reviewed incident updates and coordinated next actions.",
            "screenshot_path": "seed://us018/slack-alert.png",
        },
        {
            "offset_minutes": 14,
            "app_name": "Google Chrome",
            "window_title": "Analytics Dashboard",
            "bundle_id": "com.google.Chrome",
            "project": "customer-research",
            "topics": ["analytics", "dashboard", "retention"],
            "description": "Reviewing analytics dashboard for retention trends",
            "key_content": "dashboard highlighted weekly retention drop in segment B",
            "app_context": "analyzing product usage metrics",
            "activity_type": "analysis",
            "narrative": "Investigated product analytics to identify churn signals.",
            "screenshot_path": "seed://us018/analytics-dashboard.png",
        },
    ]

    inserted = 0
    for index, row in enumerate(rows):
        ts = now - timedelta(minutes=row["offset_minutes"])
        created_at = iso(ts)
        batch_id = str(uuid.uuid4())

        conn.execute(
            """
            INSERT INTO extraction_batches (
                id, batch_start, batch_end, capture_count, primary_activity, project_context,
                narrative, raw_response, model_used, tokens_used, cost_cents, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, NULL, NULL, ?8)
            """,
            (
                batch_id,
                created_at,
                created_at,
                1,
                row["activity_type"],
                row["project"],
                row["narrative"],
                created_at,
            ),
        )

        cursor = conn.execute(
            """
            INSERT INTO captures (
                timestamp, app_name, window_title, bundle_id, display_id,
                screenshot_path, extraction_status, extraction_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'processed', NULL, ?7)
            """,
            (
                created_at,
                row["app_name"],
                row["window_title"],
                row["bundle_id"],
                index + 1,
                row["screenshot_path"],
                created_at,
            ),
        )
        capture_id = int(cursor.lastrowid)

        cursor = conn.execute(
            """
            INSERT INTO extractions (
                capture_id, batch_id, activity_type, description, app_context,
                project, topics, people, key_content, sentiment, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'exploring', ?10)
            """,
            (
                capture_id,
                batch_id,
                row["activity_type"],
                row["description"],
                row["app_context"],
                row["project"],
                json.dumps(row["topics"]),
                json.dumps([]),
                row["key_content"],
                created_at,
            ),
        )
        extraction_id = int(cursor.lastrowid)

        conn.execute(
            "UPDATE captures SET extraction_id = ?1 WHERE id = ?2",
            (extraction_id, capture_id),
        )

        conn.execute(
            """
            INSERT INTO search_index (extraction_id, description, key_content, narrative, project, topics)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            """,
            (
                extraction_id,
                row["description"],
                row["key_content"],
                row["narrative"],
                row["project"],
                " ".join(row["topics"]),
            ),
        )

        inserted += 1

    return inserted


def main() -> None:
    DB_PATH.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(DB_PATH) as conn:
        ensure_schema(conn)
        inserted = seed(conn)
        conn.commit()

    print(f"Seeded {inserted} capture/extraction/search rows into {DB_PATH}")


if __name__ == "__main__":
    main()
