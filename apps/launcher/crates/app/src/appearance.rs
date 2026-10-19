//! Native window materials, separate from opaque content and theme colors.
use gpui::Window;
use industrialis_core::launcher_settings::{ThemeMode, WindowMaterial};

#[derive(Default)]
pub struct BackdropState {
    applied: Option<(WindowMaterial, ThemeMode)>,
    pub effective: WindowMaterial,
    pub fallback: Option<String>,
}

impl BackdropState {
    /// Run only when the material or theme changes, never on every redraw.
    pub fn sync(&mut self, window: &Window, material: WindowMaterial, mode: ThemeMode) {
        if self.applied == Some((material, mode)) {
            return;
        }
        self.applied = Some((material, mode));
        match apply_native(window, material, mode) {
            Ok(()) => {
                self.effective = material;
                self.fallback = None;
            }
            Err(reason) => {
                window.set_background_appearance(gpui::WindowBackgroundAppearance::Opaque);
                self.effective = WindowMaterial::Opaque;
                self.fallback = Some(reason);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_native(window: &Window, material: WindowMaterial, _: ThemeMode) -> Result<(), String> {
    window.set_background_appearance(gpui::WindowBackgroundAppearance::Opaque);
    if material == WindowMaterial::Opaque {
        Ok(())
    } else {
        Err("Frosted glass and Mica require Windows 11. Using an opaque background.".into())
    }
}

#[cfg(target_os = "windows")]
fn apply_native(window: &Window, material: WindowMaterial, mode: ThemeMode) -> Result<(), String> {
    use gpui::WindowBackgroundAppearance;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::core::BOOL;
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMSBT_NONE,
            DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        },
        UI::Controls::MARGINS,
    };

    let raw = HasWindowHandle::window_handle(window)
        .map_err(|_| "Window backdrop unavailable. Using an opaque background.".to_string())?;
    let RawWindowHandle::Win32(handle) = raw.as_raw() else {
        return Err("Window backdrop unavailable. Using an opaque background.".into());
    };
    let hwnd = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
    let dark = BOOL::from(mode == ThemeMode::Dark);
    let backdrop = match material {
        WindowMaterial::Opaque => DWMSBT_NONE,
        WindowMaterial::Frosted => DWMSBT_TRANSIENTWINDOW,
        WindowMaterial::Mica => DWMSBT_MAINWINDOW,
    };

    // GPUI's Blurred variant uses legacy accent-policy Acrylic. Use the
    // supported DWM system backdrops instead, including when changing modes.
    // Opaque first clears any legacy accent policy. Explicitly clear DWM too:
    // GPUI 0.3.8 doesn't reset that attribute when switching back to Opaque.
    window.set_background_appearance(WindowBackgroundAppearance::Opaque);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &DWMSBT_NONE as *const _ as _,
            std::mem::size_of_val(&DWMSBT_NONE) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as _,
            std::mem::size_of_val(&dark) as u32,
        );
    }
    if material == WindowMaterial::Opaque {
        unsafe {
            let _ = DwmExtendFrameIntoClientArea(hwnd, &MARGINS::default());
        }
        return Ok(());
    }
    if std::env::var("GPUI_DISABLE_DIRECT_COMPOSITION")
        .is_ok_and(|value| value == "true" || value == "1")
    {
        return Err(
            "Transparency is unavailable with this renderer. Using an opaque background.".into(),
        );
    }

    // This enables GPUI's transparent swap-chain clear without the legacy
    // Acrylic path. Override its Mica attribute with the selected DWM material.
    window.set_background_appearance(WindowBackgroundAppearance::MicaBackdrop);
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    let result = unsafe {
        DwmExtendFrameIntoClientArea(hwnd, &margins).and_then(|_| {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const _ as _,
                std::mem::size_of_val(&backdrop) as u32,
            )
        })
    };
    if result.is_err() {
        unsafe {
            let _ = DwmExtendFrameIntoClientArea(hwnd, &MARGINS::default());
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &DWMSBT_NONE as *const _ as _,
                std::mem::size_of_val(&DWMSBT_NONE) as u32,
            );
        }
        return Err(
            "Native backdrops require Windows 11 22H2 or later. Using an opaque background.".into(),
        );
    }
    Ok(())
}
