//! Positional launch slots: parse, validate, display and resolve shortcuts.

use super::*;
use gpui::Keystroke;

pub(super) const SLOT_COUNT: usize = industrialis_core::launcher_settings::LAUNCH_SLOT_COUNT;

pub(super) fn is_modifier_key(keystroke: &Keystroke) -> bool {
    matches!(
        keystroke.key.as_str(),
        "control"
            | "ctrl"
            | "alt"
            | "shift"
            | "platform"
            | "cmd"
            | "super"
            | "win"
            | "function"
            | "fn"
    )
}

fn has_modifier(keystroke: &Keystroke) -> bool {
    keystroke.modifiers.control
        || keystroke.modifiers.alt
        || keystroke.modifiers.shift
        || keystroke.modifiers.platform
}

/// The launcher owns these combinations; a slot must not shadow them.
fn reserved(keystroke: &Keystroke) -> bool {
    let secondary =
        (keystroke.modifiers.control || keystroke.modifiers.platform) && !keystroke.modifiers.alt;
    secondary
        && matches!(
            keystroke.key.as_str(),
            "a" | "c" | "f" | "s" | "v" | "x" | "y" | "z"
        )
        || keystroke.key == "tab"
        || keystroke.key == "escape"
        || (keystroke.key == "f10" && keystroke.modifiers.shift)
}

pub(super) fn same(a: &Keystroke, b: &Keystroke) -> bool {
    a.modifiers == b.modifiers && a.key.eq_ignore_ascii_case(&b.key)
}

/// Positional slots after dropping unbound, corrupt, reserved and duplicate
/// entries; the first valid owner of a combo keeps it.
pub(super) fn resolved(slots: &[Option<String>; SLOT_COUNT]) -> [Option<Keystroke>; SLOT_COUNT] {
    let mut resolved: [Option<Keystroke>; SLOT_COUNT] = std::array::from_fn(|_| None);
    for (index, raw) in slots.iter().enumerate() {
        let Some(keystroke) = raw.as_deref().and_then(|raw| Keystroke::parse(raw).ok()) else {
            continue;
        };
        if is_modifier_key(&keystroke) || !has_modifier(&keystroke) || reserved(&keystroke) {
            continue;
        }
        if resolved
            .iter()
            .flatten()
            .any(|existing| same(existing, &keystroke))
        {
            continue;
        }
        resolved[index] = Some(keystroke);
    }
    resolved
}

pub(super) fn validate(
    keystroke: &Keystroke,
    slots: &[Option<Keystroke>; SLOT_COUNT],
    slot: usize,
) -> Result<(), String> {
    if is_modifier_key(keystroke) || !has_modifier(keystroke) {
        return Err("Hold Ctrl, Alt, Cmd or Shift with a key.".into());
    }
    if reserved(keystroke) {
        return Err("That combination is used by the launcher.".into());
    }
    if let Some((owner, _)) = slots.iter().enumerate().find(|(index, combo)| {
        *index != slot && combo.as_ref().is_some_and(|combo| same(combo, keystroke))
    }) {
        return Err(format!("Already used by slot {}.", owner + 1));
    }
    Ok(())
}

fn key_label(key: &str) -> String {
    match key {
        "escape" => "Esc".into(),
        "space" => "Space".into(),
        "pageup" => "Page Up".into(),
        "pagedown" => "Page Down".into(),
        "backspace" => "Backspace".into(),
        "menu" => "Menu".into(),
        _ if key.len() == 1 => key.to_uppercase(),
        _ => {
            let mut chars = key.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => key.to_string(),
            }
        }
    }
}

pub(super) fn display(keystroke: &Keystroke) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(5);
    if keystroke.modifiers.control {
        parts.push("Ctrl".into());
    }
    if keystroke.modifiers.alt {
        parts.push("Alt".into());
    }
    if keystroke.modifiers.shift {
        parts.push("Shift".into());
    }
    if keystroke.modifiers.platform {
        parts.push(
            if cfg!(target_os = "macos") {
                "Cmd"
            } else if cfg!(windows) {
                "Win"
            } else {
                "Super"
            }
            .into(),
        );
    }
    parts.push(key_label(&keystroke.key));
    parts.join("+")
}

pub(super) fn chip_label(keystroke: &Keystroke) -> String {
    key_label(&keystroke.key)
}

impl LauncherRoot {
    pub(super) fn launch_slots(&self) -> [Option<Keystroke>; SLOT_COUNT] {
        resolved(&self.data.settings.instance_shortcuts)
    }

    pub(super) fn launch_shortcut_slot(&self, keystroke: &Keystroke) -> Option<usize> {
        self.launch_slots()
            .iter()
            .position(|combo| combo.as_ref().is_some_and(|combo| same(combo, keystroke)))
    }

    /// Chip labels for the first nine cards, keyed by instance id.
    pub(super) fn launch_slot_hints(&self) -> HashMap<String, String> {
        self.launch_slot_ids()
            .into_iter()
            .take(SLOT_COUNT)
            .zip(self.launch_slots())
            .filter_map(|(id, combo)| combo.map(|combo| (id, chip_label(&combo))))
            .collect()
    }

