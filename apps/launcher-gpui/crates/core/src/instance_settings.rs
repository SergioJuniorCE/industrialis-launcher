//! Port of `apps/launcher/src/lib/instance-settings.ts`.
//!
//! Note: the Electron backend's `types.ts` variant carries two extra fields
//! (`cached_size_bytes`, `custom_icon`); the frontend shape ported here is the
//! canonical settings document. Backend-only fields arrive in Phase 2.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceSettings {
    pub name: String,
    pub pack_version: String,
    pub pack_java_type: String,
    pub backups_enabled: bool,
    pub backup_retention_override: Option<u32>,
    pub java_path: Option<String>,
    pub min_ram_mb: u32,
    pub max_ram_mb: u32,
    pub perm_gen_mb: u32,
    pub jvm_args: String,
    pub auth_mode: String,
    pub username: String,
    #[serde(default)]
    pub offline_username_confirmed: bool,
    pub override_window: bool,
    pub launch_maximized: bool,
    pub window_width: u32,
    pub window_height: u32,
    pub close_after_launch: bool,
    pub quit_after_game_stop: bool,
    pub override_console: bool,
    pub show_console_on_launch: bool,
    pub show_console_on_error: bool,
    pub auto_close_console: bool,
    pub override_game_time: bool,
    pub show_game_time: bool,
    pub record_game_time: bool,
    pub total_play_seconds: u64,
    pub override_account: bool,
    pub account_id: Option<String>,
    pub join_server_on_launch: bool,
    pub join_server_address: String,
    pub override_java_location: bool,
    pub skip_java_compat: bool,
    pub override_memory: bool,
    pub override_java_args: bool,
    pub override_commands: bool,
    pub pre_launch_command: String,
    pub wrapper_command: String,
    pub post_exit_command: String,
    pub override_env: bool,
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    #[serde(default)]
    pub custom_icon: Option<String>,
}

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            pack_version: String::new(),
            pack_java_type: "java17+".to_string(),
            backups_enabled: false,
            backup_retention_override: None,
            java_path: None,
            min_ram_mb: 4096,
            max_ram_mb: 6144,
            perm_gen_mb: 128,
            jvm_args: String::new(),
            auth_mode: "offline".to_string(),
            username: String::new(),
            offline_username_confirmed: false,
            override_window: false,
            launch_maximized: false,
            window_width: 854,
            window_height: 480,
            close_after_launch: false,
            quit_after_game_stop: false,
            override_console: false,
            show_console_on_launch: false,
            show_console_on_error: true,
            auto_close_console: false,
            override_game_time: false,
            show_game_time: true,
            record_game_time: true,
            total_play_seconds: 0,
            override_account: false,
            account_id: None,
            join_server_on_launch: false,
            join_server_address: String::new(),
            override_java_location: false,
            skip_java_compat: false,
            override_memory: false,
            override_java_args: false,
            override_commands: false,
            pre_launch_command: String::new(),
            wrapper_command: String::new(),
            post_exit_command: String::new(),
            override_env: false,
            env_vars: HashMap::new(),
            custom_icon: None,
        }
    }
}

