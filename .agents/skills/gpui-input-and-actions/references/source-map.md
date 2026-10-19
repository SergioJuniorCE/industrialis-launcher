# Source map

Reviewed 2026-10-06. Signature compatibility requires checking the local pin.

- [Key dispatch](https://github.com/zed-industries/zed/blob/main/crates/gpui/docs/key_dispatch.md): actions, action handlers, key contexts, and context-scoped key bindings. Treat its keymap examples as framework concepts; a standalone app may register bindings directly.
- [Input example](https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs): focus ownership, selection and marked text, input-handler integration, clipboard actions, and text shaping/painting. Read the relevant methods rather than copying the entire example.
- [GPUI examples](https://gpui.rs/examples/): links to runnable input, counter, and testing examples; use those to find the current demonstration relevant to the task.

For the pinned Industrialis snapshot, search the registry's platform.rs,
window.rs, app.rs, and input example for EntityInputHandler, ElementInputHandler,
UTF16Selection, FocusHandle, KeyBinding, and ClipboardItem before implementing
their callbacks. Upstream main may expose different types or accessibility
helpers. Keep editing logic testable independently of a GPU window, and reserve
native verification for focus, dispatch, composition, clipboard, and rendering.
