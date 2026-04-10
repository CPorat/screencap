use std::{
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, types::Type, Connection, OptionalExtension, Row, Transaction};
#[cfg(test)]
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::config::AppConfig;

use super::models::{
    decode_string_list, encode_string_list, format_db_timestamp, parse_db_timestamp, ActivityType,
    Capture, Extraction, ExtractionBatch, ExtractionSearchHit, ExtractionStatus, Insight,
    InsightData, NewCapture, NewExtraction, NewExtractionBatch, NewInsight, Sentiment,
};

const SCHEMA: &str = r#"
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

CREATE TABLE IF NOT EXISTS insights (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    type TEXT NOT NULL,
    window_start TEXT NOT NULL,
    window_end TEXT NOT NULL,
    data TEXT NOT NULL,
    narrative TEXT NOT NULL,
    model_used TEXT,
    tokens_used INTEGER,
    cost_cents REAL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Search data spans extraction rows and their batch narratives, so keep a dedicated FTS index keyed by extraction_id.
CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
    extraction_id UNINDEXED,
    description,
    key_content,
    narrative,
    project,
    topics
);

CREATE VIRTUAL TABLE IF NOT EXISTS insights_fts USING fts5(
    insight_id UNINDEXED,
    narrative
);

CREATE INDEX IF NOT EXISTS idx_captures_timestamp ON captures(timestamp);
CREATE INDEX IF NOT EXISTS idx_captures_app ON captures(app_name);
CREATE INDEX IF NOT EXISTS idx_captures_extraction_status ON captures(extraction_status);
CREATE INDEX IF NOT EXISTS idx_extractions_batch ON extractions(batch_id);
CREATE INDEX IF NOT EXISTS idx_extractions_project ON extractions(project);
CREATE INDEX IF NOT EXISTS idx_extractions_activity ON extractions(activity_type);
CREATE INDEX IF NOT EXISTS idx_insights_type_window ON insights(type, window_start);
"#;

#[derive(Debug)]
pub struct StorageDb {
    conn: Connection,
    path: Option<PathBuf>,
}

impl StorageDb {
    pub fn open(config: &AppConfig) -> Result<Self> {
        let path = Self::database_path(config)?;
        Self::open_at_path(path)
    }

    pub fn database_path(config: &AppConfig) -> Result<PathBuf> {
        let home = home_dir()?;
        Ok(config.storage_root(&home).join("screencap.db"))
    }

