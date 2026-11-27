use super::LauncherRoot;
use crate::{
    state::format_bytes,
    ui::{btn, card, section_title, segment, segmented_control, tint, ButtonVariant},
};
use gpui::{div, prelude::*, px, Context, Div, PathPromptOptions};
use industrialis_backend::{
    content::{self, ContentEntry, ContentKind},
    modernity::{self, ReleaseAsset, ReleaseCatalog},
    recommended::{self, Plan},
};
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum AddOnTab {
    #[default]
    Mods,
    Browse,
    ResourcePacks,
    Recommended,
}

#[derive(Default)]
pub(super) struct ContentUi {
    pub(super) tab: AddOnTab,
    states: HashMap<String, ContentState>,
    recommended: HashMap<String, RecommendedState>,
}
#[derive(Default)]
struct RecommendedState {
    plans: Vec<Result<Plan, String>>,
    selected: usize,
    installed: Vec<String>,
    loading: bool,
    loaded: bool,
    busy: bool,
    error: Option<String>,
}
#[derive(Default)]
struct ContentState {
    entries: Vec<ContentEntry>,
    mods: Vec<ContentEntry>,
    loaded: bool,
    busy: bool,
    browsing: bool,
    catalog: Option<ReleaseCatalog>,
    catalog_version: String,
    error: Option<String>,
    remove: Option<String>,
}
enum Change {
    Import(Vec<PathBuf>),
    Remove(String),
    Download(ReleaseAsset),
}

