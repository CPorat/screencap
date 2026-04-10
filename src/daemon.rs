use std::{
    env, fs,
    io::ErrorKind,
    path::PathBuf,
    time::Duration,
};

use anyhow::{anyhow, ensure, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio::time;
use tracing::{debug, error, info};

use crate::{
    capture::{screenshot, window::{self, WindowInfo}},
    config::AppConfig,
    storage::{
        db::StorageDb,
        models::{Capture, NewCapture},
    },
};

pub async fn run_foreground(config: &AppConfig) -> Result<()> {
    let home = runtime_home_dir()?;
    let mut capture_loop = CaptureLoop::open(config.clone(), home)?;
    let mut interval = time::interval(Duration::from_secs(config.capture.idle_interval_secs));

    info!(
        idle_interval_secs = config.capture.idle_interval_secs,
        jpeg_quality = config.capture.jpeg_quality,
        "capture loop started"
    );

    loop {
        interval.tick().await;

        if let Err(err) = capture_loop.capture_once() {
            error!(error = %err, "capture cycle failed");
        }
    }
}

fn runtime_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME environment variable is not set"))
}

#[derive(Debug)]
struct CaptureLoop {
    config: AppConfig,
    home: PathBuf,
    db: StorageDb,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureCycleOutcome {
    Captured {
        capture_count: usize,
        app_name: String,
        window_title: String,
    },
    SkippedExcluded {
        app_name: String,
        window_title: String,
    },
    SkippedDuplicate {
        app_name: String,
        window_title: String,
        last_capture_at: DateTime<Utc>,
    },
}

impl CaptureLoop {
    fn open(config: AppConfig, home: PathBuf) -> Result<Self> {
        ensure!(
            config.capture.idle_interval_secs > 0,
            "capture idle_interval_secs must be greater than 0"
        );

        let db_path = config.storage_root(&home).join("screencap.db");
        let db = StorageDb::open_at_path(&db_path)
            .with_context(|| format!("failed to open capture database at {}", db_path.display()))?;

        Ok(Self { config, home, db })
    }

    fn capture_once(&mut self) -> Result<CaptureCycleOutcome> {
        self.capture_once_at(Utc::now())
    }

    fn capture_once_at(&mut self, timestamp: DateTime<Utc>) -> Result<CaptureCycleOutcome> {
        let active_window = window::get_active_window().context("failed to fetch active window")?;

        if self.is_excluded(&active_window) {
            info!(
                app_name = %active_window.app_name,
                window_title = %active_window.window_title,
                "skipping capture for excluded window context"
            );
            return Ok(CaptureCycleOutcome::SkippedExcluded {
                app_name: active_window.app_name,
                window_title: active_window.window_title,
            });
        }

        if let Some(last_capture) = self.duplicate_capture(&active_window, timestamp)? {
            debug!(
                app_name = %active_window.app_name,
                window_title = %active_window.window_title,
                last_capture_at = %last_capture.timestamp,
                "skipping duplicate capture within idle interval"
            );
            return Ok(CaptureCycleOutcome::SkippedDuplicate {
                app_name: active_window.app_name,
                window_title: active_window.window_title,
                last_capture_at: last_capture.timestamp,
            });
        }

        let display_ids = screenshot::display_ids().context("failed to enumerate displays")?;
        ensure!(!display_ids.is_empty(), "capture bridge returned no displays");

        let mut screenshot_paths = Vec::with_capacity(display_ids.len());
        let mut captures = Vec::with_capacity(display_ids.len());
        for display_id in display_ids {
            let screenshot_path = self.screenshot_path(timestamp, display_id);
            if let Err(err) = screenshot::capture_screenshot(
                display_id,
                &screenshot_path,
                self.config.capture.jpeg_quality,
            ) {
                cleanup_files(&screenshot_paths);
                return Err(err)
                    .with_context(|| format!("failed to capture screenshot for display {display_id}"));
            }

            screenshot_paths.push(screenshot_path.clone());
            captures.push(NewCapture {
                timestamp,
                app_name: Some(active_window.app_name.clone()),
                window_title: Some(active_window.window_title.clone()),
                bundle_id: Some(active_window.bundle_id.clone()),
                display_id: Some(i64::from(display_id)),
                screenshot_path: screenshot_path.to_string_lossy().into_owned(),
            });
        }

        if let Err(err) = self.db.insert_captures(&captures) {
            cleanup_files(&screenshot_paths);
            return Err(err).context("failed to persist captured screenshots");
        }

        info!(
            capture_count = screenshot_paths.len(),
            app_name = %active_window.app_name,
            window_title = %active_window.window_title,
            "captured screen state"
        );

        Ok(CaptureCycleOutcome::Captured {
            capture_count: screenshot_paths.len(),
            app_name: active_window.app_name,
            window_title: active_window.window_title,
        })
    }

    fn is_excluded(&self, active_window: &WindowInfo) -> bool {
        self.config
            .capture
            .excluded_apps
            .iter()
            .any(|app_name| app_name == &active_window.app_name)
            || self
                .config
                .capture
                .excluded_window_titles
                .iter()
                .any(|title| title == &active_window.window_title)
    }