    pub fn open_at_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create database directory at {}",
                    parent.display()
                )
            })?;
        }

        let conn = Connection::open(&path)
            .with_context(|| format!("failed to open sqlite database at {}", path.display()))?;
        initialize_connection(&conn)?;

        Ok(Self {
            conn,
            path: Some(path),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn =
            Connection::open_in_memory().context("failed to open in-memory sqlite database")?;
        initialize_connection(&conn)?;

        Ok(Self { conn, path: None })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn insert_capture(&mut self, capture: &NewCapture) -> Result<Capture> {
        let created_at = Utc::now();
        let id = insert_capture_record(&self.conn, capture, &created_at)
            .context("failed to insert capture")?;

        Ok(materialize_capture(id, capture, &created_at))
    }

    pub fn insert_captures(&mut self, captures: &[NewCapture]) -> Result<Vec<Capture>> {
        let created_at = Utc::now();
        let tx = self
            .conn
            .transaction()
            .context("failed to start capture transaction")?;
        let mut inserted = Vec::with_capacity(captures.len());

        for capture in captures {
            let id = insert_capture_record(&tx, capture, &created_at).with_context(|| {
                format!(
                    "failed to insert capture for screenshot {}",
                    capture.screenshot_path
                )
            })?;
            inserted.push(materialize_capture(id, capture, &created_at));
        }

        tx.commit()
            .context("failed to commit capture transaction")?;

        Ok(inserted)
    }

    pub fn get_captures_by_timerange(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Capture>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    id,
                    timestamp,
                    app_name,
                    window_title,
                    bundle_id,
                    display_id,
                    screenshot_path,
                    extraction_status,
                    extraction_id,
                    created_at
                 FROM captures
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp ASC, id ASC",
            )
            .context("failed to prepare capture time-range query")?;

        let captures = stmt
            .query_map(
                params![format_db_timestamp(&start), format_db_timestamp(&end)],
                map_capture_row,
            )
            .context("failed to query captures by time range")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to map captures by time range")?;

        Ok(captures)
    }

    pub fn count_captures_in_window(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*)
                 FROM captures
                 WHERE timestamp >= ?1 AND timestamp < ?2",
                params![format_db_timestamp(&start), format_db_timestamp(&end)],
                |row| row.get(0),
            )
            .context("failed to count captures in time window")?;

        u64::try_from(count).context("capture count overflowed u64")
    }

    pub fn get_pending_captures(&self) -> Result<Vec<Capture>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    id,
                    timestamp,
                    app_name,
                    window_title,
                    bundle_id,
                    display_id,
                    screenshot_path,
                    extraction_status,
                    extraction_id,
                    created_at
                 FROM captures
                 WHERE extraction_status = ?1
                 ORDER BY timestamp ASC, id ASC",
            )
            .context("failed to prepare pending capture query")?;

        let captures = stmt
            .query_map(params![ExtractionStatus::Pending.as_str()], map_capture_row)
            .context("failed to query pending captures")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to map pending captures")?;

        Ok(captures)
    }

    pub fn get_latest_capture(&self) -> Result<Option<Capture>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    id,
                    timestamp,
                    app_name,
                    window_title,
                    bundle_id,
                    display_id,
                    screenshot_path,
                    extraction_status,
                    extraction_id,
                    created_at
                 FROM captures
                 ORDER BY timestamp DESC, id DESC
                 LIMIT 1",
            )
            .context("failed to prepare latest capture query")?;

        let capture = stmt
            .query_row([], map_capture_row)
            .optional()
            .context("failed to query latest capture")?;

        Ok(capture)
    }

    pub fn update_capture_status(
        &mut self,
        capture_id: i64,
        status: ExtractionStatus,
        extraction_id: Option<i64>,
    ) -> Result<()> {
        let rows = self
            .conn
            .execute(
                "UPDATE captures
                 SET extraction_status = ?1, extraction_id = ?2
                 WHERE id = ?3",
                params![status.as_str(), extraction_id, capture_id],
            )
            .with_context(|| format!("failed to update capture {capture_id} status"))?;

        if rows == 0 {
            return Err(anyhow!("capture {capture_id} does not exist"));
        }

        Ok(())
    }

    pub fn insert_extraction_batch(
        &mut self,
        batch: &NewExtractionBatch,
    ) -> Result<ExtractionBatch> {
        let created_at = Utc::now();
        let tx = self
            .conn
            .transaction()
            .context("failed to start extraction batch transaction")?;
        tx.execute(
            "INSERT INTO extraction_batches (
                id,
                batch_start,
                batch_end,
                capture_count,
                primary_activity,
                project_context,
                narrative,
                raw_response,
                model_used,
                tokens_used,
                cost_cents,
                created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                batch.id.to_string(),
                format_db_timestamp(&batch.batch_start),
                format_db_timestamp(&batch.batch_end),
                batch.capture_count,
                batch.primary_activity.as_deref(),
                batch.project_context.as_deref(),
                batch.narrative.as_deref(),
                batch.raw_response.as_deref(),
                batch.model_used.as_deref(),
                batch.tokens_used,
                batch.cost_cents,
                format_db_timestamp(&created_at),
            ],
        )
        .context("failed to insert extraction batch")?;
        sync_batch_search_index(&tx, batch.id)?;
        tx.commit()
            .context("failed to commit extraction batch transaction")?;

        Ok(ExtractionBatch {
            id: batch.id,
            batch_start: batch.batch_start,
            batch_end: batch.batch_end,
            capture_count: batch.capture_count,
            primary_activity: batch.primary_activity.clone(),
            project_context: batch.project_context.clone(),
            narrative: batch.narrative.clone(),
            raw_response: batch.raw_response.clone(),
            model_used: batch.model_used.clone(),
            tokens_used: batch.tokens_used,
            cost_cents: batch.cost_cents,
            created_at,
        })
    }

    pub fn insert_extraction(&mut self, extraction: &NewExtraction) -> Result<Extraction> {
        let created_at = Utc::now();
        let topics = encode_string_list(&extraction.topics)?;
        let people = encode_string_list(&extraction.people)?;

        let tx = self
            .conn
            .transaction()
            .context("failed to start extraction transaction")?;
        tx.execute(
            "INSERT INTO extractions (
                capture_id,
                batch_id,
                activity_type,
                description,
                app_context,
                project,
                topics,
                people,
                key_content,
                sentiment,
                created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                extraction.capture_id,
                extraction.batch_id.to_string(),
                extraction.activity_type.map(ActivityType::as_str),
                extraction.description.as_deref(),
                extraction.app_context.as_deref(),
                extraction.project.as_deref(),
                topics,
                people,
                extraction.key_content.as_deref(),
                extraction.sentiment.map(Sentiment::as_str),
                format_db_timestamp(&created_at),
            ],
        )
        .context("failed to insert extraction")?;
        let extraction_id = tx.last_insert_rowid();
        sync_search_index_for_extraction(&tx, extraction_id)?;
        tx.commit()
            .context("failed to commit extraction transaction")?;

        Ok(Extraction {
            id: extraction_id,
            capture_id: extraction.capture_id,
            batch_id: extraction.batch_id,
            activity_type: extraction.activity_type,
            description: extraction.description.clone(),
            app_context: extraction.app_context.clone(),
            project: extraction.project.clone(),
            topics: extraction.topics.clone(),
            people: extraction.people.clone(),
            key_content: extraction.key_content.clone(),
            sentiment: extraction.sentiment,
            created_at,
        })
    }

    pub fn insert_insight(&mut self, insight: &NewInsight) -> Result<Insight> {
        validate_insight_payload(insight)?;

        let created_at = Utc::now();
        let payload =
            serde_json::to_string(&insight.data).context("failed to serialize insight payload")?;
        let narrative = insight.data.narrative_text().to_owned();

        let tx = self
            .conn
            .transaction()
            .context("failed to start insight transaction")?;
        tx.execute(
            "INSERT INTO insights (
                type,
                window_start,
                window_end,
                data,
                narrative,
                model_used,
                tokens_used,
                cost_cents,
                created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                insight.insight_type.as_str(),
                format_db_timestamp(&insight.window_start),
                format_db_timestamp(&insight.window_end),
                payload,
                narrative,
                insight.model_used.as_deref(),
                insight.tokens_used,
                insight.cost_cents,
                format_db_timestamp(&created_at),
            ],
        )
        .context("failed to insert insight")?;
        let insight_id = tx.last_insert_rowid();
        sync_insight_fts(&tx, insight_id)?;
        tx.commit()
            .context("failed to commit insight transaction")?;

        Ok(Insight {
            id: insight_id,
            insight_type: insight.insight_type,
            window_start: insight.window_start,
            window_end: insight.window_end,
            data: insight.data.clone(),
            narrative: insight.data.narrative_text().to_owned(),
            model_used: insight.model_used.clone(),
            tokens_used: insight.tokens_used,
            cost_cents: insight.cost_cents,
            created_at,
        })
    }

    pub fn search_extractions(&self, query: &str) -> Result<Vec<ExtractionSearchHit>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    e.id,
                    e.capture_id,
                    e.batch_id,
                    e.activity_type,
                    e.description,
                    e.app_context,
                    e.project,
                    e.topics,
                    e.people,
                    e.key_content,
                    e.sentiment,
                    e.created_at,
                    eb.narrative AS batch_narrative,
                    bm25(search_index) AS rank
                 FROM search_index
                 JOIN extractions e ON e.id = search_index.extraction_id
                 LEFT JOIN extraction_batches eb ON eb.id = e.batch_id
                 WHERE search_index MATCH ?1
                 ORDER BY rank ASC, e.id ASC",
            )
            .context("failed to prepare extraction search query")?;

        let match_query = build_fts_match_query(query)?;
        let results = stmt
            .query_map(params![match_query], map_search_hit_row)
            .with_context(|| format!("failed to search extractions for `{query}`"))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to map extraction search results")?;

        Ok(results)
    }
}

