import type { BuiltinThemePresetId } from "./theme-presets";
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
}

export const DEFAULT_LAUNCHER_SETTINGS: LauncherSettingsData = {
  theme_mode: "dark",
  theme_preset: DEFAULT_THEME_PRESET_ID,
  theme_overrides: {},
};