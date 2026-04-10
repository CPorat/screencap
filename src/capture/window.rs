//! Active window metadata via NSWorkspace Swift bridge

/// Active window metadata
#[derive(Debug, Clone)]
pub struct WindowMetadata {
    pub app_name: String,
    pub window_title: String,
    pub bundle_id: String,
}

/// Window metadata capture interface
pub struct WindowMetadataCapture {
    // Placeholder for future Swift bridge integration
}

impl WindowMetadataCapture {
    /// Create a new window metadata capture instance
    pub fn new() -> Self {
        Self {}
    }

    /// Get metadata for the currently active window
    pub fn get_active_window(&self) -> anyhow::Result<WindowMetadata> {
        // TODO: Call into Swift bridge via FFI
        Ok(WindowMetadata {
            app_name: "Unknown".to_string(),
            window_title: "Unknown".to_string(),
            bundle_id: "unknown".to_string(),
        })
    }
}

impl Default for WindowMetadataCapture {
    fn default() -> Self {
        Self::new()
    }
}