fn initialize_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)
        .context("failed to initialize sqlite schema")?;
    Ok(())
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable is not set"))
}

fn validate_insight_payload(insight: &NewInsight) -> Result<()> {
    if insight.data.insight_type() != insight.insight_type {
        return Err(anyhow!(
            "insight payload type {:?} does not match stored insight type {:?}",
            insight.data.insight_type(),
            insight.insight_type
        ));
    }

    match &insight.data {
        InsightData::Rolling {
            window_start,
            window_end,
            ..
        } => {
            if *window_start != insight.window_start || *window_end != insight.window_end {
                return Err(anyhow!(
                    "rolling insight payload window must match the indexed insight window"
                ));
            }
        }
        InsightData::Hourly {
            hour_start,
            hour_end,
            ..
        } => {
            if *hour_start != insight.window_start || *hour_end != insight.window_end {
                return Err(anyhow!(
                    "hourly insight payload window must match the indexed insight window"
                ));
            }
        }
        InsightData::Daily { date, .. } => {
            if *date != insight.window_start.date_naive() {
                return Err(anyhow!(
                    "daily insight date must match the indexed insight window start date"
                ));
            }
        }
    }

    Ok(())
}

fn build_fts_match_query(query: &str) -> Result<String> {
    let terms = query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        return Err(anyhow!("search query must not be empty"));
    }

    Ok(terms.join(" AND "))
}

