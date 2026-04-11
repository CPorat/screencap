use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Instant,
};

use anyhow::Context;
use axum::{
    body::{Body, Bytes},
    extract::{Path as AxumPath, Query, RawQuery, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::error;

use crate::{
    ai::provider::ProviderError,
    config::AppConfig,
    daemon::CAPTURE_PAUSED,
    pipeline::synthesis,
    storage::{
        db::StorageDb,
        metrics,
        models::{
            ActivityType, AppCaptureCount, Capture, CaptureDetail, CaptureQuery, Extraction,
            ExtractionSearchHit, ExtractionStatus, Insight, InsightData, InsightType,
            ProjectTimeAllocation, SearchHit, SearchQuery, TopicFrequency,
        },
        screenshots::{
            read_screenshot_file, relative_screenshot_path, sanitize_relative_screenshot_path,
        },
    },
};

use super::static_assets;
const DEFAULT_CAPTURE_LIMIT: usize = 100;
const MAX_CAPTURE_LIMIT: usize = 500;
const DEFAULT_SEARCH_LIMIT: usize = 50;
const MAX_SEARCH_LIMIT: usize = 200;
const DEFAULT_SEMANTIC_SEARCH_LIMIT: usize = 100;
const MAX_SEMANTIC_SEARCH_LIMIT: usize = 300;
const NO_PROCESSED_ACTIVITY_IN_RANGE: &str =
    "no processed activity is available in the requested range; extraction may still be pending";
const NO_RELEVANT_ACTIVITY_IN_RANGE: &str =
    "No relevant captures were found for that query in the selected range.";

#[derive(Clone)]
struct ApiState {
    config: AppConfig,
    db_path: PathBuf,
    storage_root: PathBuf,
    screenshots_root: PathBuf,
    started_at: Instant,
}

impl ApiState {
    fn new(config: &AppConfig, home: &Path) -> Self {
        Self {
            config: config.clone(),
            db_path: config.storage_root(home).join("screencap.db"),
            storage_root: config.storage_root(home),
            screenshots_root: config.screenshots_root(home),
            started_at: Instant::now(),
        }
    }

    fn open_db(&self) -> anyhow::Result<Option<StorageDb>> {
        StorageDb::open_existing_at_path(&self.db_path)
            .with_context(|| format!("failed to open api database at {}", self.db_path.display()))
    }

    fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_secs: u64,
}

#[derive(Debug, Serialize)]
struct StatsResponse {
    capture_count: u64,
    captures_today: u64,
    storage_bytes: u64,
    uptime_secs: u64,
}

#[derive(Debug, Serialize)]
struct CapturePausedResponse {
    paused: bool,
}

#[derive(Debug, Serialize)]
struct ApiCapture {
    id: i64,
    timestamp: DateTime<Utc>,
    app_name: Option<String>,
    window_title: Option<String>,
    bundle_id: Option<String>,
    display_id: Option<i64>,
    screenshot_url: Option<String>,
    extraction_status: ExtractionStatus,
    extraction_id: Option<i64>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ApiCaptureDetail {
    capture: ApiCapture,
    extraction: Option<Extraction>,
}

#[derive(Debug, Serialize)]
struct CaptureListResponse {
    captures: Vec<ApiCapture>,
    limit: usize,
    offset: usize,
}

#[derive(Debug, Serialize)]
struct AppsResponse {
    apps: Vec<AppCaptureCount>,
}

#[derive(Debug, Serialize)]
struct InsightListResponse {
    insights: Vec<Insight>,
}

#[derive(Debug, Serialize)]
struct ProjectTimeAllocationResponse {
    projects: Vec<ProjectTimeAllocation>,
}

#[derive(Debug, Serialize)]
struct TopicFrequencyResponse {
    topics: Vec<TopicFrequency>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "source_type", rename_all = "snake_case")]
enum ApiSearchHit {
    Extraction {
        timestamp: DateTime<Utc>,
        rank: f64,
        capture: ApiCapture,
        extraction: Extraction,
        batch_narrative: Option<String>,
    },
    Insight {
        timestamp: DateTime<Utc>,
        rank: f64,
        primary_project: Option<String>,
        primary_activity_type: Option<ActivityType>,
        insight: Insight,
    },
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    results: Vec<ApiSearchHit>,
    limit: usize,
}

#[derive(Debug, Serialize)]
struct SemanticSearchReference {
    capture: ApiCapture,
    extraction: Extraction,
}

#[derive(Debug, Serialize)]
struct SemanticSearchResponse {
    answer: String,
    references: Vec<SemanticSearchReference>,
    cost_cents: Option<f64>,
    tokens_used: Option<u32>,
}

#[derive(Debug, Serialize)]
struct AnalyzeResponse {
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    capture_count: u64,
    analysis_query: Option<String>,
    analysis: AnalyzePayload,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AnalyzePayload {
    RollingContext {
        batch_count: usize,
        analyzed_capture_count: usize,
        insight: InsightData,
        tokens_used: Option<u32>,
        cost_cents: Option<f64>,
    },
    QuestionAnswer {
        analyzed_capture_count: usize,
        answer: String,
        references: Vec<SemanticSearchReference>,
        tokens_used: Option<u32>,
        cost_cents: Option<f64>,
    },
}

#[derive(Debug, Deserialize)]
struct AnalyzeRequest {
    from: String,
    to: String,
    query: Option<String>,
    prompt: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize, Default)]
struct CaptureListParams {
    from: Option<String>,
    to: Option<String>,
    app: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
struct DateParams {
    date: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct DateRangeParams {
    date: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TimeRangeParams {
    from: Option<String>,
    to: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct SearchParams {
    q: Option<String>,
    app: Option<String>,
    project: Option<String>,
    activity_type: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
struct SemanticSearchParams {
    q: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }

    fn internal(error: anyhow::Error) -> Self {
        error!(error = %error, "api request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "internal server error".into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

pub fn router(config: &AppConfig, home: &Path) -> Router {
    let state = ApiState::new(config, home);

    Router::new()
        .route("/api/health", get(health))
        .route("/api/stats", get(stats))
        .route("/api/pause", post(pause_capture))
        .route("/api/resume", post(resume_capture))
        .route("/api/captures", get(list_captures))
        .route("/api/captures/{id}", get(get_capture))
        .route("/api/screenshots/{*path}", get(get_screenshot))
        .route("/api/apps", get(list_apps))
        .route("/api/insights/current", get(get_current_insight))
        .route("/api/insights/hourly", get(list_hourly_insights))
        .route("/api/insights/daily", get(get_daily_insights))
        .route("/api/insights/projects", get(list_project_time_allocations))
        .route("/api/insights/topics", get(list_topic_frequencies))
        .route("/api/search", get(search_extractions))
        .route("/api/search/semantic", get(handle_semantic_search))
        .route("/api/analyze", post(analyze_time_range))
        .route("/", get(static_assets::root_handler))
        .route("/{*path}", get(static_assets::static_handler))
        .with_state(state)
}

async fn pause_capture() -> Json<CapturePausedResponse> {
    CAPTURE_PAUSED.store(true, Ordering::SeqCst);
    Json(CapturePausedResponse { paused: true })
}

async fn resume_capture() -> Json<CapturePausedResponse> {
    CAPTURE_PAUSED.store(false, Ordering::SeqCst);
    Json(CapturePausedResponse { paused: false })
}

async fn health(State(state): State<ApiState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        uptime_secs: state.uptime_secs(),
    })
}

async fn stats(State(state): State<ApiState>) -> Result<Json<StatsResponse>, ApiError> {
    let storage_bytes = metrics::directory_size(&state.storage_root).map_err(ApiError::internal)?;
    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Ok(Json(StatsResponse {
            capture_count: 0,
            captures_today: 0,
            storage_bytes,
            uptime_secs: state.uptime_secs(),
        }));
    };
    let capture_count = db.count_captures().map_err(ApiError::internal)?;
    let captures_today =
        metrics::count_captures_today(&db, Utc::now()).map_err(ApiError::internal)?;

    Ok(Json(StatsResponse {
        capture_count,
        captures_today,
        storage_bytes,
        uptime_secs: state.uptime_secs(),
    }))
}

async fn list_captures(
    State(state): State<ApiState>,
    raw_query: RawQuery,
) -> Result<Json<CaptureListResponse>, ApiError> {
    let params = parse_capture_list_params(raw_query.0.as_deref())?;
    let from = parse_optional_timestamp("from", params.from.as_deref())?;
    let to = parse_optional_timestamp("to", params.to.as_deref())?;
    if let (Some(from), Some(to)) = (from, to) {
        if from > to {
            return Err(ApiError::bad_request(
                "`from` must be less than or equal to `to`",
            ));
        }
    }

    let offset = params.offset.unwrap_or(0);
    i64::try_from(offset).map_err(|_| ApiError::bad_request("`offset` exceeds supported range"))?;

    let query = CaptureQuery {
        from,
        to,
        app_name: params
            .app
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        limit: params
            .limit
            .unwrap_or(DEFAULT_CAPTURE_LIMIT)
            .min(MAX_CAPTURE_LIMIT),
        offset,
    };
    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Ok(Json(CaptureListResponse {
            captures: Vec::new(),
            limit: query.limit,
            offset: query.offset,
        }));
    };
    let captures = db
        .list_captures(&query)
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|capture| api_capture_from_model(&state, capture))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(CaptureListResponse {
        captures,
        limit: query.limit,
        offset: query.offset,
    }))
}

async fn get_capture(
    State(state): State<ApiState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<ApiCaptureDetail>, ApiError> {
    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Err(ApiError::not_found(format!("capture {id} was not found")));
    };
    let detail = db
        .get_capture_detail(id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found(format!("capture {id} was not found")))?;

    Ok(Json(api_capture_detail_from_model(&state, detail)?))
}

async fn get_screenshot(
    State(state): State<ApiState>,
    AxumPath(path): AxumPath<String>,
) -> Result<Response, ApiError> {
    let relative_path = sanitize_screenshot_path(&path)?;
    let screenshots_root = state.screenshots_root.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        read_screenshot_file(&screenshots_root, &relative_path)
    })
    .await
    .map_err(|error| ApiError::internal(error.into()))?
    .map_err(|error| map_screenshot_io_error(&path, error))?;

    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    Ok(response)
}

async fn list_apps(State(state): State<ApiState>) -> Result<Json<AppsResponse>, ApiError> {
    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Ok(Json(AppsResponse { apps: Vec::new() }));
    };
    let apps = db.list_app_capture_counts().map_err(ApiError::internal)?;
    Ok(Json(AppsResponse { apps }))
}

