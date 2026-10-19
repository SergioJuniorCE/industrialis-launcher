//! Context actions keep the chosen path independent of the open editor draft.
use super::{minecraft_files::instance_id, LauncherRoot};
use crate::ui::{btn, ButtonVariant};
use gpui::{div, prelude::*, Context, Div};
use industrialis_backend::{
    file_actions,
    minecraft_files::{self, MinecraftEntry},
};

#[derive(Clone, Copy)]
enum FileAction {
    External,
    Relative,
    Absolute,
    Location,
}

#[cfg(test)]
mod tests {
    use super::super::{test_support::Fixture, DetailTab};
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    #[gpui::test]
    async fn file_context_copies_clicked_paths_without_switching_the_open_document(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["pack"]);
        let game = fixture
            .paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft");
        std::fs::write(game.join("first.txt"), "first").unwrap();
        std::fs::write(game.join("second.txt"), "second").unwrap();
        let (window, root) =
            fixture.open_sized(cx, gpui::size(gpui::px(1600.), gpui::px(900.)), |v| {
                v.selected_id = Some("pack".into());
                v.instance_ui.detail_tab = DetailTab::Files;
            });
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        fixture
            .until(cx, &root, |v| {
                v.files_ui.editors["pack"].explorer.loading.is_empty()
            })
            .await;
        Fixture::click(cx, window, "minecraft-file:first.txt");
        fixture
            .until(cx, &root, |v| {
                v.files_ui.editors["pack"].selected.as_deref() == Some("first.txt")
            })
            .await;
        for (selector, expected) in [
            ("file-menu-relative", "second.txt".to_owned()),
            (
                "file-menu-absolute",
                game.join("second.txt").to_string_lossy().into_owned(),
            ),
        ] {
            {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                let bounds = visual.debug_bounds("minecraft-file:second.txt").unwrap();
                visual.simulate_mouse_down(
                    bounds.center(),
                    gpui::MouseButton::Right,
                    Default::default(),
                );
                visual.simulate_mouse_up(
                    bounds.center(),
                    gpui::MouseButton::Right,
                    Default::default(),
                );
            }
            Fixture::click(cx, window, selector);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                cx.run_until_parked();
                if cx.read_from_clipboard().and_then(|item| item.text()) == Some(expected.clone()) {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                fixture
                    .rt
                    .spawn(async { tokio::time::sleep(std::time::Duration::from_millis(2)).await })
                    .await
                    .unwrap();
            }
            assert_eq!(
                root.read_with(cx, |v, _| v.files_ui.editors["pack"].selected.clone()),
                Some("first.txt".into())
            );
        }
        fixture.cleanup();
    }
}
impl LauncherRoot {
    pub(super) fn open_instance_directory(&mut self, id: String, cx: &mut Context<Self>) {
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { file_actions::open_instance_folder(&paths, &id).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        v.data.status = format!("Could not open instance folder: {error:#}")
                    }
                    Err(error) => {
                        v.data.status = format!("Could not open instance folder: {error}")
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn open_file_menu(
        &mut self,
        id: String,
        entry: MinecraftEntry,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.dialog.is_some() || self.closing.is_some() {
            return;
        }
        self.files_ui
            .editors
            .entry(id.clone())
            .or_default()
            .explorer
            .row = Some(entry.rel_path.clone());
        self.workspace_ui.menu = Some((super::workspace::MenuTarget::File(id, entry), position));
        cx.notify();
    }
    pub(super) fn file_menu_items(
        &self,
        id: &str,
        entry: &MinecraftEntry,
        cx: &mut Context<Self>,
    ) -> Div {
        div().flex().flex_col().children(
            [
                (
                    "Open in external editor",
                    "file-menu-editor",
                    FileAction::External,
                ),
                (
                    "Copy relative path",
                    "file-menu-relative",
                    FileAction::Relative,
                ),
                (
                    "Copy absolute path",
                    "file-menu-absolute",
                    FileAction::Absolute,
                ),
                (
                    "Open file location",
                    "file-menu-location",
                    FileAction::Location,
                ),
            ]
            .into_iter()
            .map(|(label, selector, action)| {
                let id = id.to_owned();
                let entry = entry.clone();
                let disabled =
                    matches!(action, FileAction::External) && (entry.is_dir || !entry.editable);
                btn(&self.theme, label, ButtonVariant::Ghost)
                    .id(selector)
                    .debug_selector(move || selector.into())
                    .role(gpui::accesskit::Role::MenuItem)
                    .w_full()
                    .justify_start()
                    .text_xs()
                    .tab_stop(!disabled)
                    .when(disabled, |c| c.opacity(0.5).cursor_default())
                    .on_click(cx.listener(move |v, _, _, cx| {
                        if !disabled {
                            v.file_context_action(id.clone(), entry.clone(), action, cx);
                        }
                    }))
            }),
        )
    }
    fn file_context_action(
        &mut self,
        id: String,
        entry: MinecraftEntry,
        action: FileAction,
        cx: &mut Context<Self>,
    ) {
        self.workspace_ui.menu = None;
        if matches!(action, FileAction::Relative) {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(entry.rel_path));
            self.data.status = "Relative path copied".into();
            cx.notify();
            return;
        }
        if matches!(action, FileAction::External) {
            let target = id.clone();
            self.save_file_then(
                id,
                move |v, cx| v.run_file_action(target, entry, action, cx),
                cx,
            );
        } else {
            self.run_file_action(id, entry, action, cx);
        }
    }
    fn run_file_action(
        &mut self,
        id: String,
        entry: MinecraftEntry,
        action: FileAction,
        cx: &mut Context<Self>,
    ) {
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let instance = instance_id(&id).to_owned();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    if matches!(action, FileAction::External) {
                        minecraft_files::open_external_editor(&paths, &instance, &entry.rel_path)
                            .await?;
                        Ok(None)
                    } else {
                        tokio::task::spawn_blocking(move || {
                            file_actions::item_path(&paths, &instance, &entry.rel_path)
                        })
                        .await?
                        .map(Some)
                    }
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                match result {
                    Ok(Ok(Some(path))) => {
                        if matches!(action, FileAction::Absolute) {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                path.to_string_lossy().into_owned(),
                            ));
                            v.data.status = "Absolute path copied".into();
                        } else {
                            cx.reveal_path(&path);
                        }
                    }
                    Ok(Ok(None)) => {
                        v.data.status = "Opened in external editor; saved edits are mirrored".into()
                    }
                    Ok(Err(error)) => v.data.status = format!("File action failed: {error:#}"),
                    Err(error) => v.data.status = format!("File action failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
