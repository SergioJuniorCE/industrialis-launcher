//! Exercise the bounds-change layout path used by native window dragging.
//! Uses a hidden window, fictional instances and an isolated, read-only profile.
//! Measures CPU layout, not OS movement cadence or GPU presentation latency.
#![allow(dead_code)]

mod appearance;
mod assets;
mod code_highlighting;
mod desktop_window;
mod input;
mod native_quit;
mod state;
mod theme;
mod ui;
mod views;

use std::{sync::Arc, time::Instant};

use gpui::{px, size, App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use industrialis_backend::{instances::InstanceInfo, paths::LauncherPaths};
use industrialis_core::{
    instance_settings::InstanceSettings,
    launcher_settings::{LauncherSettingsData, ThemeMode, WindowMaterial},
};

fn main() {
    let budget_ms = std::env::var("GPUI_BENCH_BUDGET_MS")
        .ok()
        .map(|value| value.parse::<f64>().expect("budget must be a number"))
        .unwrap_or(1000.0 / 60.0);
    assert!(budget_ms.is_finite() && budget_ms > 0.0);
    let material: WindowMaterial = serde_json::from_value(serde_json::Value::String(
        std::env::var("GPUI_BENCH_MATERIAL").unwrap_or_else(|_| "opaque".into()),
    ))
    .expect("GPUI_BENCH_MATERIAL must be opaque, frosted or mica");
    let timings = Arc::new(std::sync::Mutex::new(Vec::new()));
    let collected = timings.clone();
    let rt = Arc::new(tokio::runtime::Runtime::new().expect("Tokio runtime"));
    // Never use LauncherPaths::system() in this benchmark. Only WorkspaceUi's
    // optional reads access this path; no backend operations or saves are run.
    let paths = LauncherPaths::new(std::env::temp_dir().join(format!(
        "industrialis-window-move-bench-{}",
        profile_nonce()
    )));
    let instances = ["benchmark-a", "benchmark-b"]
        .into_iter()
        .map(|id| InstanceInfo {
            id: id.into(),
            installed: true,
            size_bytes: 55_000_000,
            settings: InstanceSettings {
                name: format!("GTNH {id}"),
                pack_version: "2.9.0-RC-2".into(),
                ..Default::default()
            },
            group: "Main".into(),
            icon_path: None,
        })
        .collect();
    let data = state::AppData {
        paths,
        rt,
        instances,
        accounts: Vec::new(),
        settings: LauncherSettingsData {
            window_material: material,
            ..Default::default()
        },
        default_account_id: None,
        running: Default::default(),
        logs: Default::default(),
        owned_running: Default::default(),
        launching: Default::default(),
        status: "Ready".into(),
    };

    gpui_platform::application()
        .with_assets(assets::LauncherAssets)
        .run(move |cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(1600.0), px(900.0)), cx);
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Industrialis layout benchmark".into()),
                            appears_transparent: true,
                            ..Default::default()
                        }),
                        focus: false,
                        show: false,
                        ..Default::default()
                    },
                    |_, cx| {
                        cx.new(|_| {
                            let mut root = views::LauncherRoot::new(data);
                            root.selected_id = Some("benchmark-a".into());
                            root
                        })
                    },
                )
                .expect("hidden benchmark window");

            // App::update_window avoids borrowing the root entity while draw
            // renders it. WindowHandle::update would hold that entity borrow.
            for frame in 0..160 {
                let started = Instant::now();
                cx.update_window(handle.into(), |_, window, cx| {
                    // Same invalidation path as the Windows on_moved callback.
                    window.bounds_changed(cx);
                    window.draw(cx).clear(cx);
                })
                .expect("bounds-change redraw");
                if frame >= 10 {
                    collected
                        .lock()
                        .unwrap()
                        .push(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
            // Exercise switching in both themes, including restoration of the
            // opaque backdrop. Reads actual DWM state, not just the UI setting.
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                for material in [
                    WindowMaterial::Frosted,
                    WindowMaterial::Mica,
                    WindowMaterial::Opaque,
                ] {
                    handle
                        .update(cx, |root, _, cx| {
                            root.data.settings.window_material = material;
                            root.data.settings.theme_mode = mode;
                            root.theme = theme::ActiveTheme::from_settings(
                                if mode == ThemeMode::Light {
                                    "light"
                                } else {
                                    "dark"
                                },
                                "industrialis",
                                &[],
                            );
                            cx.notify();
                        })
                        .expect("switch backdrop");
                    cx.update_window(handle.into(), |_, window, cx| {
                        window.bounds_changed(cx);
                        window.draw(cx).clear(cx);
                    })
                    .expect("draw switched backdrop");
                    handle
                        .update(cx, |root, _window, _| {
                            if std::env::var_os("GPUI_BENCH_REQUIRE_BACKDROP").is_some() {
                                assert_eq!(
                                    root.theme.window_material, material,
                                    "native material must be available"
                                );
                            }
                            #[cfg(target_os = "windows")]
                            verify_native_backdrop(_window, root.theme.window_material, mode);
                        })
                        .expect("verify switched backdrop");
                }
            }
            // Linux starts calloop after this launch callback. It clears an
            // earlier stop signal, so quit only after the event loop is running.
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let _ = cx.update(|cx| cx.quit());
            })
            .detach();
        });

    let mut values = timings.lock().unwrap().clone();
    assert_eq!(values.len(), 150, "benchmark must finish all samples");
    values.sort_by(f64::total_cmp);
    let median_ms = values[values.len() / 2];
    let p95_ms = values[(values.len() * 95 / 100).min(values.len() - 1)];
    let passed = p95_ms <= budget_ms;
    println!(
        "{}",
        serde_json::json!({
            "samples": values.len(),
            "median_ms": median_ms,
            "p95_ms": p95_ms,
            "budget_ms": budget_ms,
            "passed": passed,
            "material": material,
            "backdrop_switch_checks": 6,
        })
    );
    if !passed {
        std::process::exit(1);
    }
}

#[cfg(target_os = "windows")]
fn verify_native_backdrop(window: &gpui::Window, material: WindowMaterial, mode: ThemeMode) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DwmGetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        },
    };
    let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window).unwrap().as_raw()
    else {
        panic!("expected Win32 window")
    };
    let hwnd = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
    let mut backdrop = 0i32;
    let mut dark = 0i32;
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &mut backdrop as *mut _ as _,
            4,
        )
    };
    if result.is_ok() {
        let expected = match material {
            WindowMaterial::Opaque => 1,
            WindowMaterial::Mica => 2,
            WindowMaterial::Frosted => 3,
        };
        assert_eq!(
            backdrop, expected,
            "DWM must switch and clear the actual material"
        );
    } else {
        assert_eq!(
            material,
            WindowMaterial::Opaque,
            "unsupported DWM must remain opaque"
        );
    }
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &mut dark as *mut _ as _,
            4,
        )
    }
    .is_ok()
    {
        assert_eq!(
            dark != 0,
            mode == ThemeMode::Dark,
            "native tint must match launcher theme"
        );
    }
}

fn profile_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_nanos()
}
