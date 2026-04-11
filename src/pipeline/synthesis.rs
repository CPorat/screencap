use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{ensure, Context, Result};
use chrono::{
    DateTime, Duration as ChronoDuration, NaiveDate, NaiveTime, SecondsFormat, Timelike, Utc,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::watch,
    time::{self, Instant, MissedTickBehavior},
};
use tracing::{error, info};

use crate::{
    ai::provider::{create_provider, LlmProvider, LlmProviderConfig, LlmResponse},
    config::AppConfig,
    export::markdown::export_daily,
    storage::{
        db::StorageDb,
        models::{
            DailyProjectSummary, ExtractionBatchDetail, ExtractionSearchHit, ExtractionSearchQuery,
            FocusBlock, HourlyProjectSummary, Insight, InsightData, InsightType, NewInsight,
        },
    },
};

use super::{
    json::extract_json_payload,
    prompts::{
        build_semantic_search_prompt, load_daily_prompt_template, load_hourly_prompt_template,
        load_rolling_prompt_template,
    },
};

const ROLLING_CONTEXT_WINDOW_MINUTES: i64 = 30;
const HOURLY_DIGEST_WINDOW_HOURS: i64 = 1;
const HOURLY_DIGEST_INTERVAL: Duration = Duration::from_secs(60 * 60);
const DAILY_SUMMARY_POLL_INTERVAL: Duration = Duration::from_secs(60);

const SEMANTIC_SEARCH_FALLBACK_REFERENCES: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticSearchResult {
    pub answer: String,
    pub references: Vec<ExtractionSearchHit>,
    pub tokens_used: Option<u32>,
    pub cost_cents: Option<f64>,
}

pub struct RollingContextScheduler {
    config: AppConfig,
    db: StorageDb,
    provider: Arc<dyn LlmProvider>,
}

impl RollingContextScheduler {
    pub fn open(config: AppConfig, home: impl AsRef<Path>) -> Result<Self> {
        let provider = open_synthesis_provider(&config)?;
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

        let db = open_synthesis_db(&config, home.as_ref())?;

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
        validate_rolling_context_window(&data, window_start, window_end)?;
        let insight = build_new_synthesis_insight(
            &self.config,
            InsightType::Rolling,
            window_start,
            window_end,
            data,
            &response,
        )?;
        let persisted = self.db.insert_insight(&insight)?;

        Ok(Some(persisted))
    }
}

pub struct HourlyDigestScheduler {
    config: AppConfig,
    db: StorageDb,
    provider: Arc<dyn LlmProvider>,
}

impl HourlyDigestScheduler {
    pub fn open(config: AppConfig, home: impl AsRef<Path>) -> Result<Self> {
        let provider = open_synthesis_provider(&config)?;
        Self::with_provider(config, home, provider)
    }

    pub fn with_provider(
        config: AppConfig,
        home: impl AsRef<Path>,
        provider: Arc<dyn LlmProvider>,
    ) -> Result<Self> {
        ensure!(config.synthesis.enabled, "synthesis pipeline is disabled");
        ensure!(
            config.synthesis.hourly_enabled,
            "hourly digests are disabled"
        );

        let db = open_synthesis_db(&config, home.as_ref())?;

        Ok(Self {
            config,
            db,
            provider,
        })
    }

