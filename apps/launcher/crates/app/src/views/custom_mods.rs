use super::LauncherRoot;
use crate::{
    input::FieldState,
    state::format_bytes,
    ui::{btn, card, checkbox, section_title, tint, ButtonVariant},
};
use gpui::{div, prelude::*, Context, Div, PathPromptOptions};
use industrialis_backend::{
    custom_mods as mods,
    mod_toggles::{enabled_filename, InstalledMod},
    pack_files::ModEntry,
};
use std::{collections::HashMap, path::PathBuf};

pub(super) struct ModsUi {
    states: HashMap<String, ModState>,
    pub(super) search: FieldState,
    pub(super) search_owner: String,
}

impl Default for ModsUi {
    fn default() -> Self {
        Self {
            states: HashMap::new(),
            search: FieldState::empty(),
            search_owner: String::new(),
        }
    }
}

#[derive(Default)]
struct ModState {
    entries: Vec<ModEntry>,
    pack_entries: Vec<InstalledMod>,
    loaded: bool,
    busy: bool,
    error: Option<String>,
    remove_custom: Option<String>,
}

#[derive(Clone)]
enum ModOp {
    Add(Vec<PathBuf>),
    RemoveCustom(String),
    SetEnabled { identity: String, enabled: bool },
}

impl ModOp {
    fn status(&self) -> &'static str {
        match self {
            ModOp::Add(_) => "Custom mods saved",
            ModOp::RemoveCustom(_) => "Custom mod removed",
            ModOp::SetEnabled { enabled: true, .. } => "Mod enabled",
            ModOp::SetEnabled { enabled: false, .. } => "Mod disabled",
        }
    }
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
                .spawn(async move {
                    let custom = mods::list(&paths, &target).await?;
                    let pack = mods::list_pack(&paths, &target).await?;
                    Ok::<_, anyhow::Error>((custom, pack))
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.mods_ui.states.entry(id).or_default();
                state.busy = false;
                match result {
                    Ok(Ok((custom, pack))) => {
                        state.entries = custom;
                        state.pack_entries = pack;
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
        let op = match (sources, identity) {
            (Some(sources), _) => ModOp::Add(sources),
            (None, Some(identity)) => ModOp::RemoveCustom(identity),
            (None, None) => return,
        };
        self.run_mod_op(id, op, cx);
    }

    fn run_mod_op(&mut self, id: String, op: ModOp, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        let state = self.mods_ui.states.entry(id.clone()).or_default();
        if state.busy {
            return;
        }
        state.busy = true;
        state.error = None;
        state.remove_custom = None;
        let status = op.status();
        self.busy.insert(format!("mods:{id}"));
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    match op {
                        ModOp::Add(sources) => {
                            for source in sources {
                                mods::add(&paths, &target, &source).await?;
                            }
                        }
                        ModOp::RemoveCustom(identity) => {
                            mods::remove(&paths, &target, &identity).await?
                        }
                        ModOp::SetEnabled { identity, enabled } => {
                            mods::set_enabled(&paths, &target, &identity, enabled).await?;
                        }
                    }
                    let custom = mods::list(&paths, &target).await?;
                    let pack = mods::list_pack(&paths, &target).await?;
                    Ok::<_, anyhow::Error>((custom, pack))
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.busy.remove(&format!("mods:{id}"));
                v.invalidate_content(&id);
                let state = v.mods_ui.states.entry(id.clone()).or_default();
                state.busy = false;
                match result {
                    Ok(Ok((custom, pack))) => {
                        state.entries = custom;
                        state.pack_entries = pack;
                        v.data.status = status.into();
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
        if self.mods_ui.search_owner != id {
            self.mods_ui.search = FieldState::empty();
            self.mods_ui.search_owner = id.to_string();
        }
        if !self.mods_ui.states.entry(id.into()).or_default().loaded {
            self.reload_mods(id.into(), cx);
        }
        let state = &self.mods_ui.states[id];
        let busy = state.busy || self.instance_is_busy(id);
        let entries = state.entries.clone();
        let pack_entries = state.pack_entries.clone();
        let error = state.error.clone();
        let pending = state.remove_custom.clone();
        let search = self.mods_ui.search.text.trim().to_lowercase();
        let pack_entries: Vec<InstalledMod> = if search.is_empty() {
            pack_entries
        } else {
            pack_entries
                .into_iter()
                .filter(|entry| {
                    entry.filename.to_lowercase().contains(&search)
                        || entry.identity.contains(&search)
                })
                .collect()
        };
        let pack_empty = pack_entries.is_empty();
        let search_field = self.field("mods-search", "Search mods", cx);
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
                .child(btn(&self.theme,"Remove",ButtonVariant::Outline).tab_stop(!busy).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|{if !busy {v.mods_ui.states.entry(target.clone()).or_default().remove_custom=Some(identity.clone());cx.notify();}})))
            }))
            .child(section_title(&self.theme, "Pack mods"))
            .child("Uncheck a mod to disable it for this instance. The file stays on disk and stays disabled after pack updates.")
            .child(search_field)
            .children(pack_entries.into_iter().map(|entry| {let target=id.to_string();let identity=entry.identity.clone();let selector_identity=identity.clone();let enabled=entry.enabled;let label=enabled_filename(&entry.filename).unwrap_or(&entry.filename);div().id(format!("pack-mod:{}",entry.identity)).flex_shrink_0().flex().items_center().gap_3().p_3().border_b_1().border_color(tint(self.theme.border(),0.28))
                .child(checkbox(&self.theme,label,enabled).debug_selector(move||format!("pack-mod-toggle:{}",selector_identity)).when(busy,|c|c.opacity(0.5)).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|{if !busy {v.run_mod_op(target.clone(),ModOp::SetEnabled{identity:identity.clone(),enabled:!enabled},cx);}})))
                .child(div().flex_1())
                .child(div().text_right().text_xs().text_color(self.theme.muted_foreground()).child(format!("{} / {}",entry.identity,format_bytes(entry.size_bytes))))
                .when(!enabled,|row|row.child(div().text_xs().text_color(self.theme.muted_foreground()).child("Disabled")))
            }))
            .when(pack_empty, |body| body.child(if search.is_empty() {"No pack mods installed."} else {"No pack mods match this search."})));
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
                                    v.run_mod_op(target.clone(), ModOp::RemoveCustom(identity.clone()), cx)
                                })),
                        )
                        .child(btn(&self.theme, "Cancel", ButtonVariant::Ghost).on_click(
                            cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                v.mods_ui.states.entry(cancel.clone()).or_default().remove_custom = None;
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
    async fn picker_cancel_import_removal_and_pack_mod_toggle_use_real_disk(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["pack"]);
        let game = fixture.paths.instance_dir("pack").unwrap();
        std::fs::create_dir_all(game.join(".minecraft/mods")).unwrap();
        std::fs::write(game.join(".minecraft/mods/original.jar"), b"pack-owned").unwrap();
        std::fs::write(
            game.join(".minecraft/mods/DefaultServerList-1.0.jar"),
            b"pack-owned mod",
        )
        .unwrap();
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
        assert_eq!(
            root.read_with(cx, |v, _| v.mods_ui.states["pack"].pack_entries.len()),
            2
        );
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
            v.mods_ui.states.get_mut("pack").unwrap().remove_custom = Some(identity);
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

        Fixture::click(cx, window, "pack-mod-toggle:defaultserverlist");
        fixture
            .until(cx, &root, |v| !v.mods_ui.states["pack"].busy)
            .await;
        assert!(game
            .join(".minecraft/mods/DefaultServerList-1.0.jar.disabled")
            .exists());
        assert!(!game.join(".minecraft/mods/DefaultServerList-1.0.jar").exists());
        assert_eq!(
            root.read_with(cx, |v, _| {
                v.mods_ui.states["pack"]
                    .pack_entries
                    .iter()
                    .filter(|entry| entry.enabled)
                    .count()
            }),
            1
        );
        assert!(!root.read_with(cx, |v, _| {
            v.mods_ui.states["pack"]
                .pack_entries
                .iter()
                .find(|entry| entry.identity == "defaultserverlist")
                .unwrap()
                .enabled
        }));

        Fixture::click(cx, window, "pack-mod-toggle:defaultserverlist");
        fixture
            .until(cx, &root, |v| !v.mods_ui.states["pack"].busy)
            .await;
        assert!(game.join(".minecraft/mods/DefaultServerList-1.0.jar").exists());
        assert!(!game
            .join(".minecraft/mods/DefaultServerList-1.0.jar.disabled")
            .exists());
        assert!(root.read_with(cx, |v, _| {
            v.mods_ui.states["pack"]
                .pack_entries
                .iter()
                .find(|entry| entry.identity == "defaultserverlist")
                .unwrap()
                .enabled
        }));
        root.update(cx, |v, cx| {
            v.busy.insert("update:pack".into());
            v.pick_mods("pack".into(), cx);
        });
        assert!(!cx.did_prompt_for_paths());
        fixture.cleanup();
    }
}