async fn get_current_insight(State(state): State<ApiState>) -> Result<Json<Insight>, ApiError> {
    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Err(ApiError::not_found("no rolling insight is available"));
    };
    let insight = db
        .get_latest_insight_by_type(InsightType::Rolling)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("no rolling insight is available"))?;

    Ok(Json(insight))
}

async fn list_hourly_insights(
    State(state): State<ApiState>,
    raw_query: RawQuery,
) -> Result<Json<InsightListResponse>, ApiError> {
    let params: DateParams = parse_query_params(
        raw_query.0.as_deref(),
        "invalid hourly insight query parameters",
    )?;
    let date = parse_required_date("date", params.date.as_deref())?;
    let window_start = date
        .and_hms_opt(0, 0, 0)
        .expect("midnight should be representable")
        .and_utc();
    let window_end = date
        .succ_opt()
        .expect("successor date should be representable")
        .and_hms_opt(0, 0, 0)
        .expect("midnight should be representable")
        .and_utc();

    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Ok(Json(InsightListResponse {
            insights: Vec::new(),
        }));
    };
    let insights = db
        .list_hourly_insights_in_range(window_start, window_end)
        .map_err(ApiError::internal)?;

    Ok(Json(InsightListResponse { insights }))
}

async fn get_daily_insights(
    State(state): State<ApiState>,
    raw_query: RawQuery,
) -> Result<Response, ApiError> {
    let params: DateRangeParams = parse_query_params(
        raw_query.0.as_deref(),
        "invalid daily insight query parameters",
    )?;
    let date = parse_optional_date("date", params.date.as_deref())?;
    let from = parse_optional_date("from", params.from.as_deref())?;
    let to = parse_optional_date("to", params.to.as_deref())?;

    match (date, from, to) {
        (Some(date), None, None) => {
            let Some(db) = state.open_db().map_err(ApiError::internal)? else {
                return Err(ApiError::not_found(format!(
                    "no daily insight exists for {date}"
                )));
            };
            let insight = db
                .get_latest_daily_insight_for_date(date)
                .map_err(ApiError::internal)?
                .ok_or_else(|| {
                    ApiError::not_found(format!("no daily insight exists for {date}"))
                })?;

            Ok(Json(insight).into_response())
        }
        (None, Some(from), Some(to)) => {
            if from > to {
                return Err(ApiError::bad_request(
                    "`from` must be less than or equal to `to`",
                ));
            }

            let Some(db) = state.open_db().map_err(ApiError::internal)? else {
                return Ok(Json(InsightListResponse {
                    insights: Vec::new(),
                })
                .into_response());
            };
            let insights = db
                .list_daily_insights_in_date_range(from, to)
                .map_err(ApiError::internal)?;

            Ok(Json(InsightListResponse { insights }).into_response())
        }
        (None, None, None) => Err(ApiError::bad_request(
            "either `date` or both `from` and `to` query parameters are required",
        )),
        _ => Err(ApiError::bad_request(
            "use either `date` or `from`/`to`, not both",
        )),
    }
}

