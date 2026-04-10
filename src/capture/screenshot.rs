use std::{fs, path::Path};

use anyhow::{ensure, Context, Result};

pub fn display_ids() -> Result<Vec<u32>> {
    native::display_ids()
}

pub fn get_display_count() -> Result<usize> {
    native::get_display_count()
}

pub fn capture_screenshot(display_id: u32, output_path: impl AsRef<Path>, quality: u8) -> Result<()> {
    validate_quality(quality)?;

    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create screenshot directory at {}", parent.display()))?;
    }

    native::capture_screenshot(display_id, output_path, quality)
}

fn validate_quality(quality: u8) -> Result<()> {
    ensure!(quality <= 100, "jpeg quality must be between 0 and 100, got {quality}");
    Ok(())
}

#[cfg(not(feature = "mock-capture"))]
mod native {
    use std::{
        ffi::CString,
        os::{raw::c_char, unix::ffi::OsStrExt},
        path::Path,
    };

    use anyhow::{bail, Result};

    unsafe extern "C" {
        #[link_name = "capture_screenshot"]
        fn ffi_capture_screenshot(display_id: i64, output_path: *const c_char, quality: u8) -> bool;

        #[link_name = "get_display_count"]
        fn ffi_get_display_count() -> i32;

        #[link_name = "copy_display_ids"]
        fn ffi_copy_display_ids(buffer: *mut u32, max_count: i32) -> i32;
    }

    pub(super) fn display_ids() -> Result<Vec<u32>> {
        const MAX_DISPLAY_IDS: i32 = 64;

        let mut display_ids = vec![0_u32; MAX_DISPLAY_IDS as usize];
        let copied = unsafe { ffi_copy_display_ids(display_ids.as_mut_ptr(), MAX_DISPLAY_IDS) };
        if copied < 0 {
            bail!("Swift bridge failed to enumerate display ids");
        }

        let copied = copied as usize;
        display_ids.truncate(copied);
        Ok(display_ids)
    }

    pub(super) fn get_display_count() -> Result<usize> {
        let count = unsafe { ffi_get_display_count() };
        if count < 0 {
            bail!("Swift bridge failed to enumerate displays");
        }

        Ok(count as usize)
    }

    pub(super) fn capture_screenshot(display_id: u32, output_path: &Path, quality: u8) -> Result<()> {
        let output_path = CString::new(output_path.as_os_str().as_bytes())?;
        let ok = unsafe { ffi_capture_screenshot(i64::from(display_id), output_path.as_ptr(), quality) };
        if ok {
            Ok(())
        } else {
            bail!("Swift bridge failed to capture display {display_id}");
        }
    }
}

#[cfg(feature = "mock-capture")]
mod native {
    use std::{fs::File, path::Path};

    use anyhow::{Context, Result};
    use image::{codecs::jpeg::JpegEncoder, ImageBuffer, Rgb};

    pub(super) fn display_ids() -> Result<Vec<u32>> {
        Ok(vec![0])
    }

    pub(super) fn get_display_count() -> Result<usize> {
        Ok(display_ids()?.len())
    }

    pub(super) fn capture_screenshot(display_id: u32, output_path: &Path, quality: u8) -> Result<()> {
        if display_id != 0 {
            anyhow::bail!("mock capture only exposes display 0, got {display_id}");
        }

        let mut encoder = JpegEncoder::new_with_quality(
            File::create(output_path)
                .with_context(|| format!("failed to create mock screenshot at {}", output_path.display()))?,
            quality,
        );

        let image = ImageBuffer::from_pixel(32, 20, Rgb([0x34u8, 0x98, 0xdb]));

        encoder
            .encode_image(&image)
            .with_context(|| format!("failed to encode mock JPEG at {}", output_path.display()))
    }
}

#[cfg(all(test, feature = "mock-capture"))]
mod tests {
    use std::{
        env, fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::Result;
    use image::ImageFormat;

    use super::{capture_screenshot, display_ids, get_display_count};

    fn unique_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        env::temp_dir().join(format!("screencap-screenshot-tests-{name}-{unique}.jpg"))
    }

    #[test]
    fn mock_capture_writes_a_valid_jpeg() -> Result<()> {
        let path = unique_path("valid-jpeg");
        capture_screenshot(0, &path, 75)?;

        let bytes = fs::read(&path)?;
        assert!(bytes.starts_with(&[0xFF, 0xD8]));

        let image = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg)?;
        assert_eq!(image.width(), 32);
        assert_eq!(image.height(), 20);

        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn mock_capture_reports_display_ids_consistently() -> Result<()> {
        assert_eq!(display_ids()?, vec![0]);
        assert_eq!(get_display_count()?, 1);
        Ok(())
    }

    #[test]
    fn rejects_invalid_display_id() {
        let path = unique_path("invalid-display");
        let error = capture_screenshot(1, &path, 75).expect_err("invalid display id should fail");
        assert!(error.to_string().contains("display 0"));
        assert!(!path.exists());
    }

    #[test]
    fn rejects_invalid_jpeg_quality() {
        let path = unique_path("invalid-quality");
        let error = capture_screenshot(0, &path, 101).expect_err("quality > 100 should fail");
        assert!(error.to_string().contains("jpeg quality"));
        assert!(!path.exists());
    }
}
