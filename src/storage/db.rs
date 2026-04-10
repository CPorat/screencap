//! SQLite database operations and migrations

use rusqlite::Connection;
use std::path::PathBuf;

/// Storage backend using SQLite
pub struct Storage {
    conn: Option<Connection>,
    db_path: PathBuf,
}

impl Storage {
    /// Create a new storage instance
    pub fn new() -> anyhow::Result<Self> {
        let db_path = Self::db_path()?;

        // Ensure the parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self {
            conn: None,
            db_path,
        })
    }

    /// Get the database file path
    pub fn db_path() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        Ok(home.join(".screencap").join("screencap.db"))
    }

    /// Initialize the database with schema
    pub fn initialize(&self) -> anyhow::Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Create tables
        conn.execute_batch(SCHEMA)?;

        Ok(())
    }

    /// Get a connection to the database
    pub fn connection(&mut self) -> anyhow::Result<&Connection> {
        if self.conn.is_none() {
            self.conn = Some(Connection::open(&self.db_path)?);
        }
        Ok(self.conn.as_ref().unwrap())
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::new().expect("Failed to initialize storage")
    }
}

/// Database schema
const SCHEMA: &str = r#"
-- Raw captures from Layer 1
CREATE TABLE IF NOT EXISTS captures (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    app_name TEXT,
    window_title TEXT,
    bundle_id TEXT,
    display_id INTEGER,
    screenshot_path TEXT NOT NULL,
    extraction_status TEXT DEFAULT 'pending',
    extraction_id INTEGER REFERENCES extractions(id),
    created_at TEXT DEFAULT (datetime('now'))
);

-- Structured extractions from Layer 2 (per-frame)
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
    created_at TEXT DEFAULT (datetime('now'))
);

-- Batch summaries from Layer 2
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
    created_at TEXT DEFAULT (datetime('now'))
);

-- Insights from Layer 3
CREATE TABLE IF NOT EXISTS insights (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    type TEXT NOT NULL,
    window_start TEXT NOT NULL,
    window_end TEXT NOT NULL,
    data TEXT NOT NULL,
    narrative TEXT,
    model_used TEXT,
    tokens_used INTEGER,
    cost_cents REAL,
    created_at TEXT DEFAULT (datetime('now'))
);

-- Full-text search across extractions
CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
    description,
    key_content,
    project,
    topics,
    content=extractions,
    content_rowid=id
);

-- Full-text search across insights
CREATE VIRTUAL TABLE IF NOT EXISTS insights_fts USING fts5(
    narrative,
    content=insights,
    content_rowid=id
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_captures_timestamp ON captures(timestamp);
CREATE INDEX IF NOT EXISTS idx_captures_app ON captures(app_name);
CREATE INDEX IF NOT EXISTS idx_captures_extraction_status ON captures(extraction_status);
CREATE INDEX IF NOT EXISTS idx_extractions_batch ON extractions(batch_id);
CREATE INDEX IF NOT EXISTS idx_extractions_project ON extractions(project);
CREATE INDEX IF NOT EXISTS idx_extractions_activity ON extractions(activity_type);
CREATE INDEX IF NOT EXISTS idx_insights_type_window ON insights(type, window_start);
"#;
