//! HTTP server implementation

use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use super::routes::build_router;

/// Start the HTTP server on the given port
pub async fn serve(port: u16) -> anyhow::Result<()> {
    let app = build_router().layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("HTTP server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
