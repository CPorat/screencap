use std::net::{Ipv4Addr, SocketAddr};

use anyhow::{Context, Result};
use tokio::{net::TcpListener, sync::watch};

use crate::config::AppConfig;

use super::routes;

pub async fn bind(config: &AppConfig) -> Result<TcpListener> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, config.server.port));
    TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind api server to {address}"))
}

pub async fn serve(listener: TcpListener, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    axum::serve(listener, routes::router())
        .with_graceful_shutdown(async move {
            loop {
                if shutdown.changed().await.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        })
        .await
        .context("api server terminated unexpectedly")
}

pub async fn run(config: &AppConfig, shutdown: watch::Receiver<bool>) -> Result<()> {
    let listener = bind(config).await?;
    serve(listener, shutdown).await
}