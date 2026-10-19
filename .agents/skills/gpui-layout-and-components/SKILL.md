---
name: gpui-layout-and-components
description: Build or port GPUI layouts, styled controls, scrollable lists, dialogs, images, and popovers. Use when matching an existing UI or adapting Zed UI patterns.
---

# GPUI layout and components

Start from the requested interface and the repository's existing theme and
primitives. Preserve product behavior, labels, dimensions, navigation, and
selection states when the task is a UI port. Read Cargo.toml and Cargo.lock;
verify methods against the pinned GPUI source before using upstream examples.

1. Map the reference into view state, reusable controls, and element trees.
   Use declarative elements for ordinary layouts; introduce custom Element
   implementations only for behavior the existing primitives cannot express.
2. Translate spacing and flex/grid intent into supported GPUI styles. Do not
   assume browser CSS, DOM semantics, or utility-class compatibility.
3. Keep stable element IDs and scroll handles where state must survive a
   render. Check the type after id(): styled stateful elements may have a
   different return type from a plain Div.
4. Constrain scrolling regions inside their parents. Verify shrink behavior,
   minimum sizes, long labels, and empty states at the application's smallest
   supported window as well as its default size.
5. Check popup bounds, outside-click handling, and paint order. Consult the
   pinned deferred/overlay APIs when a later sibling covers a popup.
6. Use Zed UI source to study composition and interactions. Reuse its code
   only after checking dependencies, licensing, and API compatibility. Its ui
   crate is a separate component layer, not automatically part of gpui.
7. Compile, then inspect the native interface when tooling permits. Report
   unavailable visual checks explicitly; a process staying alive proves boot,
   not visual or interaction parity.

Read [references/source-map.md](references/source-map.md) when selecting an
example or locating a component implementation.