fn sync_batch_search_index(tx: &Transaction<'_>, batch_id: Uuid) -> Result<()> {
    tx.execute(
        "DELETE FROM search_index
         WHERE extraction_id IN (
             SELECT id FROM extractions WHERE batch_id = ?1
         )",
        params![batch_id.to_string()],
    )
    .with_context(|| format!("failed to clear search index rows for batch {batch_id}"))?;

    tx.execute(
        "INSERT INTO search_index (extraction_id, description, key_content, narrative, project, topics)
         SELECT
             e.id,
             COALESCE(e.description, ''),
             COALESCE(e.key_content, ''),
             COALESCE(eb.narrative, ''),
             COALESCE(e.project, ''),
             COALESCE(e.topics, '')
         FROM extractions e
         LEFT JOIN extraction_batches eb ON eb.id = e.batch_id
         WHERE e.batch_id = ?1",
        params![batch_id.to_string()],
    )
    .with_context(|| format!("failed to rebuild search index rows for batch {batch_id}"))?;

    Ok(())
}

fn sync_search_index_for_extraction(tx: &Transaction<'_>, extraction_id: i64) -> Result<()> {
    tx.execute(
        "DELETE FROM search_index WHERE extraction_id = ?1",
        params![extraction_id],
    )
    .with_context(|| format!("failed to clear search index row for extraction {extraction_id}"))?;

    tx.execute(
        "INSERT INTO search_index (extraction_id, description, key_content, narrative, project, topics)
         SELECT
             e.id,
             COALESCE(e.description, ''),
             COALESCE(e.key_content, ''),
             COALESCE(eb.narrative, ''),
             COALESCE(e.project, ''),
             COALESCE(e.topics, '')
         FROM extractions e
         LEFT JOIN extraction_batches eb ON eb.id = e.batch_id
         WHERE e.id = ?1",
        params![extraction_id],
    )
    .with_context(|| format!("failed to rebuild search index row for extraction {extraction_id}"))?;

    Ok(())
}

fn sync_insight_fts(tx: &Transaction<'_>, insight_id: i64) -> Result<()> {
    tx.execute(
        "DELETE FROM insights_fts WHERE insight_id = ?1",
        params![insight_id],
    )
    .with_context(|| format!("failed to clear insight search row for insight {insight_id}"))?;

    tx.execute(
        "INSERT INTO insights_fts (insight_id, narrative)
         SELECT id, narrative FROM insights WHERE id = ?1",
        params![insight_id],
    )
    .with_context(|| format!("failed to rebuild insight search row for insight {insight_id}"))?;

    Ok(())
}

