use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
};

use anyhow::{Context, Result};

pub struct IntegrationTestLock {
    file: File,
}

impl IntegrationTestLock {
    pub fn acquire() -> Result<Self> {
        let path = std::env::temp_dir().join("screencap-integration-tests.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| {
                format!("failed to open integration test lock at {}", path.display())
            })?;

        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if result != 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!(
                    "failed to acquire integration test lock at {}",
                    path.display()
                )
            });
        }

        Ok(Self { file })
    }
}

impl Drop for IntegrationTestLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}