async fn list_project_time_allocations(
    State(state): State<ApiState>,
    raw_query: RawQuery,
) -> Result<Json<ProjectTimeAllocationResponse>, ApiError> {
    let params: TimeRangeParams = parse_query_params(
        raw_query.0.as_deref(),
        "invalid project insight query parameters",
    )?;
    let from = parse_optional_timestamp("from", params.from.as_deref())?;
    let to = parse_optional_timestamp("to", params.to.as_deref())?;
    validate_timestamp_range(from.as_ref(), to.as_ref())?;

    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Ok(Json(ProjectTimeAllocationResponse {
            projects: Vec::new(),
        }));
    };
    let projects = db
        .list_project_time_allocations(from, to)
        .map_err(ApiError::internal)?;

    Ok(Json(ProjectTimeAllocationResponse { projects }))
}

async fn list_topic_frequencies(
    State(state): State<ApiState>,
    raw_query: RawQuery,
) -> Result<Json<TopicFrequencyResponse>, ApiError> {
    let params: TimeRangeParams = parse_query_params(
        raw_query.0.as_deref(),
        "invalid topic insight query parameters",
    )?;
    let from = parse_optional_timestamp("from", params.from.as_deref())?;
    let to = parse_optional_timestamp("to", params.to.as_deref())?;
    validate_timestamp_range(from.as_ref(), to.as_ref())?;

    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Ok(Json(TopicFrequencyResponse { topics: Vec::new() }));
    };
    let topics = db
        .list_topic_frequencies(from, to)
        .map_err(ApiError::internal)?;

    Ok(Json(TopicFrequencyResponse { topics }))
}

