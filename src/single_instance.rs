#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::time::Duration;

    const APP_TITLE: &str = "Rayview Meta";
    const ERROR_ALREADY_EXISTS: u32 = 183;
    const SW_RESTORE: i32 = 9;
    const SW_SHOWNORMAL: i32 = 1;

    type Handle = *mut c_void;
    type Hwnd = *mut c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(
            lp_mutex_attributes: *mut c_void,
            b_initial_owner: i32,
            lp_name: *const u16,
        ) -> Handle;
        fn GetLastError() -> u32;
        fn CloseHandle(h_object: Handle) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(lp_class_name: *const u16, lp_window_name: *const u16) -> Hwnd;
        fn IsIconic(h_wnd: Hwnd) -> i32;
        fn ShowWindow(h_wnd: Hwnd, n_cmd_show: i32) -> i32;
        fn SetForegroundWindow(h_wnd: Hwnd) -> i32;
    }

    pub struct SingleInstanceGuard {
        handle: Handle,
    }

    impl Drop for SingleInstanceGuard {
        fn drop(&mut self) {
            if !self.handle.is_null() {
                // The handle only keeps the named mutex alive for this process.
                unsafe {
                    CloseHandle(self.handle);
                }
            }
        }
    }

    pub fn acquire_or_activate() -> Option<SingleInstanceGuard> {
        let mutex_name = to_wide("Local\\RayviewMeta.Client.SingleInstance");
        let handle = unsafe { CreateMutexW(null_mut(), 0, mutex_name.as_ptr()) };
        if handle.is_null() {
            return Some(SingleInstanceGuard { handle });
        }

        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if already_running {
            activate_existing_window();
            unsafe {
                CloseHandle(handle);
            }
            None
        } else {
            Some(SingleInstanceGuard { handle })
        }
    }

    fn activate_existing_window() {
        let title = to_wide(APP_TITLE);
        for _ in 0..60 {
            let hwnd = unsafe { FindWindowW(null(), title.as_ptr()) };
            if !hwnd.is_null() {
                let command = if unsafe { IsIconic(hwnd) } != 0 {
                    SW_RESTORE
                } else {
                    SW_SHOWNORMAL
                };
                unsafe {
                    ShowWindow(hwnd, command);
                    SetForegroundWindow(hwnd);
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn to_wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(not(windows))]
mod platform {
    pub struct SingleInstanceGuard;

    pub fn acquire_or_activate() -> Option<SingleInstanceGuard> {
        Some(SingleInstanceGuard)
    }
}

pub(crate) use platform::acquire_or_activate;
