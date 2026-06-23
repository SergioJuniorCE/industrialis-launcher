export type ThemeMode = "dark" | "light";

export interface ThemeOverrides {
  background?: string;
  foreground?: string;
  primary?: string;
  card?: string;
  border?: string;
  muted?: string;
  muted_foreground?: string;
  radius?: string;
}

export interface LauncherSettingsData {
  microsoft_client_id: string;
  theme_mode: ThemeMode;
  theme_overrides: ThemeOverrides;
}

export const DEFAULT_LAUNCHER_SETTINGS: LauncherSettingsData = {
  microsoft_client_id: "",
  theme_mode: "dark",
  theme_overrides: {},
};