async fn search_extractions(
    State(state): State<ApiState>,
    raw_query: RawQuery,
) -> Result<Json<SearchResponse>, ApiError> {
    let params: SearchParams =
        parse_query_params(raw_query.0.as_deref(), "invalid search query parameters")?;
    let from = parse_optional_timestamp("from", params.from.as_deref())?;
    let to = parse_optional_timestamp("to", params.to.as_deref())?;
    let activity_type = parse_optional_activity_type(
        "activity_type",
        trim_to_option(params.activity_type).as_deref(),
    )?;
    validate_timestamp_range(from.as_ref(), to.as_ref())?;

    let query = trim_to_option(params.q)
        .ok_or_else(|| ApiError::bad_request("query parameter `q` is required"))?;
    let search_query = SearchQuery {
        query,
        app_name: trim_to_option(params.app),
        project: trim_to_option(params.project),
        activity_type,
        from,
        to,
        limit: params
            .limit
            .unwrap_or(DEFAULT_SEARCH_LIMIT)
            .min(MAX_SEARCH_LIMIT),
    };

    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Ok(Json(SearchResponse {
            results: Vec::new(),
            limit: search_query.limit,
        }));
    };
    let results = db
        .search_history_filtered(&search_query)
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|hit| api_search_hit_from_model(&state, hit))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(SearchResponse {
        results,
        limit: search_query.limit,
    }))
}
async fn handle_semantic_search(
    State(state): State<ApiState>,
    Query(params): Query<SemanticSearchParams>,
) -> Result<Json<SemanticSearchResponse>, ApiError> {
    let from = parse_optional_timestamp("from", params.from.as_deref())?;
    let to = parse_optional_timestamp("to", params.to.as_deref())?;
    validate_timestamp_range(from.as_ref(), to.as_ref())?;

    let query = trim_to_option(params.q)
        .ok_or_else(|| ApiError::bad_request("query parameter `q` is required"))?;
    let limit = params
        .limit
        .unwrap_or(DEFAULT_SEMANTIC_SEARCH_LIMIT)
        .min(MAX_SEMANTIC_SEARCH_LIMIT);

    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        return Ok(Json(SemanticSearchResponse {
            answer: "No captures are available yet.".into(),
            references: Vec::new(),
            cost_cents: None,
            tokens_used: None,
        }));
    };

    let candidates = synthesis::semantic_search_candidates(&db, &query, from, to, limit)
        .map_err(ApiError::internal)?;
    drop(db);
    let result = synthesis::semantic_search(&state.config, &query, candidates)
        .await
        .map_err(ApiError::internal)?;
    let references = result
        .references
        .into_iter()
        .map(|hit| semantic_search_reference_from_hit(&state, hit))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(SemanticSearchResponse {
        answer: result.answer,
        references,
        cost_cents: result.cost_cents,
        tokens_used: result.tokens_used,
    }))
}