fn insert_capture_record(
    conn: &Connection,
    capture: &NewCapture,
    created_at: &DateTime<Utc>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO captures (
            timestamp,
            app_name,
            window_title,
            bundle_id,
            display_id,
            screenshot_path,
            extraction_status,
            extraction_id,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            format_db_timestamp(&capture.timestamp),
            capture.app_name.as_deref(),
            capture.window_title.as_deref(),
            capture.bundle_id.as_deref(),
            capture.display_id,
            capture.screenshot_path,
            ExtractionStatus::Pending.as_str(),
            Option::<i64>::None,
            format_db_timestamp(created_at),
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

fn materialize_capture(
    id: i64,
    capture: &NewCapture,
    created_at: &DateTime<Utc>,
) -> Capture {
    Capture {
        id,
        timestamp: capture.timestamp,
        app_name: capture.app_name.clone(),
        window_title: capture.window_title.clone(),
        bundle_id: capture.bundle_id.clone(),
        display_id: capture.display_id,
        screenshot_path: capture.screenshot_path.clone(),
        extraction_status: ExtractionStatus::Pending,
        extraction_id: None,
        created_at: *created_at,
    }
}

fn map_capture_row(row: &Row<'_>) -> rusqlite::Result<Capture> {
    Ok(Capture {
        id: row.get("id")?,
        timestamp: parse_timestamp_column(row.get("timestamp")?)?,
        app_name: row.get("app_name")?,
        window_title: row.get("window_title")?,
        bundle_id: row.get("bundle_id")?,
        display_id: row.get("display_id")?,
        screenshot_path: row.get("screenshot_path")?,
        extraction_status: parse_enum_column::<ExtractionStatus>(row.get("extraction_status")?)?,
        extraction_id: row.get("extraction_id")?,
        created_at: parse_timestamp_column(row.get("created_at")?)?,
    })
}

fn map_extraction_row(row: &Row<'_>) -> rusqlite::Result<Extraction> {
    Ok(Extraction {
        id: row.get("id")?,
        capture_id: row.get("capture_id")?,
        batch_id: parse_uuid_column(row.get("batch_id")?)?,
        activity_type: parse_optional_enum_column::<ActivityType>(row.get("activity_type")?)?,
        description: row.get("description")?,
        app_context: row.get("app_context")?,
        project: row.get("project")?,
        topics: parse_string_list_column(row.get("topics")?)?,
        people: parse_string_list_column(row.get("people")?)?,
        key_content: row.get("key_content")?,
        sentiment: parse_optional_enum_column::<Sentiment>(row.get("sentiment")?)?,
        created_at: parse_timestamp_column(row.get("created_at")?)?,
    })
}

fn map_search_hit_row(row: &Row<'_>) -> rusqlite::Result<ExtractionSearchHit> {
    Ok(ExtractionSearchHit {
        extraction: map_extraction_row(row)?,
        batch_narrative: row.get("batch_narrative")?,
        rank: row.get("rank")?,
    })
}

fn parse_timestamp_column(raw: String) -> rusqlite::Result<DateTime<Utc>> {
    parse_db_timestamp(&raw).map_err(into_row_error)
}

fn parse_uuid_column(raw: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&raw)
        .with_context(|| format!("failed to parse uuid `{raw}`"))
        .map_err(into_row_error)
}

fn parse_enum_column<T>(raw: String) -> rusqlite::Result<T>
where
    T: FromStr<Err = anyhow::Error>,
{
    T::from_str(&raw).map_err(into_row_error)
}

fn parse_optional_enum_column<T>(raw: Option<String>) -> rusqlite::Result<Option<T>>
where
    T: FromStr<Err = anyhow::Error>,
{
    raw.map(|value| T::from_str(&value).map_err(into_row_error))
        .transpose()
}

fn parse_string_list_column(raw: Option<String>) -> rusqlite::Result<Vec<String>> {
    decode_string_list(raw).map_err(into_row_error)
}

#[cfg(test)]
fn parse_json_column<T>(raw: String) -> rusqlite::Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse JSON payload `{raw}`"))
        .map_err(into_row_error)
}

