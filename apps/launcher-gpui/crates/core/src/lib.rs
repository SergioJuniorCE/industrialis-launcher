//! Pure launcher logic ported from the Electron app's TypeScript.
//!
//! No GPUI dependency here: this crate is testable headless and reused by
//! the app shell and any future CLI tooling. Serde shapes match the existing
//! on-disk JSON so user data dirs stay compatible.

pub mod instance_settings;
pub mod launch_log;
pub mod launcher_settings;
pub mod launcher_window;
pub mod log_buffer;
pub mod pack_version;