async fn analyze_time_range(
    State(state): State<ApiState>,
    payload: Bytes,
) -> Result<Json<AnalyzeResponse>, ApiError> {
    let request: AnalyzeRequest = serde_json::from_slice(&payload).map_err(|error| {
        ApiError::bad_request(format!("invalid analysis request body: {error}"))
    })?;
    let window_start = parse_body_timestamp("from", &request.from)?;
    let window_end = parse_body_timestamp("to", &request.to)?;
    validate_timestamp_range(Some(&window_start), Some(&window_end))?;
    let analysis_query = normalize_analysis_query(request.query, request.prompt)?;

    let Some(db) = state.open_db().map_err(ApiError::internal)? else {
        if let Some(query) = analysis_query {
            return Ok(Json(AnalyzeResponse {
                window_start,
                window_end,
                capture_count: 0,
                analysis_query: Some(query),
                analysis: AnalyzePayload::QuestionAnswer {
                    analyzed_capture_count: 0,
                    answer: NO_RELEVANT_ACTIVITY_IN_RANGE.into(),
                    references: Vec::new(),
                    tokens_used: None,
                    cost_cents: None,
                },
            }));
        }

        return Err(ApiError::not_found(NO_PROCESSED_ACTIVITY_IN_RANGE));
    };

    let capture_count = db
        .count_captures_in_window(window_start, window_end)
        .map_err(ApiError::internal)?;

    let result = match analysis_query {
        Some(query) => {
            let candidates = synthesis::semantic_search_candidates(
                &db,
                &query,
                Some(window_start),
                Some(window_end),
                DEFAULT_SEMANTIC_SEARCH_LIMIT,
            )
            .map_err(ApiError::internal)?;
            drop(db);

            synthesis::answer_time_range_query(
                &state.config,
                window_start,
                window_end,
                capture_count,
                query,
                candidates,
            )
            .await
            .map_err(map_analysis_error)?
        }
        None => {
            let batches = db
                .list_extraction_batch_details_in_range(window_start, window_end)
                .map_err(ApiError::internal)?;
            drop(db);
            if batches.is_empty() {
                return Err(ApiError::not_found(NO_PROCESSED_ACTIVITY_IN_RANGE));
            }

            synthesis::summarize_time_range(
                &state.config,
                window_start,
                window_end,
                capture_count,
                batches,
            )
            .await
            .map_err(map_analysis_error)?
        }
    };

    Ok(Json(api_analyze_response_from_result(&state, result)?))
}