fn into_row_error(error: anyhow::Error) -> rusqlite::Error {
    let io_error = std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string());
    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(io_error))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        time::{SystemTime, UNIX_EPOCH},
    };

    use chrono::{TimeZone, Timelike};
    use rusqlite::OptionalExtension;

    use super::*;
    use crate::storage::models::{
        DailyProjectSummary, FocusBlock, HourlyProjectSummary, InsightType,
    };

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        env::temp_dir()
            .join(format!("screencap-storage-tests-{name}-{unique}"))
            .join("screencap.db")
    }

    fn sample_capture(timestamp: DateTime<Utc>, suffix: &str) -> NewCapture {
        NewCapture {
            timestamp,
            app_name: Some("Code".into()),
            window_title: Some(format!("Editor {suffix}")),
            bundle_id: Some("com.microsoft.VSCode".into()),
            display_id: Some(1),
            screenshot_path: format!("/tmp/{suffix}.jpg"),
        }
    }

    #[test]
    fn open_at_path_creates_database_and_schema() {
        let path = temp_db_path("file");
        let parent = path.parent().unwrap().to_path_buf();

        let db = StorageDb::open_at_path(&path).expect("database should open");
        assert_eq!(db.path(), Some(path.as_path()));
        assert!(path.exists(), "database file should exist on disk");

        let mut stmt = db
            .connection()
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'index')")
            .expect("schema query should prepare");
        let objects = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("schema query should run")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("schema rows should map");

        for expected in [
            "captures",
            "extractions",
            "extraction_batches",
            "insights",
            "idx_captures_timestamp",
            "idx_captures_app",
            "idx_captures_extraction_status",
            "idx_extractions_batch",
            "idx_extractions_project",
            "idx_extractions_activity",
            "idx_insights_type_window",
        ] {
            assert!(
                objects.iter().any(|name| name == expected),
                "missing {expected}"
            );
        }

        let mut stmt = db
            .connection()
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('search_index', 'insights_fts')")
            .expect("fts query should prepare");
        let fts_tables = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("fts query should run")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("fts rows should map");
        assert!(fts_tables.iter().any(|name| name == "search_index"));
        assert!(fts_tables.iter().any(|name| name == "insights_fts"));

        fs::remove_dir_all(parent).expect("temp database dir should be removable");
    }

    #[test]
    fn captures_round_trip_and_pending_queries() {
        let mut db = StorageDb::open_in_memory().expect("database should open");
        let first_time = Utc
            .with_ymd_and_hms(2026, 4, 10, 14, 0, 0)
            .unwrap()
            .with_nanosecond(123_456_789)
            .unwrap();
        let second_time = Utc
            .with_ymd_and_hms(2026, 4, 10, 14, 5, 0)
            .unwrap()
            .with_nanosecond(987_654_321)
            .unwrap();

        let first = db
            .insert_capture(&sample_capture(first_time, "first"))
            .expect("first capture should insert");
        let second = db
            .insert_capture(&sample_capture(second_time, "second"))
            .expect("second capture should insert");

        let captures = db
            .get_captures_by_timerange(first_time, second_time)
            .expect("capture range query should work");
        assert_eq!(captures.len(), 2);
        assert_eq!(captures[0].id, first.id);
        assert_eq!(captures[0].timestamp, first_time);
        assert_eq!(captures[1].id, second.id);
        assert_eq!(captures[1].timestamp, second_time);
        assert!(captures
            .iter()
            .all(|capture| capture.extraction_status == ExtractionStatus::Pending));

        let pending = db
            .get_pending_captures()
            .expect("pending query should work");
        assert_eq!(pending.len(), 2);

        let count = db
            .count_captures_in_window(first_time, second_time)
            .expect("half-open capture count query should work");
        assert_eq!(count, 1);

        let count = db
            .count_captures_in_window(first_time, second_time + chrono::Duration::seconds(1))
            .expect("inclusive-by-next-second capture count query should work");
        assert_eq!(count, 2);

        let batch = db
            .insert_extraction_batch(&NewExtractionBatch {
                id: Uuid::new_v4(),
                batch_start: first_time,
                batch_end: second_time,
                capture_count: 1,
                primary_activity: Some("coding".into()),
                project_context: Some("screencap".into()),
                narrative: Some("Indexed capture update flow".into()),
                raw_response: None,
                model_used: None,
                tokens_used: None,
                cost_cents: None,
            })
            .expect("batch should insert");
        let extraction = db
            .insert_extraction(&NewExtraction {
                capture_id: first.id,
                batch_id: batch.id,
                activity_type: Some(ActivityType::Coding),
                description: Some("Updated capture status after extraction".into()),
                app_context: Some("Testing storage updates".into()),
                project: Some("screencap".into()),
                topics: vec!["storage".into()],
                people: vec![],
                key_content: Some("capture status".into()),
                sentiment: Some(Sentiment::Focused),
            })
            .expect("extraction should insert");

        db.update_capture_status(first.id, ExtractionStatus::Processed, Some(extraction.id))
            .expect("status update should work");
        let pending = db
            .get_pending_captures()
            .expect("pending query should work after update");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, second.id);

        let captures = db
            .get_captures_by_timerange(first_time, second_time)
            .expect("capture range query should still work");
        let updated = captures
            .into_iter()
            .find(|capture| capture.id == first.id)
            .unwrap();
        assert_eq!(updated.extraction_status, ExtractionStatus::Processed);
        assert_eq!(updated.extraction_id, Some(extraction.id));
    }

    #[test]
    fn extraction_search_returns_ranked_hits() {
        let mut db = StorageDb::open_in_memory().expect("database should open");
        let start = Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 4, 10, 14, 10, 0).unwrap();
        let batch = db
            .insert_extraction_batch(&NewExtractionBatch {
                id: Uuid::new_v4(),
                batch_start: start,
                batch_end: end,
                capture_count: 2,
                primary_activity: Some("coding".into()),
                project_context: Some("screencap".into()),
                narrative: Some("Debugging JWT refresh logic in the screencap auth module".into()),
                raw_response: None,
                model_used: Some("mock-model".into()),
                tokens_used: Some(123),
                cost_cents: Some(0.7),
            })
            .expect("batch should insert");

        let first_capture = db
            .insert_capture(&sample_capture(start, "jwt-heavy"))
            .expect("first capture should insert");
        let second_capture = db
            .insert_capture(&sample_capture(end, "jwt-light"))
            .expect("second capture should insert");

        let strong_hit = db
            .insert_extraction(&NewExtraction {
                capture_id: first_capture.id,
                batch_id: batch.id,
                activity_type: Some(ActivityType::Coding),
                description: Some("JWT JWT JWT refresh token bug hunt".into()),
                app_context: Some("Editing auth flow".into()),
                project: Some("screencap".into()),
                topics: vec!["jwt".into(), "auth".into()],
                people: vec![],
                key_content: Some("US-002/auth refresh_token_expires_at".into()),
                sentiment: Some(Sentiment::Focused),
            })
            .expect("strong extraction should insert");
        let weaker_hit = db
            .insert_extraction(&NewExtraction {
                capture_id: second_capture.id,
                batch_id: batch.id,
                activity_type: Some(ActivityType::Coding),
                description: Some("JWT follow-up notes".into()),
                app_context: Some("Reviewing auth flow".into()),
                project: Some("screencap".into()),
                topics: vec!["jwt".into()],
                people: vec![],
                key_content: Some("auth docs".into()),
                sentiment: Some(Sentiment::Exploring),
            })
            .expect("weaker extraction should insert");

        let hits = db.search_extractions("jwt").expect("search should succeed");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].extraction.id, strong_hit.id);
        assert_eq!(hits[1].extraction.id, weaker_hit.id);
        assert!(
            hits[0].rank <= hits[1].rank,
            "search results should be ordered by bm25 rank"
        );
        assert_eq!(
            hits[0].batch_narrative.as_deref(),
            Some("Debugging JWT refresh logic in the screencap auth module")
        );

        let punctuated_hits = db
            .search_extractions("US-002/auth")
            .expect("punctuated search should succeed");
        assert_eq!(punctuated_hits.len(), 1);
        assert_eq!(punctuated_hits[0].extraction.id, strong_hit.id);
    }

    #[test]
    fn inserts_insights_and_syncs_fts() {
        let mut db = StorageDb::open_in_memory().expect("database should open");
        let window_start = Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 14, 30, 0).unwrap();

        let insight = db
            .insert_insight(&NewInsight {
                insight_type: InsightType::Hourly,
                window_start,
                window_end,
                data: InsightData::Hourly {
                    hour_start: window_start,
                    hour_end: window_end,
                    dominant_activity: "coding".into(),
                    projects: vec![HourlyProjectSummary {
                        name: Some("screencap".into()),
                        minutes: 30,
                        activities: vec!["storage layer".into()],
                    }],
                    topics: vec!["sqlite".into(), "fts5".into()],
                    people_interacted: vec![],
                    key_moments: vec!["Built typed storage models".into()],
                    focus_score: 0.85,
                    narrative: "Spent the hour building the storage layer.".into(),
                },
                model_used: Some("mock-model".into()),
                tokens_used: Some(250),
                cost_cents: Some(1.25),
            })
            .expect("insight should insert");

        let stored_payload: String = db
            .connection()
            .query_row(
                "SELECT data FROM insights WHERE id = ?1",
                params![insight.id],
                |row| row.get(0),
            )
            .expect("insight payload should exist");
        let parsed: InsightData =
            parse_json_column(stored_payload).expect("insight payload should parse");
        assert_eq!(parsed, insight.data);

        let indexed_narrative: Option<String> = db
            .connection()
            .query_row(
                "SELECT narrative FROM insights_fts WHERE insight_id = ?1",
                params![insight.id],
                |row| row.get(0),
            )
            .optional()
            .expect("fts lookup should work");
        assert_eq!(
            indexed_narrative.as_deref(),
            Some("Spent the hour building the storage layer.")
        );
    }

    #[test]
    fn rejects_mismatched_insight_windows() {
        let mut db = StorageDb::open_in_memory().expect("database should open");
        let window_start = Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 14, 30, 0).unwrap();
        let mismatched_end = Utc.with_ymd_and_hms(2026, 4, 10, 15, 0, 0).unwrap();

        let error = db
            .insert_insight(&NewInsight {
                insight_type: InsightType::Rolling,
                window_start,
                window_end,
                data: InsightData::Rolling {
                    window_start,
                    window_end: mismatched_end,
                    current_focus: "Storage work".into(),
                    active_project: Some("screencap".into()),
                    apps_used: BTreeMap::new(),
                    context_switches: 1,
                    mood: "deep-focus".into(),
                    summary: "Worked on storage models.".into(),
                },
                model_used: None,
                tokens_used: None,
                cost_cents: None,
            })
            .expect_err("mismatched insight windows should fail");

        assert!(error.to_string().contains("rolling insight payload window"));
    }

    #[test]
    fn parses_daily_insight_payloads() {
        let mut db = StorageDb::open_in_memory().expect("database should open");
        let window_start = Utc.with_ymd_and_hms(2026, 4, 10, 0, 0, 0).unwrap();
        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 23, 59, 59).unwrap();

        let insight = db
            .insert_insight(&NewInsight {
                insight_type: InsightType::Daily,
                window_start,
                window_end,
                data: InsightData::Daily {
                    date: window_start.date_naive(),
                    total_active_hours: 7.5,
                    projects: vec![DailyProjectSummary {
                        name: "screencap".into(),
                        total_minutes: 195,
                        activities: vec!["storage layer".into(), "tests".into()],
                        key_accomplishments: vec!["Implemented SQLite schema".into()],
                    }],
                    time_allocation: BTreeMap::from([("coding".into(), "3h 15m".into())]),
                    focus_blocks: vec![FocusBlock {
                        start: "09:15".into(),
                        end: "11:45".into(),
                        duration_min: 150,
                        project: "screencap".into(),
                        quality: "deep-focus".into(),
                    }],
                    open_threads: vec!["Need to wire scheduler persistence".into()],
                    narrative: "A productive day on the storage layer.".into(),
                },
                model_used: Some("mock-model".into()),
                tokens_used: Some(400),
                cost_cents: Some(2.0),
            })
            .expect("daily insight should insert");

        assert_eq!(insight.insight_type, InsightType::Daily);
        assert_eq!(insight.window_start, window_start);
        assert_eq!(insight.window_end, window_end);
    }
}
