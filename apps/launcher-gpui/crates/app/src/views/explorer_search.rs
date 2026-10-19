//! Per-instance native search input; directory traversal stays on backend workers.
use super::{minecraft_files::instance_id, LauncherRoot};
use crate::{
    input::{NativeField, NativeFieldEvent},
    ui::{icon_btn, ButtonVariant},
};
use gpui::{div, prelude::*, px, Context, Div, ScrollStrategy};
use industrialis_backend::minecraft_file_search::{FileIndex, SearchResults};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Default)]
pub(super) struct ExplorerSearch {
    pub query: String,
    pub results: SearchResults,
    pub pending: bool,
    pub error: Option<String>,
    revision: u64,
    cancelled: Option<Arc<AtomicBool>>,
    pub index: Arc<FileIndex>,
}

impl Drop for ExplorerSearch {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.cancelled {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}
impl LauncherRoot {
    pub(super) fn search_explorer(&mut self, id: String, query: String, cx: &mut Context<Self>) {
        let tree = &mut self
            .files_ui
            .editors
            .entry(id.clone())
            .or_default()
            .explorer;
        let search = &mut tree.search;
        if let Some(cancelled) = search.cancelled.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        search.revision += 1;
        let revision = search.revision;
        let index = search.index.clone();
        search.query = query.clone();
        search.results = Default::default();
        search.error = None;
        search.pending = !query.trim().is_empty();
        tree.row = None;
        tree.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
        if !search.pending {
            cx.notify();
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        search.cancelled = Some(cancelled.clone());
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let instance = instance_id(&id).to_owned();
        cx.spawn(async move |view, cx| {
            let completion_cancelled = cancelled.clone();
            let result = rt
                .spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
                    if cancelled.load(Ordering::Relaxed) {
                        return Ok(SearchResults::default());
                    }
                    index.search(&paths, &instance, &query, cancelled).await
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                if completion_cancelled.load(Ordering::Relaxed) {
                    return;
                }
                let Some(draft) = v.files_ui.editors.get_mut(&id) else {
                    return;
                };
                let search = &mut draft.explorer.search;
                if search.revision != revision {
                    return;
                }
                search.pending = false;
                match result {
                    Ok(Ok(results)) => search.results = results,
                    Ok(Err(e)) => search.error = Some(format!("Search failed: {e:#}")),
                    Err(e) => search.error = Some(format!("Search failed: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn explorer_search_bar(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        let key = format!("explorer-search:{id}");
        let query = self.files_ui.editors[id].explorer.search.query.clone();
        if !self.native_fields.contains_key(&key) {
            let field = cx.new(|cx| {
                NativeField::new(
                    &query,
                    self.theme.clone(),
                    "Search files by name or path",
                    false,
                    cx,
                )
            });
            field.update(cx, |field, _| {
                field.accessibility_label = "Search instance files".into()
            });
            let target = id.to_owned();
            cx.subscribe(&field, move |v, _, event, cx| {
                if let NativeFieldEvent::Change(text) = event {
                    v.search_explorer(target.clone(), text.clone(), cx);
                }
            })
            .detach();
            self.native_fields.insert(key.clone(), field);
        }
        let field = self.native_fields[&key].clone();
        field.update(cx, |state, _| {
            state.value = query.clone();
            state.theme = self.theme.clone();
        });
        let clear = id.to_owned();
        let focus_result = id.to_owned();
        div()
            .flex_shrink_0()
            .px_2()
            .pb_1()
            .flex()
            .gap_1()
            .items_center()
            .on_key_down(cx.listener(move |v, e: &gpui::KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    v.search_explorer(focus_result.clone(), String::new(), cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .debug_selector(|| "minecraft-file-search".into())
                    .child(field),
            )
            .when(!query.is_empty(), |c| {
                c.child(
                    icon_btn(&self.theme, "x", ButtonVariant::Ghost)
                        .id(format!("clear-file-search:{id}"))
                        .debug_selector(|| "minecraft-search-clear".into())
                        .aria_label("Clear file search")
                        .size(px(24.))
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.search_explorer(clear.clone(), String::new(), cx)
                        })),
                )
            })
    }
}
