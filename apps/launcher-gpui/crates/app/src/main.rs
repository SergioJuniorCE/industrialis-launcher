//! Industrialis launcher — GPUI app.
//!
//! Boots a tokio runtime, loads backend snapshots synchronously, and opens
//! the main window on [`views::LauncherRoot`].

#![cfg_attr(
    all(windows, any(not(debug_assertions), feature = "dev-watch")),
    windows_subsystem = "windows"
)]

mod appearance;
mod assets;
mod code_highlighting;
mod desktop_window;
#[cfg(debug_assertions)]
mod dev_control;
mod input;
mod native_quit;
mod state;
mod theme;
mod ui;
mod views;

use std::sync::Arc;

use gpui::{px, size, App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use industrialis_backend::accounts::account_to_info;
use industrialis_backend::launch::running_instances;
use industrialis_backend::paths::LauncherPaths;
use industrialis_backend::settings::load_launcher_settings;
use industrialis_core::launcher_settings::resolve_default_account_id;

use crate::state::AppData;
use crate::views::LauncherRoot;

fn main() {
    if industrialis_backend::game_broker::run_if_requested() {
        return;
    }
    #[cfg(debug_assertions)]
    let dev_control = dev_control::DevControl::install();
    let paths = LauncherPaths::system();
    // Ownership precedes even startup PID cleanup; the guard outlives the runtime.
    use industrialis_backend::profile_owner::{acquire, Acquisition, OwnerKind};
    let _profile_owner = match acquire(&paths, OwnerKind::Launcher) {
        Ok(Acquisition::Primary(owner)) => owner,
        Ok(Acquisition::Existing(OwnerKind::Launcher)) => return,
        Ok(Acquisition::Existing(OwnerKind::Operation)) => {
            startup_error(
                "This profile is in use. Wait for the current launcher command to finish.",
            );
            return;
        }
        Err(error) => {
            startup_error(&format!("Could not open launcher profile: {error:#}"));
            return;
        }
    };
    let activation = _profile_owner.activation_signal();
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("industrialis-backend")
            .build()
            .expect("tokio runtime builds"),
    );

    rt.spawn_blocking(code_highlighting::warm_up);
    // Synchronous boot: local files only, no network.
    let (instances, accounts, running, settings, issues) = rt.block_on(async {
        let mut issues = Vec::new();
        let instances =
            match industrialis_backend::instances::load_instances_with_issues(&paths).await {
                Ok((instances, warnings)) => {
                    issues.extend(warnings);
                    instances
                }
                Err(e) => {
                    issues.push(format!("Could not load instances: {e:#}"));
                    Vec::new()
                }
            };
        let accounts = match industrialis_backend::accounts::load_accounts_with_issues(&paths).await
        {
            Ok((accounts, skipped)) => {
                if skipped > 0 {
                    issues.push(format!(
                        "{skipped} unrecognized account entries were kept for recovery."
                    ));
                }
                accounts
            }
            Err(e) => {
                issues.push(format!("Could not load accounts: {e:#}"));
                Vec::new()
            }
        };
        let running = match running_instances(&paths).await {
            Ok(running) => running,
            Err(e) => {
                issues.push(format!("Could not load running games: {e:#}"));
                Vec::new()
            }
        };
        let settings = match load_launcher_settings(&paths).await {
            Ok(settings) => settings,
            Err(e) => {
                issues.push(format!("Could not load preferences: {e:#}"));
                Default::default()
            }
        };
        (instances, accounts, running, settings, issues)
    });
    let default_account_id = resolve_default_account_id(&settings).map(str::to_string);
    let (width, height) = (
        settings.window.window_width as f32,
        settings.window.window_height as f32,
    );

    let maximized = settings.window.launch_maximized;
    let data = AppData {
        paths,
        rt: rt.clone(),
        instances,
        accounts: accounts.iter().map(account_to_info).collect(),
        settings,
        default_account_id,
        running: running
            .into_iter()
            .map(|(id, pid, alive)| (id, (pid, alive)))
            .collect(),
        logs: Default::default(),
        owned_running: Default::default(),
        launching: Default::default(),
        status: if issues.is_empty() {
            "Ready".into()
        } else {
            issues.join("\n")
        },
    };

    let app = gpui_platform::application().with_assets(assets::LauncherAssets);
    let reopen = activation.clone();
    app.on_reopen(move |_| {
        reopen.store(true, std::sync::atomic::Ordering::Release);
    });
    let links = activation.clone();
    app.on_open_urls(move |urls| {
        if urls
            .iter()
            .any(|url| url.starts_with("industrialislauncher:"))
        {
            links.store(true, std::sync::atomic::Ordering::Release);
        }
    });
    app.run(move |cx: &mut App| {
        if let Err(error) = native_quit::install() {
            startup_error(&error);
            cx.quit();
            return;
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(if maximized {
                        WindowBounds::Maximized(bounds)
                    } else {
                        WindowBounds::Windowed(bounds)
                    }),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Industrialis Launcher".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(800.0), px(600.0))),
                    ..Default::default()
                },
                |window, cx| {
                    let root = cx.new(|cx| {
                        let mut root = LauncherRoot::new(data);
                        root.set_profile_issues(issues);
                        root.start_backups(cx);
                        root.check_launcher_update(cx);
                        root.monitor_restored_games(cx);
                        root.refresh_instance_sizes(None, cx);
                        root.refresh_groups(cx);
                        root.start_pack_polling(cx);
                        root
                    });
                    let weak = root.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        if let Some(root) = weak.upgrade() {
                            root.update(cx, |root, cx| root.request_window_close(window, cx));
                            false
                        } else {
                            true
                        }
                    });
                    root
                },
            )
            .unwrap();
        cx.spawn(async move |cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            if activation.swap(false, std::sync::atomic::Ordering::AcqRel) {
                if window
                    .update(cx, |_, window, cx| {
                        desktop_window::restore(window, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
            let quit_requested = native_quit::requested();
            #[cfg(debug_assertions)]
            let quit_requested = quit_requested || dev_control.requested();
            if quit_requested {
                let _ = window.update(cx, |root, window, cx| {
                    desktop_window::restore(window, cx);
                    root.request_close(cx);
                });
            }
        })
        .detach();
        cx.activate(true);
    });
}

fn startup_error(message: &str) {
    eprintln!("{message}");
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(
                hwnd: *mut std::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                flags: u32,
            ) -> i32;
        }
        let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "Industrialis Launcher"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
        }
    }
}
