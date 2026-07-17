import { createContext, useContext } from "react";
import type {
  LauncherSettingsData,
  ThemeMode,
  ThemeOverrides,
  ThemePresetId,
} from "../lib/launcher-settings";

export interface LauncherSettingsContextValue {
  settings: LauncherSettingsData;
  loaded: boolean;
  customPresets: LauncherSettingsData["custom_theme_presets"];
  presetRepaired: boolean;
  updateSettings: (patch: Partial<LauncherSettingsData>) => void;
  saveSettingsNow: () => Promise<void>;
  scheduleSaveSettings: () => void;
  saveError: string | null;
  clearSaveError: () => void;
  setThemeMode: (mode: ThemeMode) => void;
  setThemePreset: (presetId: ThemePresetId) => void;
  setThemeOverrides: (overrides: ThemeOverrides) => void;
  resetThemeOverrides: () => void;
  saveCustomPreset: (name: string, description?: string) => void;
  deleteCustomPreset: (id: string) => void;
}

export const LauncherSettingsContext =
  createContext<LauncherSettingsContextValue | null>(null);

export function useLauncherSettings(): LauncherSettingsContextValue {
  const context = useContext(LauncherSettingsContext);
  if (!context) {
    throw new Error("useLauncherSettings must be used within LauncherSettingsProvider");
  }
  return context;
}
