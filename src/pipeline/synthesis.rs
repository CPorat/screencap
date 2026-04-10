use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::watch,
    time::{self, MissedTickBehavior},
};
use tracing::{error, info};

use crate::{
    ai::provider::{create_provider, LlmProvider, LlmProviderConfig, LlmResponse},
    config::AppConfig,
    storage::{
        db::StorageDb,
        models::{ExtractionBatchDetail, Insight, InsightData, InsightType, NewInsight},
    },
};

use super::{json::extract_json_payload, prompts::ROLLING_CONTEXT_PROMPT_TEMPLATE};

const ROLLING_CONTEXT_WINDOW_MINUTES: i64 = 30;

pub struct RollingContextScheduler {
    config: AppConfig,
    db: StorageDb,
    provider: Arc<dyn LlmProvider>,
}

impl RollingContextScheduler {
    pub fn open(config: AppConfig, home: impl AsRef<Path>) -> Result<Self> {
        let provider_config = LlmProviderConfig::from(&config.synthesis);
        let provider: Arc<dyn LlmProvider> = create_provider(&provider_config)?.into();
        Self::with_provider(config, home, provider)
    }

    pub fn with_provider(
        config: AppConfig,
        home: impl AsRef<Path>,
        provider: Arc<dyn LlmProvider>,
    ) -> Result<Self> {
        ensure!(config.synthesis.enabled, "synthesis pipeline is disabled");
        ensure!(
            config.synthesis.rolling_interval_secs > 0,
            "synthesis rolling_interval_secs must be greater than 0"
        );

        let db_path = config.storage_root(home.as_ref()).join("screencap.db");
        let db = StorageDb::open_at_path(&db_path).with_context(|| {
            format!("failed to open synthesis database at {}", db_path.display())
        })?;

        Ok(Self {
            config,
            db,
            provider,
        })
    }