    pub(super) fn activate_launch_slot(&mut self, slot: usize, cx: &mut Context<Self>) {
        let Some(id) = self.launch_slot_ids().get(slot).cloned() else {
            return;
        };
        if self.instance_is_busy(&id) {
            self.tab = Tab::Instances;
            self.selected_id = Some(id.clone());
            self.instance_ui.detail_tab = DetailTab::Console;
            self.select_instance(id, cx);
        } else {
            self.play(id, cx);
        }
    }

    pub(super) fn write_launch_slots(
        &mut self,
        slots: [Option<Keystroke>; SLOT_COUNT],
        cx: &mut Context<Self>,
    ) {
        self.data.settings.instance_shortcuts =
            std::array::from_fn(|index| slots[index].as_ref().map(Keystroke::unparse));
        self.preferences_ui.capture_slot = None;
        self.preferences_ui.shortcut_error = None;
        self.persist_launcher_preferences(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ks(raw: &str) -> Keystroke {
        Keystroke::parse(raw).unwrap()
    }

    #[test]
    fn resolved_skips_unbound_invalid_reserved_and_duplicate_slots() {
        let slots: [Option<String>; SLOT_COUNT] = std::array::from_fn(|slot| match slot {
            0 => Some("ctrl-1".into()),
            1 => Some("ctrl-1".into()),
            2 => Some("ctrl-s".into()),
            3 => Some("garbage".into()),
            4 => Some("ctrl-shift-l".into()),
            _ => None,
        });
        let resolved = resolved(&slots);
        assert_eq!(resolved.iter().flatten().count(), 2);
        assert_eq!(resolved[0].as_ref().unwrap().unparse(), "ctrl-1");
        assert_eq!(resolved[4].as_ref().unwrap().unparse(), "ctrl-shift-l");
        assert!(resolved[1..4].iter().all(Option::is_none));
    }

    #[test]
    fn validation_rejects_reserved_duplicates_and_missing_modifiers() {
        let mut slots: [Option<Keystroke>; SLOT_COUNT] = std::array::from_fn(|_| None);
        slots[0] = Some(ks("ctrl-1"));
        assert!(validate(&ks("ctrl-shift-l"), &slots, 1).is_ok());
        assert!(validate(&ks("ctrl-1"), &slots, 1).is_err());
        assert!(validate(&ks("ctrl-s"), &slots, 1).is_err());
        assert!(validate(&ks("1"), &slots, 1).is_err());
        assert!(validate(&ks("shift-f10"), &slots, 1).is_err());
        assert!(validate(&ks("ctrl-1"), &slots, 0).is_ok());
    }

    #[test]
    fn display_and_chip_labels_are_readable() {
        assert_eq!(display(&ks("ctrl-shift-l")), "Ctrl+Shift+L");
        assert_eq!(
            display(&ks("secondary-1")),
            if cfg!(target_os = "macos") {
                "Cmd+1"
            } else {
                "Ctrl+1"
            }
        );
        assert_eq!(chip_label(&ks("ctrl-3")), "3");
        assert_eq!(chip_label(&ks("ctrl-shift-escape")), "Esc");
    }

    #[gpui::test]
    async fn slots_follow_library_order_and_running_targets_open_the_console(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::views::test_support::Fixture;
        use gpui::{MouseButton, VisualTestContext};

        let fixture = Fixture::new(&["a", "b", "c"]);
        fixture
            .rt
            .block_on(industrialis_backend::fs_utils::write_json(
                &fixture.paths.groups_path(),
                &serde_json::json!({
                    "formatVersion": 1,
                    "groups": {"Games": {"instances": ["c"]}},
                    "ungrouped": {"name": "Main", "instances": ["b", "a"]},
                }),
            ))
            .unwrap();
        let (window, root) = fixture.open(cx, |root| {
            root.data.running.insert("b".into(), (4242, true));
        });
        assert_eq!(
            root.read_with(cx, |v, _| v.launch_slot_ids()),
            vec!["c".to_string(), "b".to_string(), "a".to_string()]
        );
        let hints = root.read_with(cx, |v, _| v.launch_slot_hints());
        assert_eq!(hints.get("b").map(String::as_str), Some("2"));
        // Focus the library the same way a user does, then press the slot.
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            assert!(visual.debug_bounds("launch-slot:c").is_some());
            assert!(visual.debug_bounds("launch-slot:b").is_some());
            let library = visual.debug_bounds("instance-library").unwrap();
            let blank = library.origin + gpui::point(px(24.), library.size.height - px(24.));
            visual.simulate_mouse_down(blank, MouseButton::Right, Default::default());
            visual.simulate_mouse_up(blank, MouseButton::Right, Default::default());
            visual.simulate_keystrokes("escape");
            visual.simulate_keystrokes("ctrl-2");
        }
        root.read_with(cx, |v, _| {
            assert_eq!(v.selected_id.as_deref(), Some("b"));
            assert!(v.instance_ui.detail_tab == DetailTab::Console);
            assert!(v.data.launching.is_empty());
        });
        fixture.cleanup();
    }
}