/// Mirrors `mergeInstanceSettings`: absent disk state yields defaults;
/// present state overlays defaults, with `env_vars` merged key-wise so a
/// partial overlay keeps keys it does not mention.
pub fn merge_instance_settings(disk: Option<InstanceSettings>) -> InstanceSettings {
    let Some(disk) = disk else {
        return InstanceSettings::default();
    };
    let mut merged = InstanceSettings::default();
    merged.name = disk.name;
    merged.pack_version = disk.pack_version;
    merged.pack_java_type = disk.pack_java_type;
    merged.backups_enabled = disk.backups_enabled;
    merged.backup_retention_override = disk.backup_retention_override;
    merged.java_path = disk.java_path;
    merged.min_ram_mb = disk.min_ram_mb;
    merged.max_ram_mb = disk.max_ram_mb;
    merged.perm_gen_mb = disk.perm_gen_mb;
    merged.jvm_args = disk.jvm_args;
    merged.auth_mode = disk.auth_mode;
    merged.username = disk.username;
    merged.offline_username_confirmed = disk.offline_username_confirmed;
    merged.override_window = disk.override_window;
    merged.launch_maximized = disk.launch_maximized;
    merged.window_width = disk.window_width;
    merged.window_height = disk.window_height;
    merged.close_after_launch = disk.close_after_launch;
    merged.quit_after_game_stop = disk.quit_after_game_stop;
    merged.override_console = disk.override_console;
    merged.show_console_on_launch = disk.show_console_on_launch;
    merged.show_console_on_error = disk.show_console_on_error;
    merged.auto_close_console = disk.auto_close_console;
    merged.override_game_time = disk.override_game_time;
    merged.show_game_time = disk.show_game_time;
    merged.record_game_time = disk.record_game_time;
    merged.total_play_seconds = disk.total_play_seconds;
    merged.override_account = disk.override_account;
    merged.account_id = disk.account_id;
    merged.join_server_on_launch = disk.join_server_on_launch;
    merged.join_server_address = disk.join_server_address;
    merged.override_java_location = disk.override_java_location;
    merged.skip_java_compat = disk.skip_java_compat;
    merged.override_memory = disk.override_memory;
    merged.override_java_args = disk.override_java_args;
    merged.override_commands = disk.override_commands;
    merged.pre_launch_command = disk.pre_launch_command;
    merged.wrapper_command = disk.wrapper_command;
    merged.post_exit_command = disk.post_exit_command;
    merged.override_env = disk.override_env;
    merged.env_vars.extend(disk.env_vars);
    merged.custom_icon = disk.custom_icon;
    merged
}

/// Parse a possibly-partial settings document the way the TS loader does:
/// unknown JSON with missing keys falls back to defaults field-wise.
pub fn parse_instance_settings_json(raw: &str) -> InstanceSettings {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Object(_)) => {
            // Deserialize with field defaults, then run the merge so explicit
            // `env_vars` fragments union instead of replacing.
            match serde_json::from_str::<PartialInstanceSettings>(raw) {
                Ok(partial) => partial.into_full(),
                Err(_) => InstanceSettings::default(),
            }
        }
        _ => InstanceSettings::default(),
    }
}

/// Helper for partial documents: every field optional, then overlaid.
#[derive(Debug, Default, Deserialize)]
struct PartialInstanceSettings {
    #[serde(flatten, default)]
    inner: HashMap<String, serde_json::Value>,
}

