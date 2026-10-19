# Source map

Reviewed 2026-10-06. Recheck current pins before copying any signatures.

- [GPUI README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md): application startup, renderable entities, declarative elements, and platform feature choices.
- [Contexts](https://github.com/zed-industries/zed/blob/main/crates/gpui/docs/contexts.md): App, Context<T>, async contexts, test contexts, Window, and Entity<T> ownership.
- [gpui.rs](https://github.com/zed-industries/zed/blob/main/crates/gpui/src/gpui.rs): public exports, AppContext and VisualContext traits, and test-support feature gates. Follow a symbol's re-export to its defining module rather than guessing its API.

For Industrialis, first read apps/launcher/crates/app/src/main.rs and
state.rs. The tokio Runtime is shared through AppData. Existing UI operations
spawn backend work and return through a weak view update. Preserve errors from
both the backend result and the task join. Do not silently turn failed snapshot
loads into empty data when that could discard or conceal user state.

The local Cargo registry contains gpui-pre-0.3.8/src/{app,window,platform}.rs.
Find the registry root from the environment instead of hardcoding a user's path.
Check signatures there and use cargo check --locked from apps/launcher.
