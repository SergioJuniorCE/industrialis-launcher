use super::LauncherRoot;
use crate::{
    state::format_bytes,
    ui::{btn, card, ButtonVariant},
};
use gpui::{div, prelude::*, Context, Div, PathPromptOptions};
use industrialis_backend::{custom_mods as mods, pack_files::ModEntry};
use std::{collections::HashMap, path::PathBuf};
#[derive(Default)]
pub(super) struct ModsUi {
    states: HashMap<String, ModState>,
}
#[derive(Default)]
struct ModState {
    entries: Vec<ModEntry>,
    loaded: bool,
    busy: bool,
    error: Option<String>,
    remove: Option<String>,
}
impl LauncherRoot {
    pub(super) fn invalidate_mods(&mut self, id: &str) {
        if let Some(state) = self.mods_ui.states.get_mut(id) {
            state.loaded = false;
        }
    }
    fn reload_mods(&mut self, id: String, cx: &mut Context<Self>) {
        let state = self.mods_ui.states.entry(id.clone()).or_default();
        if state.busy {
            return;
        }
        state.loaded = true;
        state.busy = true;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { mods::list(&paths, &target).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.mods_ui.states.entry(id).or_default();
                state.busy = false;
                match result {
                    Ok(Ok(entries)) => {
                        state.entries = entries;
                        // Keep a failed import/removal visible during the follow-up refresh.
                    }
                    Ok(Err(e)) => state.error = Some(format!("{e:#}")),
                    Err(e) => state.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn pick_mods(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) || self.mods_ui.states.get(&id).is_some_and(|s| s.busy) {
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Select .jar or .zip custom mods".into()),
        });
        self.mods_ui.states.entry(id.clone()).or_default().busy = true;
        cx.spawn(async move |view, cx| {
            let result = picker.await;
            let _ = view.update(cx, |v, cx| {
                v.mods_ui.states.entry(id.clone()).or_default().busy = false;
                match result {
                    Ok(Ok(Some(paths))) => v.change_mods(id, Some(paths), None, cx),
                    Ok(Err(e)) => {
                        v.mods_ui.states.entry(id).or_default().error = Some(e.to_string())
                    }
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn change_mods(
        &mut self,
        id: String,
        sources: Option<Vec<PathBuf>>,
        identity: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.instance_is_busy(&id) {
            return;
        }
        let state = self.mods_ui.states.entry(id.clone()).or_default();
        if state.busy {
            return;
        }
        state.busy = true;
        state.error = None;
        state.remove = None;
        self.busy.insert(format!("mods:{id}"));
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    if let Some(sources) = sources {
                        for source in sources {
                            mods::add(&paths, &target, &source).await?;
                        }
                    }
                    if let Some(identity) = identity {
                        mods::remove(&paths, &target, &identity).await?;
                    }
                    mods::list(&paths, &target).await
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.busy.remove(&format!("mods:{id}"));
                v.invalidate_content(&id);
                let state = v.mods_ui.states.entry(id.clone()).or_default();
                state.busy = false;
                match result {
                    Ok(Ok(entries)) => {
                        state.entries = entries;
                        v.data.status = "Custom mods saved".into();
                    }
                    Ok(Err(e)) => {
                        state.error = Some(format!("{e:#}"));
                        state.loaded = false;
                    }
                    Err(e) => state.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn open_mods_folder(&mut self, id: String, cx: &mut Context<Self>) {
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { mods::open_folder(&paths, &target).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                if let Ok(Err(e)) = result {
                    v.mods_ui.states.entry(id).or_default().error = Some(format!("{e:#}"));
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn mods_view(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        if !self.mods_ui.states.entry(id.into()).or_default().loaded {
            self.reload_mods(id.into(), cx);
        }
        let state = &self.mods_ui.states[id];
        let busy = state.busy || self.instance_is_busy(id);
        let entries = state.entries.clone();
        let error = state.error.clone();
        let pending = state.remove.clone();
        let add = id.to_string();
        let folder = add.clone();
        let refresh = add.clone();
        let mut body=div().flex_1().min_h_0().child(div().id("custom-mods").size_full().overflow_y_scroll().flex().flex_col().p_3().gap_3()
            .child("Custom mods").child("These files are kept in persistent-minecraft/mods and restored after pack updates.")
            .child(div().flex().flex_wrap().gap_2()
                .child(btn(&self.theme,"Add mods",ButtonVariant::Primary).debug_selector(||"custom-mod-add".into()).tab_stop(!busy).when(busy,|b|b.opacity(0.5)).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|v.pick_mods(add.clone(),cx))))
                .child(btn(&self.theme,"Open mods folder",ButtonVariant::Outline).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|v.open_mods_folder(folder.clone(),cx))))
                .child(btn(&self.theme,"Refresh",ButtonVariant::Outline).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|v.reload_mods(refresh.clone(),cx)))))
            .children(entries.into_iter().map(|entry| {let target=id.to_string();let identity=entry.identity.clone();card(&self.theme).id(format!("custom-mod:{}",entry.identity)).flex_shrink_0().flex().items_center().gap_2().p_3()
                .child(div().flex_1().min_w_0().child(format!("{}\n{} / {}",entry.filename,entry.identity,format_bytes(entry.size_bytes))))
                .child(btn(&self.theme,"Remove",ButtonVariant::Outline).tab_stop(!busy).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|{if !busy {v.mods_ui.states.entry(target.clone()).or_default().remove=Some(identity.clone());cx.notify();}})))
            })));
        if let Some(identity) = pending {
            let target = id.to_string();
            let cancel = target.clone();
            body =
                body.child(
                    div()
                        .p_3()
                        .border_1()
                        .border_color(self.theme.border())
                        .child(format!(
                            "Remove {identity} from the custom overlay and active mods?"
                        ))
                        .child(
                            btn(&self.theme, "Remove mod", ButtonVariant::Danger)
                                .debug_selector(|| "custom-mod-confirm-remove".into())
                                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                    v.change_mods(target.clone(), None, Some(identity.clone()), cx)
                                })),
                        )
                        .child(btn(&self.theme, "Cancel", ButtonVariant::Ghost).on_click(
                            cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                v.mods_ui.states.entry(cancel.clone()).or_default().remove = None;
                                cx.notify();
                            }),
                        )),
                );
        }
        if let Some(error) = error {
            body = body.child(div().text_color(self.theme.destructive()).child(error));
        }
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::{instance_settings::DetailTab, test_support::Fixture};
    use gpui::TestAppContext;

