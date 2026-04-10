use std::{
    path::{Component, Path, PathBuf},
    time::Instant,
};

use anyhow::Context;
use axum::{
    body::Body,
    extract::{Path as AxumPath, RawQuery, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::error;

use crate::{
    config::AppConfig,
    storage::{
        db::StorageDb,
        metrics,
        models::{
            AppCaptureCount, Capture, CaptureDetail, CaptureQuery, Extraction, ExtractionSearchHit,
            ExtractionSearchQuery, ExtractionStatus, Insight, InsightType, ProjectTimeAllocation,
            TopicFrequency,
        },
    },
};

const DEFAULT_CAPTURE_LIMIT: usize = 100;
const MAX_CAPTURE_LIMIT: usize = 500;
const DEFAULT_SEARCH_LIMIT: usize = 50;
const MAX_SEARCH_LIMIT: usize = 200;

#[derive(Clone)]
struct ApiState {
    db_path: PathBuf,
    storage_root: PathBuf,
    screenshots_root: PathBuf,
    started_at: Instant,
}

impl ApiState {
    fn new(config: &AppConfig, home: &Path) -> Self {
        Self {
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
struct ApiSearchHit {
    capture: ApiCapture,
    extraction: Extraction,
    batch_narrative: Option<String>,
    rank: f64,
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    results: Vec<ApiSearchHit>,
    limit: usize,
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
        .with_state(state)
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
    validate_timestamp_range(from.as_ref(), to.as_ref())?;

    let query = trim_to_option(params.q)
        .ok_or_else(|| ApiError::bad_request("query parameter `q` is required"))?;
    let search_query = ExtractionSearchQuery {
        query,
        app_name: trim_to_option(params.app),
        project: trim_to_option(params.project),
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
        .search_extractions_filtered(&search_query)
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|hit| api_search_hit_from_model(&state, hit))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(SearchResponse {
        results,
        limit: search_query.limit,
    }))
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
    if raw.trim().is_empty() {
        return Err(ApiError::bad_request("screenshot path cannot be empty"));
    }

    let mut sanitized = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            _ => return Err(ApiError::bad_request("invalid screenshot path")),
        }
    }

    if sanitized.as_os_str().is_empty() {
        return Err(ApiError::bad_request("screenshot path cannot be empty"));
    }

    Ok(sanitized)
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

fn api_search_hit_from_model(
    state: &ApiState,
    hit: ExtractionSearchHit,
) -> Result<ApiSearchHit, ApiError> {
    Ok(ApiSearchHit {
        capture: api_capture_from_model(state, hit.capture)?,
        extraction: hit.extraction,
        batch_narrative: hit.batch_narrative,
        rank: hit.rank,
    })
}

fn screenshot_url_from_path(state: &ApiState, screenshot_path: &str) -> Option<String> {
    let path = Path::new(screenshot_path);
    let relative_path = if path.is_absolute() {
        path.strip_prefix(&state.screenshots_root)
            .ok()?
            .to_path_buf()
    } else {
        path.to_path_buf()
    };

    let mut sanitized = PathBuf::new();
    for component in relative_path.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            _ => return None,
        }
    }
    if sanitized.as_os_str().is_empty() {
        return None;
    }

    Some(format!(
        "/api/screenshots/{}",
        sanitized
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/")
    ))
}

fn read_screenshot_file(root: &Path, relative_path: &Path) -> std::io::Result<Vec<u8>> {
    use std::{
        ffi::CString,
        fs::File,
        io::{Error, ErrorKind, Read},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };

    fn open_path(path: &Path, flags: i32) -> std::io::Result<File> {
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "path contains NUL byte"))?;
        let fd = unsafe { libc::open(path.as_ptr(), flags) };
        if fd == -1 {
            return Err(Error::last_os_error());
        }

        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn open_at(directory: &File, name: &std::ffi::OsStr, flags: i32) -> std::io::Result<File> {
        let name = CString::new(name.as_bytes())
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "path contains NUL byte"))?;
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd == -1 {
            return Err(Error::last_os_error());
        }

        Ok(unsafe { File::from_raw_fd(fd) })
    }

    let mut current = open_path(root, libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY)?;
    let mut components = relative_path.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid screenshot path",
            ));
        };

        let is_last = components.peek().is_none();
        let next = if is_last {
            open_at(
                &current,
                name,
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            )?
        } else {
            open_at(
                &current,
                name,
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            )?
        };

        if is_last {
            if !next.metadata()?.is_file() {
                return Err(Error::from(ErrorKind::NotFound));
            }

            let mut bytes = Vec::new();
            let mut next = next;
            next.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }

        current = next;
    }

    Err(Error::from(ErrorKind::NotFound))
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