impl LauncherRoot {
    pub(super) fn invalidate_content(&mut self, id: &str) {
        if let Some(state) = self.content_ui.states.get_mut(id) {
            state.loaded = false;
        }
        self.mod_browser_ui.invalidate_target(id);
    }
    pub(super) fn add_ons_view(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        let tab = self.content_ui.tab;
        let tabs = div().flex().px_5().pt_4().pb_2().child(
            segmented_control(&self.theme)
                .child(
                    segment(&self.theme, "Mods", tab == AddOnTab::Mods)
                        .debug_selector(|| "content-mods-tab".into())
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                            v.content_ui.tab = AddOnTab::Mods;
                            cx.notify();
                        })),
                )
                .child(
                    segment(&self.theme, "Browse mods", tab == AddOnTab::Browse)
                        .debug_selector(|| "content-browse-tab".into())
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                            v.content_ui.tab = AddOnTab::Browse;
                            cx.notify();
                        })),
                )
                .child(
                    segment(
                        &self.theme,
                        "Resource packs",
                        tab == AddOnTab::ResourcePacks,
                    )
                    .debug_selector(|| "content-resources-tab".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                        v.content_ui.tab = AddOnTab::ResourcePacks;
                        cx.notify();
                    })),
                )
                .child(
                    segment(&self.theme, "Recommended", tab == AddOnTab::Recommended)
                        .debug_selector(|| "content-recommended-tab".into())
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                            v.content_ui.tab = AddOnTab::Recommended;
                            cx.notify();
                        })),
                ),
        );
        let body = match tab {
            AddOnTab::Mods => self.mods_view(id, cx),
            AddOnTab::Browse => self.mod_browser_view(id, cx),
            AddOnTab::ResourcePacks => self.resource_packs_view(id, cx),
            AddOnTab::Recommended => self.recommended_add_ons_view(id, cx),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(tabs)
            .child(body)
    }

    fn reload_recommended(&mut self, id: String, cx: &mut Context<Self>) {
        let state = self.content_ui.recommended.entry(id.clone()).or_default();
        if state.loading {
            return;
        }
        state.loading = true;
        state.error = None;
        let pack_version = self
            .data
            .selected_instance(Some(&id))
            .map(|instance| instance.settings.pack_version.clone())
            .unwrap_or_default();
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let list_id = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let plans = recommended::plans(&pack_version).await;
                    let installed = content::list(&paths, &list_id, ContentKind::Mod)
                        .await
                        .map(|entries| {
                            entries
                                .into_iter()
                                .map(|entry| entry.filename.to_lowercase())
                                .collect::<Vec<_>>()
                        });
                    (plans, installed)
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.content_ui.recommended.entry(id).or_default();
                state.loading = false;
                // One attempt per instance; Refresh retries. A failing GitHub
                // request must not re-run on every frame.
                state.loaded = true;
                match result {
                    Ok((plans, installed)) => {
                        state.plans = plans;
                        match installed {
                            Ok(names) => state.installed = names,
                            Err(error) => state.error = Some(format!("{error:#}")),
                        }
                    }
                    Err(error) => state.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn install_recommended(&mut self, id: String, plan: Plan, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        let state = self.content_ui.recommended.entry(id.clone()).or_default();
        if state.busy {
            return;
        }
        state.busy = true;
        state.error = None;
        self.busy.insert(format!("recommended:{id}"));
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let install_id = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let count = recommended::install(&paths, &install_id, &plan).await?;
                    let installed = content::list(&paths, &install_id, ContentKind::Mod)
                        .await?
                        .into_iter()
                        .map(|entry| entry.filename.to_lowercase())
                        .collect::<Vec<_>>();
                    Ok::<_, anyhow::Error>((count, installed))
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.busy.remove(&format!("recommended:{id}"));
                v.invalidate_mods(&id);
                v.invalidate_content(&id);
                let state = v.content_ui.recommended.entry(id).or_default();
                state.busy = false;
                match result {
                    Ok(Ok((count, installed))) => {
                        state.installed = installed;
                        v.data.status = if count == 0 {
                            "Add-on already installed.".into()
                        } else {
                            "Recommended add-on installed. Restart the game to load it.".into()
                        };
                    }
                    Ok(Err(error)) => state.error = Some(format!("{error:#}")),
                    Err(error) => state.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn recommended_add_ons_view(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        if !self
            .content_ui
            .recommended
            .entry(id.into())
            .or_default()
            .loaded
        {
            self.reload_recommended(id.into(), cx);
        }
        let state = &self.content_ui.recommended[id];
        let loading = state.loading;
        let busy = state.busy || self.instance_is_busy(id);
        let plans = state.plans.clone();
        let installed = state.installed.clone();
        let error = state.error.clone();
        let selected = state.selected.min(plans.len().saturating_sub(1));
        let refresh = id.to_string();
        let theme = &self.theme;
        let mut list = div()
            .id("recommended-list")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if loading && plans.is_empty() {
            list = list.child(
                div()
                    .p_4()
                    .text_color(theme.muted_foreground())
                    .child("Loading releases…"),
            );
        }
        for (index, (addon, plan)) in recommended::CATALOG.iter().zip(plans.iter()).enumerate() {
            let chosen = index == selected;
            let offered = plan.is_ok();
            let target = id.to_string();
            let selector = addon.name.to_string();
            let monogram = addon
                .name
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string();
            let description = if offered {
                addon.description.to_string()
            } else {
                "Not offered for this instance".to_string()
            };
            list = list.child(
                div()
                    .id(format!("recommended-row:{}", addon.name))
                    .debug_selector(move || format!("recommended-row:{selector}"))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(tint(theme.border(), 0.45))
                    .when(chosen, |row| row.bg(tint(theme.primary(), 0.16)))
                    .when(!offered, |row| row.opacity(0.72))
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        v.content_ui
                            .recommended
                            .entry(target.clone())
                            .or_default()
                            .selected = index;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .size(px(36.0))
                            .flex_shrink_0()
                            .rounded(px(9.0))
                            .bg(tint(theme.primary(), 0.24))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(div().text_color(theme.primary()).child(monogram)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().child(addon.name))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground())
                                    .child(description),
                            ),
                    ),
            );
        }
        let mut detail = div()
            .w(px(420.0))
            .flex_shrink_0()
            .min_h_0()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(theme.border());
        let mut install: Option<(String, Plan)> = None;
        let mut install_label = String::new();
        let mut install_disabled = true;
        let mut status = error.clone().unwrap_or_default();
        let mut status_danger = error.is_some();
        if let (Some(addon), Some(plan)) = (recommended::CATALOG.get(selected), plans.get(selected)) {
            match plan {
                Ok(plan) => {
                    let primary = plan.mod_asset();
                    let primary_installed = installed.contains(&primary.name.to_lowercase());
                    let all_installed = plan
                        .assets
                        .iter()
                        .all(|asset| installed.contains(&asset.name.to_lowercase()));
                    let other_version = !primary_installed
                        && installed
                            .iter()
                            .any(|name| recommended::same_project(name, &primary.name));
                    install_disabled = all_installed || other_version || busy;
                    install_label = if all_installed {
                        "Installed".into()
                    } else if other_version {
                        "Other version installed".into()
                    } else if busy {
                        "Installing…".into()
                    } else {
                        "Install".into()
                    };
                    if !status_danger {
                        status = if other_version {
                            "Another version is already installed in this instance.".into()
                        } else if all_installed {
                            "Installed in this instance. Restart the game to load it.".into()
                        } else {
                            format!("Pinned for GTNH {} · {}", plan.series, primary.release)
                        };
                    }
                    let companions: Vec<String> = plan
                        .companions()
                        .iter()
                        .map(|asset| asset.name.clone())
                        .collect();
                    let mut assets = div().flex().flex_col().gap_1();
                    for asset in &plan.assets {
                        assets = assets.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground())
                                        .child(asset.name.clone()),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground())
                                        .child(format_bytes(asset.size)),
                                ),
                        );
                    }
                    let page = plan.page.clone();
                    detail = detail
                        .child(
                            div()
                                .px_4()
                                .py_3()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .border_b_1()
                                .border_color(theme.border())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(div().text_lg().child(addon.name))
                                        .child(div().flex_1())
                                        .child(
                                            btn(theme, "Releases", ButtonVariant::Ghost).on_click(
                                                move |_: &gpui::ClickEvent, _, cx| {
                                                    cx.open_url(&page)
                                                },
                                            ),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground())
                                        .child(addon.repo),
                                ),
                        )
                        .child(
                            div()
                                .id("recommended-detail")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .p_4()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(section_title(theme, "About"))
                                .child(addon.description)
                                .when(!companions.is_empty(), |body| {
                                    body.child(section_title(theme, "Also installs"))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground())
                                                .child(companions.join(", ")),
                                        )
                                })
                                .child(section_title(theme, "Release files"))
                                .child(assets),
                        );
                    install = Some((id.to_string(), plan.clone()));
                }
                Err(reason) => {
                    status = reason.clone();
                    status_danger = true;
                    let page = format!("https://github.com/{}/releases", addon.repo);
                    detail = detail
                        .child(
                            div()
                                .px_4()
                                .py_3()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .border_b_1()
                                .border_color(theme.border())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(div().text_lg().child(addon.name))
                                        .child(div().flex_1())
                                        .child(
                                            btn(theme, "Releases", ButtonVariant::Ghost).on_click(
                                                move |_: &gpui::ClickEvent, _, cx| {
                                                    cx.open_url(&page)
                                                },
                                            ),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground())
                                        .child(addon.repo),
                                ),
                        )
                        .child(
                            div()
                                .p_4()
                                .child(div().text_color(theme.destructive()).child(reason.clone())),
                        );
                }
            }
        } else {
            detail = detail.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.muted_foreground())
                    .child(if loading {
                        "Loading releases…"
                    } else {
                        "No recommended add-ons for this instance."
                    }),
            );
        }
        let mut bar = div()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(theme.border())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(if status_danger {
                        theme.destructive()
                    } else {
                        theme.muted_foreground()
                    })
                    .child(status),
            )
            .child(
                btn(theme, "Refresh", ButtonVariant::Ghost)
                    .debug_selector(|| "recommended-refresh".into())
                    .tab_stop(!loading)
                    .when(loading, |b| b.opacity(0.5))
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        v.reload_recommended(refresh.clone(), cx)
                    })),
            );
        if let Some((target, plan)) = install {
            let name = plan.addon.name.to_string();
            bar = bar.child(
                btn(theme, &install_label, ButtonVariant::Secondary)
                    .debug_selector(move || format!("recommended-install:{name}"))
                    .tab_stop(!install_disabled)
                    .when(install_disabled, |b| b.opacity(0.5))
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if !install_disabled {
                            v.install_recommended(target.clone(), plan.clone(), cx);
                        }
                    })),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(div().flex_1().min_h_0().flex().child(list).child(detail))
            .child(bar)
    }

    fn reload_resources(&mut self, id: String, cx: &mut Context<Self>) {
        let state = self.content_ui.states.entry(id.clone()).or_default();
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
                .spawn(async move { load_content(&paths, &target).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.content_ui.states.entry(id).or_default();
                state.busy = false;
                match result {
                    Ok(Ok((entries, mods))) => {
                        state.entries = entries;
                        state.mods = mods;
                    }
                    Ok(Err(e)) => state.error = Some(format!("{e:#}")),
                    Err(e) => state.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn pick_resources(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) || self.content_ui.states.get(&id).is_some_and(|s| s.busy) {
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Select resource-pack ZIP files".into()),
        });
        self.content_ui.states.entry(id.clone()).or_default().busy = true;
        cx.spawn(async move |view, cx| {
            let result = picker.await;
            let _ = view.update(cx, |v, cx| {
                v.content_ui.states.entry(id.clone()).or_default().busy = false;
                match result {
                    Ok(Ok(Some(paths))) => v.change_content(id, Change::Import(paths), cx),
                    Ok(Err(e)) => {
                        v.content_ui.states.entry(id).or_default().error = Some(e.to_string())
                    }
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn browse_modernity(&mut self, id: String, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        let version = self
            .data
            .selected_instance(Some(&id))
            .map(|i| i.settings.pack_version.clone())
            .unwrap_or_default();
        let state = self.content_ui.states.entry(id.clone()).or_default();
        if state.browsing {
            return;
        }
        state.browsing = true;
        state.error = None;
        let rt = self.data.rt.clone();
        let requested = version.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { modernity::catalog(&requested).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.content_ui.states.entry(id.clone()).or_default();
                state.browsing = false;
                let current = v
                    .data
                    .selected_instance(Some(&id))
                    .map(|i| i.settings.pack_version.as_str())
                    .unwrap_or_default();
                if current != version {
                    state.catalog = None;
                    state.error =
                        Some("The instance version changed. Browse releases again.".into());
                } else {
                    match result {
                        Ok(Ok(catalog)) => {
                            state.catalog = Some(catalog);
                            state.catalog_version = version;
                        }
                        Ok(Err(e)) => state.error = Some(format!("{e:#}")),
                        Err(e) => state.error = Some(e.to_string()),
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn change_content(&mut self, id: String, change: Change, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        let state = self.content_ui.states.entry(id.clone()).or_default();
        if state.busy {
            return;
        }
        state.busy = true;
        state.error = None;
        state.remove = None;
        self.busy.insert(format!("content:{id}"));
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt.spawn(async move {
                match change {
                    Change::Import(sources) => { for source in sources { content::import(&paths, &target, ContentKind::ResourcePack, &source).await?; } },
                    Change::Remove(name) => content::remove(&paths, &target, ContentKind::ResourcePack, &name).await?,
                    Change::Download(asset) => modernity::install(&paths, &target, asset).await?,
                }
                load_content(&paths, &target).await
            }).await;
            let _ = view.update(cx, |v, cx| {
                v.busy.remove(&format!("content:{id}"));
                // MyCTMLib downloads appear in the existing custom Mods manager.
                v.invalidate_mods(&id);
                let state = v.content_ui.states.entry(id).or_default(); state.busy = false;
                match result { Ok(Ok((entries, mods))) => { state.entries = entries; state.mods = mods; v.data.status = "Add-ons saved. Select resource packs in Minecraft Options > Resource Packs.".into(); }, Ok(Err(e)) => { state.error = Some(format!("{e:#}")); state.loaded = false; }, Err(e) => state.error = Some(e.to_string()) }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn open_resources(&mut self, id: String, cx: &mut Context<Self>) {
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    content::open_folder(&paths, &target, ContentKind::ResourcePack).await
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                match result {
                    Ok(Err(e)) => {
                        v.content_ui.states.entry(id).or_default().error = Some(format!("{e:#}"))
                    }
                    Err(e) => v.data.status = e.to_string(),
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn resource_packs_view(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        if !self.content_ui.states.entry(id.into()).or_default().loaded {
            self.reload_resources(id.into(), cx);
        }
        let state = &self.content_ui.states[id];
        let busy = state.busy || self.instance_is_busy(id);
        let browsing = state.browsing;
        let entries = state.entries.clone();
        let installed_mods = state.mods.clone();
        let error = state.error.clone();
        let pending = state.remove.clone();
        let version = self
            .data
            .selected_instance(Some(id))
            .map(|i| i.settings.pack_version.as_str())
            .unwrap_or_default();
        let catalog = if version == state.catalog_version {
            state.catalog.clone()
        } else {
            None
        };
        let add = id.to_owned();
        let folder = add.clone();
        let refresh = add.clone();
        let browse = add.clone();
        let mut body = div().id("resource-packs").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().px_5().py_3().gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(section_title(&self.theme, "Resource packs"))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(self.theme.muted_foreground())
                            .child("Installed packs survive GTNH updates. Enable and arrange them in Minecraft Options > Resource Packs."),
                    ),
            )
            .child(div().flex().flex_wrap().gap_2()
                .child(btn(&self.theme,"Import ZIP",ButtonVariant::Primary).debug_selector(|| "resource-pack-add".into()).tab_stop(!busy).when(busy,|b|b.opacity(0.5)).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|v.pick_resources(add.clone(),cx))))
                .child(btn(&self.theme,"Open folder",ButtonVariant::Outline).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|v.open_resources(folder.clone(),cx))))
                .child(btn(&self.theme,"Refresh",ButtonVariant::Outline).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|v.reload_resources(refresh.clone(),cx)))))
            .children(entries.iter().map(|entry| {
                let target = id.to_owned(); let name = entry.filename.clone();
                let mut row = card(&self.theme).id(format!("resource-pack-row:{}",entry.filename)).flex_shrink_0().flex().items_center().gap_3().px_4().py_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(div().text_size(px(13.0)).font_weight(gpui::FontWeight::MEDIUM).child(entry.filename.clone()))
                            .child(div().text_size(px(12.0)).text_color(self.theme.muted_foreground()).child(format!("{}{}",format_bytes(entry.size_bytes),entry.source.as_ref().map(|s|format!("  {}",s.release)).unwrap_or_default()))),
                    );
                if entry.managed { row = row.child(btn(&self.theme,"Remove",ButtonVariant::Outline).tab_stop(!busy).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|{if !busy {v.content_ui.states.entry(target.clone()).or_default().remove=Some(name.clone()); cx.notify();}}))); }
                else { row = row.child(div().text_size(px(12.0)).text_color(self.theme.muted_foreground()).child("Existing pack")); }
                row
            }))
            .when(entries.is_empty(), |b| b.child(div().text_size(px(13.0)).text_color(self.theme.muted_foreground()).child("No resource packs installed.")))
            .child(
                div()
                    .mt_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(section_title(&self.theme, "Modernity GTNH"))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(self.theme.muted_foreground())
                            .child("Weekly builds with optional UI packs, textures and MyCTMLib. Install the main pack first; put it at the bottom of the selected pack list, with add-ons above it. Choose one UI style and one MyCTMLib version."),
                    ),
            )
            .child(btn(&self.theme, if browsing {"Loading releases..."} else {"Browse Modernity releases"}, ButtonVariant::Outline).flex_shrink_0()
                .debug_selector(|| "modernity-browse".into()).tab_stop(!browsing).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|v.browse_modernity(browse.clone(),cx))));
        if let Some(name) = pending {
            let target = id.to_owned();
            let cancel = target.clone();
            body = body.child(div().p_3().border_1().border_color(self.theme.border()).child(format!("Remove {name} from this instance? Disable it in Minecraft first if selected."))
                .child(btn(&self.theme,"Remove pack",ButtonVariant::Danger).debug_selector(|| "resource-pack-confirm-remove".into()).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|v.change_content(target.clone(),Change::Remove(name.clone()),cx))))
                .child(btn(&self.theme,"Cancel",ButtonVariant::Ghost).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|{v.content_ui.states.entry(cancel.clone()).or_default().remove=None;cx.notify();}))));
        }
        if let Some(error) = error {
            body = body.child(div().text_color(self.theme.destructive()).child(error));
        }
        if busy {
            body =
                body.child(div().child("Working... downloads are verified before installation."));
        }
        if let Some(catalog) = catalog {
            let page = catalog.page.clone();
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(catalog.tag)
                    .child(
                        btn(&self.theme, "Release notes", ButtonVariant::Ghost)
                            .on_click(move |_: &gpui::ClickEvent, _, cx| cx.open_url(&page)),
                    ),
            );
            for asset in catalog.assets {
                let target = id.to_owned();
                let name = asset.name.clone();
                let selector = name.clone();
                let control_id = format!("modernity-install:{}", asset.name);
                let installed = match asset.kind {
                    ContentKind::ResourcePack => entries
                        .iter()
                        .any(|e| e.filename.eq_ignore_ascii_case(&name)),
                    ContentKind::Mod => installed_mods.iter().any(|e| {
                        industrialis_backend::pack_files::mod_identity_from_filename(&e.filename)
                            == industrialis_backend::pack_files::mod_identity_from_filename(&name)
                    }),
                };
                let different_mod_version = installed
                    && asset.kind == ContentKind::Mod
                    && !installed_mods
                        .iter()
                        .any(|e| e.filename.eq_ignore_ascii_case(&name));
                let label = if different_mod_version {
                    "Other version installed"
                } else if installed {
                    "Installed"
                } else if asset.kind == ContentKind::Mod {
                    "Install mod"
                } else {
                    "Install pack"
                };
                body = body.child(
                    card(&self.theme)
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .p_3()
                        .child(
                            div().flex_1().min_w_0().child(name).child(
                                div()
                                    .text_color(self.theme.muted_foreground())
                                    .child(format!(
                                        "{} / {}",
                                        if asset.kind == ContentKind::Mod {
                                            "Optional connected-texture mod"
                                        } else {
                                            "Resource pack"
                                        },
                                        format_bytes(asset.size)
                                    )),
                            ),
                        )
                        .child(
                            btn(&self.theme, label, ButtonVariant::Secondary)
                                .id(control_id)
                                .debug_selector(move || format!("modernity-install:{selector}"))
                                .tab_stop(!busy && !installed)
                                .when(busy || installed, |b| b.opacity(0.5))
                                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                    if !installed {
                                        v.change_content(
                                            target.clone(),
                                            Change::Download(asset.clone()),
                                            cx,
                                        );
                                    }
                                })),
                        ),
                );
            }
        }
        div().flex_1().min_h_0().child(body.size_full())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::{instance_settings::DetailTab, test_support::Fixture};
    use gpui::TestAppContext;
    use std::io::{Cursor, Write};
    #[gpui::test]
    async fn separate_release_buttons_dispatch_the_selected_asset(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["pack"]);
        let tag = "weekly-2.9.x-2026-10-12";
        let first = ReleaseAsset {
            name: "Modernity-first.zip".into(),
            kind: ContentKind::ResourcePack,
            url: "https://example.invalid".into(),
            sha256: "a".repeat(64),
            size: 1,
            release: tag.into(),
            series: "2.9".into(),
        };
        let second = ReleaseAsset {
            name: "Modernity-second.zip".into(),
            kind: ContentKind::ResourcePack,
            url: format!(
                "{}/releases/download/{tag}/Modernity-second.zip",
                modernity::PROJECT_URL
            ),
            sha256: "a".repeat(64),
            size: 0,
            release: tag.into(),
            series: "2.9".into(),
        };
        let (window, root) =
            fixture.open_sized(cx, gpui::size(gpui::px(1600.), gpui::px(1100.)), |v| {
                v.selected_id = Some("pack".into());
                v.instance_ui.detail_tab = DetailTab::Mods;
                v.data.instances[0].settings.pack_version = "2.9.0-RC-2".into();
                v.content_ui.tab = AddOnTab::ResourcePacks;
                v.content_ui.states.insert(
                    "pack".into(),
                    ContentState {
                        loaded: true,
                        catalog_version: "2.9.0-RC-2".into(),
                        catalog: Some(ReleaseCatalog {
                            tag: tag.into(),
                            page: modernity::PROJECT_URL.into(),
                            assets: vec![first, second],
                        }),
                        ..Default::default()
                    },
                );
            });
        Fixture::click(cx, window, "modernity-install:Modernity-first.zip");
        fixture
            .until(cx, &root, |v| !v.content_ui.states["pack"].busy)
            .await;
        assert!(root.read_with(cx, |v, _| v.content_ui.states["pack"]
            .error
            .as_ref()
            .unwrap()
            .contains("Unexpected release asset URL")));
        // Allow the post-failure inventory refresh before another real button.
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        fixture
            .until(cx, &root, |v| !v.content_ui.states["pack"].busy)
            .await;
        Fixture::click(cx, window, "modernity-install:Modernity-second.zip");
        fixture
            .until(cx, &root, |v| !v.content_ui.states["pack"].busy)
            .await;
        assert!(root.read_with(cx, |v, _| v.content_ui.states["pack"]
            .error
            .as_ref()
            .unwrap()
            .contains("Invalid release checksum or size")));
        assert!(!fixture
            .paths
            .instance_dir("pack")
            .unwrap()
            .join("persistent-minecraft/resourcepacks")
            .exists());
        fixture.cleanup();
    }
    #[gpui::test]
    async fn resource_picker_import_invalid_file_and_removal_use_real_disk(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["pack"]);
        let game = fixture.paths.instance_dir("pack").unwrap();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("pack.mcmeta", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"{"pack":{"pack_format":1}}"#).unwrap();
        let source = fixture.paths.base().join("textures.zip");
        std::fs::write(&source, zip.finish().unwrap().into_inner()).unwrap();
        let invalid = fixture.paths.base().join("bad.zip");
        std::fs::write(&invalid, b"invalid").unwrap();
        let (window, root) = fixture.open(cx, |v| {
            v.selected_id = Some("pack".into());
            v.instance_ui.detail_tab = DetailTab::Mods;
        });
        Fixture::click(cx, window, "content-resources-tab");
        fixture
            .until(cx, &root, |v| {
                v.content_ui.states.get("pack").is_some_and(|s| !s.busy)
            })
            .await;
        Fixture::click(cx, window, "resource-pack-add");
        cx.simulate_path_prompt_response(|opts| {
            assert!(opts.files && opts.multiple && !opts.directories);
            None
        });
        fixture
            .until(cx, &root, |v| !v.content_ui.states["pack"].busy)
            .await;
        Fixture::click(cx, window, "resource-pack-add");
        cx.simulate_path_prompt_response(|_| Some(vec![source]));
        fixture
            .until(cx, &root, |v| !v.content_ui.states["pack"].busy)
            .await;
        assert!(game.join(".minecraft/resourcepacks/textures.zip").is_file());
        assert!(game
            .join("persistent-minecraft/resourcepacks/textures.zip")
            .is_file());
        Fixture::click(cx, window, "resource-pack-add");
        cx.simulate_path_prompt_response(|_| Some(vec![invalid]));
        fixture
            .until(cx, &root, |v| !v.content_ui.states["pack"].busy)
            .await;
        assert!(root.read_with(cx, |v, _| v.content_ui.states["pack"].error.is_some()));
        assert!(!game.join(".minecraft/resourcepacks/bad.zip").exists());
        root.update(cx, |v, _| {
            v.content_ui.states.get_mut("pack").unwrap().remove = Some("textures.zip".into())
        });
        Fixture::click(cx, window, "resource-pack-confirm-remove");
        fixture
            .until(cx, &root, |v| !v.content_ui.states["pack"].busy)
            .await;
        assert!(!game.join(".minecraft/resourcepacks/textures.zip").exists());
        root.update(cx, |v, cx| {
            v.busy.insert("update:pack".into());
            v.pick_resources("pack".into(), cx);
        });
        assert!(!cx.did_prompt_for_paths());
        fixture.cleanup();
    }

    fn pinned_plan(addon: &'static recommended::AddOn) -> Plan {
        Plan {
            addon,
            series: "2.9".into(),
            page: format!("https://example.invalid/{}", addon.name),
            assets: vec![recommended::Asset {
                repo: addon.repo,
                name: format!("{}-pinned.jar", addon.prefix),
                url: "https://example.invalid/pinned.jar".into(),
                sha256: "a".repeat(64),
                size: 1,
                release: "pinned".into(),
            }],
        }
    }

    #[gpui::test]
    async fn recommended_tab_switches_the_detail_pane_and_installs_the_selection(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["pack"]);
        let (window, _root) =
            fixture.open_sized(cx, gpui::size(gpui::px(1600.), gpui::px(1100.)), |v| {
                v.selected_id = Some("pack".into());
                v.instance_ui.detail_tab = DetailTab::Mods;
                v.data.instances[0].settings.pack_version = "2.9.0-RC-2".into();
                v.content_ui.tab = AddOnTab::Recommended;
                v.content_ui.recommended.insert(
                    "pack".into(),
                    RecommendedState {
                        loaded: true,
                        plans: vec![
                            Ok(pinned_plan(&recommended::CATALOG[0])),
                            Ok(pinned_plan(&recommended::CATALOG[1])),
                            Err("Bandit is not offered for GTNH 2.9. Available for 2.8.".into()),
                        ],
                        ..Default::default()
                    },
                );
            });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
            for selector in [
                "recommended-row:GTNotLeisure",
                "recommended-row:GTNHRates",
                "recommended-row:Bandit",
            ] {
                assert!(
                    visual.debug_bounds(selector).is_some(),
                    "{selector} did not render"
                );
            }
            assert!(visual.debug_bounds("recommended-install:GTNotLeisure").is_some());
            assert!(visual.debug_bounds("recommended-install:GTNHRates").is_none());
        }
        Fixture::click(cx, window, "recommended-row:GTNHRates");
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
            assert!(visual.debug_bounds("recommended-install:GTNHRates").is_some());
            assert!(visual.debug_bounds("recommended-install:GTNotLeisure").is_none());
        }
        Fixture::click(cx, window, "recommended-row:Bandit");
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
            assert!(visual.debug_bounds("recommended-install:GTNHRates").is_none());
        }
        fixture.cleanup();
    }
}

async fn load_content(
    paths: &industrialis_backend::paths::LauncherPaths,
    id: &str,
) -> anyhow::Result<(Vec<ContentEntry>, Vec<ContentEntry>)> {
    let entries = content::list(paths, id, ContentKind::ResourcePack).await?;
    let mods = content::list(paths, id, ContentKind::Mod).await?;
    Ok((entries, mods))
}
