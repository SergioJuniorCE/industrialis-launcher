use super::super::{test_support::Fixture, DetailTab};
use super::*;
use gpui::{ClipboardItem, TestAppContext, VisualTestContext};

#[gpui::test]
async fn native_search_finds_collapsed_files_opens_results_and_preserves_drafts(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(&["pack", "other"]);
    let config = fixture
        .paths
        .instance_dir("pack")
        .unwrap()
        .join(".minecraft/config/nested");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("Café.cfg"), "general {\n B:enabled=true\n}\n").unwrap();
    let (window, root) = fixture.open(cx, |v| {
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
    cx.write_to_clipboard(ClipboardItem::new_string("NESTED\\CAFÉ".into()));
    {
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let bounds = visual.debug_bounds("minecraft-file-search").unwrap();
        visual.simulate_click(bounds.center(), Default::default());
        visual.simulate_keystrokes("ctrl-v");
    }
    fixture
        .until(cx, &root, |v| {
            let search = &v.files_ui.editors["pack"].explorer.search;
            search.query == "NESTED\\CAFÉ" && !search.pending
        })
        .await;
    assert_eq!(
        root.read_with(cx, |v, _| v.files_ui.editors["pack"]
            .explorer
            .search
            .results
            .entries
            .len()),
        1
    );
    Fixture::click(cx, window, "minecraft-file:config/nested/Café.cfg");
    fixture
        .until(cx, &root, |v| !v.files_ui.editors["pack"].loading)
        .await;
    assert_eq!(
        root.read_with(cx, |v, _| v.files_ui.editors["pack"].selected.clone()),
        Some("config/nested/Café.cfg".into())
    );
    root.update(cx, |v, cx| {
        v.files_ui.editors.get_mut("pack").unwrap().content = "unsaved draft".into();
        v.search_explorer("pack".into(), "missing".into(), cx);
        v.search_explorer("pack".into(), "café".into(), cx);
    });
    fixture
        .until(cx, &root, |v| {
            !v.files_ui.editors["pack"].explorer.search.pending
        })
        .await;
    assert_eq!(
        root.read_with(cx, |v, _| v.files_ui.editors["pack"]
            .explorer
            .search
            .results
            .entries
            .len()),
        1
    );
    Fixture::click(cx, window, "minecraft-search-clear");
    assert!(root.read_with(cx, |v, _| v.files_ui.editors["pack"]
        .explorer
        .search
        .query
        .is_empty()
        && v.files_ui.editors["pack"].content == "unsaved draft"));
    root.update(cx, |v, cx| {
        v.search_explorer("pack".into(), "missing".into(), cx)
    });
    fixture
        .until(cx, &root, |v| {
            !v.files_ui.editors["pack"].explorer.search.pending
        })
        .await;
    assert!(root.read_with(cx, |v, _| v.files_ui.editors["pack"]
        .explorer
        .search
        .results
        .entries
        .is_empty()));
    fixture.cleanup();
}
