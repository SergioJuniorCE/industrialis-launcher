---
name: gpui-state-and-contexts
description: Implement GPUI application boot, entities, view state, context lifetimes, asynchronous work, and entity events. Use for GPUI state ownership or async context errors.
---

# GPUI state and contexts

Read Cargo.toml and Cargo.lock before selecting an API. Upstream Zed main is a
learning source; the installed crate source determines what compiles. In the
Industrialis port, gpui and gpui_platform are aliases for gpui-pre snapshots.
Preserve their exact compatible pins unless a dependency upgrade is requested.

1. Identify the entity owning the state and the lifetime of its window. Keep
   parsing and filesystem/network services outside Render implementations.
2. Choose the required context: App owns application state; Context<T> adds
   entity-specific notification and events; Window supplies window operations.
   A renderable entity implements Render and builds elements from its state.
3. Mutate through the owning entity, then request a redraw with cx.notify().
   Use cx.listener for callbacks requiring access to the owning view.
4. Keep borrowed contexts out of futures crossing await. Use the pinned
   version's async context and weak entity update API. Handle closed entities
   as an expected outcome rather than unwrapping asynchronous updates.
5. Run slow work on the backend runtime or background executor. Return an
   owned result to the UI. Keep task/subscription handles when their lifetime
   matters; detach only work intended to outlive the initiating callback.
6. Compile the actual application target. Verify stale-result handling when
   navigation or a newer request can supersede an in-flight operation.

For choosing source files and adapting APIs, read
[references/source-map.md](references/source-map.md).

Completion means state ownership is explicit, UI-thread blocking is avoided,
errors reach the user, and validation distinguishes compilation from native
interaction or external-service verification.
