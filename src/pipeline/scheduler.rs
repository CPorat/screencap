use std::{fs, path::Path, sync::Arc, time::Duration};

use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::Utc;
use tokio::{
    sync::watch,
    time::{self, MissedTickBehavior},
};
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    ai::provider::{
        create_provider, ImageInput, LlmProvider, LlmProviderConfig, LlmResponse, ProviderError,
        TokenUsage,
    },
    config::AppConfig,
    storage::{
        db::StorageDb,
        models::{Capture, NewExtraction, NewExtractionBatch},
    },
};

use super::extraction::{build_extraction_prompt, parse_extraction_response, ExtractionResult};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ExtractionRunReport {
    pub processed_batches: usize,
    pub processed_captures: usize,
    pub failed_batches: usize,
    pub failed_captures: usize,
}

impl ExtractionRunReport {
    fn is_idle(self) -> bool {
        self.processed_batches == 0 && self.failed_batches == 0
    }
}

enum BatchDisposition {
    Processed,
    Failed,
    Deferred,
}

struct BatchRecordFields<'a> {
    primary_activity: Option<&'a str>,
    project_context: Option<&'a str>,
    narrative: Option<&'a str>,
    raw_response: Option<&'a str>,
    usage: Option<TokenUsage>,
    cost_cents: Option<f64>,
}

pub struct ExtractionScheduler {
    config: AppConfig,
    db: StorageDb,
    provider: Arc<dyn LlmProvider>,
}

impl ExtractionScheduler {
    pub fn open(config: AppConfig, home: impl AsRef<Path>) -> Result<Self> {
        let provider_config = LlmProviderConfig::from(&config.extraction);
        let provider: Arc<dyn LlmProvider> = create_provider(&provider_config)?.into();
        Self::with_provider(config, home, provider)
    }

    pub fn with_provider(
        config: AppConfig,
        home: impl AsRef<Path>,
        provider: Arc<dyn LlmProvider>,
    ) -> Result<Self> {
        ensure!(config.extraction.enabled, "extraction pipeline is disabled");
        ensure!(
            config.extraction.interval_secs > 0,
            "extraction interval_secs must be greater than 0"
        );
        ensure!(
            config.extraction.max_images_per_batch > 0,
            "extraction max_images_per_batch must be greater than 0"
        );

        let db_path = config.storage_root(home.as_ref()).join("screencap.db");
        let db = StorageDb::open_at_path(&db_path).with_context(|| {
            format!(
                "failed to open extraction database at {}",
                db_path.display()
            )
        })?;

        Ok(Self {
            config,
            db,
            provider,
        })
    }

