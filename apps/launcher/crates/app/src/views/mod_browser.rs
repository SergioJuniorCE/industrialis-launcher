//! Modrinth browser for the Add-ons tab. Network work runs on the backend
//! runtime; every instance keeps its own search, selection and plan state.
use super::LauncherRoot;
use crate::{
    input::{NativeField, NativeFieldEvent},
    state::format_bytes,
    ui::{btn, card, ButtonVariant},
};
use gpui::{div, prelude::*, px, Context, Div, Entity};
use industrialis_backend::{
    game_target::{self, GameTarget},
    modrinth::{self, InstallPlan, Project, Selection, Version},
};
use std::collections::HashMap;

const VISIBLE_VERSIONS: usize = 10;
const MAX_OFFSET: usize = 1000;

#[derive(Default)]
pub(super) struct ModBrowserUi {
    states: HashMap<String, BrowserState>,
}
impl ModBrowserUi {
    pub(super) fn invalidate_target(&mut self, id: &str) {
        if let Some(state) = self.states.get_mut(id) {
            state.target = None;
            state.target_loading = false;
            state.target_attempted = false;
            state.projects.clear();
            state.searched = false;
            state.total = 0;
            state.next_offset = 0;
            state.versions.clear();
            state.selected_version = None;
            state.selections.clear();
            state.plan = None;
            state.plan_generation += 1;
        }
    }
}

#[derive(Default, Clone)]
pub(super) struct BrowserState {
    pub(super) target: Option<GameTarget>,
    target_loading: bool,
    target_attempted: bool,
    pub(super) query: String,
    requested: String,
    searched: bool,
    loading: bool,
    search_generation: u64,
    projects: Vec<Project>,
    total: usize,
    next_offset: usize,
    pub(super) selected: Option<Project>,
    versions: Vec<Version>,
    versions_loading: bool,
    versions_generation: u64,
    pub(super) selected_version: Option<String>,
    pub(super) selections: Vec<Selection>,
    resolving: bool,
    plan_generation: u64,
    plan: Option<InstallPlan>,
    installing: bool,
    error: Option<String>,
    notice: Option<String>,
}

fn downloads(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M downloads", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k downloads", count as f64 / 1_000.0)
    } else {
        format!("{count} downloads")
    }
}

fn version_label(version: &Version) -> String {
    format!(
        "{} / {} / {} / {}",
        version.name,
        version.version_number,
        version.version_type,
        format_bytes(version.size)
    )
}