    #[gpui::test]
    async fn picker_cancel_multi_import_and_confirmed_removal_preserve_pack_mods(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["pack"]);
        let game = fixture.paths.instance_dir("pack").unwrap();
        std::fs::create_dir_all(game.join(".minecraft/mods")).unwrap();
        std::fs::write(game.join(".minecraft/mods/original.jar"), b"pack-owned").unwrap();
        let jar = fixture.paths.base().join("custom-one.jar");
        let zip = fixture.paths.base().join("custom-two.zip");
        std::fs::write(&jar, b"generated jar").unwrap();
        std::fs::write(&zip, b"generated zip").unwrap();
        let (window, root) = fixture.open(cx, |v| {
            v.selected_id = Some("pack".into());
            v.instance_ui.detail_tab = DetailTab::Mods;
        });
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        fixture
            .until(cx, &root, |v| !v.mods_ui.states["pack"].busy)
            .await;
        Fixture::click(cx, window, "custom-mod-add");
        assert!(cx.did_prompt_for_paths());
        cx.simulate_path_prompt_response(|options| {
            assert!(options.files && options.multiple && !options.directories);
            None
        });
        fixture
            .until(cx, &root, |v| !v.mods_ui.states["pack"].busy)
            .await;
        assert!(!game.join("persistent-minecraft/mods").exists());
        Fixture::click(cx, window, "custom-mod-add");
        cx.simulate_path_prompt_response(|_| Some(vec![jar, zip]));
        fixture
            .until(cx, &root, |v| !v.mods_ui.states["pack"].busy)
            .await;
        assert_eq!(
            root.read_with(cx, |v, _| v.mods_ui.states["pack"].entries.len()),
            2
        );
        for (name, bytes) in [
            ("custom-one.jar", b"generated jar".as_slice()),
            ("custom-two.zip", b"generated zip".as_slice()),
        ] {
            assert_eq!(
                std::fs::read(game.join(".minecraft/mods").join(name)).unwrap(),
                bytes
            );
            assert_eq!(
                std::fs::read(game.join("persistent-minecraft/mods").join(name)).unwrap(),
                bytes
            );
        }
        root.update(cx, |v, _| {
            let identity = v.mods_ui.states["pack"].entries[0].identity.clone();
            v.mods_ui.states.get_mut("pack").unwrap().remove = Some(identity);
        });
        Fixture::click(cx, window, "custom-mod-confirm-remove");
        fixture
            .until(cx, &root, |v| !v.mods_ui.states["pack"].busy)
            .await;
        assert_eq!(
            fixture
                .rt
                .block_on(mods::list(&fixture.paths, "pack"))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            std::fs::read(game.join(".minecraft/mods/original.jar")).unwrap(),
            b"pack-owned"
        );
        root.update(cx, |v, cx| {
            v.busy.insert("update:pack".into());
            v.pick_mods("pack".into(), cx);
        });
        assert!(!cx.did_prompt_for_paths());
        fixture.cleanup();
    }
}