    fn duplicate_capture(
        &self,
        active_window: &WindowInfo,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<Capture>> {
        let Some(last_capture) = self.db.get_latest_capture()? else {
            return Ok(None);
        };

        if last_capture.app_name.as_deref() != Some(active_window.app_name.as_str())
            || last_capture.window_title.as_deref() != Some(active_window.window_title.as_str())
        {
            return Ok(None);
        }

        let elapsed = timestamp.signed_duration_since(last_capture.timestamp);
        if elapsed < ChronoDuration::zero() || elapsed >= self.idle_interval()? {
            return Ok(None);
        }

        Ok(Some(last_capture))
    }

    fn idle_interval(&self) -> Result<ChronoDuration> {
        let seconds = i64::try_from(self.config.capture.idle_interval_secs)
            .context("capture idle_interval_secs exceeds supported range")?;
        Ok(ChronoDuration::seconds(seconds))
    }

    fn screenshot_path(&self, timestamp: DateTime<Utc>, display_id: u32) -> PathBuf {
        let filename = format!("{}-{display_id}.jpg", timestamp.format("%H%M%S"));
        self.config
            .screenshots_root(&self.home)
            .join(timestamp.format("%Y").to_string())
            .join(timestamp.format("%m").to_string())
            .join(timestamp.format("%d").to_string())
            .join(filename)
    }
}

fn cleanup_files(paths: &[PathBuf]) {
    for path in paths {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => debug!(path = %path.display(), error = %err, "failed to clean up screenshot")
        }
    }
}

#[cfg(all(test, feature = "mock-capture"))]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::Result;
    use chrono::TimeZone;

    use super::{CaptureCycleOutcome, CaptureLoop};
    use crate::config::AppConfig;

    fn temp_home_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("screencap-daemon-tests-{name}-{unique}"))
    }

    fn test_config(home: &Path) -> AppConfig {
        let app_root = home.join(".screencap");
        let mut config =
            AppConfig::load_from_root_and_home(&app_root, home).expect("config should load");
        config.capture.idle_interval_secs = 300;
        config.capture.excluded_apps.clear();
        config.capture.excluded_window_titles.clear();
        config
    }

    #[test]
    fn excluded_app_is_skipped() -> Result<()> {
        let home = temp_home_root("excluded-app");
        let mut config = test_config(&home);
        config.capture.excluded_apps = vec!["MockApp".into()];
        let timestamp = chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();

        let mut capture_loop = CaptureLoop::open(config, home.clone())?;
        let outcome = capture_loop.capture_once_at(timestamp)?;
        assert!(matches!(
            outcome,
            CaptureCycleOutcome::SkippedExcluded { ref app_name, .. } if app_name == "MockApp"
        ));
        assert!(capture_loop.db.get_pending_captures()?.is_empty());

        drop(capture_loop);
        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[test]
    fn dedup_skips_unchanged_context_within_idle_interval() -> Result<()> {
        let home = temp_home_root("dedup");
        let config = test_config(&home);
        let first_time = chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 0, 0).unwrap();
        let second_time = first_time + chrono::Duration::seconds(60);

        let mut capture_loop = CaptureLoop::open(config, home.clone())?;
        let first = capture_loop.capture_once_at(first_time)?;
        assert!(matches!(first, CaptureCycleOutcome::Captured { capture_count: 1, .. }));

        let second = capture_loop.capture_once_at(second_time)?;
        assert!(matches!(
            second,
            CaptureCycleOutcome::SkippedDuplicate {
                ref app_name,
                ref window_title,
                last_capture_at,
            } if app_name == "MockApp"
                && window_title == "Mock Window"
                && last_capture_at == first_time
        ));

        let captures = capture_loop.db.get_pending_captures()?;
        assert_eq!(captures.len(), 1);
        let skipped_path = capture_loop.screenshot_path(second_time, 0);
        assert!(!skipped_path.exists());

        drop(capture_loop);
        fs::remove_dir_all(&home)?;
        Ok(())
    }

    #[test]
    fn persisted_capture_uses_partitioned_screenshot_path() -> Result<()> {
        let home = temp_home_root("path");
        let config = test_config(&home);
        let timestamp = chrono::Utc.with_ymd_and_hms(2026, 4, 10, 14, 5, 6).unwrap();
        let expected_path = home
            .join(".screencap")
            .join("screenshots")
            .join("2026")
            .join("04")
            .join("10")
            .join("140506-0.jpg");

        let mut capture_loop = CaptureLoop::open(config, home.clone())?;
        let outcome = capture_loop.capture_once_at(timestamp)?;
        assert!(matches!(outcome, CaptureCycleOutcome::Captured { capture_count: 1, .. }));

        let captures = capture_loop.db.get_pending_captures()?;
        assert_eq!(captures.len(), 1);
        assert_eq!(PathBuf::from(&captures[0].screenshot_path), expected_path);
        assert_eq!(captures[0].display_id, Some(0));
        assert_eq!(captures[0].app_name.as_deref(), Some("MockApp"));
        assert_eq!(captures[0].window_title.as_deref(), Some("Mock Window"));
        assert!(expected_path.exists());
        assert_eq!(captures[0].extraction_status.as_str(), "pending");

        drop(capture_loop);
        fs::remove_dir_all(&home)?;
        Ok(())
    }
}
