---
name: gpui-input-and-actions
description: Implement GPUI keyboard shortcuts, action dispatch, focus, text editing, clipboard handling, and IME support. Use for keyboard or text-input bugs and accessible custom controls.
---

# GPUI input and actions

Check the installed GPUI version before adopting action macros or input-handler
signatures. Read [references/source-map.md](references/source-map.md) for the
official key-dispatch guide and text-input example.

1. Separate application commands from text insertion. Bind logical actions
   through key contexts and view handlers. Scope bindings to the focused UI
   so a launcher shortcut cannot consume a key intended for a text field.
2. Keep FocusHandle values across renders, attach them to the intended
   elements, and move focus explicitly when opening or dismissing a modal.
   Verify Enter, Escape, and Tab from the actual focused element.
3. Route committed text and composition through the pinned platform input
   handler APIs when supporting general text entry. Raw key events alone do
   not establish IME, selection, or full text-editor parity.
4. Distinguish byte offsets, character/grapheme boundaries, and platform UTF-16
   ranges. Convert at API boundaries. Never split a Rust string at an
   unverified byte index.
5. Make clipboard operations and selection semantics explicit: copy, cut,
   paste, replacement, caret navigation, and deletion. Decide multiline versus
   single-line behavior before implementing Enter or accepting pasted text.
6. Expose keyboard paths and accessible meaning for custom clickable controls
   using APIs actually present in the pinned version. Check focus visibility,
   disabled behavior, and whether handled input propagates to parents.
7. Test editing invariants and action routing, then exercise native focus and
   platform input when possible. Label partial input implementations honestly.

Industrialis `crates/app/src/input.rs` wraps the pinned native gpui-base editing
engine. Its semantic field frame shares the engine's focus identity; an unrelated
frame handle can expose an Edit node while keyboard focus announces only the
window. Verify the focused native node after Tab/Shift+Tab, not just its presence.
Accessible value replacement goes through `replace_text_in_range` with UTF-16
ranges so form Change events and Undo survive. Reserve `set_value` for model
synchronization. Current native evidence and physical input/disabled-announcement
limits live in [the migration tracker](../../../apps/launcher-gpui/MIGRATION_TRACKER.md).
