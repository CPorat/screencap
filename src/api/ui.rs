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

    embedded_asset(normalized_asset_path(uri.path()))
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
