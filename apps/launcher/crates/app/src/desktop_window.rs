//! The pinned Windows platform's App::hide is a no-op. Preserve native hide/restore.
#[cfg(windows)]
fn hwnd(window: &gpui::Window) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as _),
        _ => None,
    }
}
#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn ShowWindowAsync(hwnd: *mut std::ffi::c_void, command: i32) -> i32;
    fn IsIconic(hwnd: *mut std::ffi::c_void) -> i32;
}
pub fn hide(window: &gpui::Window, cx: &gpui::App) {
    #[cfg(windows)]
    {
        if let Some(hwnd) = hwnd(window) {
            unsafe {
                ShowWindowAsync(hwnd, 0);
            }
        }
        let _ = cx;
    }
    #[cfg(target_os = "macos")]
    {
        use objc::runtime::Object;
        use objc::{msg_send, sel, sel_impl};
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window) {
            if let RawWindowHandle::AppKit(view) = handle.as_raw() {
                unsafe {
                    let view = view.ns_view.as_ptr().cast::<Object>();
                    let native: *mut Object = msg_send![view, window];
                    if !native.is_null() {
                        let _: () = msg_send![native, orderOut: std::ptr::null_mut::<Object>()];
                    }
                }
            }
        }
        let _ = cx;
    }
    #[cfg(target_os = "linux")]
    {
        let _ = cx;
        window.minimize_window();
    }
}
pub fn restore(window: &gpui::Window, cx: &gpui::App) {
    #[cfg(windows)]
    if let Some(hwnd) = hwnd(window) {
        unsafe {
            ShowWindowAsync(hwnd, if IsIconic(hwnd) != 0 { 9 } else { 5 });
        }
    }
    window.activate_window();
    cx.activate(true);
}
