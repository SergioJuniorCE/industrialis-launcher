//! Modrinth browser for the Add-ons tab. Network work runs on the backend
//! runtime; every instance keeps its own search, selection and plan state.
use super::LauncherRoot;
use crate::{
    input::{NativeField, NativeFieldEvent},
    state::format_bytes,
    theme::ActiveTheme,
    ui::{badge, btn, card, icon, tint, BadgeKind, ButtonVariant},
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
        "{}  [{}]  ·  {}",
        version.version_number,
        version.version_type,
        format_bytes(version.size)
    )
}

fn project_color(id: &str) -> gpui::Rgba {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    const COLORS: [u32; 6] = [0x2563eb, 0x7c3aed, 0x059669, 0xd97706, 0xdc2626, 0x0891b2];
    gpui::rgba((COLORS[(hasher.finish() as usize) % COLORS.len()] << 8) | 0xff)
}

fn project_tile(project: &Project, size: f32) -> Div {
    let letter = project
        .name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    div()
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .rounded(px(8.0))
        .bg(project_color(&project.id))
        .flex()
        .items_center()
        .justify_center()
        .text_color(gpui::rgb(0xffffff))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_size(px(size * 0.42))
        .child(letter)
}

fn plain_body(body: &str) -> Vec<String> {
    if body.trim().is_empty() {
        return Vec::new();
    }
    let normalized = body
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n\n")
        .replace("</li>", "\n")
        .replace("</h1>", "\n\n")
        .replace("</h2>", "\n\n")
        .replace("</h3>", "\n\n");
    let mut text = String::new();
    let mut in_tag = false;
    for ch in normalized.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .take(12)
        .map(|line| line.chars().take(1200).collect())
        .collect()
}

fn external_link(
    theme: &ActiveTheme,
    label: &str,
    url: Option<String>,
) -> Option<gpui::Stateful<Div>> {
    let url = url.filter(|url| url.starts_with("https://"))?;
    Some(
        btn(theme, label, ButtonVariant::Ghost)
            .text_xs()
            .child(icon("external-link", 12.0))
            .on_click(move |_: &gpui::ClickEvent, _, cx| cx.open_url(&url)),
    )
}

