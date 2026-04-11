use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "web/dist/"]
struct EmbeddedUi;

pub async fn serve(uri: Uri) -> Response {
    if uri.path().starts_with("/api") {
        return StatusCode::NOT_FOUND.into_response();
    }

    let requested_path = uri.path().trim_start_matches('/');
    let asset_path = if requested_path.is_empty() {
        "index.html"
    } else {
        requested_path
    };

    embedded_asset(asset_path)
        .or_else(|| embedded_asset("index.html"))
        .unwrap_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "embedded frontend assets are missing; run `npm run build` inside `web/` before `cargo build`",
            )
                .into_response()
        })
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
