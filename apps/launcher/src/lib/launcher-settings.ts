import type { BuiltinThemePresetId, SavedThemePreset } from "./theme-presets";
import { DEFAULT_THEME_PRESET_ID } from "./theme-presets";
import { DEFAULT_LAUNCHER_WINDOW_HEIGHT, DEFAULT_LAUNCHER_WINDOW_WIDTH, type LauncherWindowSettings } from "./launcher-window";

export type ThemeMode = "dark" | "light";

export type ThemePresetId = BuiltinThemePresetId | (string & {});

export interface ThemeOverrides {
  background?: string;
  foreground?: string;
  primary?: string;
  card?: string;
  border?: string;
  muted?: string;
  muted_foreground?: string;
  accent?: string;
  accent_foreground?: string;
  radius?: string;
}

export interface LauncherSettingsData extends LauncherWindowSettings {
  theme_mode: ThemeMode;
  theme_preset: ThemePresetId;
  theme_overrides: ThemeOverrides;
  custom_theme_presets: SavedThemePreset[];
  /** Account used for every launch unless an instance overrides it. */
  default_account_id?: string | null;
  /** Java executable used for every launch unless an instance overrides it. */
  default_java_path?: string | null;
  /** @deprecated Renamed to default_account_id */
  active_account_id?: string | null;
  /** @deprecated Legacy setting retained for compatibility; card widths are fixed. */
  instance_grid_columns?: number;
  /** Number of cloud snapshots retained per instance and provider. */
  backup_retention_limit: number;
}

export function resolveDefaultAccountId(settings: Partial<LauncherSettingsData>): string | null {
  return settings.default_account_id ?? settings.active_account_id ?? null;
}

export const DEFAULT_LAUNCHER_SETTINGS: LauncherSettingsData = {
  theme_mode: "dark",
  theme_preset: DEFAULT_THEME_PRESET_ID,
  theme_overrides: {},
  custom_theme_presets: [],
  default_account_id: null,
  default_java_path: null,
  instance_grid_columns: 4,
  backup_retention_limit: 10,
  launch_maximized: false,
  window_width: DEFAULT_LAUNCHER_WINDOW_WIDTH,
  window_height: DEFAULT_LAUNCHER_WINDOW_HEIGHT,
};
