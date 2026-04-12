use axum::{
    body::Body,
    extract::Path,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct EmbeddedUi;

pub async fn root_handler() -> Response {
    serve_asset_path("index.html")
}

pub async fn static_handler(Path(path): Path<String>) -> Response {
    if is_api_path(&path) {
        return StatusCode::NOT_FOUND.into_response();
    }

    serve_asset_path(&path)
}

fn is_api_path(path: &str) -> bool {
    path == "api" || path.starts_with("api/")
}

fn serve_asset_path(path: &str) -> Response {
    embedded_asset(normalized_asset_path(path))
        .or_else(|| embedded_asset("index.html"))
        .unwrap_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "embedded frontend assets are missing; run `npm run build` inside `web/` before `cargo build`",
            )
                .into_response()
        })
}

fn normalized_asset_path(path: &str) -> &str {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        "index.html"
    } else {
        path
    }
}

fn embedded_asset(path: &str) -> Option<Response> {
    let asset = EmbeddedUi::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();

    let mut response = Response::new(Body::from(asset.data.into_owned()));
    if let Ok(content_type) = HeaderValue::from_str(mime.as_ref()) {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, content_type);
    }

    Some(response)
}
