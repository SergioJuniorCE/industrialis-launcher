export interface GtnhVersion {
  title: string;
  description: string;
  releaseDate: string;
  maxJavaVersion: number;
  mmc: { java8Url: string; java17_2XUrl: string };
  client: { java8Url: string };
}

export interface PatchAssetIndex {
  id: string;
  sha1?: string;
  size?: number;
  totalSize?: number;
  url?: string;
}

export interface LaunchConfig {
  mainClass: string;
  minecraftVersion: string;
  libraries: string[];
  gameDir: string;
  assetsDir: string;
  jvmArgs: string[];
  programArgs: string[];
  minecraftArgumentsTemplate?: string;
  assetIndex?: PatchAssetIndex;
}

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
  offline_username_confirmed: boolean;
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
  cached_size_bytes: number;
  custom_icon: string | null;
}

export const defaultInstanceSettings = (): InstanceSettings => ({
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
  cached_size_bytes: 0,
  custom_icon: null,
});

export interface InstanceInfo {
  id: string;
  installed: boolean;
  size_bytes: number;
  settings: InstanceSettings;
  group: string;
  icon_path?: string | null;
}

export interface JavaInfo {
  path: string;
  version: number;
}

export interface InstanceGroupsState {
  collapsed: Record<string, boolean>;
  groups: string[];
  instance_order: Record<string, string[]>;
  ungrouped_name: string;
}

export interface MinecraftDirEntry {
  name: string;
  rel_path: string;
  is_dir: boolean;
  has_persistent_override: boolean;
  editable: boolean;
}

export interface ModEntry {
  filename: string;
  identity: string;
  size_bytes: number;
}

export interface ModPreviewEntry extends ModEntry {
  in_persistent_overlay: boolean;
}

export interface UpdateModPreview {
  current_pack_version: string;
  target_pack_version: string;
  custom_mods: ModPreviewEntry[];
  new_pack_mods_count: number;
  updated_pack_mods_count: number;
  removed_from_pack_count: number;
}

export interface LaunchLogLine {
  stream: string;
  line: string;
}

export interface AccountInfo {
  id: string;
  username: string;
  uuid: string;
  account_type: string;
  skin_png_base64?: string;
  owns_minecraft?: boolean;
  can_play_minecraft?: boolean;
}

export interface StoredToken {
  token: string;
  expires_at: number;
  extra?: unknown;
}

export interface MsaToken {
  access_token: string;
  refresh_token: string;
  expires_at: number;
}

export interface MinecraftProfile {
  id: string;
  name: string;
  skins?: ProfileSkin[];
}

export interface ProfileSkin {
  id: string;
  state: string;
  url: string;
  variant: string;
}

export interface MinecraftEntitlement {
  owns_minecraft: boolean;
  can_play_minecraft: boolean;
}

export interface AccountData {
  format_version: number;
  account_type: string;
  id: string;
  msa_token?: MsaToken;
  user_token?: StoredToken;
  mojangservices_token?: StoredToken;
  yggdrasil_token?: StoredToken;
  minecraft_profile?: MinecraftProfile;
  minecraft_entitlement?: MinecraftEntitlement;
  skin_png_base64?: string;
}

export interface LauncherSettings {
  theme_mode: "dark" | "light";
  theme_preset: string;
  theme_overrides: Record<string, string | undefined>;
  custom_theme_presets: unknown[];
  default_account_id: string | null;
  default_java_path: string | null;
  instance_grid_columns: number;
}

export const defaultLauncherSettings = (): LauncherSettings => ({
  theme_mode: "dark",
  theme_preset: "industrialis",
  theme_overrides: {},
  custom_theme_presets: [],
  default_account_id: null,
  default_java_path: null,
  instance_grid_columns: 3,
});

export interface DeviceCodeInfo {
  user_code: string;
  verification_uri: string;
  message: string;
}

export interface LauncherUpdateState {
  status: string;
  current_version: string;
  version?: string;
  body?: string;
  progress?: number;
  error?: string;
  download_url?: string;
  release_url?: string;
}

export interface DownloadProgress {
  stage: string;
  pct: number;
  operation?: string;
  id?: string;
  name?: string;
  log_line?: string;
  speed_mbps?: number;
  downloaded_mb?: number;
  total_mb?: number;
}
