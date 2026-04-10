//! Library tests

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::storage::Storage;

    #[test]
    fn test_storage_initialization() {
        // Create storage
        let storage = Storage::new().expect("Failed to create storage");

        // Initialize should succeed
        let result = storage.initialize();
        assert!(result.is_ok(), "Storage initialization should succeed");
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();

        assert_eq!(config.capture.idle_interval_secs, 300);
        assert_eq!(config.capture.event_settle_ms, 500);
        assert_eq!(config.capture.jpeg_quality, 75);
        assert!(!config.capture.excluded_apps.is_empty());

        assert_eq!(config.extraction.interval_secs, 600);
        assert_eq!(config.extraction.provider, "openrouter");

        assert_eq!(config.synthesis.rolling_interval_secs, 1800);
        assert_eq!(config.server.port, 7878);
    }

    #[test]
    fn test_config_load_missing_file() {
        // Loading a non-existent config should return defaults
        let result = Config::load();
        assert!(
            result.is_ok(),
            "Config::load should succeed even without file"
        );
    }
}
