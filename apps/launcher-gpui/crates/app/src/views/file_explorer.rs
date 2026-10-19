//! Lazy instance-relative Explorer. Expanding folders never changes the editor draft.
use super::{minecraft_files::instance_id, LauncherRoot};
use crate::ui::{btn, icon, icon_btn, ButtonVariant};
use gpui::{
    div, prelude::*, px, uniform_list, Context, Div, FocusHandle, KeyDownEvent, ScrollStrategy,
    UniformListScrollHandle,
};
use industrialis_backend::minecraft_files::{self as files, MinecraftEntry};
use std::collections::{HashMap, HashSet};

pub(super) struct Explorer {
    pub loaded: bool,
    root_open: bool,
    pub children: HashMap<String, Vec<MinecraftEntry>>,
    pub loading: HashSet<String>,
    expanded: HashSet<String>,
    generation: u64,
    pub(super) row: Option<String>,
    focus: Option<FocusHandle>,
    pub scroll: UniformListScrollHandle,
    pub width: f32,
    menu_anchor: std::rc::Rc<std::cell::Cell<gpui::Point<gpui::Pixels>>>,
    pub resizing: Option<(f32, f32)>,
    pub(super) search: super::explorer_search::ExplorerSearch,
}

#[cfg(test)]
mod performance_tests {
    use super::super::{test_support::Fixture, DetailTab};
    use super::*;
    use gpui::{size, TestAppContext, VisualTestContext};

