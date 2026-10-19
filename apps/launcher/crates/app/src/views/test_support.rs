//! Shared disposable-profile helpers for UI-to-disk acceptance tests.
use super::LauncherRoot;
use crate::state::AppData;
use gpui::{px, size, AppContext, Entity, TestAppContext, VisualTestContext, WindowHandle};
use industrialis_backend::{instances, paths::LauncherPaths};
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(super) struct Fixture {
    pub paths: LauncherPaths,
    pub rt: Arc<tokio::runtime::Runtime>,
}
impl Fixture {
    pub fn new(ids: &[&str]) -> Self {
        let paths = LauncherPaths::new(std::env::temp_dir().join(format!(
            "industrialis-ui-acceptance-{}-{}", std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        )));
        for id in ids {
            let folder = paths.instance_dir(id).unwrap();
            std::fs::create_dir_all(folder.join(".minecraft/config")).unwrap();
            std::fs::write(folder.join("mmc-pack.json"), "{}").unwrap();
            std::fs::write(
                paths.settings_path(id).unwrap(),
                serde_json::json!({"name":id,"acceptanceExtension":true}).to_string(),
            )
            .unwrap();
        }
        Self {
            paths,
            rt: Arc::new(tokio::runtime::Runtime::new().unwrap()),
        }
    }
    pub fn open(
        &self,
        cx: &mut TestAppContext,
        configure: impl FnOnce(&mut LauncherRoot),
    ) -> (WindowHandle<LauncherRoot>, Entity<LauncherRoot>) {
        self.open_sized(cx, size(px(1100.), px(850.)), configure)
    }
    pub fn open_sized(
        &self,
        cx: &mut TestAppContext,
        dimensions: gpui::Size<gpui::Pixels>,
        configure: impl FnOnce(&mut LauncherRoot),
    ) -> (WindowHandle<LauncherRoot>, Entity<LauncherRoot>) {
        cx.executor().allow_parking();
        let data = AppData {
            instances: self
                .rt
                .block_on(instances::load_instances(&self.paths))
                .unwrap(),
            settings: self
                .rt
                .block_on(industrialis_backend::settings::load_launcher_settings(
                    &self.paths,
                ))
                .unwrap(),
            paths: self.paths.clone(),
            rt: self.rt.clone(),
            accounts: Vec::new(),
            default_account_id: None,
            running: Default::default(),
            owned_running: Default::default(),
            logs: Default::default(),
            launching: Default::default(),
            status: "Ready".into(),
        };
        let window = cx.open_window(dimensions, |_, _| {
            let mut root = LauncherRoot::new(data);
            configure(&mut root);
            root
        });
        let root = window.root(cx).unwrap();
        (window, root)
    }
    pub async fn until(
        &self,
        cx: &mut TestAppContext,
        root: &Entity<LauncherRoot>,
        done: impl Fn(&LauncherRoot) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            cx.run_until_parked();
            if root.read_with(cx, |v, _| done(v)) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "UI acceptance operation did not settle"
            );
            self.rt
                .spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                .await
                .unwrap();
        }
    }
    pub fn click(
        cx: &mut TestAppContext,
        window: WindowHandle<LauncherRoot>,
        selector: &'static str,
    ) {
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let bounds = visual
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("Missing control {selector}"));
        visual.simulate_click(bounds.center(), Default::default());
    }
    pub fn cleanup(&self) {
        std::fs::remove_dir_all(self.paths.base()).unwrap();
    }
}