fn parse_body_timestamp(label: &str, raw: &str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| {
            ApiError::bad_request(format!(
                "request body field `{label}` must be a valid ISO 8601 timestamp"
            ))
        })
}

fn normalize_analysis_query(
    query: Option<String>,
    prompt: Option<String>,
) -> Result<Option<String>, ApiError> {
    let query = trim_to_option(query);
    let prompt = trim_to_option(prompt);

    match (query, prompt) {
        (Some(query), Some(prompt)) if query != prompt => Err(ApiError::bad_request(
            "provide either `query` or `prompt`, not both",
        )),
        (Some(query), Some(_)) | (Some(query), None) => Ok(Some(query)),
        (None, Some(prompt)) => Ok(Some(prompt)),
        (None, None) => Ok(None),
    }
}

fn map_analysis_error(error: anyhow::Error) -> ApiError {
    if let Some(provider_error) = find_provider_error(&error) {
        return ApiError::service_unavailable(format!(
            "analysis provider unavailable: {provider_error}"
        ));
    }

    if error.to_string().contains("synthesis pipeline is disabled") {
        return ApiError::service_unavailable(
            "analysis provider unavailable: synthesis pipeline is disabled",
        );
    }

    ApiError::internal(error)
}

fn find_provider_error(error: &anyhow::Error) -> Option<&ProviderError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<ProviderError>())
}

fn api_analyze_response_from_result(
    state: &ApiState,
    result: synthesis::TimeRangeAnalysisResult,
) -> Result<AnalyzeResponse, ApiError> {
    let synthesis::TimeRangeAnalysisResult {
        window_start,
        window_end,
        capture_count,
        analysis_query,
        analysis,
    } = result;

    let analysis = match analysis {
        synthesis::TimeRangeAnalysis::RollingContext {
            batch_count,
            analyzed_capture_count,
            insight,
            tokens_used,
            cost_cents,
        } => AnalyzePayload::RollingContext {
            batch_count,
            analyzed_capture_count,
            insight,
            tokens_used,
            cost_cents,
        },
        synthesis::TimeRangeAnalysis::QuestionAnswer {
            analyzed_capture_count,
            result,
        } => AnalyzePayload::QuestionAnswer {
            analyzed_capture_count,
            answer: result.answer,
            references: result
                .references
                .into_iter()
                .map(|hit| semantic_search_reference_from_hit(state, hit))
                .collect::<Result<Vec<_>, _>>()?,
            tokens_used: result.tokens_used,
            cost_cents: result.cost_cents,
        },
    };

    Ok(AnalyzeResponse {
        window_start,
        window_end,
        capture_count,
        analysis_query,
        analysis,
    })
}

fn parse_capture_list_params(raw: Option<&str>) -> Result<CaptureListParams, ApiError> {
    parse_query_params(raw, "invalid capture query parameters")
}

fn parse_query_params<T>(raw: Option<&str>, invalid_message: &'static str) -> Result<T, ApiError>
where
    T: DeserializeOwned + Default,
{
    match raw {
        None | Some("") => Ok(T::default()),
        Some(raw) => {
            serde_urlencoded::from_str(raw).map_err(|_| ApiError::bad_request(invalid_message))
        }
    }
}

fn parse_optional_timestamp(
    label: &str,
    raw: Option<&str>,
) -> Result<Option<DateTime<Utc>>, ApiError> {
    raw.map(|value| {
        DateTime::parse_from_rfc3339(value)
            .map(|timestamp| timestamp.with_timezone(&Utc))
            .map_err(|_| {
                ApiError::bad_request(format!(
                    "query parameter `{label}` must be a valid ISO 8601 timestamp"
                ))
            })
    })
    .transpose()
}

fn parse_optional_activity_type(
    label: &str,
    raw: Option<&str>,
) -> Result<Option<ActivityType>, ApiError> {
    raw.map(|value| {
        value
            .trim()
            .to_ascii_lowercase()
            .parse::<ActivityType>()
            .map_err(|_| {
                ApiError::bad_request(format!(
                    "query parameter `{label}` must be one of: coding, browsing, communication, reading, writing, design, terminal, meeting, media, other"
                ))
            })
    })
    .transpose()
}

fn parse_optional_date(label: &str, raw: Option<&str>) -> Result<Option<NaiveDate>, ApiError> {
    raw.map(|value| {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
            ApiError::bad_request(format!(
                "query parameter `{label}` must be a valid YYYY-MM-DD date"
            ))
        })
    })
    .transpose()
}

