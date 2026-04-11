use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub app_name: String,
    pub window_title: String,
    pub bundle_id: String,
}

pub fn get_active_window() -> Result<WindowInfo> {
    native::get_active_window()
}

pub fn start_app_change_listener(callback: extern "C" fn()) {
    native::start_app_change_listener(callback);
}

pub fn stop_app_change_listener() {
    native::stop_app_change_listener();
}

#[cfg(not(feature = "mock-capture"))]
mod native {
    use std::ffi::{c_char, CStr};

    use anyhow::{anyhow, bail, Context, Result};

    use super::WindowInfo;

    unsafe extern "C" {
        #[link_name = "get_active_window"]
        fn ffi_get_active_window(
            out_app_name: *mut *mut c_char,
            out_window_title: *mut *mut c_char,
            out_bundle_id: *mut *mut c_char,
        ) -> bool;

        #[link_name = "free_bridge_string"]
        fn ffi_free_bridge_string(value: *mut c_char);

        #[link_name = "start_app_change_listener"]
        fn ffi_start_app_change_listener(callback: extern "C" fn());

        #[link_name = "stop_app_change_listener"]
        fn ffi_stop_app_change_listener();
    }

    pub(super) fn get_active_window() -> Result<WindowInfo> {
        let mut app_name = std::ptr::null_mut();
        let mut window_title = std::ptr::null_mut();
        let mut bundle_id = std::ptr::null_mut();

        let ok = unsafe { ffi_get_active_window(&mut app_name, &mut window_title, &mut bundle_id) };
        if !ok {
            bail!("Swift bridge failed to fetch active window info");
        }

        let app_name = BridgeString::new(app_name, "app_name")?;
        let window_title = BridgeString::new(window_title, "window_title")?;
        let bundle_id = BridgeString::new(bundle_id, "bundle_id")?;

        Ok(WindowInfo {
            app_name: app_name.to_string()?,
            window_title: window_title.to_string()?,
            bundle_id: bundle_id.to_string()?,
        })
    }

    pub(super) fn start_app_change_listener(callback: extern "C" fn()) {
        unsafe { ffi_start_app_change_listener(callback) };
    }

    pub(super) fn stop_app_change_listener() {
        unsafe { ffi_stop_app_change_listener() };
    }

    struct BridgeString {
        value: *mut c_char,
        field: &'static str,
    }

    impl BridgeString {
        fn new(value: *mut c_char, field: &'static str) -> Result<Self> {
            if value.is_null() {
                Err(anyhow!("Swift bridge returned null for {field}"))
            } else {
                Ok(Self { value, field })
            }
        }

        fn to_string(&self) -> Result<String> {
            unsafe { CStr::from_ptr(self.value) }
                .to_str()
                .map(|value| value.to_owned())
                .with_context(|| format!("Swift bridge returned non-utf8 {}", self.field))
        }
    }

    impl Drop for BridgeString {
        fn drop(&mut self) {
            unsafe { ffi_free_bridge_string(self.value) };
        }
    }
}

#[cfg(feature = "mock-capture")]
mod native {
    use anyhow::Result;

    use super::WindowInfo;

    pub(super) fn get_active_window() -> Result<WindowInfo> {
        Ok(WindowInfo {
            app_name: "MockApp".into(),
            window_title: "Mock Window".into(),
            bundle_id: "com.mock.app".into(),
        })
    }

    pub(super) fn start_app_change_listener(_callback: extern "C" fn()) {}

    pub(super) fn stop_app_change_listener() {}
}

#[cfg(all(test, feature = "mock-capture"))]
mod tests {
    use anyhow::Result;

    use super::{get_active_window, WindowInfo};

    #[test]
    fn mock_capture_returns_active_window_info() -> Result<()> {
        assert_eq!(
            get_active_window()?,
            WindowInfo {
                app_name: "MockApp".into(),
                window_title: "Mock Window".into(),
                bundle_id: "com.mock.app".into(),
            }
        );
        Ok(())
    }
}