impl LauncherRoot {
    fn load_modrinth_target(&mut self, id: String, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        let state = self.mod_browser_ui.states.entry(id.clone()).or_default();
        if state.target.is_some() || state.target_loading || state.target_attempted {
            return;
        }
        state.target_loading = true;
        state.target_attempted = true;
        state.error = None;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let target_id = id.clone();
            let result = rt
                .spawn(async move {
                    let instance = paths.instance_dir(&target_id)?;
                    game_target::load(&instance).await
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.mod_browser_ui.states.entry(id).or_default();
                state.target_loading = false;
                match result {
                    Ok(Ok(target)) => state.target = Some(target),
                    Ok(Err(e)) => state.error = Some(format!("{e:#}")),
                    Err(e) => state.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn search_modrinth(&mut self, id: String, more: bool, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        let state = self.mod_browser_ui.states.entry(id.clone()).or_default();
        if state.loading {
            return;
        }
        if state.target.is_none() {
            state.notice = Some("Checking the instance Minecraft version and mod loader...".into());
            cx.notify();
            return;
        }
        state.search_generation += 1;
        let generation = state.search_generation;
        state.loading = true;
        state.error = None;
        state.notice = None;
        let query = if more {
            state.requested.clone()
        } else {
            state.query.trim().to_owned()
        };
        let offset = if more {
            state.next_offset
        } else {
            state.next_offset = 0;
            0
        };
        state.requested = query.clone();
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target_id = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let instance = paths.instance_dir(&target_id)?;
                    let target = game_target::load(&instance).await?;
                    let page = modrinth::search(&target, &query, offset).await?;
                    Ok::<_, anyhow::Error>((target, page))
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.mod_browser_ui.states.entry(id).or_default();
                if state.search_generation != generation {
                    return;
                }
                state.loading = false;
                match result {
                    Ok(Ok((target, page))) => {
                        if state.target.as_ref().is_some_and(|old| old != &target) {
                            state.selections.clear();
                            state.plan = None;
                            state.plan_generation += 1;
                            state.versions.clear();
                            state.selected_version = None;
                            state.selected = None;
                        }
                        state.target = Some(target);
                        state.searched = true;
                        state.total = page.total;
                        state.next_offset = offset + page.projects.len();
                        if offset == 0 {
                            state.projects.clear();
                        }
                        if page.projects.is_empty() {
                            state.total = state.next_offset;
                        }
                        for project in page.projects {
                            if !state.projects.iter().any(|old| old.id == project.id) {
                                state.projects.push(project);
                            }
                        }
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

    fn load_modrinth_versions(&mut self, id: String, project: Project, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        let state = self.mod_browser_ui.states.entry(id.clone()).or_default();
        let Some(game_target) = state.target.clone() else {
            state.notice = Some("Checking the instance Minecraft version and mod loader...".into());
            cx.notify();
            return;
        };
        state.selected = Some(project.clone());
        state.versions.clear();
        state.selected_version = None;
        state.versions_loading = true;
        state.versions_generation += 1;
        let generation = state.versions_generation;
        state.error = None;
        state.notice = None;
        let rt = self.data.rt.clone();
        let project_id = project.id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { modrinth::versions(&game_target, &project_id).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.mod_browser_ui.states.entry(id).or_default();
                if state.versions_generation != generation {
                    return;
                }
                state.versions_loading = false;
                match result {
                    Ok(Ok(versions)) => {
                        // The API returns newest first; preselect the newest file.
                        state.selected_version = versions.first().map(|version| version.id.clone());
                        state.versions = versions;
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

    fn choose_modrinth_version(&mut self, id: &str, version_id: &str, cx: &mut Context<Self>) {
        let state = self.mod_browser_ui.states.entry(id.into()).or_default();
        state.selected_version = Some(version_id.to_owned());
        state.notice = None;
        cx.notify();
    }

    fn add_modrinth_selection(&mut self, id: &str, cx: &mut Context<Self>) {
        let state = self.mod_browser_ui.states.entry(id.into()).or_default();
        if state.resolving || state.installing {
            return;
        }
        let selected = state.selected.clone();
        let version = selected.as_ref().and_then(|_| {
            let wanted = state.selected_version.as_deref()?;
            state
                .versions
                .iter()
                .find(|version| version.id == wanted)
                .cloned()
        });
        let (Some(project), Some(version)) = (selected, version) else {
            state.notice = Some("Choose a version before adding this mod.".into());
            cx.notify();
            return;
        };
        // One entry per project: a different version replaces the previous one.
        state.selections.retain(|old| old.project.id != project.id);
        state.selections.push(Selection {
            project: project.clone(),
            version: version.clone(),
        });
        state.plan = None;
        state.plan_generation += 1;
        state.notice = Some(format!(
            "{} {} added to the install list.",
            project.name, version.version_number
        ));
        cx.notify();
    }

    fn remove_modrinth_selection(&mut self, id: &str, project_id: &str, cx: &mut Context<Self>) {
        let state = self.mod_browser_ui.states.entry(id.into()).or_default();
        if state.resolving || state.installing {
            return;
        }
        state.selections.retain(|old| old.project.id != project_id);
        state.plan = None;
        state.plan_generation += 1;
        state.notice = None;
        cx.notify();
    }

    fn review_modrinth_install(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        let state = self.mod_browser_ui.states.entry(id.clone()).or_default();
        if state.resolving || state.installing || state.selections.is_empty() {
            return;
        }
        state.resolving = true;
        state.error = None;
        state.notice = None;
        state.plan = None;
        state.plan_generation += 1;
        let generation = state.plan_generation;
        let selections = state.selections.clone();
        self.busy.insert(format!("modrinth:{id}"));
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { modrinth::plan(&paths, &target, selections).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.busy.remove(&format!("modrinth:{id}"));
                let state = v.mod_browser_ui.states.entry(id).or_default();
                if state.plan_generation != generation {
                    return;
                }
                state.resolving = false;
                match result {
                    Ok(Ok(plan)) => {
                        if plan.items.is_empty() {
                            state.notice =
                                Some("Nothing new to install; nothing was planned.".into());
                        }
                        state.plan = Some(plan);
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

    fn install_modrinth(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        let state = self.mod_browser_ui.states.entry(id.clone()).or_default();
        if state.installing {
            return;
        }
        let Some(plan) = state.plan.clone().filter(|plan| !plan.items.is_empty()) else {
            state.notice = Some("Review the install list before installing.".into());
            cx.notify();
            return;
        };
        if !plan.conflicts.is_empty() {
            state.notice = Some("Remove the conflicting mods before installing.".into());
            cx.notify();
            return;
        }
        state.installing = true;
        state.error = None;
        state.notice = None;
        let generation = state.plan_generation;
        self.busy.insert(format!("modrinth:{id}"));
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { modrinth::install(&paths, &target, &plan).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.busy.remove(&format!("modrinth:{id}"));
                let mut installed = None;
                {
                    let state = v.mod_browser_ui.states.entry(id.clone()).or_default();
                    state.installing = false;
                    match result {
                        Ok(Ok(count)) => {
                            if state.plan_generation == generation {
                                state.selections.clear();
                                state.plan = None;
                            }
                            state.notice = None;
                            state.error = None;
                            installed = Some(count);
                        }
                        Ok(Err(e)) => state.error = Some(format!("{e:#}")),
                        Err(e) => state.error = Some(e.to_string()),
                    }
                }
                if let Some(count) = installed {
                    v.data.status = format!("Installed {count} mod(s) from Modrinth");
                    v.invalidate_content(&id);
                    v.invalidate_mods(&id);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn modrinth_query_field(&mut self, id: &str, cx: &mut Context<Self>) -> Entity<NativeField> {
        let key = format!("modrinth-query:{id}");
        let state = self.mod_browser_ui.states.entry(id.into()).or_default();
        let query = state.query.clone();
        let placeholder = state
            .target
            .as_ref()
            .map(|target| format!("Search Modrinth for {} mods", target.summary()))
            .unwrap_or_else(|| "Search Modrinth mods".into());
        if !self.native_fields.contains_key(&key) {
            let field =
                cx.new(|cx| NativeField::new(&query, self.theme.clone(), &placeholder, false, cx));
            let target = id.to_owned();
            cx.subscribe(&field, move |v, _, event, cx| match event {
                NativeFieldEvent::Change(text) => {
                    v.mod_browser_ui
                        .states
                        .entry(target.clone())
                        .or_default()
                        .query = text.clone();
                    cx.notify();
                }
                NativeFieldEvent::Submit => v.search_modrinth(target.clone(), false, cx),
                _ => {}
            })
            .detach();
            self.native_fields.insert(key.clone(), field);
        }
        let field = self.native_fields[&key].clone();
        let query = self
            .mod_browser_ui
            .states
            .get(id)
            .map(|state| state.query.clone())
            .unwrap_or_default();
        field.update(cx, |state, _| {
            state.value = query.clone();
            state.theme = self.theme.clone();
            state.placeholder = placeholder.clone();
            state.accessibility_label = placeholder.clone();
        });
        field
    }

    pub(super) fn mod_browser_view(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        if self
            .mod_browser_ui
            .states
            .get(id)
            .map_or(true, |state| !state.target_attempted)
        {
            self.load_modrinth_target(id.to_owned(), cx);
        }
        let field = self.modrinth_query_field(id, cx);
        let theme = self.theme.clone();
        let state = self
            .mod_browser_ui
            .states
            .entry(id.into())
            .or_default()
            .clone();
        let target = id.to_owned();
        let game_target = state.target.clone();
        let target_text = game_target
            .as_ref()
            .map(GameTarget::summary)
            .unwrap_or_else(|| {
                if state.target_loading {
                    "Checking the instance Minecraft version and mod loader...".into()
                } else {
                    "The instance Minecraft version and mod loader could not be determined.".into()
                }
            });
        let loading = state.loading;
        let ready = game_target.is_some();
        let busy = self.instance_is_busy(id) || state.installing || state.resolving;
        let mut body = div()
            .id("modrinth-browser")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child("Browse Modrinth")
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground())
                            .child(format!(
                                "{target_text}. Required dependencies are added automatically; optional dependencies are not installed."
                            ))
                            .child("Downloads are verified before anything is written."),
                    ),
            );

        let search = target.clone();
        let can_search = !loading && ready;
        body = body.child(
            div()
                .flex()
                .gap_2()
                .child(div().flex_1().min_w_0().child(field))
                .child(
                    btn(
                        &theme,
                        if loading { "Searching..." } else { "Search" },
                        ButtonVariant::Primary,
                    )
                    .debug_selector(|| "modrinth-search".into())
                    .tab_stop(can_search)
                    .when(!can_search, |button| button.opacity(0.5))
                    .on_click(cx.listener(
                        move |v, _: &gpui::ClickEvent, _, cx| {
                            if can_search {
                                v.search_modrinth(search.clone(), false, cx);
                            }
                        },
                    )),
                ),
        );
        if let Some(error) = state.error.clone() {
            body = body.child(div().text_color(theme.destructive()).child(error));
        }
        if let Some(notice) = state.notice.clone() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground())
                    .child(notice),
            );
        }
        if !ready && !state.target_loading {
            let retry = target.clone();
            body = body.child(
                btn(&theme, "Retry version check", ButtonVariant::Outline)
                    .debug_selector(|| "modrinth-target-retry".into())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if let Some(state) = v.mod_browser_ui.states.get_mut(&retry) {
                            state.target_attempted = false;
                            state.target_loading = false;
                        }
                        v.load_modrinth_target(retry.clone(), cx);
                    })),
            );
        }

        let mut results = div()
            .id("modrinth-results")
            .h(px(320.))
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for project in state.projects.clone() {
            let project_id = project.id.clone();
            let page = project.page();
            let selected = state
                .selected
                .as_ref()
                .is_some_and(|old| old.id == project.id);
            let browse = target.clone();
            let chosen = project.clone();
            results = results.child(
                card(&theme)
                    .id(format!("modrinth-project-row:{}", project.id))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(project.name.clone())
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground())
                                    .child(downloads(project.downloads)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_color(theme.muted_foreground())
                                    .child(project.description.clone()),
                            ),
                    )
                    .child(
                        btn(&theme, "Details", ButtonVariant::Ghost)
                            .text_xs()
                            .on_click(move |_: &gpui::ClickEvent, _, cx| cx.open_url(&page)),
                    )
                    .child(
                        btn(
                            &theme,
                            if selected { "Selected" } else { "Versions" },
                            ButtonVariant::Secondary,
                        )
                        .debug_selector(move || format!("modrinth-project:{project_id}").into())
                        .on_click(cx.listener(
                            move |v, _: &gpui::ClickEvent, _, cx| {
                                v.load_modrinth_versions(browse.clone(), chosen.clone(), cx)
                            },
                        )),
                    ),
            );
        }
        body = body.child(results);
        if state.searched && !loading && state.projects.is_empty() {
            body = body.child(
                div().text_color(theme.muted_foreground()).child(
                    game_target
                        .as_ref()
                        .map(|target| format!("No compatible {} mods found.", target.summary()))
                        .unwrap_or_else(|| "No compatible mods found.".into()),
                ),
            );
        }
        if !loading && state.next_offset < state.total && state.next_offset <= MAX_OFFSET {
            let more = target.clone();
            body = body.child(
                btn(&theme, "Load more", ButtonVariant::Ghost)
                    .debug_selector(|| "modrinth-more".into())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        v.search_modrinth(more.clone(), true, cx);
                    })),
            );
        }

        if let Some(project) = state.selected.clone() {
            let versions: Vec<Version> = state
                .versions
                .iter()
                .filter(|version| {
                    game_target
                        .as_ref()
                        .is_some_and(|target| version.is_compatible(target))
                })
                .take(VISIBLE_VERSIONS)
                .cloned()
                .collect();
            let mut section = div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .child(div().child(format!("Versions for {}", project.name)));
            if state.versions_loading {
                section = section.child(
                    div()
                        .text_color(theme.muted_foreground())
                        .child("Loading versions..."),
                );
            } else if versions.is_empty() {
                section = section.child(
                    div().text_color(theme.muted_foreground()).child(
                        game_target
                            .as_ref()
                            .map(|target| {
                                format!("No {} files for this project.", target.summary())
                            })
                            .unwrap_or_else(|| "No compatible files for this project.".into()),
                    ),
                );
            }
            for version in versions {
                let selector_id = version.id.clone();
                let picked = version.id.clone();
                let choose = target.clone();
                let chosen = state.selected_version.as_deref() == Some(version.id.as_str());
                section = section.child(
                    btn(
                        &theme,
                        &version_label(&version),
                        if chosen {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Outline
                        },
                    )
                    .flex_shrink_0()
                    .debug_selector(move || format!("modrinth-version:{selector_id}").into())
                    .on_click(cx.listener(
                        move |v, _: &gpui::ClickEvent, _, cx| {
                            v.choose_modrinth_version(&choose, &picked, cx)
                        },
                    )),
                );
            }
            let can_add = state.selected_version.is_some() && !busy && ready;
            let add = target.clone();
            section = section.child(
                btn(&theme, "Add to install list", ButtonVariant::Primary)
                    .debug_selector(|| "modrinth-add".into())
                    .tab_stop(can_add)
                    .when(!can_add, |button| button.opacity(0.5))
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if can_add {
                            v.add_modrinth_selection(&add, cx);
                        }
                    })),
            );
            if state.selected_version.is_none() && !state.versions_loading {
                section = section.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground())
                        .child("Choose a version before adding this mod."),
                );
            }
            body = body.child(
                card(&theme)
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .child(section),
            );
        }

