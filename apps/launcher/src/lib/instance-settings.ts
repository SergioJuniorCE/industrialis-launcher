export interface InstanceSettings {
  name: string;
  pack_version: string;
  pack_java_type: string;
  java_path: string | null;
  min_ram_mb: number;
  max_ram_mb: number;
  perm_gen_mb: number;
  jvm_args: string;
  auth_mode: string;
  username: string;
  offline_username_confirmed?: boolean;

  override_window: boolean;
  launch_maximized: boolean;
  window_width: number;
  window_height: number;
  close_after_launch: boolean;
  quit_after_game_stop: boolean;

  override_console: boolean;
  show_console_on_launch: boolean;
  show_console_on_error: boolean;
  auto_close_console: boolean;

  override_game_time: boolean;
  show_game_time: boolean;
  record_game_time: boolean;
  total_play_seconds: number;

  override_account: boolean;
  account_id: string | null;

  join_server_on_launch: boolean;
  join_server_address: string;

  override_java_location: boolean;
  skip_java_compat: boolean;
  override_memory: boolean;
  override_java_args: boolean;

  override_commands: boolean;
  pre_launch_command: string;
  wrapper_command: string;
  post_exit_command: string;

  override_env: boolean;
  env_vars: Record<string, string>;

  custom_icon?: string | null;
}

export const DEFAULT_INSTANCE_SETTINGS: InstanceSettings = {
  name: "",
  pack_version: "",
  pack_java_type: "java17+",
  java_path: null,
  min_ram_mb: 4096,
  max_ram_mb: 6144,
  perm_gen_mb: 128,
  jvm_args: "",
  auth_mode: "offline",
  username: "",
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
  account_id: null,

  join_server_on_launch: false,
  join_server_address: "",

  override_java_location: false,
  skip_java_compat: false,
  override_memory: false,
  override_java_args: false,

  override_commands: false,
  pre_launch_command: "",
  wrapper_command: "",
  post_exit_command: "",

  override_env: false,
  env_vars: {},
};

export function mergeInstanceSettings(
  disk: Partial<InstanceSettings> | null | undefined
): InstanceSettings {
  if (!disk) return { ...DEFAULT_INSTANCE_SETTINGS };
  return {
    ...DEFAULT_INSTANCE_SETTINGS,
    ...disk,
    env_vars: { ...DEFAULT_INSTANCE_SETTINGS.env_vars, ...(disk.env_vars ?? {}) },
  };
}

export function formatPlayTime(seconds: number): string {
  if (seconds <= 0) return "0m";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}