fn parse_required_date(label: &str, raw: Option<&str>) -> Result<NaiveDate, ApiError> {
    parse_optional_date(label, raw)?
        .ok_or_else(|| ApiError::bad_request(format!("query parameter `{label}` is required")))
}

fn validate_timestamp_range(
    from: Option<&DateTime<Utc>>,
    to: Option<&DateTime<Utc>>,
) -> Result<(), ApiError> {
    if let (Some(from), Some(to)) = (from, to) {
        if from > to {
            return Err(ApiError::bad_request(
                "`from` must be less than or equal to `to`",
            ));
        }
    }

    Ok(())
}

fn trim_to_option(raw: Option<String>) -> Option<String> {
    raw.and_then(|value| {
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_owned())
        }
    })
}

fn sanitize_screenshot_path(raw: &str) -> Result<PathBuf, ApiError> {
    sanitize_relative_screenshot_path(raw).ok_or_else(|| {
        if raw.trim().is_empty() {
            ApiError::bad_request("screenshot path cannot be empty")
        } else {
            ApiError::bad_request("invalid screenshot path")
        }
    })
}

fn api_capture_from_model(state: &ApiState, capture: Capture) -> Result<ApiCapture, ApiError> {
    Ok(ApiCapture {
        id: capture.id,
        timestamp: capture.timestamp,
        app_name: capture.app_name,
        window_title: capture.window_title,
        bundle_id: capture.bundle_id,
        display_id: capture.display_id,
        screenshot_url: screenshot_url_from_path(state, &capture.screenshot_path),
        extraction_status: capture.extraction_status,
        extraction_id: capture.extraction_id,
        created_at: capture.created_at,
    })
}

fn api_capture_detail_from_model(
    state: &ApiState,
    detail: CaptureDetail,
) -> Result<ApiCaptureDetail, ApiError> {
    Ok(ApiCaptureDetail {
        capture: api_capture_from_model(state, detail.capture)?,
        extraction: detail.extraction,
    })
}

fn api_search_hit_from_model(state: &ApiState, hit: SearchHit) -> Result<ApiSearchHit, ApiError> {
    Ok(match hit {
        SearchHit::Extraction {
            timestamp,
            rank,
            capture,
            extraction,
            batch_narrative,
        } => ApiSearchHit::Extraction {
            timestamp,
            rank,
            capture: api_capture_from_model(state, capture)?,
            extraction,
            batch_narrative,
        },
        SearchHit::Insight {
            timestamp,
            rank,
            primary_project,
            primary_activity_type,
            insight,
        } => ApiSearchHit::Insight {
            timestamp,
            rank,
            primary_project,
            primary_activity_type,
            insight,
        },
    })
}

fn semantic_search_reference_from_hit(
    state: &ApiState,
    hit: ExtractionSearchHit,
) -> Result<SemanticSearchReference, ApiError> {
    Ok(SemanticSearchReference {
        capture: api_capture_from_model(state, hit.capture)?,
        extraction: hit.extraction,
    })
}

fn screenshot_url_from_path(state: &ApiState, screenshot_path: &str) -> Option<String> {
    let relative_path = relative_screenshot_path(&state.screenshots_root, screenshot_path)?;

    Some(format!(
        "/api/screenshots/{}",
        relative_path
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/")
    ))
}

fn map_screenshot_io_error(path: &str, error: std::io::Error) -> ApiError {
    match error.kind() {
        std::io::ErrorKind::NotFound => {
            ApiError::not_found(format!("screenshot `{path}` was not found"))
        }
        _ => match error.raw_os_error() {
            Some(code) if code == libc::ELOOP || code == libc::ENOTDIR => {
                ApiError::not_found(format!("screenshot `{path}` was not found"))
            }
            _ => ApiError::internal(error.into()),
        },
    }
}