        if !state.selections.is_empty() {
            let mut list = div().flex().flex_col().gap_1().child(div().child(format!(
                "Install list ({} selected)",
                state.selections.len()
            )));
            for selection in state.selections.clone() {
                let project_id = selection.project.id.clone();
                let remove = target.clone();
                let removed = project_id.clone();
                list = list.child(
                    card(&theme)
                        .id(format!("modrinth-selection:{}", selection.project.id))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .p_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(selection.project.name.clone())
                                .child(div().text_xs().text_color(theme.muted_foreground()).child(
                                    format!(
                                        "{} / {}",
                                        selection.version.version_number,
                                        format_bytes(selection.version.size)
                                    ),
                                )),
                        )
                        .child(
                            btn(&theme, "Remove", ButtonVariant::Outline)
                                .debug_selector(move || {
                                    format!("modrinth-remove:{project_id}").into()
                                })
                                .tab_stop(!busy)
                                .when(busy, |button| button.opacity(0.5))
                                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                    if !busy {
                                        v.remove_modrinth_selection(&remove, &removed, cx);
                                    }
                                })),
                        ),
                );
            }
            let review = target.clone();
            let resolving = state.resolving;
            list = list.child(
                btn(
                    &theme,
                    if resolving {
                        "Resolving dependencies..."
                    } else {
                        "Review install"
                    },
                    ButtonVariant::Secondary,
                )
                .debug_selector(|| "modrinth-review".into())
                .tab_stop(!resolving && !busy)
                .when(resolving || busy, |button| button.opacity(0.5))
                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                    if !resolving && !busy {
                        v.review_modrinth_install(review.clone(), cx);
                    }
                })),
            );
            body = body.child(list);
        }

        if let Some(InstallPlan {
            items,
            skipped,
            conflicts,
            ..
        }) = state.plan.clone()
        {
            let count = items.len();
            let mut section = div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().child(format!("Install review ({count} file(s))")));
            for item in items {
                let mut row = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().flex_1().min_w_0().child(item.project.name.clone()))
                            .child(div().text_xs().text_color(theme.muted_foreground()).child(
                                format!(
                                    "{} / {}",
                                    item.version.version_number,
                                    format_bytes(item.version.size)
                                ),
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground())
                            .child(item.version.filename.clone()),
                    );
                if !item.required_by.is_empty() {
                    row = row.child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground())
                            .child(format!("required by: {}", item.required_by.join(", "))),
                    );
                }
                section = section.child(row);
            }
            for note in skipped {
                section = section.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground())
                        .child(note),
                );
            }
            for conflict in &conflicts {
                section = section.child(
                    div()
                        .text_xs()
                        .text_color(theme.destructive())
                        .child(conflict.clone()),
                );
            }
            if !conflicts.is_empty() {
                section = section.child(
                    div()
                        .text_xs()
                        .text_color(theme.destructive())
                        .child("Remove the conflicting selection before installing."),
                );
            } else if count == 0 {
                section = section.child(
                    div()
                        .text_color(theme.muted_foreground())
                        .child("Nothing new to install."),
                );
            } else if state.installing {
                section = section.child(
                    btn(&theme, "Installing...", ButtonVariant::Primary)
                        .debug_selector(|| "modrinth-install".into())
                        .tab_stop(false)
                        .opacity(0.5),
                );
            } else {
                let install = target.clone();
                section = section.child(
                    btn(
                        &theme,
                        &format!("Install {count} mods"),
                        ButtonVariant::Primary,
                    )
                    .debug_selector(|| "modrinth-install".into())
                    .tab_stop(!busy)
                    .when(busy, |button| button.opacity(0.5))
                    .on_click(cx.listener(
                        move |v, _: &gpui::ClickEvent, _, cx| {
                            if !busy {
                                v.install_modrinth(install.clone(), cx);
                            }
                        },
                    )),
                );
            }
            body = body.child(
                card(&theme)
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .child(section),
            );
        }

        div().flex_1().min_h_0().child(body.size_full())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::{content::AddOnTab, instance_settings::DetailTab, test_support::Fixture};
    use gpui::{px, size, TestAppContext};
    use industrialis_backend::game_target::ModLoader;

    fn project() -> Project {
        Project {
            id: "GTNHMod".into(),
            slug: "gtnh-mod".into(),
            name: "GTNH Mod".into(),
            description: "A test mod".into(),
            downloads: 12_345,
            icon_url: None,
        }
    }

    fn version() -> Version {
        Version {
            project_id: "GTNHMod".into(),
            id: "abc12345".into(),
            name: "GTNH Mod 1.0".into(),
            version_number: "1.0".into(),
            version_type: "release".into(),
            game_versions: vec!["1.7.10".into()],
            loaders: vec!["forge".into()],
            date_published: "2026-01-01T00:00:00Z".into(),
            filename: "gtnh-mod-1.0.jar".into(),
            url: "https://cdn.modrinth.com/data/GTNHMod/versions/abc12345/gtnh-mod-1.0.jar".into(),
            size: 2048,
            sha512: "a".repeat(128),
            sha1: "b".repeat(40),
            dependencies: Vec::new(),
        }
    }

    #[gpui::test]
    async fn loads_the_instance_target_from_mmc_pack(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["pack"]);
        std::fs::write(
            fixture
                .paths
                .instance_dir("pack")
                .unwrap()
                .join("mmc-pack.json"),
            r#"{"components":[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.15.0"}]}"#,
        )
        .unwrap();
        let (window, root) = fixture.open_sized(cx, size(px(1100.), px(1200.)), |v| {
            v.selected_id = Some("pack".into());
            v.instance_ui.detail_tab = DetailTab::Mods;
            v.content_ui.tab = AddOnTab::Browse;
        });
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        fixture
            .until(cx, &root, |v| {
                v.mod_browser_ui
                    .states
                    .get("pack")
                    .is_some_and(|state| !state.target_loading && state.target.is_some())
            })
            .await;
        root.read_with(cx, |v, _| {
            assert_eq!(
                v.mod_browser_ui.states["pack"]
                    .target
                    .as_ref()
                    .unwrap()
                    .summary(),
                "Minecraft 1.20.1 / Fabric"
            );
        });
        fixture.cleanup();
    }

    #[gpui::test]
    async fn add_and_remove_update_the_install_list_without_network_calls(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["pack"]);
        std::fs::write(
            fixture
                .paths
                .instance_dir("pack")
                .unwrap()
                .join("mmc-pack.json"),
            r#"{"components":[{"uid":"net.minecraft","version":"1.7.10"},{"uid":"net.minecraftforge","version":"10.13.4.1614"}]}"#,
        )
        .unwrap();
        let project = project();
        let version = version();
        let (window, root) = fixture.open_sized(cx, size(px(1100.), px(2200.)), |v| {
            v.selected_id = Some("pack".into());
            v.instance_ui.detail_tab = DetailTab::Mods;
            v.content_ui.tab = AddOnTab::Browse;
            v.mod_browser_ui.states.insert(
                "pack".into(),
                BrowserState {
                    target: Some(GameTarget {
                        game_version: "1.7.10".into(),
                        loaders: vec![ModLoader::Forge],
                    }),
                    searched: true,
                    selected: Some(project.clone()),
                    versions: vec![version.clone()],
                    selected_version: Some(version.id.clone()),
                    ..Default::default()
                },
            );
        });
        Fixture::click(cx, window, "modrinth-add");
        root.read_with(cx, |v, _| {
            let state = &v.mod_browser_ui.states["pack"];
            assert_eq!(state.selections.len(), 1);
            assert_eq!(state.selections[0].project.id, "GTNHMod");
            assert_eq!(state.selections[0].version.id, "abc12345");
        });
        Fixture::click(cx, window, "modrinth-remove:GTNHMod");
        root.read_with(cx, |v, _| {
            assert!(v.mod_browser_ui.states["pack"].selections.is_empty());
        });
        fixture.cleanup();
    }
}