    #[gpui::test]
    fn large_folder_hover_work_is_bounded_by_the_viewport(cx: &mut TestAppContext) {
        let mut timings = Vec::new();
        for count in [32, 5000] {
            let fixture = Fixture::new(&["pack"]);
            let (window, _) = fixture.open_sized(cx, size(px(1600.), px(900.)), |root| {
                root.selected_id = Some("pack".into());
                root.instance_ui.detail_tab = DetailTab::Files;
                let tree = &mut root
                    .files_ui
                    .editors
                    .entry("pack".into())
                    .or_default()
                    .explorer;
                tree.loaded = true;
                tree.children.insert(
                    String::new(),
                    (0..count)
                        .map(|index| MinecraftEntry {
                            name: format!("file-{index:05}.cfg"),
                            rel_path: format!("file-{index:05}.cfg"),
                            is_dir: false,
                            has_persistent_override: false,
                            editable: true,
                            too_large_to_edit: false,
                        })
                        .collect(),
                );
            });
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            let first = visual
                .debug_bounds("minecraft-file:file-00000.cfg")
                .unwrap()
                .center();
            let second = visual
                .debug_bounds("minecraft-file:file-00001.cfg")
                .unwrap()
                .center();
            visual.simulate_mouse_move(first, None, Default::default());
            let start = std::time::Instant::now();
            for index in 0..8 {
                visual.simulate_mouse_move(
                    if index % 2 == 0 { second } else { first },
                    None,
                    Default::default(),
                );
            }
            let per_frame_ms = start.elapsed().as_secs_f64() * 1000. / 8.;
            eprintln!("Explorer {count} entries: {per_frame_ms:.2} ms per hover redraw");
            timings.push(per_frame_ms);
            fixture.cleanup();
        }
        assert!(
            timings[1] <= timings[0] * 5. + 20.,
            "Large-folder hover redraws scale with directory size: {timings:?} ms"
        );
    }
}
impl Default for Explorer {
    fn default() -> Self {
        Self {
            loaded: false,
            root_open: true,
            children: HashMap::new(),
            loading: HashSet::new(),
            expanded: HashSet::new(),
            generation: 0,
            row: None,
            focus: None,
            scroll: UniformListScrollHandle::new(),
            width: 280.,
            menu_anchor: std::rc::Rc::new(std::cell::Cell::new(gpui::point(px(0.), px(44.)))),
            resizing: None,
            search: Default::default(),
        }
    }
}
#[derive(Clone)]
struct Row {
    entry: MinecraftEntry,
    depth: usize,
}
impl Explorer {
    fn rows(&self) -> Vec<Row> {
        if !self.search.query.trim().is_empty() {
            return self
                .search
                .results
                .entries
                .iter()
                .cloned()
                .map(|entry| Row { entry, depth: 0 })
                .collect();
        }
        if !self.root_open {
            return Vec::new();
        }
        let mut rows = Vec::new();
        let mut pending: Vec<_> = self
            .children
            .get("")
            .into_iter()
            .flatten()
            .rev()
            .map(|entry| Row {
                entry: entry.clone(),
                depth: 0,
            })
            .collect();
        while let Some(row) = pending.pop() {
            if row.entry.is_dir && self.expanded.contains(&row.entry.rel_path) {
                if let Some(children) = self.children.get(&row.entry.rel_path) {
                    pending.extend(children.iter().rev().map(|entry| Row {
                        entry: entry.clone(),
                        depth: row.depth + 1,
                    }));
                }
            }
            rows.push(row);
        }
        rows
    }
}
fn file_icon(entry: &MinecraftEntry, expanded: bool) -> (&'static str, u32) {
    if entry.is_dir {
        return (if expanded { "folder-open" } else { "folder" }, 0xc6a56b);
    }
    let ext = entry
        .name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "json" => ("file-braces", 0xd2ac62),
        "cfg" | "conf" | "properties" | "ini" | "toml" | "yaml" | "yml" => ("settings", 0x88b8d6),
        "zs" | "js" | "lua" | "xml" | "sh" | "bat" => ("file-code", 0x90c5ae),
        "png" | "jpg" | "jpeg" | "gif" | "webp" => ("image", 0xb8a4d6),
        "zip" | "jar" | "gz" => ("archive", 0xc6a56b),
        _ => ("file-text", 0x9ca8b7),
    }
}
impl LauncherRoot {
    pub(super) fn refresh_explorer(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(draft) = self.files_ui.editors.get(&id) {
            draft.explorer.search.index.invalidate();
        }
        let query = self
            .files_ui
            .editors
            .get(&id)
            .map(|d| d.explorer.search.query.clone())
            .unwrap_or_default();
        if !query.trim().is_empty() {
            self.search_explorer(id.clone(), query, cx);
        }
        let tree = &mut self
            .files_ui
            .editors
            .entry(id.clone())
            .or_default()
            .explorer;
        tree.loaded = true;
        tree.generation += 1;
        tree.loading.clear();
        tree.children.clear();
        let mut folders: Vec<_> = tree.expanded.iter().cloned().collect();
        folders.sort();
        folders.insert(0, String::new());
        for folder in folders {
            self.load_explorer_folder(id.clone(), folder, cx);
        }
    }
    fn load_explorer_folder(&mut self, id: String, folder: String, cx: &mut Context<Self>) {
        let tree = &mut self.files_ui.editors.get_mut(&id).unwrap().explorer;
        if !tree.loading.insert(folder.clone()) {
            return;
        }
        let generation = tree.generation;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let instance = instance_id(&id).to_owned();
        let requested = folder.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { files::list_entries(&paths, &instance, &requested).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                let Some(draft) = v.files_ui.editors.get_mut(&id) else {
                    return;
                };
                if draft.explorer.generation != generation {
                    return;
                }
                draft.explorer.loading.remove(&folder);
                match result {
                    Ok(Ok(entries)) => {
                        draft.explorer.children.insert(folder, entries);
                    }
                    Ok(Err(e)) => draft.error = Some(format!("Could not read folder: {e:#}")),
                    Err(e) => draft.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn toggle_explorer_folder(
        &mut self,
        id: String,
        folder: String,
        cx: &mut Context<Self>,
    ) {
        let tree = &mut self
            .files_ui
            .editors
            .entry(id.clone())
            .or_default()
            .explorer;
        tree.row = Some(folder.clone());
        if !tree.expanded.remove(&folder) {
            tree.expanded.insert(folder.clone());
            if !tree.children.contains_key(&folder) {
                self.load_explorer_folder(id, folder, cx);
            }
        }
        cx.notify();
    }
    fn explorer_key(&mut self, id: &str, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "menu"
            || (event.keystroke.key == "f10" && event.keystroke.modifiers.shift)
        {
            let tree = &self.files_ui.editors[id].explorer;
            let rows = tree.rows();
            if let Some(row) = rows
                .iter()
                .find(|r| tree.row.as_ref() == Some(&r.entry.rel_path))
                .or_else(|| rows.first())
            {
                self.open_file_menu(id.to_owned(), row.entry.clone(), tree.menu_anchor.get(), cx);
                cx.stop_propagation();
            }
            return;
        }
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.alt
        {
            return;
        }
        let tree = &mut self.files_ui.editors.get_mut(id).unwrap().explorer;
        let rows = tree.rows();
        if rows.is_empty() {
            return;
        }
        let current = tree
            .row
            .as_ref()
            .and_then(|path| rows.iter().position(|r| &r.entry.rel_path == path));
        let index = current.unwrap_or(0);
        let row = &rows[index];
        let next = match event.keystroke.key.as_str() {
            "down" => Some(current.map_or(0, |index| (index + 1).min(rows.len() - 1))),
            "up" => Some(index.saturating_sub(1)),
            "home" => Some(0),
            "end" => Some(rows.len() - 1),
            "left" => {
                if row.entry.is_dir && tree.expanded.remove(&row.entry.rel_path) {
                    None
                } else {
                    row.entry.rel_path.rsplit_once('/').and_then(|(parent, _)| {
                        rows.iter().position(|r| r.entry.rel_path == parent)
                    })
                }
            }
            "right" => {
                if row.entry.is_dir && !tree.expanded.contains(&row.entry.rel_path) {
                    self.toggle_explorer_folder(id.to_owned(), row.entry.rel_path.clone(), cx);
                    cx.stop_propagation();
                    return;
                }
                if row.entry.is_dir && rows.get(index + 1).is_some_and(|r| r.depth > row.depth) {
                    Some(index + 1)
                } else {
                    None
                }
            }
            "enter" | "space" => {
                if !row.entry.is_dir {
                    // Opening an editor can shrink the browser viewport. Keep
                    // keyboard selection visible, just like pointer selection.
                    tree.scroll
                        .scroll_to_item_strict(index, ScrollStrategy::Top);
                }
                self.navigate_file(id.to_owned(), row.entry.clone(), cx);
                cx.stop_propagation();
                return;
            }
            _ => return,
        };
        if let Some(index) = next {
            tree.row = Some(rows[index].entry.rel_path.clone());
            tree.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn file_explorer(
        &mut self,
        id: &str,
        panel_width: f32,
        wide: bool,
        has_selection: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let tree = &mut self.files_ui.editors.get_mut(id).unwrap().explorer;
        let focus = tree.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        let rows = tree.rows();
        let expanded = tree.expanded.clone();
        let loading = tree.loading.clone();
        let scroll = tree.scroll.clone();
        let menu_anchor = tree.menu_anchor.clone();
        let selected = tree
            .row
            .clone()
            .or_else(|| self.files_ui.editors[id].selected.clone());
        let width = self.files_ui.editors[id]
            .explorer
            .width
            .min((panel_width * 0.55).max(220.));
        let refresh = id.to_owned();
        let collapse = id.to_owned();
        let popout = id.to_owned();
        let keyboard = id.to_owned();
        let root_id = id.to_owned();
        let root_open = self.files_ui.editors[id].explorer.root_open;
        let searching = !self.files_ui.editors[id]
            .explorer
            .search
            .query
            .trim()
            .is_empty();
        let search = &self.files_ui.editors[id].explorer.search;
        let search_status = if search.pending {
            "Searching…".to_owned()
        } else if let Some(error) = &search.error {
            error.clone()
        } else if search.results.entries.is_empty() {
            "No files match".to_owned()
        } else {
            format!(
                "{}{} files{}",
                if search.results.truncated {
                    "First "
                } else {
                    ""
                },
                search.results.entries.len(),
                if search.results.truncated {
                    " — narrow your search"
                } else if search.results.unreadable > 0 {
                    " — some folders could not be read"
                } else {
                    ""
                }
            )
        };
        let detached = id.contains('\0');
        let header = div()
            .h(px(34.))
            .flex_shrink_0()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(self.theme.muted_foreground())
                    .child("EXPLORER"),
            )
            .when(!detached, |header| {
                header
                    .child(
                        icon_btn(&self.theme, "refresh-cw", ButtonVariant::Ghost)
                            .id(format!("explorer-refresh:{id}"))
                            .size(px(24.))
                            .aria_label("Refresh file tree")
                            .debug_selector(|| "minecraft-tree-refresh".into())
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.refresh_explorer(refresh.clone(), cx)
                            })),
                    )
                    .child(
                        icon_btn(&self.theme, "fold-vertical", ButtonVariant::Ghost)
                            .id(format!("explorer-collapse:{id}"))
                            .size(px(24.))
                            .aria_label("Collapse all folders")
                            .debug_selector(|| "minecraft-tree-collapse".into())
                            .on_click(cx.listener(move |v, _, _, cx| {
                                let tree =
                                    &mut v.files_ui.editors.get_mut(&collapse).unwrap().explorer;
                                tree.expanded.clear();
                                tree.row = None;
                                tree.scroll.scroll_to_item(0, ScrollStrategy::Top);
                                cx.notify();
                            })),
                    )
                    .child(
                        icon_btn(&self.theme, "external-link", ButtonVariant::Ghost)
                            .id(format!("explorer-popout:{id}"))
                            .debug_selector(|| "minecraft-popout".into())
                            .size(px(24.))
                            .aria_label("Pop out file editor")
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.popout_file_editor(popout.clone(), cx)
                            })),
                    )
            });
        // Only create elements for the requested viewport. Hover and game-log
        // notifications must not lay out every file in an expanded directory.
        let render_id = id.to_owned();
        let render_focus = focus.clone();
        let row_loading = loading.clone();
        let items = uniform_list(
            format!("minecraft-file-viewport:{id}"),
            rows.len(),
            cx.processor(move |v, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|index| {
                        let row = rows[index].clone();
                        let id = render_id.as_str();
                        let entry = row.entry;
                        let open = expanded.contains(&entry.rel_path);
                        let pending = row_loading.contains(&entry.rel_path);
                        let chosen = selected.as_ref() == Some(&entry.rel_path);
                        let (glyph, color) = file_icon(&entry, open);
                        let target = id.to_owned();
                        let path = entry.rel_path.clone();
                        let selector = format!("minecraft-file:{path}");
                        let row_focus = render_focus.clone();
                        let context_focus = row_focus.clone();
                        let context_id = target.clone();
                        let context_entry = entry.clone();
                        let measured = menu_anchor.clone();
                        let label = format!(
                            "{}{}{}",
                            if searching {
                                &entry.rel_path
                            } else {
                                &entry.name
                            },
                            if entry.has_persistent_override {
                                ", persistent override"
                            } else {
                                ""
                            },
                            if !entry.is_dir && !entry.editable {
                                ", read-only"
                            } else {
                                ""
                            }
                        );
                        let mut control = btn(&v.theme, "", ButtonVariant::Ghost)
                            .id(format!("explorer-row:{id}:{path}"))
                            .role(gpui::accesskit::Role::TreeItem)
                            .aria_label(label)
                            .aria_selected(chosen)
                            .when(entry.is_dir, |c| c.aria_expanded(open))
                            .tab_stop(false)
                            .debug_selector(move || selector.clone().into())
                            .h(px(28.))
                            .w_full()
                            .flex_shrink_0()
                            .relative()
                            .when(chosen, |c| {
                                c.child(
                                    gpui::canvas(
                                        move |bounds, _, _| {
                                            measured.set(
                                                bounds.origin
                                                    + gpui::point(px(0.), bounds.size.height),
                                            )
                                        },
                                        |_, _, _, _| {},
                                    )
                                    .absolute()
                                    .inset_0(),
                                )
                            })
                            .rounded(px(0.))
                            .px_1()
                            .pl(px(8. + row.depth as f32 * 16.))
                            .gap(px(5.))
                            .justify_start()
                            .text_xs()
                            .font_weight(gpui::FontWeight::NORMAL)
                            .overflow_hidden()
                            .when(chosen, |c| {
                                c.bg(crate::ui::tint(v.theme.primary(), 0.17))
                                    .border_l_2()
                                    .border_color(v.theme.primary())
                            })
                            .child(div().w(px(12.)).flex_shrink_0().when(entry.is_dir, |c| {
                                c.child(icon(
                                    if pending {
                                        "loader-circle"
                                    } else if open {
                                        "chevron-down"
                                    } else {
                                        "chevron-right"
                                    },
                                    12.,
                                ))
                            }))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(if v.theme.dark {
                                        gpui::rgb(color)
                                    } else {
                                        v.theme.muted_foreground()
                                    })
                                    .child(icon(glyph, 14.)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(if searching {
                                        entry.rel_path.clone()
                                    } else {
                                        entry.name.clone()
                                    }),
                            )
                            .when(entry.has_persistent_override, |c| {
                                c.child(
                                    div()
                                        .size(px(5.))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(v.theme.primary()),
                                )
                            })
                            .on_mouse_down(
                                gpui::MouseButton::Right,
                                cx.listener(move |v, event: &gpui::MouseDownEvent, window, cx| {
                                    window.focus(&context_focus, cx);
                                    v.open_file_menu(
                                        context_id.clone(),
                                        context_entry.clone(),
                                        event.position,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .on_click(cx.listener(move |v, _, window, cx| {
                                window.focus(&row_focus, cx);
                                let tree =
                                    &mut v.files_ui.editors.get_mut(&target).unwrap().explorer;
                                tree.row = Some(path.clone());
                                if !entry.is_dir {
                                    tree.scroll
                                        .scroll_to_item_strict(index, ScrollStrategy::Top);
                                }
                                v.navigate_file(target.clone(), entry.clone(), cx);
                                cx.notify();
                            }));
                        for level in 0..row.depth {
                            control = control.child(
                                div()
                                    .absolute()
                                    .left(px(14. + level as f32 * 16.))
                                    .top_0()
                                    .bottom_0()
                                    .w(px(1.))
                                    .bg(crate::ui::tint(v.theme.border(), 0.55)),
                            );
                        }
                        control
                    })
                    .collect()
            }),
        )
        .track_scroll(&scroll)
        .flex_1()
        .min_h_0();
        let list = div()
            .id("minecraft-file-list")
            .debug_selector(|| "minecraft-file-list".into())
            .role(gpui::accesskit::Role::Tree)
            .aria_label("Minecraft files")
            .track_focus(&focus)
            .tab_stop(true)
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .on_key_down(
                cx.listener(move |v, e: &KeyDownEvent, _, cx| v.explorer_key(&keyboard, e, cx)),
            )
            .child(items);
        div()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(self.theme.card())
            .border_r_1()
            .border_color(self.theme.border())
            .when(wide, |c| c.w(px(width)).flex_shrink_0())
            .when(!wide && has_selection, |c| c.h(px(140.)).flex_shrink_0())
            .when(!wide && !has_selection, |c| c.flex_1())
            .child(header)
            .when(!detached, |c| c.child(self.explorer_search_bar(id, cx)))
            .when(searching, |c| {
                c.child(
                    div()
                        .px_2()
                        .pb_1()
                        .text_xs()
                        .text_color(self.theme.muted_foreground())
                        .child(search_status),
                )
            })
            .when(!searching, |c| {
                c.child(
                    btn(&self.theme, "", ButtonVariant::Ghost)
                        .id(format!("explorer-root:{id}"))
                        .aria_label("Minecraft root folder")
                        .aria_expanded(root_open)
                        .h(px(28.))
                        .w_full()
                        .rounded(px(0.))
                        .px_2()
                        .gap_1()
                        .justify_start()
                        .text_xs()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(icon(
                            if root_open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            12.,
                        ))
                        .child(".MINECRAFT")
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.files_ui
                                .editors
                                .get_mut(&root_id)
                                .unwrap()
                                .explorer
                                .root_open = !root_open;
                            cx.notify();
                        })),
                )
            })
            .when(!detached, |c| c.child(list))
            .when(loading.contains(""), |c| {
                c.child(
                    div()
                        .flex_shrink_0()
                        .p_2()
                        .text_xs()
                        .text_color(self.theme.muted_foreground())
                        .child("Loading files..."),
                )
            })
    }
}
