//! Launcher backend services ported from `apps/launcher/electron/backend/`.
//!
//! Async (tokio) I/O and network live here; pure parsing/merge logic stays in
//! `industrialis-core` so it can be tested without a runtime.

pub mod accounts;
pub mod console_log;
pub mod config_presets;
pub mod fs_utils;
pub mod game_broker;
mod game_capture;
pub mod groups;
pub mod http;
pub mod icons;
pub mod install;
pub mod instance_guard;
pub mod instance_ops;
pub mod instances;
pub mod java;
pub mod launch;
pub mod launch_config;
pub mod lifecycle;
mod legacy_launcher;
pub mod migration;
pub mod minecraft_files;
pub mod minecraft_file_search;
pub mod file_actions;
pub mod pack_cache;
pub mod pack_files;
pub mod pack_prep;
pub mod paths;
pub mod process_manager;
pub mod profile_owner;
mod profile_store;
pub mod release_updater;
pub mod running_pids;
pub mod settings;

pub mod custom_mods;
pub mod content;
mod content_archive;
pub mod modernity;

pub mod credentials;

pub mod backups;

pub mod backup_manager;
pub mod google_drive;

#[cfg(any(test, target_os = "macos", target_os = "linux"))]
mod credential_unix;
mod component_metadata;
mod native_libraries;

pub mod openvsx;
mod theme_archive;
