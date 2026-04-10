//! Main daemon loop, orchestrates all layers

use crate::api::serve;
use crate::config::Config;
use crate::storage::Storage;

/// The main Screencap daemon
pub struct Daemon {
    config: Config,
    storage: Storage,
}

impl Daemon {
    /// Create a new daemon instance
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let storage = Storage::new()?;
        Ok(Self { config, storage })
    }

    /// Run the daemon in the foreground
    pub async fn run(&self) -> anyhow::Result<()> {
        // Initialize storage
        self.storage.initialize()?;

        // Start HTTP server
        serve(self.config.server.port).await?;

        Ok(())
    }
}
