import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@tauri-apps/api/core";
import { Button } from "../components/ui/button";
import {
  DEFAULT_LAUNCHER_SETTINGS,
  type LauncherSettingsData,
  type ThemeMode,
  type ThemeOverrides,
  type ThemePresetId,
} from "../lib/launcher-settings";
import {
  buildCustomPresetFromSettings,
  deleteCustomPresetFromSettings,
  migrateSettingsFromLegacyStorage,
  parseSavedThemePresets,
  repairThemeSettings,
} from "../lib/theme-store";
import {
  applyTheme,
  mergeThemeCacheIntoSettings,
  readThemeCache,
  writeThemeCache,
} from "../lib/theme";

interface LauncherSettingsContextValue {
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

const LauncherSettingsContext = createContext<LauncherSettingsContextValue | null>(null);

export function useLauncherSettings(): LauncherSettingsContextValue {
  const ctx = useContext(LauncherSettingsContext);
  if (!ctx) {
    throw new Error("useLauncherSettings must be used within LauncherSettingsProvider");
  }
  return ctx;
}

function normalizeDiskSettings(disk: Partial<LauncherSettingsData>): LauncherSettingsData {
  return {
    ...DEFAULT_LAUNCHER_SETTINGS,
    ...disk,
    theme_preset: disk.theme_preset ?? DEFAULT_LAUNCHER_SETTINGS.theme_preset,
    custom_theme_presets: parseSavedThemePresets(disk.custom_theme_presets ?? []),
    theme_overrides: disk.theme_overrides ?? {},
  };
}

function initialLauncherSettings(): LauncherSettingsData {
  const settings = mergeThemeCacheIntoSettings(
    DEFAULT_LAUNCHER_SETTINGS,
    readThemeCache()
  );
  applyTheme(
    settings.theme_mode,
    settings.theme_preset,
    settings.theme_overrides,
    settings.custom_theme_presets
  );
  return settings;
}

export function LauncherSettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<LauncherSettingsData>(initialLauncherSettings);
  const [loaded, setLoaded] = useState(false);
  const [presetRepaired, setPresetRepaired] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const settingsRef = useRef(settings);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  const clearSaveError = useCallback(() => setSaveError(null), []);

  const saveSettingsNow = useCallback(async () => {
    if (saveTimerRef.current) {
      clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
    const snapshot = settingsRef.current;
    if (!isTauri()) return;
    try {
      await invoke("save_launcher_settings", { settings: snapshot });
      clearSaveError();
    } catch (e) {
      setSaveError(`Save failed: ${e}`);
    }
  }, [clearSaveError]);

  const scheduleSaveSettings = useCallback(() => {
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => {
      saveTimerRef.current = null;
      void saveSettingsNow();
    }, 300);
  }, [saveSettingsNow]);

  const updateSettings = useCallback((patch: Partial<LauncherSettingsData>) => {
    setSettings((prev) => ({
      ...prev,
      ...patch,
      theme_overrides:
        patch.theme_overrides !== undefined
          ? { ...prev.theme_overrides, ...patch.theme_overrides }
          : prev.theme_overrides,
      custom_theme_presets:
        patch.custom_theme_presets !== undefined
          ? parseSavedThemePresets(patch.custom_theme_presets)
          : prev.custom_theme_presets,
    }));
  }, []);

  const setThemeMode = useCallback(
    (mode: ThemeMode) => {
      updateSettings({ theme_mode: mode });
      void saveSettingsNow();
    },
    [updateSettings, saveSettingsNow]
  );

  const setThemePreset = useCallback(
    (presetId: ThemePresetId) => {
      setSettings((prev) => ({
        ...prev,
        theme_preset: presetId,
        theme_overrides: {},
      }));
      void saveSettingsNow();
    },
    [saveSettingsNow]
  );

  const setThemeOverrides = useCallback(
    (overrides: ThemeOverrides) => {
      setSettings((prev) => ({ ...prev, theme_overrides: overrides }));
      scheduleSaveSettings();
    },
    [scheduleSaveSettings]
  );

  const resetThemeOverrides = useCallback(() => {
    setSettings((prev) => ({ ...prev, theme_overrides: {} }));
    void saveSettingsNow();
  }, [saveSettingsNow]);

  const saveCustomPreset = useCallback(
    (name: string, description?: string) => {
      const { settings: nextSettings } = buildCustomPresetFromSettings(
        settingsRef.current,
        settingsRef.current.custom_theme_presets,
        name,
        description
      );
      setSettings(nextSettings);
      void saveSettingsNow();
    },
    [saveSettingsNow]
  );

  const deleteCustomPreset = useCallback(
    (id: string) => {
      const { settings: nextSettings } = deleteCustomPresetFromSettings(
        settingsRef.current,
        settingsRef.current.custom_theme_presets,
        id
      );
      setSettings(nextSettings);
      void saveSettingsNow();
    },
    [saveSettingsNow]
  );

  useEffect(() => {
    if (!isTauri()) {
      const migrated = migrateSettingsFromLegacyStorage(settingsRef.current);
      const repaired = repairThemeSettings(migrated.settings, migrated.settings.custom_theme_presets);
      if (migrated.migrated || repaired.repaired) {
        setSettings(repaired.settings);
        setPresetRepaired(repaired.repaired);
      }
      setLoaded(true);
      return;
    }
    invoke<LauncherSettingsData>("get_launcher_settings")
      .then((disk) => {
        const migrated = migrateSettingsFromLegacyStorage(normalizeDiskSettings(disk));
        const repaired = repairThemeSettings(
          migrated.settings,
          migrated.settings.custom_theme_presets
        );
        setSettings(repaired.settings);
        setPresetRepaired(repaired.repaired);
        if (migrated.migrated || repaired.repaired) {
          void invoke("save_launcher_settings", { settings: repaired.settings }).catch(() => {});
        }
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  useEffect(() => {
    applyTheme(
      settings.theme_mode,
      settings.theme_preset,
      settings.theme_overrides,
      settings.custom_theme_presets
    );
    writeThemeCache(
      settings.theme_mode,
      settings.theme_preset,
      settings.theme_overrides,
      settings.custom_theme_presets
    );
  }, [
    settings.theme_mode,
    settings.theme_preset,
    settings.theme_overrides,
    settings.custom_theme_presets,
  ]);

  useEffect(() => {
    return () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
        void saveSettingsNow();
      }
    };
  }, [saveSettingsNow]);

  const value: LauncherSettingsContextValue = {
    settings,
    loaded,
    customPresets: settings.custom_theme_presets,
    presetRepaired,
    updateSettings,
    saveSettingsNow,
    scheduleSaveSettings,
    saveError,
    clearSaveError,
    setThemeMode,
    setThemePreset,
    setThemeOverrides,
    resetThemeOverrides,
    saveCustomPreset,
    deleteCustomPreset,
  };

  return (
    <LauncherSettingsContext.Provider value={value}>
      {children}
      {presetRepaired && (
        <div className="fixed bottom-4 left-4 bg-muted text-foreground border border-border p-3 rounded shadow-lg max-w-sm z-50">
          <p className="text-sm">Theme preset was missing and has been reset to Industrialis.</p>
        </div>
      )}
      {saveError && (
        <div className="fixed bottom-4 right-4 bg-destructive text-destructive-foreground p-3 rounded shadow-lg max-w-sm z-50">
          <p className="text-sm">{saveError}</p>
          <Button size="sm" variant="ghost" className="mt-1" onClick={clearSaveError}>
            Dismiss
          </Button>
        </div>
      )}
    </LauncherSettingsContext.Provider>
  );
}