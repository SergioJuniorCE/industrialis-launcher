//! Route macOS Dock/menu Quit through the same save guards and asynchronous drain.
//! GPUI 0.3.8's final on_app_quit futures have a 200 ms timeout, so they cannot
//! safely own maintenance/backup shutdown. Cancel native termination first.
//! https://developer.apple.com/documentation/appkit/nsapplication/terminate(_:)
//! https://developer.apple.com/documentation/objectivec/class_addmethod(_:_:_:_:)
#[cfg(any(target_os = "macos", test))]
struct QuitGate {
    requested: std::sync::atomic::AtomicBool,
    ready: std::sync::atomic::AtomicBool,
}
#[cfg(any(target_os = "macos", test))]
impl QuitGate {
    const fn new() -> Self {
        Self {
            requested: std::sync::atomic::AtomicBool::new(false),
            ready: std::sync::atomic::AtomicBool::new(false),
        }
    }
    fn should_terminate(&self) -> usize {
        use std::sync::atomic::Ordering;
        if self.ready.load(Ordering::Acquire) {
            1
        } else {
            self.requested.store(true, Ordering::Release);
            0
        }
    }
}
#[cfg(target_os = "macos")]
static QUIT: QuitGate = QuitGate::new();

pub fn install() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        use std::ffi::{c_char, c_void};
        #[link(name = "objc")]
        unsafe extern "C" {
            fn objc_getClass(name: *const c_char) -> *mut c_void;
            fn sel_registerName(name: *const c_char) -> *mut c_void;
            fn class_addMethod(
                class: *mut c_void,
                selector: *mut c_void,
                implementation: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> usize,
                encoding: *const c_char,
            ) -> bool;
        }
        extern "C" fn should_terminate(_: *mut c_void, _: *mut c_void, _: *mut c_void) -> usize {
            QUIT.should_terminate()
        }
        // This class and its missing termination selector were verified against
        // the exact pinned adapter. Refuse startup if a future pin changes it.
        let class = objc_getClass(c"GPUIApplicationDelegate".as_ptr());
        if class.is_null()
            || !class_addMethod(
                class,
                sel_registerName(c"applicationShouldTerminate:".as_ptr()),
                should_terminate,
                c"Q@:@".as_ptr(),
            )
        {
            return Err("Could not install safe macOS quit handling for this GPUI version.".into());
        }
    }
    Ok(())
}
pub fn requested() -> bool {
    #[cfg(target_os = "macos")]
    {
        QUIT.requested
            .swap(false, std::sync::atomic::Ordering::AcqRel)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
pub fn allow_after_drain() {
    #[cfg(target_os = "macos")]
    QUIT.ready.store(true, std::sync::atomic::Ordering::Release);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_quit_is_cancelled_until_backend_drain_succeeds() {
        let gate = QuitGate::new();
        assert_eq!(gate.should_terminate(), 0);
        assert!(gate
            .requested
            .swap(false, std::sync::atomic::Ordering::AcqRel));
        assert_eq!(gate.should_terminate(), 0);
        gate.ready.store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(gate.should_terminate(), 1);
    }
}