    pub async fn run_until_shutdown(&mut self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let mut interval =
            time::interval(Duration::from_secs(self.config.extraction.interval_secs));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let report = self.run_once().await?;
                    if !report.is_idle() {
                        info!(
                            processed_batches = report.processed_batches,
                            processed_captures = report.processed_captures,
                            failed_batches = report.failed_batches,
                            failed_captures = report.failed_captures,
                            "extraction run completed"
                        );
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn run_once(&mut self) -> Result<ExtractionRunReport> {
        let mut report = ExtractionRunReport::default();
        let batch_size = self.config.extraction.max_images_per_batch as usize;

        loop {
            let captures = self.db.get_pending_captures_batch(batch_size)?;
            if captures.is_empty() {
                break;
            }

            match self.process_batch(&captures).await? {
                BatchDisposition::Processed => {
                    report.processed_batches += 1;
                    report.processed_captures += captures.len();
                }
                BatchDisposition::Failed => {
                    report.failed_batches += 1;
                    report.failed_captures += captures.len();
                }
                BatchDisposition::Deferred => break,
            }
        }

        Ok(report)
    }

    async fn process_batch(&mut self, captures: &[Capture]) -> Result<BatchDisposition> {
        let capture_ids = capture_ids(captures);
        let prompt = build_extraction_prompt(captures, &[]);

        let images = match load_capture_images(captures) {
            Ok(images) => images,
            Err(error) => {
                self.db.mark_captures_failed(&capture_ids)?;
                error!(
                    error = %error,
                    capture_count = captures.len(),
                    "failed to load screenshot batch for extraction"
                );
                return Ok(BatchDisposition::Failed);
            }
        };

        let response = match self.provider.complete(&prompt, &images).await {
            Ok(response) => response,
            Err(error) => {
                if matches!(&error, ProviderError::Authentication { .. }) {
                    error!(
                        error = %error,
                        capture_count = captures.len(),
                        "vision extraction authentication failed; leaving captures pending"
                    );
                    return Ok(BatchDisposition::Deferred);
                }

                self.db.mark_captures_failed(&capture_ids)?;
                error!(
                    error = %error,
                    capture_count = captures.len(),
                    "vision extraction request failed"
                );
                return Ok(BatchDisposition::Failed);
            }
        };

        let parsed = match parse_and_validate_response(captures, &response.content) {
            Ok(parsed) => parsed,
            Err(error) => {
                let failed_batch = self.failed_batch_record(captures, &response)?;
                self.db
                    .record_failed_extraction_batch(&failed_batch, &capture_ids)?;
                error!(
                    error = %error,
                    batch_id = %failed_batch.id,
                    capture_count = captures.len(),
                    "extraction response was rejected"
                );
                return Ok(BatchDisposition::Failed);
            }
        };

        let batch_id = Uuid::new_v4();
        let batch = self.success_batch_record(batch_id, captures, &parsed, &response)?;
        let extractions = build_new_extractions(batch_id, &parsed);
        self.db
            .persist_extraction_batch(&batch, &extractions)
            .with_context(|| format!("failed to persist extraction batch {}", batch.id))?;

        Ok(BatchDisposition::Processed)
    }

    fn success_batch_record(
        &self,
        batch_id: Uuid,
        captures: &[Capture],
        parsed: &ExtractionResult,
        response: &LlmResponse,
    ) -> Result<NewExtractionBatch> {
        build_batch_record(
            &self.config,
            batch_id,
            captures,
            BatchRecordFields {
                primary_activity: Some(&parsed.batch_summary.primary_activity),
                project_context: Some(&parsed.batch_summary.project_context),
                narrative: Some(&parsed.batch_summary.narrative),
                raw_response: Some(response.content.as_str()),
                usage: response.usage,
                cost_cents: response.cost_cents,
            },
        )
    }

    fn failed_batch_record(
        &self,
        captures: &[Capture],
        response: &LlmResponse,
    ) -> Result<NewExtractionBatch> {
        build_batch_record(
            &self.config,
            Uuid::new_v4(),
            captures,
            BatchRecordFields {
                primary_activity: None,
                project_context: None,
                narrative: None,
                raw_response: Some(response.content.as_str()),
                usage: response.usage,
                cost_cents: response.cost_cents,
            },
        )
    }
}

pub async fn run_extraction_scheduler(
    config: AppConfig,
    home: impl AsRef<Path>,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut scheduler = ExtractionScheduler::open(config, home)?;
    scheduler.run_until_shutdown(shutdown).await
}

fn build_batch_record(
    config: &AppConfig,
    batch_id: Uuid,
    captures: &[Capture],
    fields: BatchRecordFields<'_>,
) -> Result<NewExtractionBatch> {
    let (batch_start, batch_end) = capture_window(captures)?;
    let capture_count =
        i64::try_from(captures.len()).context("capture batch size exceeds sqlite range")?;
    let tokens_used = fields
        .usage
        .map(|usage| i64::try_from(usage.total_tokens).context("token usage exceeds sqlite range"))
        .transpose()?;

    Ok(NewExtractionBatch {
        id: batch_id,
        batch_start,
        batch_end,
        capture_count,
        primary_activity: fields.primary_activity.map(ToOwned::to_owned),
        project_context: fields.project_context.map(ToOwned::to_owned),
        narrative: fields.narrative.map(ToOwned::to_owned),
        raw_response: fields.raw_response.map(ToOwned::to_owned),
        model_used: Some(config.extraction.model.clone()),
        tokens_used,
        cost_cents: fields.cost_cents,
    })
}

fn parse_and_validate_response(
    captures: &[Capture],
    raw_response: &str,
) -> Result<ExtractionResult> {
    let parsed = parse_extraction_response(raw_response)?;
    validate_response_matches_requested(captures, &parsed)?;
    Ok(parsed)
}

fn validate_response_matches_requested(
    captures: &[Capture],
    parsed: &ExtractionResult,
) -> Result<()> {
    if parsed.frames.len() != captures.len() {
        bail!(
            "extraction response returned {} frame(s) for {} requested capture(s)",
            parsed.frames.len(),
            captures.len()
        );
    }

    for (index, (capture, frame)) in captures.iter().zip(&parsed.frames).enumerate() {
        if frame.capture_id != capture.id {
            bail!(
                "extraction response frame {} referenced capture_id {} but expected {}",
                index,
                frame.capture_id,
                capture.id
            );
        }
    }

    Ok(())
}

fn build_new_extractions(batch_id: Uuid, parsed: &ExtractionResult) -> Vec<NewExtraction> {
    parsed
        .frames
        .iter()
        .map(|frame| NewExtraction {
            capture_id: frame.capture_id,
            batch_id,
            activity_type: Some(frame.activity_type),
            description: Some(frame.description.clone()),
            app_context: Some(frame.app_context.clone()),
            project: frame.project.clone(),
            topics: frame.topics.clone(),
            people: frame.people.clone(),
            key_content: Some(frame.key_content.clone()),
            sentiment: Some(frame.sentiment),
        })
        .collect()
}

fn load_capture_images(captures: &[Capture]) -> Result<Vec<ImageInput>> {
    let mut images = Vec::with_capacity(captures.len());
    for capture in captures {
        let bytes = fs::read(&capture.screenshot_path).with_context(|| {
            format!(
                "failed to read screenshot for capture {} at {}",
                capture.id, capture.screenshot_path
            )
        })?;
        ensure!(
            !bytes.is_empty(),
            "screenshot for capture {} at {} is empty",
            capture.id,
            capture.screenshot_path
        );
        images.push(ImageInput::jpeg(BASE64_STANDARD.encode(bytes)));
    }

    Ok(images)
}

fn capture_window(captures: &[Capture]) -> Result<(chrono::DateTime<Utc>, chrono::DateTime<Utc>)> {
    let first = captures
        .first()
        .context("cannot build extraction batch from an empty capture list")?;
    let last = captures
        .last()
        .context("cannot build extraction batch from an empty capture list")?;
    Ok((first.timestamp, last.timestamp))
}

fn capture_ids(captures: &[Capture]) -> Vec<i64> {
    captures.iter().map(|capture| capture.id).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::{Path, PathBuf},
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::Result;
    use chrono::{Duration as ChronoDuration, TimeZone, Utc};
    use image::{codecs::jpeg::JpegEncoder, Rgb, RgbImage};

    use super::*;
    use crate::{
        ai::{
            mock::MockLlmProvider,
            provider::{ProviderError, TokenUsage},
        },
        storage::models::{CaptureQuery, ExtractionStatus, NewCapture},
    };

    #[tokio::test]
    async fn run_once_processes_pending_captures_and_updates_search_index() -> Result<()> {
        let home = temp_home_root("scheduler-success");
        let config = test_config(&home, 8);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler = create_scheduler(config, &home, provider.clone())?;
        let captures = seed_captures(&mut scheduler, 5)?;
        provider.push_response(Ok(LlmResponse::with_usage_and_cost(
            success_response_json(
                &captures,
                "JWT batch summary",
                "screencap",
                "Focused on the extraction scheduler and JWT indexing.",
            ),
            TokenUsage {
                prompt_tokens: 120,
                completion_tokens: 80,
                total_tokens: 200,
            },
            0.45,
        )));

        let report = scheduler.run_once().await?;
        assert_eq!(
            report,
            ExtractionRunReport {
                processed_batches: 1,
                processed_captures: 5,
                failed_batches: 0,
                failed_captures: 0,
            }
        );

        let pending = scheduler.db.get_pending_captures()?;
        assert!(pending.is_empty());

        let captures_after = scheduler.db.list_captures(&CaptureQuery {
            from: None,
            to: None,
            app_name: None,
            project: None,
            activity_type: None,
            limit: 10,
            offset: 0,
        })?;
        assert!(captures_after
            .iter()
            .all(|capture| capture.extraction_status == ExtractionStatus::Processed));
        assert!(captures_after
            .iter()
            .all(|capture| capture.extraction_id.is_some()));

        let extraction_count: i64 =
            scheduler
                .db
                .connection()
                .query_row("SELECT COUNT(*) FROM extractions", [], |row| row.get(0))?;
        assert_eq!(extraction_count, 5);

        let batch_row = scheduler.db.connection().query_row(
            "SELECT capture_count, narrative, raw_response, model_used, tokens_used, cost_cents FROM extraction_batches",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<f64>>(5)?,
                ))
            },
        )?;
        assert_eq!(batch_row.0, 5);
        assert_eq!(
            batch_row.1.as_deref(),
            Some("Focused on the extraction scheduler and JWT indexing."),
        );
        assert!(batch_row
            .2
            .as_deref()
            .is_some_and(|value| value.contains("capture_id")));
        assert_eq!(batch_row.3.as_deref(), Some("mock-vision-model"));
        assert_eq!(batch_row.4, Some(200));
        assert_eq!(batch_row.5, Some(0.45));