impl PartialInstanceSettings {
    fn into_full(self) -> InstanceSettings {
        let mut full = InstanceSettings::default();
        let get_str = |key: &str| self.inner.get(key).and_then(|v| v.as_str()).map(str::to_string);
        let get_bool = |key: &str| self.inner.get(key).and_then(|v| v.as_bool());
        let get_u32 = |key: &str| self.inner.get(key).and_then(|v| v.as_u64()).and_then(|v| u32::try_from(v).ok());
        let get_u64 = |key: &str| self.inner.get(key).and_then(|v| v.as_u64());
        if let Some(v) = get_str("name") {
            full.name = v;
        }
        if let Some(v) = get_str("pack_version") {
            full.pack_version = v;
        }
        if let Some(v) = get_str("pack_java_type") {
            full.pack_java_type = v;
        }
        if let Some(v) = get_bool("backups_enabled") {
            full.backups_enabled = v;
        }
        if let Some(v) = self.inner.get("backup_retention_override").and_then(|v| v.as_u64()).and_then(|v| u32::try_from(v).ok()) {
            full.backup_retention_override = Some(v);
        }
        if let Some(v) = get_str("java_path") {
            full.java_path = Some(v);
        }
        if let Some(v) = get_u32("min_ram_mb") {
            full.min_ram_mb = v;
        }
        if let Some(v) = get_u32("max_ram_mb") {
            full.max_ram_mb = v;
        }
        if let Some(v) = get_u32("perm_gen_mb") {
            full.perm_gen_mb = v;
        }
        if let Some(v) = get_str("jvm_args") {
            full.jvm_args = v;
        }
        if let Some(v) = get_str("auth_mode") {
            full.auth_mode = v;
        }
        if let Some(v) = get_str("username") {
            full.username = v;
        }
        if let Some(v) = get_bool("offline_username_confirmed") {
            full.offline_username_confirmed = v;
        }
        for (key, target) in [
            ("override_window", &mut full.override_window),
            ("launch_maximized", &mut full.launch_maximized),
            ("close_after_launch", &mut full.close_after_launch),
            ("quit_after_game_stop", &mut full.quit_after_game_stop),
            ("override_console", &mut full.override_console),
            ("show_console_on_launch", &mut full.show_console_on_launch),
            ("show_console_on_error", &mut full.show_console_on_error),
            ("auto_close_console", &mut full.auto_close_console),
            ("override_game_time", &mut full.override_game_time),
            ("show_game_time", &mut full.show_game_time),
            ("record_game_time", &mut full.record_game_time),
            ("override_account", &mut full.override_account),
            ("join_server_on_launch", &mut full.join_server_on_launch),
            ("override_java_location", &mut full.override_java_location),
            ("skip_java_compat", &mut full.skip_java_compat),
            ("override_memory", &mut full.override_memory),
            ("override_java_args", &mut full.override_java_args),
            ("override_commands", &mut full.override_commands),
            ("override_env", &mut full.override_env),
        ] {
            if let Some(v) = get_bool(key) {
                *target = v;
            }
        }
        if let Some(v) = get_u32("window_width") {
            full.window_width = v;
        }
        if let Some(v) = get_u32("window_height") {
            full.window_height = v;
        }
        if let Some(v) = get_u64("total_play_seconds") {
            full.total_play_seconds = v;
        }
        if let Some(v) = get_str("account_id") {
            full.account_id = Some(v);
        }
        if let Some(v) = get_str("join_server_address") {
            full.join_server_address = v;
        }
        if let Some(v) = get_str("pre_launch_command") {
            full.pre_launch_command = v;
        }
        if let Some(v) = get_str("wrapper_command") {
            full.wrapper_command = v;
        }
        if let Some(v) = get_str("post_exit_command") {
            full.post_exit_command = v;
        }
        if let Some(env) = self.inner.get("env_vars").and_then(|v| v.as_object()) {
            for (k, v) in env {
                if let Some(s) = v.as_str() {
                    full.env_vars.insert(k.clone(), s.to_string());
                }
            }
        }
        if let Some(v) = get_str("custom_icon") {
            full.custom_icon = Some(v);
        }
        full
    }
}

pub fn format_play_time(seconds: i64) -> String {
    if seconds <= 0 {
        return "0m".to_string();
    }
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_disk_yields_defaults() {
        assert_eq!(merge_instance_settings(None), InstanceSettings::default());
    }

    #[test]
    fn partial_json_merges_over_defaults() {
        let parsed = parse_instance_settings_json(r#"{"name":"GTNH","max_ram_mb":8192,"env_vars":{"A":"1"}}"#);
        assert_eq!(parsed.name, "GTNH");
        assert_eq!(parsed.max_ram_mb, 8192);
        // Untouched keys keep defaults.
        assert_eq!(parsed.min_ram_mb, 4096);
        assert_eq!(parsed.auth_mode, "offline");
        assert_eq!(parsed.env_vars.get("A").map(String::as_str), Some("1"));
    }

    #[test]
    fn invalid_json_yields_defaults() {
        assert_eq!(parse_instance_settings_json("not json"), InstanceSettings::default());
    }

    #[test]
    fn formats_play_time() {
        assert_eq!(format_play_time(0), "0m");
        assert_eq!(format_play_time(-5), "0m");
        assert_eq!(format_play_time(59), "0m");
        assert_eq!(format_play_time(90), "1m");
        assert_eq!(format_play_time(7380), "2h 3m");
    }
}