    pub async fn run_until_shutdown(&mut self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let mut interval = time::interval(Duration::from_secs(
            self.config.synthesis.rolling_interval_secs,
        ));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.run_once().await {
                        Ok(Some(insight)) => {
                            info!(
                                insight_id = insight.id,
                                window_start = %insight.window_start,
                                window_end = %insight.window_end,
                                "rolling context updated"
                            );
                        }
                        Ok(None) => {}
                        Err(error) => {
                            error!(error = %error, "rolling context synthesis failed");
                        }
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

    pub async fn run_once(&mut self) -> Result<Option<Insight>> {
        self.run_once_at(Utc::now()).await
    }

    async fn run_once_at(&mut self, window_end: DateTime<Utc>) -> Result<Option<Insight>> {
        let window_start = window_end - ChronoDuration::minutes(ROLLING_CONTEXT_WINDOW_MINUTES);
        let batches = self
            .db
            .list_extraction_batch_details_in_range(window_start, window_end)?;
        if batches.is_empty() {
            return Ok(None);
        }

        let prompt = build_rolling_context_prompt(window_start, window_end, &batches);
        let response = self
            .provider
            .complete_text(&prompt)
            .await
            .context("rolling context request failed")?;
        let data = parse_rolling_context_response(&response.content)?;
        validate_requested_window(&data, window_start, window_end)?;
        let insight = self.build_new_insight(window_start, window_end, data, &response)?;
        let persisted = self.db.insert_insight(&insight)?;

        Ok(Some(persisted))
    }

    fn build_new_insight(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        data: InsightData,
        response: &LlmResponse,
    ) -> Result<NewInsight> {
        let tokens_used = response
            .usage
            .map(|usage| {
                i64::try_from(usage.total_tokens)
                    .context("synthesis token usage exceeds sqlite integer range")
            })
            .transpose()?;

        Ok(NewInsight {
            insight_type: InsightType::Rolling,
            window_start,
            window_end,
            data,
            model_used: Some(self.config.synthesis.model.clone()),
            tokens_used,
            cost_cents: None,
        })
    }
}

pub async fn run_rolling_context_scheduler(
    config: AppConfig,
    home: impl AsRef<Path>,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut scheduler = RollingContextScheduler::open(config, home)?;
    scheduler.run_until_shutdown(shutdown).await
}

pub fn build_rolling_context_prompt(
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    batches: &[ExtractionBatchDetail],
) -> String {
    let mut prompt =
        String::with_capacity(ROLLING_CONTEXT_PROMPT_TEMPLATE.len() + batches.len() * 1024);
    prompt.push_str(ROLLING_CONTEXT_PROMPT_TEMPLATE);
    prompt.push_str("\n\nRequested window:\n");
    prompt.push_str(&format!(
        "- window_start: {}\n- window_end: {}\n",
        format_prompt_timestamp(window_start),
        format_prompt_timestamp(window_end),
    ));
    prompt.push_str("\nExtraction batches:\n");

    if batches.is_empty() {
        prompt.push_str("- none\n");
    }

    for batch_detail in batches {
        let batch = &batch_detail.batch;
        prompt.push_str(&format!(
            concat!(
                "- batch_id: {batch_id}\n",
                "  batch_start_utc: {batch_start}\n",
                "  batch_end_utc: {batch_end}\n",
                "  primary_activity: {primary_activity}\n",
                "  project_context: {project_context}\n",
                "  narrative: {narrative}\n",
                "  frames:\n"
            ),
            batch_id = batch.id,
            batch_start = format_prompt_timestamp(batch.batch_start),
            batch_end = format_prompt_timestamp(batch.batch_end),
            primary_activity = batch.primary_activity.as_deref().unwrap_or("unknown"),
            project_context = batch.project_context.as_deref().unwrap_or("unknown"),
            narrative = batch.narrative.as_deref().unwrap_or("unknown"),
        ));

        for frame_detail in &batch_detail.frames {
            let capture = &frame_detail.capture;
            let extraction = &frame_detail.extraction;
            prompt.push_str(&format!(
                concat!(
                    "  - capture_id: {capture_id}\n",
                    "    capture_timestamp_utc: {timestamp}\n",
                    "    app_name: {app_name}\n",
                    "    window_title: {window_title}\n",
                    "    bundle_id: {bundle_id}\n",
                    "    display_id: {display_id}\n",
                    "    activity_type: {activity_type}\n",
                    "    description: {description}\n",
                    "    app_context: {app_context}\n",
                    "    project: {project}\n",
                    "    topics: {topics}\n",
                    "    people: {people}\n",
                    "    key_content: {key_content}\n",
                    "    sentiment: {sentiment}\n"
                ),
                capture_id = capture.id,
                timestamp = format_prompt_timestamp(capture.timestamp),
                app_name = capture.app_name.as_deref().unwrap_or(""),
                window_title = capture.window_title.as_deref().unwrap_or(""),
                bundle_id = capture.bundle_id.as_deref().unwrap_or(""),
                display_id = capture
                    .display_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                activity_type = extraction
                    .activity_type
                    .map(|value| value.as_str())
                    .unwrap_or("unknown"),
                description = extraction.description.as_deref().unwrap_or(""),
                app_context = extraction.app_context.as_deref().unwrap_or(""),
                project = extraction.project.as_deref().unwrap_or("null"),
                topics = format_string_list(&extraction.topics),
                people = format_string_list(&extraction.people),
                key_content = extraction.key_content.as_deref().unwrap_or(""),
                sentiment = extraction
                    .sentiment
                    .map(|value| value.as_str())
                    .unwrap_or("unknown"),
            ));
        }
    }

    prompt.push_str(
        "\nUse only the extraction data above. Return JSON only; do not wrap it in markdown or add commentary.",
    );
    prompt
}

pub fn parse_rolling_context_response(json_str: &str) -> Result<InsightData> {
    let payload = extract_json_payload(json_str);
    let parsed = serde_json::from_str::<RollingContextPayload>(payload)
        .context("failed to parse rolling context response JSON")?;
    ensure!(
        parsed.insight_type == InsightType::Rolling,
        "expected rolling context response, received insight type `{}`",
        parsed.insight_type
    );

    Ok(InsightData::Rolling {
        window_start: parsed.window_start,
        window_end: parsed.window_end,
        current_focus: parsed.current_focus,
        active_project: parsed.active_project,
        apps_used: parsed.apps_used,
        context_switches: parsed.context_switches,
        mood: parsed.mood,
        summary: parsed.summary,
    })
}

fn validate_requested_window(
    data: &InsightData,
    expected_start: DateTime<Utc>,
    expected_end: DateTime<Utc>,
) -> Result<()> {
    let InsightData::Rolling {
        window_start,
        window_end,
        ..
    } = data
    else {
        unreachable!("rolling context parser only returns rolling insight data");
    };

    ensure!(
        *window_start == expected_start && *window_end == expected_end,
        "rolling context response window {}..{} did not match requested window {}..{}",
        format_prompt_timestamp(*window_start),
        format_prompt_timestamp(*window_end),
        format_prompt_timestamp(expected_start),
        format_prompt_timestamp(expected_end),
    );

    Ok(())
}

fn format_prompt_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn format_string_list(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RollingContextPayload {
    #[serde(rename = "type")]
    insight_type: InsightType,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    current_focus: String,
    active_project: Option<String>,
    apps_used: BTreeMap<String, String>,
    context_switches: u32,
    mood: String,
    summary: String,
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::Result;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use super::*;
    use crate::{
        ai::{
            mock::{MockCallKind, MockLlmProvider},
            provider::{LlmResponse, TokenUsage},
        },
        config::AppConfig,
        storage::models::{
            ActivityType, Capture, Extraction, ExtractionBatch, ExtractionFrameDetail,
            ExtractionStatus, NewCapture, NewExtraction, NewExtractionBatch, Sentiment,
        },
    };

    #[test]
    fn build_rolling_context_prompt_includes_batches_and_frames() {
        let window_start = Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 14, 30, 0).unwrap();
        let prompt =
            build_rolling_context_prompt(window_start, window_end, &[sample_batch_detail()]);

        assert!(prompt.contains("You are synthesizing a rolling context summary"));
        assert!(prompt.contains("window_start: 2026-04-10T14:00:00Z"));
        assert!(prompt.contains("window_end: 2026-04-10T14:30:00Z"));
        assert!(prompt.contains("batch_id: 123e4567-e89b-12d3-a456-426614174000"));
        assert!(prompt
            .contains("narrative: Investigated the JWT refresh path and wrote validation tests."));
        assert!(prompt.contains("capture_id: 101"));
        assert!(prompt.contains("app_name: Ghostty"));
        assert!(prompt.contains("topics: [\"jwt\",\"authentication\"]"));
        assert!(prompt.contains("people: [\"@alice\"]"));
        assert!(prompt.contains("sentiment: focused"));
    }

    #[test]
    fn parse_rolling_context_response_accepts_fenced_json() {
        let parsed = parse_rolling_context_response(
            "```json\n{\n  \"type\": \"rolling\",\n  \"window_start\": \"2026-04-10T14:00:00Z\",\n  \"window_end\": \"2026-04-10T14:30:00Z\",\n  \"current_focus\": \"Debugging JWT token refresh in the screencap auth module\",\n  \"active_project\": \"screencap\",\n  \"apps_used\": {\"VS Code\": \"18 min\", \"Chrome\": \"8 min\"},\n  \"context_switches\": 2,\n  \"mood\": \"deep-focus\",\n  \"summary\": \"Focused coding session on auth refresh handling.\"\n}\n```",
        )
        .expect("parse rolling context response");

        assert_eq!(
            parsed,
            InsightData::Rolling {
                window_start: Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap(),
                window_end: Utc.with_ymd_and_hms(2026, 4, 10, 14, 30, 0).unwrap(),
                current_focus: "Debugging JWT token refresh in the screencap auth module".into(),
                active_project: Some("screencap".into()),
                apps_used: BTreeMap::from([
                    ("Chrome".into(), "8 min".into()),
                    ("VS Code".into(), "18 min".into()),
                ]),
                context_switches: 2,
                mood: "deep-focus".into(),
                summary: "Focused coding session on auth refresh handling.".into(),
            }
        );
    }

    #[test]
    fn parse_rolling_context_response_rejects_malformed_json() {
        let error = parse_rolling_context_response(
            "```json\n{\"type\":\"rolling\",\"window_start\":}\n```",
        )
        .expect_err("malformed json should fail");
        assert!(error
            .to_string()
            .contains("failed to parse rolling context response JSON"));
    }

    #[tokio::test]
    async fn run_once_persists_rolling_context_insight() -> Result<()> {
        let home = temp_home_root("rolling-context");
        let config = test_config(&home);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler =
            RollingContextScheduler::with_provider(config, &home, provider.clone())?;
        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 14, 30, 0).unwrap();
        seed_recent_extractions(&mut scheduler.db, window_end)?;

        provider.push_response(Ok(LlmResponse::with_usage(
            success_response_json(window_end - ChronoDuration::minutes(30), window_end),
            TokenUsage {
                prompt_tokens: 180,
                completion_tokens: 80,
                total_tokens: 260,
            },
        )));

        let insight = scheduler
            .run_once_at(window_end)
            .await?
            .expect("rolling context should be created");

        assert_eq!(insight.insight_type, InsightType::Rolling);
        assert_eq!(insight.model_used.as_deref(), Some("mock-synthesis-model"));
        assert_eq!(insight.tokens_used, Some(260));
        assert_eq!(insight.cost_cents, None);

        let InsightData::Rolling {
            current_focus,
            active_project,
            apps_used,
            context_switches,
            mood,
            summary,
            ..
        } = &insight.data
        else {
            unreachable!("expected rolling insight payload");
        };
        assert_eq!(current_focus, "Debugging the JWT refresh path in Screencap");
        assert_eq!(active_project.as_deref(), Some("screencap"));
        assert_eq!(apps_used.get("Ghostty").map(String::as_str), Some("22 min"));
        assert_eq!(*context_switches, 3);
        assert_eq!(mood, "deep-focus");
        assert!(summary.contains("JWT refresh path"));

        let insight_count: i64 = scheduler.db.connection().query_row(
            "SELECT COUNT(*) FROM insights WHERE type = 'rolling'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(insight_count, 1);

        let fts_narrative: String = scheduler.db.connection().query_row(
            "SELECT narrative FROM insights_fts WHERE insight_id = ?1",
            [insight.id],
            |row| row.get(0),
        )?;
        assert!(fts_narrative.contains("JWT refresh path"));

        let calls = provider.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].kind, MockCallKind::Text);
        assert!(calls[0].images.is_empty());
        assert!(calls[0]
            .prompt
            .contains("Investigated the JWT refresh path"));
        assert!(calls[0].prompt.contains("capture_id: 1"));

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    fn sample_batch_detail() -> ExtractionBatchDetail {
        let created_at = Utc.with_ymd_and_hms(2026, 4, 10, 14, 11, 0).unwrap();
        ExtractionBatchDetail {
            batch: ExtractionBatch {
                id: Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
                batch_start: Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap(),
                batch_end: Utc.with_ymd_and_hms(2026, 4, 10, 14, 10, 0).unwrap(),
                capture_count: 1,
                primary_activity: Some("coding".into()),
                project_context: Some("screencap".into()),
                narrative: Some(
                    "Investigated the JWT refresh path and wrote validation tests.".into(),
                ),
                raw_response: None,
                model_used: Some("mock-vision-model".into()),
                tokens_used: Some(120),
                cost_cents: None,
                created_at,
            },
            frames: vec![ExtractionFrameDetail {
                capture: Capture {
                    id: 101,
                    timestamp: Utc.with_ymd_and_hms(2026, 4, 10, 14, 2, 0).unwrap(),
                    app_name: Some("Ghostty".into()),
                    window_title: Some("cargo test --features mock-capture".into()),
                    bundle_id: Some("com.mitchellh.ghostty".into()),
                    display_id: Some(1),
                    screenshot_path: "/tmp/ghostty.jpg".into(),
                    extraction_status: ExtractionStatus::Processed,
                    extraction_id: Some(501),
                    created_at,
                },
                extraction: Extraction {
                    id: 501,
                    capture_id: 101,
                    batch_id: Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
                    activity_type: Some(ActivityType::Coding),
                    description: Some("Debugging the auth refresh flow".into()),
                    app_context: Some("Running Rust tests while editing auth code".into()),
                    project: Some("screencap".into()),
                    topics: vec!["jwt".into(), "authentication".into()],
                    people: vec!["@alice".into()],
                    key_content: Some("fn refresh_session".into()),
                    sentiment: Some(Sentiment::Focused),
                    created_at,
                },
            }],
        }
    }

    fn seed_recent_extractions(db: &mut StorageDb, window_end: DateTime<Utc>) -> Result<()> {
        let captures = db.insert_captures(&[
            NewCapture {
                timestamp: window_end - ChronoDuration::minutes(24),
                app_name: Some("Ghostty".into()),
                window_title: Some("src/auth.rs".into()),
                bundle_id: Some("com.mitchellh.ghostty".into()),
                display_id: Some(1),
                screenshot_path: "/tmp/capture-1.jpg".into(),
            },
            NewCapture {
                timestamp: window_end - ChronoDuration::minutes(18),
                app_name: Some("Chrome".into()),
                window_title: Some("JWT refresh tokens - docs".into()),
                bundle_id: Some("com.google.Chrome".into()),
                display_id: Some(1),
                screenshot_path: "/tmp/capture-2.jpg".into(),
            },
            NewCapture {
                timestamp: window_end - ChronoDuration::minutes(7),
                app_name: Some("Ghostty".into()),
                window_title: Some("cargo test".into()),
                bundle_id: Some("com.mitchellh.ghostty".into()),
                display_id: Some(2),
                screenshot_path: "/tmp/capture-3.jpg".into(),
            },
        ])?;

        let first_batch_id = Uuid::new_v4();
        db.persist_extraction_batch(
            &NewExtractionBatch {
                id: first_batch_id,
                batch_start: captures[0].timestamp,
                batch_end: captures[1].timestamp,
                capture_count: 2,
                primary_activity: Some("coding".into()),
                project_context: Some("screencap auth".into()),
                narrative: Some(
                    "Investigated the JWT refresh path and compared it with docs.".into(),
                ),
                raw_response: Some("{}".into()),
                model_used: Some("mock-vision-model".into()),
                tokens_used: Some(120),
                cost_cents: None,
            },
            &[
                NewExtraction {
                    capture_id: captures[0].id,
                    batch_id: first_batch_id,
                    activity_type: Some(ActivityType::Coding),
                    description: Some("Tracing the auth refresh logic in Rust".into()),
                    app_context: Some("Editing the screencap auth module".into()),
                    project: Some("screencap".into()),
                    topics: vec!["jwt".into(), "auth".into()],
                    people: vec![],
                    key_content: Some("fn refresh_session".into()),
                    sentiment: Some(Sentiment::Focused),
                },
                NewExtraction {
                    capture_id: captures[1].id,
                    batch_id: first_batch_id,
                    activity_type: Some(ActivityType::Browsing),
                    description: Some("Reading refresh token documentation".into()),
                    app_context: Some("Comparing implementation against reference docs".into()),
                    project: Some("screencap".into()),
                    topics: vec!["jwt".into(), "refresh tokens".into()],
                    people: vec![],
                    key_content: Some("refresh token rotation".into()),
                    sentiment: Some(Sentiment::Exploring),
                },
            ],
        )?;

        let second_batch_id = Uuid::new_v4();
        db.persist_extraction_batch(
            &NewExtractionBatch {
                id: second_batch_id,
                batch_start: captures[2].timestamp,
                batch_end: captures[2].timestamp,
                capture_count: 1,
                primary_activity: Some("terminal".into()),
                project_context: Some("screencap auth".into()),
                narrative: Some("Ran targeted tests after the JWT refresh fix.".into()),
                raw_response: Some("{}".into()),
                model_used: Some("mock-vision-model".into()),
                tokens_used: Some(90),
                cost_cents: None,
            },
            &[NewExtraction {
                capture_id: captures[2].id,
                batch_id: second_batch_id,
                activity_type: Some(ActivityType::Terminal),
                description: Some("Running cargo test for the auth module".into()),
                app_context: Some("Validating the JWT refresh fix".into()),
                project: Some("screencap".into()),
                topics: vec!["jwt".into(), "tests".into()],
                people: vec![],
                key_content: Some("cargo test auth_refresh".into()),
                sentiment: Some(Sentiment::Focused),
            }],
        )?;

        Ok(())
    }

    fn success_response_json(window_start: DateTime<Utc>, window_end: DateTime<Utc>) -> String {
        format!(
            concat!(
                "{{",
                "\"type\":\"rolling\",",
                "\"window_start\":\"{}\",",
                "\"window_end\":\"{}\",",
                "\"current_focus\":\"Debugging the JWT refresh path in Screencap\",",
                "\"active_project\":\"screencap\",",
                "\"apps_used\":{{\"Chrome\":\"8 min\",\"Ghostty\":\"22 min\"}},",
                "\"context_switches\":3,",
                "\"mood\":\"deep-focus\",",
                "\"summary\":\"Focused on the JWT refresh path across code, docs, and tests. The user investigated the bug in Ghostty, checked Chrome docs, and finished by running targeted validation commands.\"",
                "}}"
            ),
            format_prompt_timestamp(window_start),
            format_prompt_timestamp(window_end),
        )
    }

    fn test_config(home: &Path) -> AppConfig {
        let mut config = AppConfig::default();
        config.storage.path = home.to_string_lossy().into_owned();
        config.synthesis.model = "mock-synthesis-model".into();
        config.synthesis.rolling_interval_secs = 60;
        config
    }

    fn temp_home_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("screencap-{label}-{unique}"));
        fs::create_dir_all(&path).expect("temp home should be creatable");
        path
    }
}