        let hits = scheduler.db.search_extractions("jwt scheduler")?;
        assert_eq!(hits.len(), 5);
        assert!(hits.iter().all(|hit| {
            hit.batch_narrative
                .as_deref()
                .is_some_and(|value| value.contains("JWT indexing"))
        }));

        let calls = provider.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].images.len(), 5);
        assert!(calls[0]
            .prompt
            .contains(&format!("capture_id: {}", captures[0].id)));

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_once_marks_failed_captures_when_response_parse_fails() -> Result<()> {
        let home = temp_home_root("scheduler-parse-failure");
        let config = test_config(&home, 8);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler = create_scheduler(config, &home, provider.clone())?;
        let captures = seed_captures(&mut scheduler, 2)?;
        provider.push_response(Ok(LlmResponse::new(format!(
            "{{\"frames\":[{{\"capture_id\":{},\"activity_type\":\"coding\",\"description\":\"Only one frame\",\"app_context\":\"Ghostty\",\"project\":\"screencap\",\"topics\":[\"rust\"],\"people\":[],\"key_content\":\"fn scheduler\",\"sentiment\":\"focused\"}}],\"batch_summary\":{{\"primary_activity\":\"coding\",\"project_context\":\"screencap\",\"narrative\":\"Missing one frame\"}}}}",
            captures[0].id
        ))));

        let report = scheduler.run_once().await?;
        assert_eq!(
            report,
            ExtractionRunReport {
                processed_batches: 0,
                processed_captures: 0,
                failed_batches: 1,
                failed_captures: 2,
            }
        );

        let captures_after = scheduler.db.list_captures(&CaptureQuery {
            from: None,
            to: None,
            app_name: None,
            project: None,
            activity_type: None,
            limit: 10,
            offset: 0,
        })?;
        assert!(captures_after
            .iter()
            .all(|capture| capture.extraction_status == ExtractionStatus::Failed));
        assert!(captures_after
            .iter()
            .all(|capture| capture.extraction_id.is_none()));

        let batch_row = scheduler.db.connection().query_row(
            "SELECT capture_count, narrative, raw_response FROM extraction_batches",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )?;
        assert_eq!(batch_row.0, 2);
        assert_eq!(batch_row.1, None);
        assert!(batch_row
            .2
            .as_deref()
            .is_some_and(|value| value.contains("Only one frame")));

        let extraction_count: i64 =
            scheduler
                .db
                .connection()
                .query_row("SELECT COUNT(*) FROM extractions", [], |row| row.get(0))?;
        assert_eq!(extraction_count, 0);

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_once_continues_after_provider_failure() -> Result<()> {
        let home = temp_home_root("scheduler-provider-failure");
        let config = test_config(&home, 2);
        let provider = Arc::new(MockLlmProvider::with_responses([Err(
            ProviderError::RateLimited {
                message: "slow down".into(),
            },
        )]));
        let mut scheduler = create_scheduler(config, &home, provider.clone())?;
        let captures = seed_captures(&mut scheduler, 3)?;
        provider.push_response(Ok(LlmResponse::new(success_response_json_for_ids(
            &[captures[2].id],
            "Recovered on the second batch",
            "screencap",
            "The scheduler kept going after one provider failure.",
        ))));

        let report = scheduler.run_once().await?;
        assert_eq!(
            report,
            ExtractionRunReport {
                processed_batches: 1,
                processed_captures: 1,
                failed_batches: 1,
                failed_captures: 2,
            }
        );

        let captures_after = scheduler.db.list_captures(&CaptureQuery {
            from: None,
            to: None,
            app_name: None,
            project: None,
            activity_type: None,
            limit: 10,
            offset: 0,
        })?;
        assert_eq!(
            captures_after[0].extraction_status,
            ExtractionStatus::Failed
        );
        assert_eq!(
            captures_after[1].extraction_status,
            ExtractionStatus::Failed
        );
        assert_eq!(
            captures_after[2].extraction_status,
            ExtractionStatus::Processed
        );

        let batch_count: i64 = scheduler.db.connection().query_row(
            "SELECT COUNT(*) FROM extraction_batches",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(batch_count, 1);

        let calls = provider.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].images.len(), 2);
        assert_eq!(calls[1].images.len(), 1);

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_once_is_idle_when_no_captures_are_pending() -> Result<()> {
        let home = temp_home_root("scheduler-empty");
        let config = test_config(&home, 8);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler = create_scheduler(config, &home, provider.clone())?;

        let report = scheduler.run_once().await?;
        assert_eq!(report, ExtractionRunReport::default());
        assert!(scheduler.db.get_pending_captures()?.is_empty());
        assert!(provider.calls().is_empty());

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_once_leaves_captures_pending_on_authentication_failure() -> Result<()> {
        let home = temp_home_root("scheduler-auth-failure");
        let config = test_config(&home, 8);
        let provider = Arc::new(MockLlmProvider::with_responses([Err(
            ProviderError::Authentication {
                message: "bad api key".into(),
            },
        )]));
        let mut scheduler = create_scheduler(config, &home, provider.clone())?;
        let captures = seed_captures(&mut scheduler, 2)?;

        let report = scheduler.run_once().await?;
        assert_eq!(report, ExtractionRunReport::default());

        let pending = scheduler.db.get_pending_captures()?;
        assert_eq!(pending.len(), captures.len());
        assert!(pending
            .iter()
            .all(|capture| capture.extraction_status == ExtractionStatus::Pending));

        let batch_count: i64 = scheduler.db.connection().query_row(
            "SELECT COUNT(*) FROM extraction_batches",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(batch_count, 0);

        let calls = provider.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].images.len(), captures.len());

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_once_marks_failed_captures_when_response_is_non_json() -> Result<()> {
        let home = temp_home_root("scheduler-garbage-response");
        let config = test_config(&home, 8);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler = create_scheduler(config, &home, provider.clone())?;
        let _captures = seed_captures(&mut scheduler, 2)?;
        provider.push_response(Ok(LlmResponse::new("this is not json")));

        let report = scheduler.run_once().await?;
        assert_eq!(
            report,
            ExtractionRunReport {
                processed_batches: 0,
                processed_captures: 0,
                failed_batches: 1,
                failed_captures: 2,
            }
        );

        let captures_after = scheduler.db.list_captures(&CaptureQuery {
            from: None,
            to: None,
            app_name: None,
            project: None,
            activity_type: None,
            limit: 10,
            offset: 0,
        })?;
        assert!(captures_after
            .iter()
            .all(|capture| capture.extraction_status == ExtractionStatus::Failed));

        let batch_row = scheduler.db.connection().query_row(
            "SELECT capture_count, narrative, raw_response FROM extraction_batches",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )?;
        assert_eq!(batch_row.0, 2);
        assert_eq!(batch_row.1, None);
        assert_eq!(batch_row.2.as_deref(), Some("this is not json"));

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_once_marks_missing_screenshot_captures_failed_without_calling_llm() -> Result<()> {
        let home = temp_home_root("scheduler-missing-screenshot");
        let config = test_config(&home, 8);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler = create_scheduler(config, &home, provider.clone())?;
        let captures = seed_captures(&mut scheduler, 1)?;
        fs::remove_file(&captures[0].screenshot_path)?;

        let report = scheduler.run_once().await?;
        assert_eq!(
            report,
            ExtractionRunReport {
                processed_batches: 0,
                processed_captures: 0,
                failed_batches: 1,
                failed_captures: 1,
            }
        );

        let captures_after = scheduler.db.list_captures(&CaptureQuery {
            from: None,
            to: None,
            app_name: None,
            project: None,
            activity_type: None,
            limit: 10,
            offset: 0,
        })?;
        assert_eq!(captures_after.len(), 1);
        assert_eq!(
            captures_after[0].extraction_status,
            ExtractionStatus::Failed
        );
        assert!(captures_after[0].extraction_id.is_none());

        let batch_count: i64 = scheduler.db.connection().query_row(
            "SELECT COUNT(*) FROM extraction_batches",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(batch_count, 0);
        assert!(provider.calls().is_empty());

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    fn create_scheduler(
        config: AppConfig,
        home: &Path,
        provider: Arc<MockLlmProvider>,
    ) -> Result<ExtractionScheduler> {
        let provider: Arc<dyn LlmProvider> = provider;
        ExtractionScheduler::with_provider(config, home, provider)
    }

    fn seed_captures(scheduler: &mut ExtractionScheduler, count: usize) -> Result<Vec<Capture>> {
        let mut captures = Vec::with_capacity(count);
        let start = Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
        let screenshot_root = PathBuf::from(&scheduler.config.storage.path);

        for index in 0..count {
            let timestamp = start + ChronoDuration::minutes(index as i64 * 5);
            let screenshot_path = screenshot_root.join(format!("capture-{index}.jpg"));
            write_test_jpeg(&screenshot_path, index as u8)?;
            captures.push(scheduler.db.insert_capture(&NewCapture {
                timestamp,
                app_name: Some("Ghostty".into()),
                window_title: Some(format!("Frame {index}")),
                bundle_id: Some("com.mitchellh.ghostty".into()),
                display_id: Some(1),
                screenshot_path: screenshot_path.to_string_lossy().into_owned(),
            })?);
        }

        Ok(captures)
    }

    fn success_response_json(
        captures: &[Capture],
        primary_activity: &str,
        project_context: &str,
        narrative: &str,
    ) -> String {
        success_response_json_for_ids(
            &captures
                .iter()
                .map(|capture| capture.id)
                .collect::<Vec<_>>(),
            primary_activity,
            project_context,
            narrative,
        )
    }

    fn success_response_json_for_ids(
        capture_ids: &[i64],
        primary_activity: &str,
        project_context: &str,
        narrative: &str,
    ) -> String {
        let frames = capture_ids
            .iter()
            .map(|capture_id| {
                format!(
                    concat!(
                        "{{",
                        "\"capture_id\":{capture_id},",
                        "\"activity_type\":\"coding\",",
                        "\"description\":\"Editing extraction scheduler frame {capture_id}\",",
                        "\"app_context\":\"Ghostty running cargo test for Screencap\",",
                        "\"project\":\"screencap\",",
                        "\"topics\":[\"jwt\",\"scheduler\"],",
                        "\"people\":[],",
                        "\"key_content\":\"jwt scheduler frame {capture_id}\",",
                        "\"sentiment\":\"focused\"",
                        "}}"
                    ),
                    capture_id = capture_id,
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        format!(
            concat!(
                "{{",
                "\"frames\":[{frames}],",
                "\"batch_summary\":{{",
                "\"primary_activity\":\"{primary_activity}\",",
                "\"project_context\":\"{project_context}\",",
                "\"narrative\":\"{narrative}\"",
                "}}",
                "}}"
            ),
            frames = frames,
            primary_activity = primary_activity,
            project_context = project_context,
            narrative = narrative,
        )
    }

    fn write_test_jpeg(path: &Path, seed: u8) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let pixel = Rgb([seed, seed.saturating_add(32), seed.saturating_add(64)]);
        let image = RgbImage::from_pixel(2, 2, pixel);
        let mut bytes = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 75);
        encoder.encode_image(&image)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    fn test_config(home: &Path, max_images_per_batch: u32) -> AppConfig {
        let storage_root = home.join("runtime");
        let mut config = AppConfig::default();
        config.storage.path = storage_root.to_string_lossy().into_owned();
        config.extraction.interval_secs = 1;
        config.extraction.max_images_per_batch = max_images_per_batch;
        config.extraction.model = "mock-vision-model".into();
        config
    }

    fn temp_home_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        env::temp_dir().join(format!("screencap-scheduler-tests-{name}-{unique}"))
    }
}
