use super::super::test_support::Fixture;
use super::*;
use gpui::{size, TestAppContext, VisualTestContext};

#[gpui::test]
fn easy_mode_hover_cost_is_bounded_by_visible_fields(cx: &mut TestAppContext) {
    let mut timings = Vec::new();
    for count in [16, 500] {
        let fixture = Fixture::new(&["pack"]);
        let text = format!(
            "general {{\n{}\n}}\n",
            (0..count)
                .map(|i| {
                    if i == 0 {
                        " B:enabled=true".to_owned()
                    } else {
                        format!(" # Description for setting {i}\n I:setting{i}=42")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        );
        let (window, root) = fixture.open_sized(cx, size(px(1600.), px(900.)), |v| {
            v.selected_id = Some("pack".into());
            v.instance_ui.detail_tab = DetailTab::Files;
            let draft = v.files_ui.editors.entry("pack".into()).or_default();
            draft.selected = Some("config/test.cfg".into());
            draft.content = text.clone();
            draft.saved = text;
            draft.easy = true;
            draft.explorer.loaded = true;
        });
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let first = visual.debug_bounds("forge-value:0").unwrap().center();
        let second = visual.debug_bounds("forge-value:1").unwrap().center();
        visual.simulate_mouse_move(first, None, Default::default());
        let start = std::time::Instant::now();
        for index in 0..6 {
            visual.simulate_mouse_move(
                if index % 2 == 0 { second } else { first },
                None,
                Default::default(),
            );
        }
        let ms = start.elapsed().as_secs_f64() * 1000. / 6.;
        eprintln!("Easy mode {count} fields: {ms:.2} ms per hover redraw");
        timings.push(ms);
        let frame = visual.debug_bounds("minecraft-editor:pack").unwrap();
        let save = visual.debug_bounds("minecraft-save").unwrap();
        assert!(
            frame.contains(&save.center()) && save.bottom() <= frame.bottom(),
            "Save must remain in the editor viewport for a large config"
        );
        if count == 500 {
            assert!(
                root.read_with(cx, |v, _| v
                    .native_fields
                    .keys()
                    .filter(|key| key.starts_with("forge:"))
                    .count())
                    < 40,
                "Off-screen inputs must not be constructed"
            );
            visual.update(|window, cx| {
                root.update(cx, |v, cx| {
                    v.files_ui.editors["pack"]
                        .easy_list
                        .as_ref()
                        .unwrap()
                        .scroll_to_end();
                    cx.notify();
                });
                window.draw(cx).clear(cx);
            });
            let bounds = visual.debug_bounds("forge-value:499").unwrap();
            assert!(visual
                .debug_bounds("forge-easy")
                .unwrap()
                .contains(&bounds.center()));
            visual.simulate_click(bounds.center(), Default::default());
            visual.update(|_, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("1234".into()))
            });
            visual.simulate_keystrokes("ctrl-a ctrl-v");
            assert!(root.read_with(&visual, |v, _| v.files_ui.editors["pack"]
                .content
                .contains("I:setting499=1234")));
            assert!(
                visual
                    .debug_bounds("forge-easy")
                    .unwrap()
                    .contains(&visual.debug_bounds("forge-value:499").unwrap().center()),
                "Editing must preserve the virtual list scroll anchor"
            );
        }
        fixture.cleanup();
    }
    assert!(
        timings[1] <= timings[0] * 5. + 20.,
        "Easy-mode redraw scales with total fields: {timings:?} ms"
    );
}
