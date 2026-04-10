//! Event listener for CGEventTap-driven capture triggers

/// Event listener for meaningful activity events
pub struct EventListener {
    // Placeholder for future CGEventTap integration
}

impl EventListener {
    /// Create a new event listener
    pub fn new() -> Self {
        Self {}
    }

    /// Start listening for events
    pub fn start(&self) -> anyhow::Result<()> {
        // TODO: Set up CGEventTap
        Ok(())
    }

    /// Stop listening for events
    pub fn stop(&self) -> anyhow::Result<()> {
        // TODO: Teardown CGEventTap
        Ok(())
    }
}

impl Default for EventListener {
    fn default() -> Self {
        Self::new()
    }
}
