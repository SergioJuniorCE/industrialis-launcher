import type { BuiltinThemePresetId, SavedThemePreset } from "./theme-presets";
import { DEFAULT_THEME_PRESET_ID } from "./theme-presets";

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

export interface LauncherSettingsData {
  theme_mode: ThemeMode;
  theme_preset: ThemePresetId;
  theme_overrides: ThemeOverrides;
  custom_theme_presets: SavedThemePreset[];
  /** Account used for every launch unless an instance overrides it. */
  default_account_id?: string | null;
  /** @deprecated Renamed to default_account_id */
  active_account_id?: string | null;
  /** Number of columns in the instance grid (2–5). */
  instance_grid_columns?: number;
}

export function resolveDefaultAccountId(
  settings: Partial<LauncherSettingsData>,
): string | null {
  return settings.default_account_id ?? settings.active_account_id ?? null;
}

export const DEFAULT_LAUNCHER_SETTINGS: LauncherSettingsData = {
  theme_mode: "dark",
  theme_preset: DEFAULT_THEME_PRESET_ID,
  theme_overrides: {},
  custom_theme_presets: [],
  default_account_id: null,
  instance_grid_columns: 3,
};