//! API route definitions

use axum::{
    routing::{get, post},
    Router,
};

/// Build the API router
pub fn build_router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/stats", get(stats))
        .route("/api/captures", get(captures_list))
        .route("/api/captures/:id", get(captures_get))
        .route("/api/screenshots/:path", get(screenshots_get))
        .route("/api/search", get(search))
        .route("/api/search/semantic", get(search_semantic))
        .route("/api/insights/current", get(insights_current))
        .route("/api/insights/hourly", get(insights_hourly))
        .route("/api/insights/daily", get(insights_daily))
        .route("/api/insights/projects", get(insights_projects))
        .route("/api/insights/topics", get(insights_topics))
        .route("/api/apps", get(apps))
        .route("/api/analyze", post(analyze))
}

// Placeholder handlers

async fn health() -> &'static str {
    "OK"
}

async fn stats() -> &'static str {
    "{}"
}

async fn captures_list() -> &'static str {
    "[]"
}

async fn captures_get() -> &'static str {
    "{}"
}

async fn screenshots_get() -> &'static str {
    ""
}

async fn search() -> &'static str {
    "[]"
}

async fn search_semantic() -> &'static str {
    "[]"
}

async fn insights_current() -> &'static str {
    "{}"
}

async fn insights_hourly() -> &'static str {
    "[]"
}

async fn insights_daily() -> &'static str {
    "{}"
}

async fn insights_projects() -> &'static str {
    "[]"
}

async fn insights_topics() -> &'static str {
    "[]"
}

async fn apps() -> &'static str {
    "[]"
}

async fn analyze() -> &'static str {
    "{}"
}
