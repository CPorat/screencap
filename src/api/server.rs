use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};

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

pub async fn serve(
    listener: TcpListener,
    config: AppConfig,
    home: PathBuf,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let app = routes::router(&config, &home);

    axum::serve(listener, app)
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

pub async fn run(config: AppConfig, home: PathBuf, shutdown: watch::Receiver<bool>) -> Result<()> {
    let listener = bind(&config).await?;
    serve(listener, config, home, shutdown).await
}
