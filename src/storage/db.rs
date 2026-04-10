use std::{
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{
    params, params_from_iter,
    types::{Type, Value},
    Connection, OpenFlags, OptionalExtension, Row, Transaction,
};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::config::AppConfig;

use super::models::{
    decode_string_list, encode_string_list, format_db_timestamp, parse_db_timestamp, ActivityType,
    AppCaptureCount, Capture, CaptureDetail, CaptureQuery, Extraction, ExtractionBatch,
    ExtractionBatchDetail, ExtractionFrameDetail, ExtractionSearchHit, ExtractionSearchQuery,
    ExtractionStatus, Insight, InsightData, InsightType, NewCapture, NewExtraction,
    NewExtractionBatch, NewInsight, ProjectTimeAllocation, Sentiment, TopicFrequency,
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

const CAPTURE_SELECT: &str = "SELECT
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
    FROM captures";

const EXTRACTION_BATCH_SELECT: &str = "SELECT
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
    FROM extraction_batches";

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

    pub fn open_existing_at_path(path: impl AsRef<Path>) -> Result<Option<Self>> {
        let path = path.as_ref().to_path_buf();
        match fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to stat sqlite database at {}", path.display())
                })
            }
        }

        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("failed to open sqlite database at {}", path.display()))?;
        configure_connection(&conn)?;

        Ok(Some(Self {
            conn,
            path: Some(path),
        }))
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
        self.get_pending_captures_with_limit(None)
    }

    pub fn get_pending_captures_batch(&self, limit: usize) -> Result<Vec<Capture>> {
        self.get_pending_captures_with_limit(Some(limit))
    }

    fn get_pending_captures_with_limit(&self, limit: Option<usize>) -> Result<Vec<Capture>> {
        if matches!(limit, Some(0)) {
            return Ok(Vec::new());
        }

        let mut sql = String::from(
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
        );

        let mut params = vec![Value::Text(ExtractionStatus::Pending.as_str().to_owned())];
        if let Some(limit) = limit {
            sql.push_str(" LIMIT ?2");
            params.push(Value::Integer(
                i64::try_from(limit).context("pending capture limit exceeds sqlite range")?,
            ));
        }

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("failed to prepare pending capture query")?;

        let captures = stmt
            .query_map(params_from_iter(params.iter()), map_capture_row)
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

    pub fn count_captures(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM captures", [], |row| row.get(0))
            .context("failed to count captures")?;

        u64::try_from(count).context("capture count overflowed u64")
    }

    pub fn list_captures(&self, query: &CaptureQuery) -> Result<Vec<Capture>> {
        let mut sql = String::from(CAPTURE_SELECT);
        let mut filters = Vec::new();
        let mut params = Vec::new();

        if let Some(from) = query.from.as_ref() {
            filters.push("timestamp >= ?");
            params.push(Value::Text(format_db_timestamp(from)));
        }

        if let Some(to) = query.to.as_ref() {
            filters.push("timestamp <= ?");
            params.push(Value::Text(format_db_timestamp(to)));
        }

        if let Some(app_name) = query.app_name.as_ref() {
            filters.push("app_name = ?");
            params.push(Value::Text(app_name.clone()));
        }

        if !filters.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&filters.join(" AND "));
        }

        sql.push_str(" ORDER BY timestamp ASC, id ASC LIMIT ? OFFSET ?");
        params.push(Value::Integer(
            i64::try_from(query.limit).context("capture query limit exceeds sqlite range")?,
        ));
        params.push(Value::Integer(
            i64::try_from(query.offset).context("capture query offset exceeds sqlite range")?,
        ));

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("failed to prepare filtered capture query")?;
        let captures = stmt
            .query_map(params_from_iter(params.iter()), map_capture_row)
            .context("failed to query filtered captures")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to map filtered captures")?;

        Ok(captures)
    }

    pub fn get_capture_detail(&self, capture_id: i64) -> Result<Option<CaptureDetail>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    captures.id,
                    captures.timestamp,
                    captures.app_name,
                    captures.window_title,
                    captures.bundle_id,
                    captures.display_id,
                    captures.screenshot_path,
                    captures.extraction_status,
                    captures.extraction_id,
                    captures.created_at,
                    extractions.id AS detail_extraction_id,
                    extractions.capture_id AS detail_extraction_capture_id,
                    extractions.batch_id AS detail_extraction_batch_id,
                    extractions.activity_type AS detail_extraction_activity_type,
                    extractions.description AS detail_extraction_description,
                    extractions.app_context AS detail_extraction_app_context,
                    extractions.project AS detail_extraction_project,
                    extractions.topics AS detail_extraction_topics,
                    extractions.people AS detail_extraction_people,
                    extractions.key_content AS detail_extraction_key_content,
                    extractions.sentiment AS detail_extraction_sentiment,
                    extractions.created_at AS detail_extraction_created_at
                 FROM captures
                 LEFT JOIN extractions ON extractions.id = captures.extraction_id
                 WHERE captures.id = ?1",
            )
            .with_context(|| format!("failed to prepare capture detail query for {capture_id}"))?;

        let detail = stmt
            .query_row(params![capture_id], |row| {
                Ok(CaptureDetail {
                    capture: map_capture_row(row)?,
                    extraction: map_optional_detail_extraction_row(row)?,
                })
            })
            .optional()
            .with_context(|| format!("failed to query capture detail for {capture_id}"))?;

        Ok(detail)
    }

    pub fn list_extraction_batch_details_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<ExtractionBatchDetail>> {
        let mut batch_stmt = self
            .conn
            .prepare(&format!(
                "{EXTRACTION_BATCH_SELECT}
                 WHERE batch_end >= ?1 AND batch_start <= ?2
                 ORDER BY batch_start ASC, id ASC"
            ))
            .context("failed to prepare extraction batch detail query")?;

        let batches = batch_stmt
            .query_map(
                params![format_db_timestamp(&start), format_db_timestamp(&end)],
                map_extraction_batch_row,
            )
            .context("failed to query extraction batches for synthesis")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to map extraction batches for synthesis")?;

        let mut details = Vec::with_capacity(batches.len());
        for batch in batches {
            let frames = self
                .list_extraction_frame_details_for_batch(batch.id, start, end)
                .with_context(|| {
                    format!("failed to load extraction frames for batch {}", batch.id)
                })?;
            if frames.is_empty() {
                continue;
            }

            details.push(ExtractionBatchDetail { batch, frames });
        }

        Ok(details)
    }

    fn list_extraction_frame_details_for_batch(
        &self,
        batch_id: Uuid,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<ExtractionFrameDetail>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    captures.id,
                    captures.timestamp,
                    captures.app_name,
                    captures.window_title,
                    captures.bundle_id,
                    captures.display_id,
                    captures.screenshot_path,
                    captures.extraction_status,
                    captures.extraction_id,
                    captures.created_at,
                    extractions.id AS frame_extraction_id,
                    extractions.capture_id AS frame_extraction_capture_id,
                    extractions.batch_id AS frame_extraction_batch_id,
                    extractions.activity_type AS frame_extraction_activity_type,
                    extractions.description AS frame_extraction_description,
                    extractions.app_context AS frame_extraction_app_context,
                    extractions.project AS frame_extraction_project,
                    extractions.topics AS frame_extraction_topics,
                    extractions.people AS frame_extraction_people,
                    extractions.key_content AS frame_extraction_key_content,
                    extractions.sentiment AS frame_extraction_sentiment,
                    extractions.created_at AS frame_extraction_created_at
                 FROM extractions
                 JOIN captures ON captures.id = extractions.capture_id
                 WHERE extractions.batch_id = ?1
                   AND captures.timestamp >= ?2
                   AND captures.timestamp <= ?3
                 ORDER BY captures.timestamp ASC, extractions.id ASC",
            )
            .with_context(|| {
                format!("failed to prepare frame detail query for batch {batch_id}")
            })?;

        let frames = stmt
            .query_map(
                params![
                    batch_id.to_string(),
                    format_db_timestamp(&start),
                    format_db_timestamp(&end),
                ],
                map_extraction_frame_detail_row,
            )
            .with_context(|| format!("failed to query frame details for batch {batch_id}"))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .with_context(|| format!("failed to map frame details for batch {batch_id}"))?;

        Ok(frames)
    }

    pub fn list_app_capture_counts(&self) -> Result<Vec<AppCaptureCount>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    app_name,
                    COUNT(*) AS capture_count
                 FROM captures
                 WHERE app_name IS NOT NULL AND trim(app_name) <> ''
                 GROUP BY app_name
                 ORDER BY capture_count DESC, app_name ASC",
            )
            .context("failed to prepare app capture count query")?;

        let app_counts = stmt
            .query_map([], |row| {
                let capture_count: i64 = row.get("capture_count")?;
                Ok(AppCaptureCount {
                    app_name: row.get("app_name")?,
                    capture_count: u64::try_from(capture_count)
                        .map_err(|error| into_row_error(error.into()))?,
                })
            })
            .context("failed to query app capture counts")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to map app capture counts")?;

        Ok(app_counts)
    }

    pub fn update_capture_status(
        &mut self,
        capture_id: i64,
        status: ExtractionStatus,
        extraction_id: Option<i64>,
    ) -> Result<()> {
        update_capture_status_record(&self.conn, capture_id, status, extraction_id)
            .with_context(|| format!("failed to update capture {capture_id} status"))
    }

    pub fn mark_captures_failed(&mut self, capture_ids: &[i64]) -> Result<()> {
        if capture_ids.is_empty() {
            return Ok(());
        }

        let tx = self
            .conn
            .transaction()
            .context("failed to start failed-capture transaction")?;

        for capture_id in capture_ids {
            update_capture_status_record(&tx, *capture_id, ExtractionStatus::Failed, None)
                .with_context(|| format!("failed to mark capture {capture_id} as failed"))?;
        }

        tx.commit()
            .context("failed to commit failed-capture transaction")?;

        Ok(())
    }

    pub fn record_failed_extraction_batch(
        &mut self,
        batch: &NewExtractionBatch,
        capture_ids: &[i64],
    ) -> Result<ExtractionBatch> {
        validate_batch_capture_count(batch.capture_count, capture_ids.len())?;

        let created_at = Utc::now();
        let tx = self
            .conn
            .transaction()
            .context("failed to start failed extraction batch transaction")?;

        insert_extraction_batch_record(&tx, batch, &created_at)
            .context("failed to insert failed extraction batch")?;

        for capture_id in capture_ids {
            update_capture_status_record(&tx, *capture_id, ExtractionStatus::Failed, None)
                .with_context(|| format!("failed to mark capture {capture_id} as failed"))?;
        }

        tx.commit()
            .context("failed to commit failed extraction batch transaction")?;

        Ok(materialize_extraction_batch(batch, &created_at))
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

        insert_extraction_batch_record(&tx, batch, &created_at)
            .context("failed to insert extraction batch")?;
        sync_batch_search_index(&tx, batch.id)?;
        tx.commit()
            .context("failed to commit extraction batch transaction")?;

        Ok(materialize_extraction_batch(batch, &created_at))
    }

    pub fn persist_extraction_batch(
        &mut self,
        batch: &NewExtractionBatch,
        extractions: &[NewExtraction],
    ) -> Result<(ExtractionBatch, Vec<Extraction>)> {
        validate_batch_capture_count(batch.capture_count, extractions.len())?;

        let created_at = Utc::now();
        let tx = self
            .conn
            .transaction()
            .context("failed to start extraction persistence transaction")?;

        insert_extraction_batch_record(&tx, batch, &created_at)
            .context("failed to insert extraction batch")?;

        let mut persisted = Vec::with_capacity(extractions.len());
        for extraction in extractions {
            let extraction_id = insert_extraction_record(&tx, extraction, &created_at)
                .with_context(|| {
                    format!(
                        "failed to insert extraction for capture {}",
                        extraction.capture_id
                    )
                })?;
            update_capture_status_record(
                &tx,
                extraction.capture_id,
                ExtractionStatus::Processed,
                Some(extraction_id),
            )
            .with_context(|| {
                format!(
                    "failed to link processed extraction {} to capture {}",
                    extraction_id, extraction.capture_id
                )
            })?;
            persisted.push(materialize_extraction(
                extraction_id,
                extraction,
                &created_at,
            ));
        }

        sync_batch_search_index(&tx, batch.id)?;
        tx.commit()
            .context("failed to commit extraction persistence transaction")?;

        Ok((materialize_extraction_batch(batch, &created_at), persisted))
    }

    pub fn insert_extraction(&mut self, extraction: &NewExtraction) -> Result<Extraction> {
        let created_at = Utc::now();
        let tx = self
            .conn
            .transaction()
            .context("failed to start extraction transaction")?;
        let extraction_id = insert_extraction_record(&tx, extraction, &created_at)
            .context("failed to insert extraction")?;
        sync_search_index_for_extraction(&tx, extraction_id)?;
        tx.commit()
            .context("failed to commit extraction transaction")?;

        Ok(materialize_extraction(
            extraction_id,
            extraction,
            &created_at,
        ))
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

    pub fn get_latest_insight_by_type(&self, insight_type: InsightType) -> Result<Option<Insight>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    id,
                    type,
                    window_start,
                    window_end,
                    data,
                    narrative,
                    model_used,
                    tokens_used,
                    cost_cents,
                    created_at
                 FROM insights
                 WHERE type = ?1
                 ORDER BY window_end DESC, created_at DESC, id DESC
                 LIMIT 1",
            )
            .with_context(|| format!("failed to prepare latest {insight_type} insight query"))?;

        let insight = stmt
            .query_row(params![insight_type.as_str()], map_insight_row)
            .optional()
            .with_context(|| format!("failed to query latest {insight_type} insight"))?;

        Ok(insight)
    }

    pub fn list_insights_in_range(
        &self,
        insight_type: InsightType,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<Insight>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    id,
                    type,
                    window_start,
                    window_end,
                    data,
                    narrative,
                    model_used,
                    tokens_used,
                    cost_cents,
                    created_at
                 FROM insights
                 WHERE type = ?1 AND window_start >= ?2 AND window_end <= ?3
                 ORDER BY window_start ASC, id ASC",
            )
            .with_context(|| format!("failed to prepare {insight_type} insight range query"))?;

        let insights = stmt
            .query_map(
                params![
                    insight_type.as_str(),
                    format_db_timestamp(&window_start),
                    format_db_timestamp(&window_end),
                ],
                map_insight_row,
            )
            .with_context(|| {
                format!(
                    "failed to query {insight_type} insights for {}..{}",
                    window_start, window_end
                )
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .with_context(|| format!("failed to map {insight_type} insight range results"))?;

        Ok(insights)
    }

    pub fn list_hourly_insights_in_range(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<Insight>> {
        self.list_insights_in_range(InsightType::Hourly, window_start, window_end)
    }

    pub fn list_daily_insights_in_date_range(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Insight>> {
        let day_start = from
            .and_hms_opt(0, 0, 0)
            .expect("midnight should be representable")
            .and_utc();
        let next_day_start = to
            .succ_opt()
            .expect("successor date should be representable")
            .and_hms_opt(0, 0, 0)
            .expect("midnight should be representable")
            .and_utc();

        self.list_insights_in_range(InsightType::Daily, day_start, next_day_start)
    }

    pub fn get_latest_daily_insight_for_date(&self, date: NaiveDate) -> Result<Option<Insight>> {
        let day_start = date
            .and_hms_opt(0, 0, 0)
            .expect("midnight should be representable")
            .and_utc();
        let next_day_start = date
            .succ_opt()
            .expect("successor date should be representable")
            .and_hms_opt(0, 0, 0)
            .expect("midnight should be representable")
            .and_utc();

        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    id,
                    type,
                    window_start,
                    window_end,
                    data,
                    narrative,
                    model_used,
                    tokens_used,
                    cost_cents,
                    created_at
                 FROM insights
                 WHERE type = ?1 AND window_start >= ?2 AND window_start < ?3
                 ORDER BY window_end DESC, created_at DESC, id DESC
                 LIMIT 1",
            )
            .context("failed to prepare latest daily insight query")?;

        let insight = stmt
            .query_row(
                params![
                    InsightType::Daily.as_str(),
                    format_db_timestamp(&day_start),
                    format_db_timestamp(&next_day_start),
                ],
                map_insight_row,
            )
            .optional()
            .with_context(|| format!("failed to query daily insight for {date}"))?;

        Ok(insight)
    }

    pub fn list_project_time_allocations(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<Vec<ProjectTimeAllocation>> {
        let mut sql = String::from(
            "SELECT
                e.project,
                COUNT(DISTINCT e.capture_id) AS capture_count
             FROM extractions e
             JOIN captures c ON c.id = e.capture_id",
        );
        let mut filters = Vec::new();
        let mut params = Vec::new();

        if let Some(from) = from.as_ref() {
            filters.push("c.timestamp >= ?");
            params.push(Value::Text(format_db_timestamp(from)));
        }

        if let Some(to) = to.as_ref() {
            filters.push("c.timestamp <= ?");
            params.push(Value::Text(format_db_timestamp(to)));
        }

        if !filters.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&filters.join(" AND "));
        }

        sql.push_str(
            " GROUP BY e.project
              ORDER BY capture_count DESC, e.project IS NULL ASC, e.project ASC",
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("failed to prepare project time allocation query")?;

        let projects = stmt
            .query_map(params_from_iter(params.iter()), |row| {
                let capture_count: i64 = row.get("capture_count")?;
                let capture_count = u64::try_from(capture_count)
                    .context("project capture count overflowed u64")
                    .map_err(into_row_error)?;

                Ok(ProjectTimeAllocation {
                    project: row.get("project")?,
                    capture_count,
                })
            })
            .context("failed to query project time allocations")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to map project time allocations")?;

        Ok(projects)
    }

    pub fn list_topic_frequencies(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<Vec<TopicFrequency>> {
        let mut sql = String::from(
            "SELECT
                topic,
                COUNT(*) AS capture_count
             FROM (
                SELECT DISTINCT
                    e.id AS extraction_id,
                    LOWER(TRIM(je.value)) AS topic
                FROM extractions e
                JOIN captures c ON c.id = e.capture_id
                JOIN json_each(COALESCE(e.topics, '[]')) AS je
                WHERE TRIM(je.value) != ''",
        );
        let mut params = Vec::new();

        if let Some(from) = from.as_ref() {
            sql.push_str(" AND c.timestamp >= ?");
            params.push(Value::Text(format_db_timestamp(from)));
        }

        if let Some(to) = to.as_ref() {
            sql.push_str(" AND c.timestamp <= ?");
            params.push(Value::Text(format_db_timestamp(to)));
        }

        sql.push_str(
            " ) topic_counts
              GROUP BY topic
              ORDER BY capture_count DESC, topic ASC",
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("failed to prepare topic frequency query")?;

        let topics = stmt
            .query_map(params_from_iter(params.iter()), |row| {
                let capture_count: i64 = row.get("capture_count")?;
                let capture_count = u64::try_from(capture_count)
                    .context("topic capture count overflowed u64")
                    .map_err(into_row_error)?;

                Ok(TopicFrequency {
                    topic: row.get("topic")?,
                    capture_count,
                })
            })
            .context("failed to query topic frequencies")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to map topic frequencies")?;

        Ok(topics)
    }

    pub fn search_extractions(&self, query: &str) -> Result<Vec<ExtractionSearchHit>> {
        self.search_extractions_filtered(&ExtractionSearchQuery {
            query: query.to_owned(),
            app_name: None,
            project: None,
            from: None,
            to: None,
            limit: i64::MAX as usize,
        })
    }

    pub fn search_extractions_filtered(
        &self,
        query: &ExtractionSearchQuery,
    ) -> Result<Vec<ExtractionSearchHit>> {
        let match_query = build_fts_match_query(&query.query)?;
        let mut sql = String::from(
            "SELECT
                c.id AS search_capture_id,
                c.timestamp AS search_capture_timestamp,
                c.app_name AS search_capture_app_name,
                c.window_title AS search_capture_window_title,
                c.bundle_id AS search_capture_bundle_id,
                c.display_id AS search_capture_display_id,
                c.screenshot_path AS search_capture_screenshot_path,
                c.extraction_status AS search_capture_extraction_status,
                c.extraction_id AS search_capture_extraction_id,
                c.created_at AS search_capture_created_at,
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
             JOIN captures c ON c.id = e.capture_id
             LEFT JOIN extraction_batches eb ON eb.id = e.batch_id",
        );
        let mut filters = vec!["search_index MATCH ?".to_string()];
        let mut params = vec![Value::Text(match_query)];

        if let Some(app_name) = query.app_name.as_ref() {
            filters.push("c.app_name = ?".to_string());
            params.push(Value::Text(app_name.clone()));
        }

        if let Some(project) = query.project.as_ref() {
            filters.push("e.project = ?".to_string());
            params.push(Value::Text(project.clone()));
        }

        if let Some(from) = query.from.as_ref() {
            filters.push("c.timestamp >= ?".to_string());
            params.push(Value::Text(format_db_timestamp(from)));
        }

        if let Some(to) = query.to.as_ref() {
            filters.push("c.timestamp <= ?".to_string());
            params.push(Value::Text(format_db_timestamp(to)));
        }

        sql.push_str(" WHERE ");
        sql.push_str(&filters.join(" AND "));
        sql.push_str(" ORDER BY rank ASC, c.timestamp DESC, e.id DESC LIMIT ?");
        params.push(Value::Integer(
            i64::try_from(query.limit).context("search query limit exceeds sqlite range")?,
        ));

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("failed to prepare extraction search query")?;

        let results = stmt
            .query_map(params_from_iter(params.iter()), map_search_hit_row)
            .with_context(|| format!("failed to search extractions for `{}`", query.query))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to map extraction search results")?;

        Ok(results)
    }
}

fn configure_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .context("failed to configure sqlite connection")?;
    Ok(())
}

fn initialize_connection(conn: &Connection) -> Result<()> {
    configure_connection(conn)?;
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

fn insert_extraction_batch_record(
    conn: &Connection,
    batch: &NewExtractionBatch,
    created_at: &DateTime<Utc>,
) -> rusqlite::Result<()> {
    conn.execute(
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
            format_db_timestamp(created_at),
        ],
    )?;

    Ok(())
}

fn insert_extraction_record(
    conn: &Connection,
    extraction: &NewExtraction,
    created_at: &DateTime<Utc>,
) -> Result<i64> {
    let topics = encode_string_list(&extraction.topics)?;
    let people = encode_string_list(&extraction.people)?;

    conn.execute(
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
            format_db_timestamp(created_at),
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

fn update_capture_status_record(
    conn: &Connection,
    capture_id: i64,
    status: ExtractionStatus,
    extraction_id: Option<i64>,
) -> Result<()> {
    let rows = conn.execute(
        "UPDATE captures
         SET extraction_status = ?1, extraction_id = ?2
         WHERE id = ?3",
        params![status.as_str(), extraction_id, capture_id],
    )?;

    if rows == 0 {
        return Err(anyhow!("capture {capture_id} does not exist"));
    }

    Ok(())
}

fn materialize_capture(id: i64, capture: &NewCapture, created_at: &DateTime<Utc>) -> Capture {
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

fn materialize_extraction_batch(
    batch: &NewExtractionBatch,
    created_at: &DateTime<Utc>,
) -> ExtractionBatch {
    ExtractionBatch {
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
        created_at: *created_at,
    }
}

fn materialize_extraction(
    id: i64,
    extraction: &NewExtraction,
    created_at: &DateTime<Utc>,
) -> Extraction {
    Extraction {
        id,
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
        created_at: *created_at,
    }
}

fn validate_batch_capture_count(expected: i64, actual: usize) -> Result<()> {
    let actual = i64::try_from(actual).context("capture batch size exceeds sqlite range")?;
    if expected != actual {
        return Err(anyhow!(
            "extraction batch capture_count {} does not match {} frame(s)",
            expected,
            actual
        ));
    }

    Ok(())
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

fn map_extraction_batch_row(row: &Row<'_>) -> rusqlite::Result<ExtractionBatch> {
    Ok(ExtractionBatch {
        id: parse_uuid_column(row.get("id")?)?,
        batch_start: parse_timestamp_column(row.get("batch_start")?)?,
        batch_end: parse_timestamp_column(row.get("batch_end")?)?,
        capture_count: row.get("capture_count")?,
        primary_activity: row.get("primary_activity")?,
        project_context: row.get("project_context")?,
        narrative: row.get("narrative")?,
        raw_response: row.get("raw_response")?,
        model_used: row.get("model_used")?,
        tokens_used: row.get("tokens_used")?,
        cost_cents: row.get("cost_cents")?,
        created_at: parse_timestamp_column(row.get("created_at")?)?,
    })
}

fn map_extraction_frame_detail_row(row: &Row<'_>) -> rusqlite::Result<ExtractionFrameDetail> {
    Ok(ExtractionFrameDetail {
        capture: map_capture_row(row)?,
        extraction: Extraction {
            id: row.get("frame_extraction_id")?,
            capture_id: row.get("frame_extraction_capture_id")?,
            batch_id: parse_uuid_column(row.get("frame_extraction_batch_id")?)?,
            activity_type: parse_optional_enum_column::<ActivityType>(
                row.get("frame_extraction_activity_type")?,
            )?,
            description: row.get("frame_extraction_description")?,
            app_context: row.get("frame_extraction_app_context")?,
            project: row.get("frame_extraction_project")?,
            topics: parse_string_list_column(row.get("frame_extraction_topics")?)?,
            people: parse_string_list_column(row.get("frame_extraction_people")?)?,
            key_content: row.get("frame_extraction_key_content")?,
            sentiment: parse_optional_enum_column::<Sentiment>(
                row.get("frame_extraction_sentiment")?,
            )?,
            created_at: parse_timestamp_column(row.get("frame_extraction_created_at")?)?,
        },
    })
}

fn map_optional_detail_extraction_row(row: &Row<'_>) -> rusqlite::Result<Option<Extraction>> {
    let Some(id) = row.get("detail_extraction_id")? else {
        return Ok(None);
    };

    Ok(Some(Extraction {
        id,
        capture_id: row.get("detail_extraction_capture_id")?,
        batch_id: parse_uuid_column(row.get("detail_extraction_batch_id")?)?,
        activity_type: parse_optional_enum_column::<ActivityType>(
            row.get("detail_extraction_activity_type")?,
        )?,
        description: row.get("detail_extraction_description")?,
        app_context: row.get("detail_extraction_app_context")?,
        project: row.get("detail_extraction_project")?,
        topics: parse_string_list_column(row.get("detail_extraction_topics")?)?,
        people: parse_string_list_column(row.get("detail_extraction_people")?)?,
        key_content: row.get("detail_extraction_key_content")?,
        sentiment: parse_optional_enum_column::<Sentiment>(
            row.get("detail_extraction_sentiment")?,
        )?,
        created_at: parse_timestamp_column(row.get("detail_extraction_created_at")?)?,
    }))
}

fn map_search_hit_row(row: &Row<'_>) -> rusqlite::Result<ExtractionSearchHit> {
    Ok(ExtractionSearchHit {
        capture: Capture {
            id: row.get("search_capture_id")?,
            timestamp: parse_timestamp_column(row.get("search_capture_timestamp")?)?,
            app_name: row.get("search_capture_app_name")?,
            window_title: row.get("search_capture_window_title")?,
            bundle_id: row.get("search_capture_bundle_id")?,
            display_id: row.get("search_capture_display_id")?,
            screenshot_path: row.get("search_capture_screenshot_path")?,
            extraction_status: parse_enum_column::<ExtractionStatus>(
                row.get("search_capture_extraction_status")?,
            )?,
            extraction_id: row.get("search_capture_extraction_id")?,
            created_at: parse_timestamp_column(row.get("search_capture_created_at")?)?,
        },
        extraction: map_extraction_row(row)?,
        batch_narrative: row.get("batch_narrative")?,
        rank: row.get("rank")?,
    })
}

fn map_insight_row(row: &Row<'_>) -> rusqlite::Result<Insight> {
    Ok(Insight {
        id: row.get("id")?,
        insight_type: parse_enum_column::<InsightType>(row.get("type")?)?,
        window_start: parse_timestamp_column(row.get("window_start")?)?,
        window_end: parse_timestamp_column(row.get("window_end")?)?,
        data: parse_json_column(row.get("data")?)?,
        narrative: row.get("narrative")?,
        model_used: row.get("model_used")?,
        tokens_used: row.get("tokens_used")?,
        cost_cents: row.get("cost_cents")?,
        created_at: parse_timestamp_column(row.get("created_at")?)?,
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
    fn capture_api_queries_return_filtered_results_and_details() {
        let mut db = StorageDb::open_in_memory().expect("database should open");
        let first_time = Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
        let second_time = Utc.with_ymd_and_hms(2026, 4, 10, 14, 5, 0).unwrap();
        let third_time = Utc.with_ymd_and_hms(2026, 4, 10, 14, 10, 0).unwrap();

        let first = db
            .insert_capture(&sample_capture(first_time, "code"))
            .expect("first capture should insert");
        let mut second_capture = sample_capture(second_time, "safari-1");
        second_capture.app_name = Some("Safari".into());
        second_capture.window_title = Some("Docs".into());
        let second = db
            .insert_capture(&second_capture)
            .expect("second capture should insert");
        let mut third_capture = sample_capture(third_time, "safari-2");
        third_capture.app_name = Some("Safari".into());
        third_capture.window_title = Some("Issue".into());
        let third = db
            .insert_capture(&third_capture)
            .expect("third capture should insert");

        let captures = db
            .list_captures(&CaptureQuery {
                from: Some(first_time),
                to: Some(third_time),
                app_name: Some("Safari".into()),
                limit: 1,
                offset: 1,
            })
            .expect("filtered capture query should work");
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].id, third.id);

        let batch = db
            .insert_extraction_batch(&NewExtractionBatch {
                id: Uuid::new_v4(),
                batch_start: second_time,
                batch_end: third_time,
                capture_count: 2,
                primary_activity: Some("browsing".into()),
                project_context: Some("docs".into()),
                narrative: Some("Read docs and filed notes".into()),
                raw_response: None,
                model_used: Some("mock-model".into()),
                tokens_used: Some(42),
                cost_cents: Some(0.3),
            })
            .expect("batch should insert");
        let extraction = db
            .insert_extraction(&NewExtraction {
                capture_id: second.id,
                batch_id: batch.id,
                activity_type: Some(ActivityType::Browsing),
                description: Some("Reading Screencap API docs".into()),
                app_context: Some("Safari docs tab".into()),
                project: Some("screencap".into()),
                topics: vec!["api".into(), "axum".into()],
                people: vec![],
                key_content: Some("GET /api/captures".into()),
                sentiment: Some(Sentiment::Exploring),
            })
            .expect("extraction should insert");
        db.update_capture_status(second.id, ExtractionStatus::Processed, Some(extraction.id))
            .expect("status update should work");

        let detail = db
            .get_capture_detail(second.id)
            .expect("capture detail should query")
            .expect("capture detail should exist");
        assert_eq!(detail.capture.id, second.id);
        assert_eq!(
            detail.extraction.as_ref().map(|value| value.id),
            Some(extraction.id)
        );
        assert_eq!(
            detail
                .extraction
                .as_ref()
                .and_then(|value| value.description.as_deref()),
            Some("Reading Screencap API docs")
        );

        let apps = db
            .list_app_capture_counts()
            .expect("app capture counts should query");
        assert!(apps
            .iter()
            .any(|app| app.app_name == "Code" && app.capture_count == 1));
        assert!(apps
            .iter()
            .any(|app| app.app_name == "Safari" && app.capture_count == 2));
        assert_eq!(first.id, 1);
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