    pub async fn run_until_shutdown(&mut self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let first_tick = next_hour_boundary(Utc::now());
        let mut interval = time::interval_at(interval_start(first_tick), HOURLY_DIGEST_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.run_once().await {
                        Ok(Some(insight)) => {
                            info!(
                                insight_id = insight.id,
                                hour_start = %insight.window_start,
                                hour_end = %insight.window_end,
                                "hourly digest updated"
                            );
                        }
                        Ok(None) => {}
                        Err(error) => {
                            error!(error = %error, "hourly digest synthesis failed");
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
        self.run_once_at(truncate_to_hour(Utc::now())).await
    }

    async fn run_once_at(&mut self, hour_end: DateTime<Utc>) -> Result<Option<Insight>> {
        let hour_end = truncate_to_hour(hour_end);
        let hour_start = hour_end - ChronoDuration::hours(HOURLY_DIGEST_WINDOW_HOURS);
        let batches = self
            .db
            .list_extraction_batch_details_in_range(hour_start, hour_end)?;
        if batches.is_empty() {
            return Ok(None);
        }

        let prompt = build_hourly_digest_prompt(hour_start, hour_end, &batches);
        let response = self
            .provider
            .complete_text(&prompt)
            .await
            .context("hourly digest request failed")?;
        let data = parse_hourly_digest_response(&response.content)?;
        validate_hourly_digest_window(&data, hour_start, hour_end)?;
        let insight = build_new_synthesis_insight(
            &self.config,
            InsightType::Hourly,
            hour_start,
            hour_end,
            data,
            &response,
        )?;
        let persisted = self.db.insert_insight(&insight)?;

        Ok(Some(persisted))
    }
}

pub struct DailySummaryScheduler {
    config: AppConfig,
    db: StorageDb,
    provider: Arc<dyn LlmProvider>,
    home: PathBuf,
}

impl DailySummaryScheduler {
    pub fn open(config: AppConfig, home: impl AsRef<Path>) -> Result<Self> {
        let provider = open_synthesis_provider(&config)?;
        Self::with_provider(config, home, provider)
    }

    pub fn with_provider(
        config: AppConfig,
        home: impl AsRef<Path>,
        provider: Arc<dyn LlmProvider>,
    ) -> Result<Self> {
        ensure!(config.synthesis.enabled, "synthesis pipeline is disabled");
        parse_daily_summary_time(&config.synthesis.daily_summary_time)?;

        let home = home.as_ref().to_path_buf();
        let db = open_synthesis_db(&config, &home)?;

        Ok(Self {
            config,
            db,
            provider,
            home,
        })
    }

    pub async fn run_until_shutdown(&mut self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let mut interval = time::interval(DAILY_SUMMARY_POLL_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.run_once().await {
                        Ok(Some(insight)) => {
                            info!(
                                insight_id = insight.id,
                                date = %insight.window_start.date_naive(),
                                window_end = %insight.window_end,
                                "daily summary available"
                            );
                        }
                        Ok(None) => {}
                        Err(error) => {
                            error!(error = %error, "daily summary synthesis failed");
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
        let summary_time = parse_daily_summary_time(&self.config.synthesis.daily_summary_time)?;
        let now = Utc::now();
        let date = now.date_naive();
        let has_daily_summary = self.db.get_latest_daily_insight_for_date(date)?.is_some();
        if !should_run_daily_summary(now, summary_time, has_daily_summary) {
            return Ok(None);
        }

        self.run_once_at(utc_datetime_on_date(date, summary_time))
            .await
    }

    async fn run_once_at(&mut self, window_end: DateTime<Utc>) -> Result<Option<Insight>> {
        let date = window_end.date_naive();
        if let Some(existing) = self.db.get_latest_daily_insight_for_date(date)? {
            if self.config.synthesis.daily_export_markdown {
                export_daily(&existing, None, &self.config, &self.home).await?;
            }
            return Ok(Some(existing));
        }

        let window_start = utc_datetime_on_date(
            date,
            NaiveTime::from_hms_opt(0, 0, 0).expect("midnight should be representable"),
        );
        let hourly_insights = self
            .db
            .list_hourly_insights_in_range(window_start, window_end)?;
        if hourly_insights.is_empty() {
            return Ok(None);
        }

        let prompt = build_daily_summary_prompt(date, window_start, window_end, &hourly_insights);
        let response = self
            .provider
            .complete_text(&prompt)
            .await
            .context("daily summary request failed")?;
        let data = parse_daily_summary_response(&response.content)?;
        validate_daily_summary_date(&data, date)?;
        let insight = build_new_synthesis_insight(
            &self.config,
            InsightType::Daily,
            window_start,
            window_end,
            data,
            &response,
        )?;
        let persisted = self.db.insert_insight(&insight)?;
        if self.config.synthesis.daily_export_markdown {
            export_daily(&persisted, None, &self.config, &self.home).await?;
        }

        Ok(Some(persisted))
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

pub async fn run_hourly_digest_scheduler(
    config: AppConfig,
    home: impl AsRef<Path>,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut scheduler = HourlyDigestScheduler::open(config, home)?;
    scheduler.run_until_shutdown(shutdown).await
}

pub async fn run_daily_summary_scheduler(
    config: AppConfig,
    home: impl AsRef<Path>,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut scheduler = DailySummaryScheduler::open(config, home)?;
    scheduler.run_until_shutdown(shutdown).await
}

pub async fn get_or_generate_today_summary_at_home(
    config: AppConfig,
    home: impl AsRef<Path>,
    now: DateTime<Utc>,
) -> Result<Option<Insight>> {
    let home = home.as_ref();
    let date = now.date_naive();
    let day_start = utc_datetime_on_date(
        date,
        NaiveTime::from_hms_opt(0, 0, 0).expect("midnight should be representable"),
    );
    let db = open_synthesis_db(&config, home)?;

    if let Some(existing) = db.get_latest_daily_insight_for_date(date)? {
        return Ok(Some(existing));
    }

    if db.list_hourly_insights_in_range(day_start, now)?.is_empty() {
        return Ok(None);
    }

    let mut scheduler = DailySummaryScheduler::open(config, home)?;
    scheduler.run_once_at(now).await
}

pub fn semantic_search_candidates(
    db: &StorageDb,
    query: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: usize,
) -> Result<Vec<ExtractionSearchHit>> {
    let query = query.trim();
    ensure!(!query.is_empty(), "semantic search query must not be empty");

    let base_query = ExtractionSearchQuery {
        query: query.to_owned(),
        app_name: None,
        project: None,
        from,
        to,
        limit,
    };
    let direct_hits = db.search_extractions_filtered(&base_query)?;
    if !direct_hits.is_empty() {
        return Ok(direct_hits);
    }

    let mut hits = Vec::new();
    let mut seen_extraction_ids = HashSet::new();
    for term in semantic_search_fallback_terms(query) {
        let term_hits = db.search_extractions_filtered(&ExtractionSearchQuery {
            query: term,
            ..base_query.clone()
        })?;
        for hit in term_hits {
            if seen_extraction_ids.insert(hit.extraction.id) {
                hits.push(hit);
                if hits.len() >= limit {
                    return Ok(hits);
                }
            }
        }
    }

    Ok(hits)
}

fn semantic_search_fallback_terms(query: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "a", "an", "and", "are", "as", "at", "be", "by", "did", "do", "for", "from", "how", "i",
        "in", "into", "is", "it", "of", "on", "or", "that", "the", "to", "was", "were", "what",
        "when", "where", "which", "who", "why", "with", "you", "your",
    ];

    let mut terms = query
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter_map(|segment| {
            let lowered = segment.to_ascii_lowercase();
            if lowered.len() < 3 || STOP_WORDS.contains(&lowered.as_str()) {
                None
            } else {
                Some(lowered)
            }
        })
        .collect::<Vec<_>>();
    terms.sort_by_key(|term| std::cmp::Reverse(term.len()));
    terms.dedup();
    terms
}

pub async fn semantic_search(
    config: &AppConfig,
    query: &str,
    candidates: Vec<ExtractionSearchHit>,
) -> Result<SemanticSearchResult> {
    let provider = open_synthesis_provider(config)?;
    semantic_search_with_provider(provider.as_ref(), query, candidates).await
}

pub async fn semantic_search_with_provider(
    provider: &dyn LlmProvider,
    query: &str,
    candidates: Vec<ExtractionSearchHit>,
) -> Result<SemanticSearchResult> {
    let query = query.trim();
    ensure!(!query.is_empty(), "semantic search query must not be empty");

    semantic_search_from_candidates(provider, query, candidates).await
}

async fn semantic_search_from_candidates(
    provider: &dyn LlmProvider,
    query: &str,
    candidates: Vec<ExtractionSearchHit>,
) -> Result<SemanticSearchResult> {
    if candidates.is_empty() {
        return Ok(SemanticSearchResult {
            answer: "No relevant captures were found for that query in the selected range.".into(),
            references: Vec::new(),
            tokens_used: None,
            cost_cents: None,
        });
    }

    let prompt = build_semantic_search_prompt(
        query,
        &candidates
            .iter()
            .map(|candidate| candidate.extraction.clone())
            .collect::<Vec<_>>(),
    );
    let response = provider
        .complete_text(&prompt)
        .await
        .context("semantic search request failed")?;
    let tokens_used = response
        .usage
        .and_then(|usage| u32::try_from(usage.total_tokens).ok());

    match parse_semantic_search_response(&response.content) {
        Ok(parsed) => Ok(SemanticSearchResult {
            answer: parsed.answer,
            references: rank_semantic_references(candidates, &parsed.capture_ids),
            tokens_used,
            cost_cents: response.cost_cents,
        }),
        Err(_) => Ok(SemanticSearchResult {
            answer: response.content,
            references: candidates
                .into_iter()
                .take(SEMANTIC_SEARCH_FALLBACK_REFERENCES)
                .collect(),
            tokens_used,
            cost_cents: response.cost_cents,
        }),
    }
}

pub fn build_rolling_context_prompt(
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    batches: &[ExtractionBatchDetail],
) -> String {
    let rolling_prompt = load_rolling_prompt_template();
    let mut prompt = String::with_capacity(rolling_prompt.len() + batches.len() * 1024);
    prompt.push_str(&rolling_prompt);
    append_requested_window(
        &mut prompt,
        "window_start",
        window_start,
        "window_end",
        window_end,
    );
    append_extraction_batches(&mut prompt, batches);
    append_prompt_footer(&mut prompt);
    prompt
}

pub fn build_hourly_digest_prompt(
    hour_start: DateTime<Utc>,
    hour_end: DateTime<Utc>,
    batches: &[ExtractionBatchDetail],
) -> String {
    let hourly_prompt = load_hourly_prompt_template();
    let mut prompt = String::with_capacity(hourly_prompt.len() + batches.len() * 1024);
    prompt.push_str(&hourly_prompt);
    append_requested_window(&mut prompt, "hour_start", hour_start, "hour_end", hour_end);
    append_extraction_batches(&mut prompt, batches);
    append_prompt_footer(&mut prompt);
    prompt
}

pub fn build_daily_summary_prompt(
    date: NaiveDate,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    hourly_insights: &[Insight],
) -> String {
    let daily_prompt = load_daily_prompt_template();
    let mut prompt = String::with_capacity(daily_prompt.len() + hourly_insights.len() * 768);
    prompt.push_str(&daily_prompt);
    prompt.push_str("\n\nRequested date:\n");
    prompt.push_str(&format!(
        "- date: {}\n- window_start: {}\n- window_end: {}\n",
        date,
        format_prompt_timestamp(window_start),
        format_prompt_timestamp(window_end),
    ));
    append_hourly_insights(&mut prompt, hourly_insights);
    append_prompt_footer(&mut prompt);
    prompt
}

pub fn parse_semantic_search_response(json_str: &str) -> Result<SemanticSearchPayload> {
    let payload = extract_json_payload(json_str);
    serde_json::from_str::<SemanticSearchPayload>(payload)
        .context("failed to parse semantic search response JSON")
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

pub fn parse_hourly_digest_response(json_str: &str) -> Result<InsightData> {
    let payload = extract_json_payload(json_str);
    let parsed = serde_json::from_str::<HourlyDigestPayload>(payload)
        .context("failed to parse hourly digest response JSON")?;
    ensure!(
        parsed.insight_type == InsightType::Hourly,
        "expected hourly digest response, received insight type `{}`",
        parsed.insight_type
    );

    Ok(InsightData::Hourly {
        hour_start: parsed.hour_start,
        hour_end: parsed.hour_end,
        dominant_activity: parsed.dominant_activity,
        projects: parsed.projects,
        topics: parsed.topics,
        people_interacted: parsed.people_interacted,
        key_moments: parsed.key_moments,
        focus_score: parsed.focus_score,
        narrative: parsed.narrative,
    })
}

pub fn parse_daily_summary_response(json_str: &str) -> Result<InsightData> {
    let payload = extract_json_payload(json_str);
    let parsed = serde_json::from_str::<DailySummaryPayload>(payload)
        .context("failed to parse daily summary response JSON")?;
    ensure!(
        parsed.insight_type == InsightType::Daily,
        "expected daily summary response, received insight type `{}`",
        parsed.insight_type
    );

    Ok(InsightData::Daily {
        date: parsed.date,
        total_active_hours: parsed.total_active_hours,
        projects: parsed.projects,
        time_allocation: parsed.time_allocation,
        focus_blocks: parsed.focus_blocks,
        open_threads: parsed.open_threads,
        narrative: parsed.narrative,
    })
}

fn rank_semantic_references(
    mut candidates: Vec<ExtractionSearchHit>,
    capture_ids: &[i64],
) -> Vec<ExtractionSearchHit> {
    let mut ranked = Vec::new();

    for capture_id in capture_ids {
        if let Some(index) = candidates
            .iter()
            .position(|candidate| candidate.capture.id == *capture_id)
        {
            ranked.push(candidates.remove(index));
        }
    }

    if ranked.is_empty() {
        return candidates
            .into_iter()
            .take(SEMANTIC_SEARCH_FALLBACK_REFERENCES)
            .collect();
    }

    ranked
}

fn open_synthesis_provider(config: &AppConfig) -> Result<Arc<dyn LlmProvider>> {
    let provider_config = LlmProviderConfig::from(&config.synthesis);
    let provider: Arc<dyn LlmProvider> = create_provider(&provider_config)?.into();
    Ok(provider)
}

fn open_synthesis_db(config: &AppConfig, home: &Path) -> Result<StorageDb> {
    let db_path = config.storage_root(home).join("screencap.db");
    StorageDb::open_at_path(&db_path)
        .with_context(|| format!("failed to open synthesis database at {}", db_path.display()))
}

fn build_new_synthesis_insight(
    config: &AppConfig,
    insight_type: InsightType,
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
        insight_type,
        window_start,
        window_end,
        data,
        model_used: Some(config.synthesis.model.clone()),
        tokens_used,
        cost_cents: response.cost_cents,
    })
}

fn append_requested_window(
    prompt: &mut String,
    start_label: &str,
    start: DateTime<Utc>,
    end_label: &str,
    end: DateTime<Utc>,
) {
    prompt.push_str("\n\nRequested window:\n");
    prompt.push_str(&format!(
        "- {start_label}: {}\n- {end_label}: {}\n",
        format_prompt_timestamp(start),
        format_prompt_timestamp(end),
    ));
}

fn append_extraction_batches(prompt: &mut String, batches: &[ExtractionBatchDetail]) {
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
}

fn append_hourly_insights(prompt: &mut String, insights: &[Insight]) {
    prompt.push_str("\nHourly digests:\n");

    if insights.is_empty() {
        prompt.push_str("- none\n");
    }

    for insight in insights {
        let InsightData::Hourly {
            hour_start,
            hour_end,
            dominant_activity,
            projects,
            topics,
            people_interacted,
            key_moments,
            focus_score,
            narrative,
        } = &insight.data
        else {
            unreachable!("daily summary prompts only accept hourly insights");
        };

        prompt.push_str(&format!(
            concat!(
                "- insight_id: {insight_id}\n",
                "  hour_start_utc: {hour_start}\n",
                "  hour_end_utc: {hour_end}\n",
                "  dominant_activity: {dominant_activity}\n",
                "  projects: {projects}\n",
                "  topics: {topics}\n",
                "  people_interacted: {people}\n",
                "  key_moments: {key_moments}\n",
                "  focus_score: {focus_score}\n",
                "  narrative: {narrative}\n"
            ),
            insight_id = insight.id,
            hour_start = format_prompt_timestamp(*hour_start),
            hour_end = format_prompt_timestamp(*hour_end),
            dominant_activity = dominant_activity,
            projects = format_json_value(projects),
            topics = format_string_list(topics),
            people = format_string_list(people_interacted),
            key_moments = format_string_list(key_moments),
            focus_score = focus_score,
            narrative = narrative,
        ));
    }
}

fn append_prompt_footer(prompt: &mut String) {
    prompt.push_str(
        "\nUse only the structured data above. Return JSON only; do not wrap it in markdown or add commentary.",
    );
}

fn validate_rolling_context_window(
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

    validate_requested_window(
        "rolling context response window",
        *window_start,
        *window_end,
        expected_start,
        expected_end,
    )
}

fn validate_hourly_digest_window(
    data: &InsightData,
    expected_start: DateTime<Utc>,
    expected_end: DateTime<Utc>,
) -> Result<()> {
    let InsightData::Hourly {
        hour_start,
        hour_end,
        ..
    } = data
    else {
        unreachable!("hourly digest parser only returns hourly insight data");
    };

    validate_requested_window(
        "hourly digest response window",
        *hour_start,
        *hour_end,
        expected_start,
        expected_end,
    )
}

fn validate_daily_summary_date(data: &InsightData, expected_date: NaiveDate) -> Result<()> {
    let InsightData::Daily { date, .. } = data else {
        unreachable!("daily summary parser only returns daily insight data");
    };

    ensure!(
        *date == expected_date,
        "daily summary response date {} did not match requested date {}",
        date,
        expected_date,
    );

    Ok(())
}

fn validate_requested_window(
    label: &str,
    actual_start: DateTime<Utc>,
    actual_end: DateTime<Utc>,
    expected_start: DateTime<Utc>,
    expected_end: DateTime<Utc>,
) -> Result<()> {
    ensure!(
        actual_start == expected_start && actual_end == expected_end,
        "{label} {}..{} did not match requested window {}..{}",
        format_prompt_timestamp(actual_start),
        format_prompt_timestamp(actual_end),
        format_prompt_timestamp(expected_start),
        format_prompt_timestamp(expected_end),
    );

    Ok(())
}

fn truncate_to_hour(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    timestamp
        .with_minute(0)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .expect("UTC timestamps should always truncate to the hour")
}

fn next_hour_boundary(after: DateTime<Utc>) -> DateTime<Utc> {
    let boundary = truncate_to_hour(after);
    if after == boundary {
        boundary
    } else {
        boundary + ChronoDuration::hours(1)
    }
}

fn parse_daily_summary_time(raw: &str) -> Result<NaiveTime> {
    NaiveTime::parse_from_str(raw, "%H:%M")
        .with_context(|| format!("invalid synthesis daily_summary_time `{raw}`; expected HH:MM"))
}

fn utc_datetime_on_date(date: NaiveDate, time: NaiveTime) -> DateTime<Utc> {
    date.and_time(time).and_utc()
}

#[cfg(test)]
fn next_daily_summary_boundary(after: DateTime<Utc>, summary_time: NaiveTime) -> DateTime<Utc> {
    let boundary = utc_datetime_on_date(after.date_naive(), summary_time);
    if after <= boundary {
        boundary
    } else {
        utc_datetime_on_date(
            after
                .date_naive()
                .succ_opt()
                .expect("successor date should exist"),
            summary_time,
        )
    }
}

fn should_run_daily_summary(
    now: DateTime<Utc>,
    summary_time: NaiveTime,
    has_daily_summary: bool,
) -> bool {
    now >= utc_datetime_on_date(now.date_naive(), summary_time) && !has_daily_summary
}

fn interval_start(target: DateTime<Utc>) -> Instant {
    let now = Utc::now();
    let delay = target
        .signed_duration_since(now)
        .to_std()
        .unwrap_or(Duration::ZERO);
    Instant::now() + delay
}

fn format_prompt_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn format_string_list(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

fn format_json_value<T>(value: &T) -> String
where
    T: Serialize,
{
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSearchPayload {
    pub answer: String,
    #[serde(default)]
    pub capture_ids: Vec<i64>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HourlyDigestPayload {
    #[serde(rename = "type")]
    insight_type: InsightType,
    hour_start: DateTime<Utc>,
    hour_end: DateTime<Utc>,
    dominant_activity: String,
    projects: Vec<HourlyProjectSummary>,
    topics: Vec<String>,
    people_interacted: Vec<String>,
    key_moments: Vec<String>,
    focus_score: f64,
    narrative: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DailySummaryPayload {
    #[serde(rename = "type")]
    insight_type: InsightType,
    date: NaiveDate,
    total_active_hours: f64,
    projects: Vec<DailyProjectSummary>,
    time_allocation: BTreeMap<String, String>,
    focus_blocks: Vec<FocusBlock>,
    open_threads: Vec<String>,
    narrative: String,
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
    fn build_hourly_digest_prompt_includes_batches_and_frames() {
        let hour_start = Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
        let hour_end = Utc.with_ymd_and_hms(2026, 4, 10, 15, 0, 0).unwrap();
        let prompt = build_hourly_digest_prompt(hour_start, hour_end, &[sample_batch_detail()]);

        assert!(prompt.contains("hour_start: 2026-04-10T14:00:00Z"));
        assert!(prompt.contains("hour_end: 2026-04-10T15:00:00Z"));
        assert!(prompt.contains("batch_id: 123e4567-e89b-12d3-a456-426614174000"));
        assert!(prompt.contains("capture_id: 101"));
        assert!(prompt.contains("app_name: Ghostty"));
        assert!(prompt.contains("people: [\"@alice\"]"));
    }

    #[test]
    fn build_daily_summary_prompt_includes_hourly_digests() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let window_start = Utc.with_ymd_and_hms(2026, 4, 10, 0, 0, 0).unwrap();
        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 18, 0, 0).unwrap();
        let prompt = build_daily_summary_prompt(
            date,
            window_start,
            window_end,
            &[sample_hourly_insight(1, 9), sample_hourly_insight(2, 10)],
        );

        assert!(prompt.contains("date: 2026-04-10"));
        assert!(prompt.contains("window_start: 2026-04-10T00:00:00Z"));
        assert!(prompt.contains("window_end: 2026-04-10T18:00:00Z"));
        assert!(prompt.contains("insight_id: 1"));
        assert!(prompt.contains("dominant_activity: coding"));
        assert!(prompt.contains("projects: [{\"name\":\"screencap\""));
        assert!(prompt.contains("people_interacted: [\"@alice\"]"));
        assert!(prompt.contains("narrative: Productive coding hour"));
    }

    #[test]
    fn parse_semantic_search_response_accepts_fenced_json() {
        let parsed = parse_semantic_search_response(
            "```json\n{\n  \"answer\": \"You were debugging JWT refresh handling.\",\n  \"capture_ids\": [2, 1]\n}\n```",
        )
        .expect("parse semantic search response");

        assert_eq!(parsed.answer, "You were debugging JWT refresh handling.");
        assert_eq!(parsed.capture_ids, vec![2, 1]);
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
    fn parse_hourly_digest_response_accepts_fenced_json() {
        let parsed = parse_hourly_digest_response(
            "```json\n{\n  \"type\": \"hourly\",\n  \"hour_start\": \"2026-04-10T14:00:00Z\",\n  \"hour_end\": \"2026-04-10T15:00:00Z\",\n  \"dominant_activity\": \"coding\",\n  \"projects\": [\n    {\"name\": \"screencap\", \"minutes\": 42, \"activities\": [\"debugging auth\", \"writing tests\"]}\n  ],\n  \"topics\": [\"JWT\", \"authentication\"],\n  \"people_interacted\": [\"@alice\"],\n  \"key_moments\": [\"Found the bug\"],\n  \"focus_score\": 0.72,\n  \"narrative\": \"Productive coding hour.\"\n}\n```",
        )
        .expect("parse hourly digest response");

        assert_eq!(
            parsed,
            InsightData::Hourly {
                hour_start: Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap(),
                hour_end: Utc.with_ymd_and_hms(2026, 4, 10, 15, 0, 0).unwrap(),
                dominant_activity: "coding".into(),
                projects: vec![HourlyProjectSummary {
                    name: Some("screencap".into()),
                    minutes: 42,
                    activities: vec!["debugging auth".into(), "writing tests".into()],
                }],
                topics: vec!["JWT".into(), "authentication".into()],
                people_interacted: vec!["@alice".into()],
                key_moments: vec!["Found the bug".into()],
                focus_score: 0.72,
                narrative: "Productive coding hour.".into(),
            }
        );
    }

    #[test]
    fn parse_daily_summary_response_accepts_fenced_json() {
        let parsed = parse_daily_summary_response(
            "```json\n{\n  \"type\": \"daily\",\n  \"date\": \"2026-04-10\",\n  \"total_active_hours\": 7.5,\n  \"projects\": [\n    {\"name\": \"screencap\", \"total_minutes\": 195, \"activities\": [\"auth module debugging\"], \"key_accomplishments\": [\"Fixed JWT refresh bug\"]}\n  ],\n  \"time_allocation\": {\"coding\": \"3h 15m\"},\n  \"focus_blocks\": [\n    {\"start\": \"09:15\", \"end\": \"11:45\", \"duration_min\": 150, \"project\": \"screencap\", \"quality\": \"deep-focus\"}\n  ],\n  \"open_threads\": [\"Need to finish the export path\"],\n  \"narrative\": \"Productive day focused on screencap.\"\n}\n```",
        )
        .expect("parse daily summary response");

        assert_eq!(
            parsed,
            InsightData::Daily {
                date: NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
                total_active_hours: 7.5,
                projects: vec![DailyProjectSummary {
                    name: "screencap".into(),
                    total_minutes: 195,
                    activities: vec!["auth module debugging".into()],
                    key_accomplishments: vec!["Fixed JWT refresh bug".into()],
                }],
                time_allocation: BTreeMap::from([("coding".into(), "3h 15m".into())]),
                focus_blocks: vec![FocusBlock {
                    start: "09:15".into(),
                    end: "11:45".into(),
                    duration_min: 150,
                    project: "screencap".into(),
                    quality: "deep-focus".into(),
                }],
                open_threads: vec!["Need to finish the export path".into()],
                narrative: "Productive day focused on screencap.".into(),
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

    #[test]
    fn parse_hourly_digest_response_rejects_malformed_json() {
        let error =
            parse_hourly_digest_response("```json\n{\"type\":\"hourly\",\"hour_start\":}\n```")
                .expect_err("malformed json should fail");
        assert!(error
            .to_string()
            .contains("failed to parse hourly digest response JSON"));
    }

    #[test]
    fn parse_daily_summary_response_rejects_malformed_json() {
        let error = parse_daily_summary_response("```json\n{\"type\":\"daily\",\"date\":}\n```")
            .expect_err("malformed json should fail");
        assert!(error
            .to_string()
            .contains("failed to parse daily summary response JSON"));
    }
    #[test]
    fn parse_semantic_search_response_rejects_malformed_json() {
        let error =
            parse_semantic_search_response("```json\n{\"answer\":\"bad\",\"capture_ids\":}\n```")
                .expect_err("malformed semantic json should fail");
        assert!(error
            .to_string()
            .contains("failed to parse semantic search response JSON"));
    }

    #[tokio::test]
    async fn semantic_search_with_provider_returns_ranked_references() -> Result<()> {
        let home = temp_home_root("semantic-ranked");
        let config = test_config(&home);
        let mut db = open_synthesis_db(&config, &home)?;
        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 14, 30, 0).unwrap();
        seed_recent_extractions(&mut db, window_end)?;

        let provider = MockLlmProvider::new();
        provider.push_response(Ok(LlmResponse::with_usage_and_cost(
            "```json\n{\"answer\":\"You were fixing JWT refresh logic and validating it with tests.\",\"capture_ids\":[2,1]}\n```",
            TokenUsage {
                prompt_tokens: 140,
                completion_tokens: 60,
                total_tokens: 200,
            },
            0.17,
        )));

        let candidates = semantic_search_candidates(
            &db,
            "what was I doing around jwt refresh?",
            None,
            None,
            10,
        )?;
        let result = semantic_search_with_provider(
            &provider,
            "what was I doing around jwt refresh?",
            candidates,
        )
        .await?;

        assert_eq!(
            result.answer,
            "You were fixing JWT refresh logic and validating it with tests."
        );
        assert_eq!(result.tokens_used, Some(200));
        assert_eq!(result.cost_cents, Some(0.17));
        assert_eq!(
            result
                .references
                .iter()
                .map(|reference| reference.capture.id)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );

        let calls = provider.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].kind, MockCallKind::Text);
        assert!(calls[0]
            .prompt
            .contains("what was I doing around jwt refresh?"));
        assert!(calls[0].prompt.contains("capture_id: 1"));

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn semantic_search_with_provider_falls_back_to_raw_answer() -> Result<()> {
        let home = temp_home_root("semantic-fallback");
        let config = test_config(&home);
        let mut db = open_synthesis_db(&config, &home)?;
        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 14, 30, 0).unwrap();
        seed_recent_extractions(&mut db, window_end)?;

        let provider = MockLlmProvider::new();
        provider.push_text_response("Raw non-JSON fallback answer");

        let candidates = semantic_search_candidates(&db, "jwt refresh", None, None, 10)?;
        let result = semantic_search_with_provider(&provider, "jwt refresh", candidates).await?;

        assert_eq!(result.answer, "Raw non-JSON fallback answer");
        assert_eq!(result.tokens_used, None);
        assert!(result.cost_cents.is_none());
        assert!(!result.references.is_empty());

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[test]
    fn next_hour_boundary_rounds_up_unless_already_aligned() {
        let mid_hour = Utc.with_ymd_and_hms(2026, 4, 10, 14, 23, 45).unwrap();
        let aligned = Utc.with_ymd_and_hms(2026, 4, 10, 15, 0, 0).unwrap();

        assert_eq!(
            next_hour_boundary(mid_hour),
            Utc.with_ymd_and_hms(2026, 4, 10, 15, 0, 0).unwrap()
        );
        assert_eq!(next_hour_boundary(aligned), aligned);
    }

    #[test]
    fn next_daily_summary_boundary_rounds_up_unless_already_aligned() {
        let summary_time = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        let before_boundary = Utc.with_ymd_and_hms(2026, 4, 10, 17, 45, 0).unwrap();
        let aligned = Utc.with_ymd_and_hms(2026, 4, 10, 18, 0, 0).unwrap();
        let after_boundary = Utc.with_ymd_and_hms(2026, 4, 10, 18, 5, 0).unwrap();

        assert_eq!(
            next_daily_summary_boundary(before_boundary, summary_time),
            aligned
        );
        assert_eq!(next_daily_summary_boundary(aligned, summary_time), aligned);
        assert_eq!(
            next_daily_summary_boundary(after_boundary, summary_time),
            Utc.with_ymd_and_hms(2026, 4, 11, 18, 0, 0).unwrap()
        );
    }

    #[test]
    fn parse_daily_summary_time_accepts_hh_mm() {
        assert_eq!(
            parse_daily_summary_time("23:50").expect("valid 24-hour HH:MM time"),
            NaiveTime::from_hms_opt(23, 50, 0).unwrap()
        );
        assert!(parse_daily_summary_time("24:00").is_err());
    }

    #[test]
    fn should_run_daily_summary_requires_time_and_missing_summary() {
        let summary_time = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        let before_summary = Utc.with_ymd_and_hms(2026, 4, 10, 17, 59, 59).unwrap();
        let at_summary = Utc.with_ymd_and_hms(2026, 4, 10, 18, 0, 0).unwrap();

        assert!(!should_run_daily_summary(
            before_summary,
            summary_time,
            false
        ));
        assert!(should_run_daily_summary(at_summary, summary_time, false));
        assert!(!should_run_daily_summary(at_summary, summary_time, true));
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

    #[tokio::test]
    async fn run_once_skips_empty_hourly_windows() -> Result<()> {
        let home = temp_home_root("hourly-empty");
        let config = test_config(&home);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler = HourlyDigestScheduler::with_provider(config, &home, provider.clone())?;

        let hour_end = Utc.with_ymd_and_hms(2026, 4, 10, 15, 0, 0).unwrap();
        let insight = scheduler.run_once_at(hour_end).await?;
        assert!(insight.is_none());
        assert!(provider.calls().is_empty());

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_once_skips_empty_daily_windows() -> Result<()> {
        let home = temp_home_root("daily-empty");
        let config = test_config(&home);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler = DailySummaryScheduler::with_provider(config, &home, provider.clone())?;

        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 18, 0, 0).unwrap();
        let insight = scheduler.run_once_at(window_end).await?;
        assert!(insight.is_none());
        assert!(provider.calls().is_empty());

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_once_persists_hourly_digest_insight() -> Result<()> {
        let home = temp_home_root("hourly-digest");
        let config = test_config(&home);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler = HourlyDigestScheduler::with_provider(config, &home, provider.clone())?;
        let hour_end = Utc.with_ymd_and_hms(2026, 4, 10, 15, 0, 0).unwrap();
        seed_recent_extractions(&mut scheduler.db, hour_end)?;

        provider.push_response(Ok(LlmResponse::with_usage_and_cost(
            hourly_success_response_json(hour_end - ChronoDuration::hours(1), hour_end),
            TokenUsage {
                prompt_tokens: 220,
                completion_tokens: 80,
                total_tokens: 300,
            },
            0.42,
        )));

        let insight = scheduler
            .run_once_at(hour_end)
            .await?
            .expect("hourly digest should be created");

        assert_eq!(insight.insight_type, InsightType::Hourly);
        assert_eq!(insight.window_start, hour_end - ChronoDuration::hours(1));
        assert_eq!(insight.window_end, hour_end);
        assert_eq!(insight.model_used.as_deref(), Some("mock-synthesis-model"));
        assert_eq!(insight.tokens_used, Some(300));
        assert_eq!(insight.cost_cents, Some(0.42));

        let InsightData::Hourly {
            dominant_activity,
            projects,
            topics,
            people_interacted,
            key_moments,
            focus_score,
            narrative,
            ..
        } = &insight.data
        else {
            unreachable!("expected hourly insight payload");
        };
        assert_eq!(dominant_activity, "coding");
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name.as_deref(), Some("screencap"));
        assert_eq!(projects[0].minutes, 42);
        assert_eq!(projects[1].name, None);
        assert_eq!(
            topics,
            &vec![
                "JWT".to_owned(),
                "authentication".to_owned(),
                "testing".to_owned()
            ]
        );
        assert_eq!(people_interacted, &vec!["@alice".to_owned()]);
        assert_eq!(key_moments.len(), 2);
        assert_eq!(*focus_score, 0.72);
        assert!(narrative.contains("Productive coding hour"));

        let insight_count: i64 = scheduler.db.connection().query_row(
            "SELECT COUNT(*) FROM insights WHERE type = 'hourly'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(insight_count, 1);

        let fts_narrative: String = scheduler.db.connection().query_row(
            "SELECT narrative FROM insights_fts WHERE insight_id = ?1",
            [insight.id],
            |row| row.get(0),
        )?;
        assert!(fts_narrative.contains("Productive coding hour"));

        let calls = provider.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].kind, MockCallKind::Text);
        assert!(calls[0].images.is_empty());
        assert!(calls[0].prompt.contains("hour_start: 2026-04-10T14:00:00Z"));
        assert!(calls[0].prompt.contains("capture_id: 1"));

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_once_persists_daily_summary_insight() -> Result<()> {
        let home = temp_home_root("daily-summary");
        let config = test_config(&home);
        let provider = Arc::new(MockLlmProvider::new());
        let mut scheduler = DailySummaryScheduler::with_provider(config, &home, provider.clone())?;
        let window_end = Utc.with_ymd_and_hms(2026, 4, 10, 18, 0, 0).unwrap();
        seed_hourly_insights(&mut scheduler.db, window_end.date_naive())?;

        provider.push_response(Ok(LlmResponse::with_usage_and_cost(
            daily_success_response_json(window_end.date_naive()),
            TokenUsage {
                prompt_tokens: 320,
                completion_tokens: 120,
                total_tokens: 440,
            },
            0.61,
        )));

        let insight = scheduler
            .run_once_at(window_end)
            .await?
            .expect("daily summary should be created");

        assert_eq!(insight.insight_type, InsightType::Daily);
        assert_eq!(
            insight.window_start,
            Utc.with_ymd_and_hms(2026, 4, 10, 0, 0, 0).unwrap()
        );
        assert_eq!(insight.window_end, window_end);
        assert_eq!(insight.model_used.as_deref(), Some("mock-synthesis-model"));
        assert_eq!(insight.tokens_used, Some(440));
        assert_eq!(insight.cost_cents, Some(0.61));

        let InsightData::Daily {
            date,
            total_active_hours,
            projects,
            time_allocation,
            focus_blocks,
            open_threads,
            narrative,
        } = &insight.data
        else {
            unreachable!("expected daily insight payload");
        };
        assert_eq!(*date, NaiveDate::from_ymd_opt(2026, 4, 10).unwrap());
        assert_eq!(*total_active_hours, 7.5);
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "screencap");
        assert_eq!(
            time_allocation.get("coding").map(String::as_str),
            Some("3h 15m")
        );
        assert_eq!(focus_blocks.len(), 2);
        assert_eq!(open_threads.len(), 2);
        assert!(narrative.contains("Productive day focused on screencap"));

        let insight_count: i64 = scheduler.db.connection().query_row(
            "SELECT COUNT(*) FROM insights WHERE type = 'daily'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(insight_count, 1);

        let fts_narrative: String = scheduler.db.connection().query_row(
            "SELECT narrative FROM insights_fts WHERE insight_id = ?1",
            [insight.id],
            |row| row.get(0),
        )?;
        assert!(fts_narrative.contains("Productive day focused on screencap"));

        let calls = provider.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].kind, MockCallKind::Text);
        assert!(calls[0].images.is_empty());
        assert!(calls[0].prompt.contains("date: 2026-04-10"));
        assert!(calls[0].prompt.contains("insight_id:"));
        let exported_markdown_path = home.join(".screencap/daily/2026-04-10.md");
        assert!(exported_markdown_path.exists());
        let exported_markdown = fs::read_to_string(&exported_markdown_path)?;
        assert!(exported_markdown.contains("# Screencap: 2026-04-10"));
        assert!(exported_markdown.contains("## Open Threads"));

        fs::remove_dir_all(&home)?;
        Ok(())
    }

    fn sample_hourly_insight(id: i64, hour_start_utc: u32) -> Insight {
        let hour_start = Utc
            .with_ymd_and_hms(2026, 4, 10, hour_start_utc, 0, 0)
            .unwrap();
        let hour_end = hour_start + ChronoDuration::hours(1);
        Insight {
            id,
            insight_type: InsightType::Hourly,
            window_start: hour_start,
            window_end: hour_end,
            data: InsightData::Hourly {
                hour_start,
                hour_end,
                dominant_activity: "coding".into(),
                projects: vec![HourlyProjectSummary {
                    name: Some("screencap".into()),
                    minutes: 42,
                    activities: vec!["debugging auth".into(), "writing tests".into()],
                }],
                topics: vec!["JWT".into(), "authentication".into()],
                people_interacted: vec!["@alice".into()],
                key_moments: vec!["Found the bug".into()],
                focus_score: 0.72,
                narrative: "Productive coding hour. The user traced the auth flow and wrote tests."
                    .into(),
            },
            narrative: "Productive coding hour. The user traced the auth flow and wrote tests."
                .into(),
            model_used: Some("mock-synthesis-model".into()),
            tokens_used: Some(300),
            cost_cents: Some(0.42),
            created_at: hour_end,
        }
    }

    fn sample_hourly_new_insight(date: NaiveDate, start_hour: u32) -> NewInsight {
        let hour_start =
            utc_datetime_on_date(date, NaiveTime::from_hms_opt(start_hour, 0, 0).unwrap());
        let hour_end = hour_start + ChronoDuration::hours(1);
        NewInsight {
            insight_type: InsightType::Hourly,
            window_start: hour_start,
            window_end: hour_end,
            data: InsightData::Hourly {
                hour_start,
                hour_end,
                dominant_activity: if start_hour < 12 {
                    "coding".into()
                } else {
                    "communication".into()
                },
                projects: vec![
                    HourlyProjectSummary {
                        name: Some("screencap".into()),
                        minutes: 42,
                        activities: vec!["debugging auth".into(), "writing tests".into()],
                    },
                    HourlyProjectSummary {
                        name: None,
                        minutes: 18,
                        activities: vec!["Slack conversations".into()],
                    },
                ],
                topics: vec!["JWT".into(), "authentication".into(), "testing".into()],
                people_interacted: vec!["@alice".into()],
                key_moments: vec![
                    "Found the JWT refresh bug and validated the fix".into(),
                    "Shared the outcome with Alice in Slack".into(),
                ],
                focus_score: 0.72,
                narrative: "Productive coding hour. The user traced the JWT refresh path, checked documentation, ran targeted tests, and shared the result in Slack.".into(),
            },
            model_used: Some("mock-synthesis-model".into()),
            tokens_used: Some(300),
            cost_cents: Some(0.42),
        }
    }

    fn seed_hourly_insights(db: &mut StorageDb, date: NaiveDate) -> Result<()> {
        db.insert_insight(&sample_hourly_new_insight(date, 9))?;
        db.insert_insight(&sample_hourly_new_insight(date, 10))?;
        db.insert_insight(&sample_hourly_new_insight(date, 14))?;
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

    fn hourly_success_response_json(hour_start: DateTime<Utc>, hour_end: DateTime<Utc>) -> String {
        format!(
            concat!(
                "{{",
                "\"type\":\"hourly\",",
                "\"hour_start\":\"{}\",",
                "\"hour_end\":\"{}\",",
                "\"dominant_activity\":\"coding\",",
                "\"projects\":[",
                "{{\"name\":\"screencap\",\"minutes\":42,\"activities\":[\"debugging auth\",\"writing tests\"]}},",
                "{{\"name\":null,\"minutes\":18,\"activities\":[\"Slack conversations\"]}}",
                "],",
                "\"topics\":[\"JWT\",\"authentication\",\"testing\"],",
                "\"people_interacted\":[\"@alice\"],",
                "\"key_moments\":[",
                "\"Found the JWT refresh bug and validated the fix\",",
                "\"Shared the outcome with Alice in Slack\"",
                "],",
                "\"focus_score\":0.72,",
                "\"narrative\":\"Productive coding hour. The user traced the JWT refresh path, checked documentation, ran targeted tests, and shared the result in Slack.\"",
                "}}"
            ),
            format_prompt_timestamp(hour_start),
            format_prompt_timestamp(hour_end),
        )
    }

    fn daily_success_response_json(date: NaiveDate) -> String {
        format!(
            concat!(
                "{{",
                "\"type\":\"daily\",",
                "\"date\":\"{}\",",
                "\"total_active_hours\":7.5,",
                "\"projects\":[",
                "{{\"name\":\"screencap\",\"total_minutes\":195,\"activities\":[\"auth module debugging\",\"test writing\"],\"key_accomplishments\":[\"Fixed JWT refresh bug\"]}},",
                "{{\"name\":\"admin\",\"total_minutes\":85,\"activities\":[\"Slack\",\"email\"],\"key_accomplishments\":[\"Aligned on deployment timeline\"]}}",
                "],",
                "\"time_allocation\":{{\"coding\":\"3h 15m\",\"communication\":\"1h 25m\"}},",
                "\"focus_blocks\":[",
                "{{\"start\":\"09:15\",\"end\":\"11:45\",\"duration_min\":150,\"project\":\"screencap\",\"quality\":\"deep-focus\"}},",
                "{{\"start\":\"14:00\",\"end\":\"15:30\",\"duration_min\":90,\"project\":\"screencap\",\"quality\":\"moderate-focus\"}}",
                "],",
                "\"open_threads\":[",
                "\"Need to finish the export path\",",
                "\"Follow up with Alice on API docs\"",
                "],",
                "\"narrative\":\"Productive day focused on screencap. The user made progress on auth, tests, and follow-up communication.\"",
                "}}"
            ),
            date,
        )
    }

    fn test_config(home: &Path) -> AppConfig {
        let mut config = AppConfig::default();
        config.storage.path = home.to_string_lossy().into_owned();
        config.synthesis.model = "mock-synthesis-model".into();
        config.synthesis.rolling_interval_secs = 60;
        config.synthesis.hourly_enabled = true;
        config.synthesis.daily_summary_time = "18:00".into();
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
