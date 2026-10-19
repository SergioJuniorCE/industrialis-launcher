# Source map

Reviewed 2026-10-06; upstream main can differ from the installed snapshot.

- [Examples](https://gpui.rs/examples/): entry points for views, styling, layout, events, and tests. Follow the linked runnable examples; validate their APIs against the local pin.
- [Zed crates](https://github.com/zed-industries/zed/tree/main/crates): navigate to the crate owning the behavior before adopting a pattern. Check its manifest for framework, theme, and application dependencies.
- [Zed UI source](https://github.com/zed-industries/zed/tree/main/crates/ui/src): component composition, style helpers, and traits. ui.rs exports components and styles and identifies ui_input as the separate single-line input component.

In Industrialis, reuse apps/launcher/crates/app/src/theme.rs and ui.rs. Compare
layouts against apps/launcher/UI_PARITY.md; the top toolbar and tab selection
colors are accepted product requirements. Put each functional panel in a
focused views module rather than growing one render function indefinitely.

The installed gpui-pre source is the place to check div.rs for interaction,
img.rs for image sizing, deferred.rs for paint ordering, and list elements for
virtualization. Keep callbacks attached to stable per-row IDs and own the row
data needed by the closure. Measure native behavior before calling a port 1:1.