fn selection_box(theme: &ActiveTheme, checked: bool) -> Div {
    div()
        .size(px(18.0))
        .flex_shrink_0()
        .rounded(px(4.0))
        .border_1()
        .border_color(theme.primary())
        .bg(if checked {
            theme.primary()
        } else {
            gpui::rgba(0x00000000)
        })
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme.primary_foreground())
        .when(checked, |boxed| boxed.child(icon("check", 12.0)))
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
        let detail_id = project.id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let (detail, versions) = tokio::join!(
                        modrinth::project(&detail_id),
                        modrinth::versions(&game_target, &project_id)
                    );
                    Ok::<_, anyhow::Error>((detail.ok(), versions?))
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.mod_browser_ui.states.entry(id).or_default();
                if state.versions_generation != generation {
                    return;
                }
                state.versions_loading = false;
                match result {
                    Ok(Ok((detail, versions))) => {
                        if let Some(mut detail) = detail {
                            if detail.author.is_empty() {
                                detail.author = state
                                    .selected
                                    .as_ref()
                                    .map(|project| project.author.clone())
                                    .unwrap_or_default();
                            }
                            if let Some(current) = state.selected.as_mut() {
                                if current.id == detail.id {
                                    *current = detail;
                                }
                            }
                        }
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
        state.plan = None;
        state.plan_generation += 1;
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

    fn toggle_modrinth_selection(&mut self, id: &str, project: &Project, cx: &mut Context<Self>) {
        let in_list = self.mod_browser_ui.states.get(id).is_some_and(|state| {
            state
                .selections
                .iter()
                .any(|selection| selection.project.id == project.id)
        });
        if in_list {
            self.remove_modrinth_selection(id, &project.id, cx);
            return;
        }
        let is_open = self.mod_browser_ui.states.get(id).is_some_and(|state| {
            state
                .selected
                .as_ref()
                .is_some_and(|selected| selected.id == project.id)
        });
        if is_open {
            self.add_modrinth_selection(id, cx);
        } else {
            self.load_modrinth_versions(id.to_owned(), project.clone(), cx);
            if let Some(state) = self.mod_browser_ui.states.get_mut(id) {
                state.notice =
                    Some("Choose a version on the right, then add it to the install list.".into());
            }
        }
    }

    fn clear_modrinth_selections(&mut self, id: &str, cx: &mut Context<Self>) {
        let state = self.mod_browser_ui.states.entry(id.into()).or_default();
        if state.resolving || state.installing {
            return;
        }
        state.selections.clear();
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

    #[inline(never)]
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
        let selected_version = state.selected_version.clone();
        let selections = state.selections.clone();
        let selected_count = selections.len();
        let plan = state.plan.clone();
        let plan_count = plan.as_ref().map_or(0, |plan| plan.items.len());

        let header = self.modrinth_header(
            &target,
            &theme,
            &state,
            field,
            target_text,
            selected_count,
            loading,
            ready,
            cx,
        );

        let results = self.modrinth_results(
            &target,
            &theme,
            &state,
            &selections,
            game_target.as_ref(),
            loading,
            cx,
        );
        let left = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border())
                    .child(div().text_xs().text_color(theme.muted_foreground()).child(
                        if state.searched {
                            format!("{} results", state.total)
                        } else {
                            "Search results".into()
                        },
                    ))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground())
                            .child(if loading { "Searching..." } else { "" }),
                    ),
            )
            .child(results);

        let detail = self.modrinth_detail(
            &target,
            &theme,
            &state,
            &selections,
            game_target.as_ref(),
            selected_version.as_deref(),
            plan.as_ref(),
            busy,
            ready,
            cx,
        );

        let right = div()
            .w(px(420.0))
            .flex_shrink_0()
            .min_h_0()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(theme.border())
            .child(detail);

        let actions = self.modrinth_actions(
            &target,
            &theme,
            &state,
            plan.as_ref(),
            selected_count,
            plan_count,
            busy,
            cx,
        );

        let bottom = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(theme.border())
            .child(div().text_xs().text_color(theme.muted_foreground()).child(
                if selected_count == 0 {
                    "No mods selected".into()
                } else {
                    format!("{selected_count} mod(s) selected")
                },
            ))
            .child(actions);

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().flex().child(left).child(right))
            .child(bottom)
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn modrinth_detail(
        &mut self,
        target: &str,
        theme: &ActiveTheme,
        state: &BrowserState,
        selections: &[Selection],
        game_target: Option<&GameTarget>,
        selected_version: Option<&str>,
        plan: Option<&InstallPlan>,
        busy: bool,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut detail = div()
            .id("modrinth-detail")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_3();
        if let Some(project) = state.selected.clone() {
            detail = detail.child(Self::modrinth_project_header(theme, &project));
            detail = detail.child(Self::modrinth_project_links(theme, &project));
            if let Some(body) = Self::modrinth_project_body(theme, &project) {
                detail = detail.child(body);
            }
            detail = detail.child(self.modrinth_versions_card(
                target,
                theme,
                state,
                &project,
                game_target,
                selected_version,
                busy,
                ready,
                cx,
            ));
        } else {
            detail = detail.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_6()
                    .text_color(theme.muted_foreground())
                    .child("Select a mod to see its description, links and versions."),
            );
        }
        if let Some(list) = self.modrinth_install_list(target, theme, selections, busy, cx) {
            detail = detail.child(list);
        }
        if let Some(review) = plan.and_then(|plan| Self::modrinth_review(theme, plan)) {
            detail = detail.child(review);
        }
        detail
    }

    #[inline(never)]
    fn modrinth_project_header(theme: &ActiveTheme, project: &Project) -> Div {
        let meta = if project.author.is_empty() {
            downloads(project.downloads)
        } else {
            format!("by {} · {}", project.author, downloads(project.downloads))
        };
        div()
            .flex()
            .items_start()
            .gap_3()
            .child(project_tile(project, 56.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(project.name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground())
                            .child(meta),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground())
                            .child(project.description.clone()),
                    ),
            )
    }

    #[inline(never)]
    fn modrinth_project_links(theme: &ActiveTheme, project: &Project) -> Div {
        let mut links = div().flex().flex_wrap().gap_1();
        for (label, url) in [
            ("Project page", Some(project.page())),
            ("Source", project.source_url.clone()),
            ("Issues", project.issues_url.clone()),
            ("Wiki", project.wiki_url.clone()),
            ("Discord", project.discord_url.clone()),
        ] {
            if let Some(link) = external_link(theme, label, url) {
                links = links.child(link);
            }
        }
        for donation in &project.donations {
            if let Some(link) = external_link(
                theme,
                &format!("Donate via {}", donation.platform),
                Some(donation.url.clone()),
            ) {
                links = links.child(link);
            }
        }
        links
    }

    #[inline(never)]
    fn modrinth_project_body(theme: &ActiveTheme, project: &Project) -> Option<Div> {
        let paragraphs = plain_body(&project.body);
        if paragraphs.is_empty() {
            return None;
        }
        let mut body = card(theme).p_3().flex().flex_col().gap_2();
        for paragraph in paragraphs {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground())
                    .child(paragraph),
            );
        }
        Some(body)
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn modrinth_versions_card(
        &mut self,
        target: &str,
        theme: &ActiveTheme,
        state: &BrowserState,
        project: &Project,
        game_target: Option<&GameTarget>,
        selected_version: Option<&str>,
        busy: bool,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let versions: Vec<Version> = state
            .versions
            .iter()
            .filter(|version| game_target.is_some_and(|target| version.is_compatible(target)))
            .take(VISIBLE_VERSIONS)
            .cloned()
            .collect();
        let mut versions_card = card(theme).p_3().flex().flex_col().gap_2().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Versions"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground())
                        .child(format!("{} compatible", versions.len())),
                ),
        );
        if state.versions_loading {
            versions_card = versions_card.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground())
                    .child("Loading versions..."),
            );
        } else if versions.is_empty() {
            versions_card = versions_card.child(
                div().text_xs().text_color(theme.muted_foreground()).child(
                    game_target
                        .map(|target| format!("No {} files for this project.", target.summary()))
                        .unwrap_or_else(|| "No compatible files for this project.".into()),
                ),
            );
        }
        let mut version_list = div().flex().flex_col().gap_1();
        for version in &versions {
            let selector_id = version.id.clone();
            let picked = version.id.clone();
            let choose = target.to_owned();
            let chosen = selected_version == Some(version.id.as_str());
            version_list = version_list.child(
                btn(
                    theme,
                    &version_label(version),
                    if chosen {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Outline
                    },
                )
                .w_full()
                .justify_start()
                .text_xs()
                .debug_selector(move || format!("modrinth-version:{selector_id}").into())
                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                    v.choose_modrinth_version(&choose, &picked, cx)
                })),
            );
        }
        versions_card = versions_card.child(version_list);
        let listed_version = state
            .selections
            .iter()
            .find(|selection| selection.project.id == project.id)
            .map(|selection| selection.version.id.clone());
        let same_version = listed_version.as_deref() == selected_version;
        let can_add = selected_version.is_some() && !busy && ready;
        let add_target = target.to_owned();
        let add_project = project.clone();
        versions_card = versions_card.child(
            btn(
                theme,
                if listed_version.is_some() && same_version {
                    "Remove from install list"
                } else if listed_version.is_some() {
                    "Use this version"
                } else {
                    "Add to install list"
                },
                if listed_version.is_some() && same_version {
                    ButtonVariant::Outline
                } else {
                    ButtonVariant::Primary
                },
            )
            .debug_selector(|| "modrinth-add".into())
            .tab_stop(can_add)
            .when(!can_add, |button| button.opacity(0.5))
            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                if can_add {
                    if listed_version.is_some() && same_version {
                        v.remove_modrinth_selection(&add_target, &add_project.id, cx);
                    } else {
                        v.add_modrinth_selection(&add_target, cx);
                    }
                }
            })),
        );
        versions_card
    }

    #[inline(never)]
    fn modrinth_install_list(
        &mut self,
        target: &str,
        theme: &ActiveTheme,
        selections: &[Selection],
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if selections.is_empty() {
            return None;
        }
        let mut list = card(theme).p_3().flex().flex_col().gap_2().child(
            div().flex().items_center().justify_between().child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("Install list ({})", selections.len())),
            ),
        );
        for selection in selections {
            let project_id = selection.project.id.clone();
            let remove = target.to_owned();
            let removed = project_id.clone();
            list = list.child(
                div()
                    .id(format!("modrinth-selection:{}", selection.project.id))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(project_tile(&selection.project, 24.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(selection.project.name.clone())
                            .child(div().text_xs().text_color(theme.muted_foreground()).child(
                                format!(
                                    "{} · {}",
                                    selection.version.version_number,
                                    format_bytes(selection.version.size)
                                ),
                            )),
                    )
                    .child(
                        btn(theme, "Remove", ButtonVariant::Outline)
                            .text_xs()
                            .debug_selector(move || format!("modrinth-remove:{project_id}").into())
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
        Some(list)
    }

    #[inline(never)]
    fn modrinth_review(theme: &ActiveTheme, plan: &InstallPlan) -> Option<Div> {
        let mut review = card(theme).p_3().flex().flex_col().gap_2().child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(format!("Install review ({} file(s))", plan.items.len())),
        );
        for item in &plan.items {
            let mut row = div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(item.project.name.clone()))
                        .child(div().text_xs().text_color(theme.muted_foreground()).child(
                            format!(
                                "{} · {}",
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
            review = review.child(row);
        }
        for note in &plan.skipped {
            review = review.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground())
                    .child(note.clone()),
            );
        }
        for conflict in &plan.conflicts {
            review = review.child(
                div()
                    .text_xs()
                    .text_color(theme.destructive())
                    .child(conflict.clone()),
            );
        }
        if plan.items.is_empty() {
            review = review.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground())
                    .child("Nothing new to install."),
            );
        }
        Some(review)
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn modrinth_actions(
        &mut self,
        target: &str,
        theme: &ActiveTheme,
        state: &BrowserState,
        plan: Option<&InstallPlan>,
        selected_count: usize,
        plan_count: usize,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut actions = div().flex().items_center().gap_2();
        if selected_count > 0 && !busy {
            let clear = target.to_owned();
            actions = actions.child(
                btn(theme, "Clear", ButtonVariant::Ghost)
                    .text_xs()
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        v.clear_modrinth_selections(&clear, cx);
                    })),
            );
        }
        if state.resolving {
            actions = actions.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground())
                    .child("Resolving dependencies..."),
            );
        } else if state.installing {
            actions = actions.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground())
                    .child("Installing..."),
            );
        } else if let Some(plan) = plan {
            if !plan.conflicts.is_empty() {
                actions = actions.child(
                    div()
                        .text_xs()
                        .text_color(theme.destructive())
                        .child("Resolve conflicts before installing."),
                );
            } else if plan_count == 0 {
                actions = actions.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground())
                        .child("Nothing new to install."),
                );
            } else {
                let install = target.to_owned();
                actions = actions.child(
                    btn(
                        theme,
                        &format!("Install {plan_count} mods"),
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
        } else if selected_count > 0 {
            let review = target.to_owned();
            actions = actions.child(
                btn(theme, "Review install", ButtonVariant::Secondary)
                    .debug_selector(|| "modrinth-review".into())
                    .tab_stop(!busy)
                    .when(busy, |button| button.opacity(0.5))
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if !busy {
                            v.review_modrinth_install(review.clone(), cx);
                        }
                    })),
            );
        }
        actions
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn modrinth_header(
        &mut self,
        target: &str,
        theme: &ActiveTheme,
        state: &BrowserState,
        field: Entity<NativeField>,
        target_text: String,
        selected_count: usize,
        loading: bool,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut header = div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .pt_3()
            .pb_2()
            .border_b_1()
            .border_color(theme.border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_base()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Browse mods"),
                    )
                    .child(badge(theme, "Modrinth", BadgeKind::Normal))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground())
                            .child(target_text),
                    )
                    .child(div().flex_1())
                    .when(selected_count > 0, |row| {
                        row.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground())
                                .child(format!("{selected_count} selected")),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(field))
                    .child({
                        let search = target.to_owned();
                        let can_search = !loading && ready;
                        btn(
                            theme,
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
                        ))
                    }),
            );
        if let Some(error) = state.error.clone() {
            header = header.child(div().text_color(theme.destructive()).child(error));
        }
        if let Some(notice) = state.notice.clone() {
            header = header.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground())
                    .child(notice),
            );
        }
        if !ready && !state.target_loading {
            let retry = target.to_owned();
            header = header.child(
                btn(theme, "Retry version check", ButtonVariant::Outline)
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
        header.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground())
                .child("Required dependencies are added automatically. Downloads are verified before anything is written."),
        )
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn modrinth_results(
        &mut self,
        target: &str,
        theme: &ActiveTheme,
        state: &BrowserState,
        selections: &[Selection],
        game_target: Option<&GameTarget>,
        loading: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut results = div()
            .id("modrinth-results")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .p_2()
            .gap_1();
        for project in state.projects.clone() {
            let selected = state
                .selected
                .as_ref()
                .is_some_and(|old| old.id == project.id);
            let checked = selections
                .iter()
                .any(|selection| selection.project.id == project.id);
            let details_target = target.to_owned();
            let details_project = project.clone();
            let toggle_target = target.to_owned();
            let toggle_project = project.clone();
            let selector = project.id.clone();
            let details_selector = project.id.clone();
            let meta = if project.author.is_empty() {
                downloads(project.downloads)
            } else {
                format!("{} · by {}", downloads(project.downloads), project.author)
            };
            results = results.child(
                div()
                    .id(format!("modrinth-project-row:{}", project.id))
                    .flex()
                    .items_start()
                    .gap_2()
                    .p_2()
                    .rounded(theme.radius())
                    .when(selected, |row| row.bg(tint(theme.primary(), 0.10)))
                    .child(
                        btn(theme, "", ButtonVariant::Ghost)
                            .w(px(24.0))
                            .px_0()
                            .aria_label(format!("Add {} to the install list", project.name))
                            .debug_selector(move || format!("modrinth-project:{selector}").into())
                            .child(selection_box(theme, checked))
                            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                v.toggle_modrinth_selection(&toggle_target, &toggle_project, cx);
                            })),
                    )
                    .child(project_tile(&project, 40.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(project.name.clone()),
                                    )
                                    .when(checked, |name| {
                                        name.child(badge(theme, "In list", BadgeKind::Normal))
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground())
                                    .child(project.description.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground())
                                    .child(meta),
                            ),
                    )
                    .child(
                        btn(theme, "Details", ButtonVariant::Ghost)
                            .text_xs()
                            .debug_selector(move || {
                                format!("modrinth-details:{details_selector}").into()
                            })
                            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                v.load_modrinth_versions(
                                    details_target.clone(),
                                    details_project.clone(),
                                    cx,
                                );
                            })),
                    ),
            );
        }
        if state.searched && !loading && state.projects.is_empty() {
            results = results.child(
                div().p_6().text_color(theme.muted_foreground()).child(
                    game_target
                        .map(|target| format!("No compatible {} mods found.", target.summary()))
                        .unwrap_or_else(|| "No compatible mods found.".into()),
                ),
            );
        }
        if !loading && state.next_offset < state.total && state.next_offset <= MAX_OFFSET {
            let more = target.to_owned();
            results = results.child(
                btn(theme, "Load more", ButtonVariant::Ghost)
                    .debug_selector(|| "modrinth-more".into())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        v.search_modrinth(more.clone(), true, cx);
                    })),
            );
        }
        results
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
            author: "tester".into(),
            body: String::new(),
            source_url: None,
            issues_url: None,
            wiki_url: None,
            discord_url: None,
            donations: Vec::new(),
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
    async fn opening_browse_from_mods_renders_and_loads_the_target(cx: &mut TestAppContext) {
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
        let (window, root) = fixture.open_sized(cx, size(px(1200.), px(900.)), |v| {
            v.selected_id = Some("pack".into());
            v.instance_ui.detail_tab = DetailTab::Mods;
        });
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        Fixture::click(cx, window, "content-browse-tab");
        fixture
            .until(cx, &root, |v| {
                v.mod_browser_ui
                    .states
                    .get("pack")
                    .is_some_and(|state| !state.target_loading && state.target.is_some())
            })
            .await;
        root.read_with(cx, |v, _| {
            assert!(v.mod_browser_ui.states["pack"].error.is_none());
        });
        fixture.cleanup();
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
    async fn list_checkbox_toggles_the_install_selection(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["pack"]);
        let project = project();
        let version = version();
        let (window, root) = fixture.open_sized(cx, size(px(1100.), px(1200.)), |v| {
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
                    projects: vec![project.clone()],
                    selected: Some(project.clone()),
                    versions: vec![version.clone()],
                    selected_version: Some(version.id.clone()),
                    ..Default::default()
                },
            );
        });
        Fixture::click(cx, window, "modrinth-project:GTNHMod");
        root.read_with(cx, |v, _| {
            assert_eq!(v.mod_browser_ui.states["pack"].selections.len(), 1);
        });
        Fixture::click(cx, window, "modrinth-project:GTNHMod");
        root.read_with(cx, |v, _| {
            assert!(v.mod_browser_ui.states["pack"].selections.is_empty());
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
