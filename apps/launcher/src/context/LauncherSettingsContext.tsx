import {
  useCallback,
  useEffect,
  useMemo,
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
import {
  LauncherSettingsContext,
  type LauncherSettingsContextValue,
} from "./launcher-settings-context";

function normalizeDiskSettings(disk: Partial<LauncherSettingsData>): LauncherSettingsData {
  const defaultAccountId =
    disk.default_account_id ?? disk.active_account_id ?? DEFAULT_LAUNCHER_SETTINGS.default_account_id;
  const themeMode =
    disk.theme_mode === "light" || disk.theme_mode === "dark"
      ? disk.theme_mode
      : DEFAULT_LAUNCHER_SETTINGS.theme_mode;
  return {
    ...DEFAULT_LAUNCHER_SETTINGS,
    ...disk,
    theme_mode: themeMode,
    theme_preset: disk.theme_preset ?? DEFAULT_LAUNCHER_SETTINGS.theme_preset,
    custom_theme_presets: parseSavedThemePresets(disk.custom_theme_presets ?? []),
    theme_overrides: disk.theme_overrides ?? {},
    default_account_id: defaultAccountId ?? null,
    active_account_id: undefined,
  };
}

function mergeSettings(
  prev: LauncherSettingsData,
  patch: Partial<LauncherSettingsData>,
): LauncherSettingsData {
  return {
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

  const commitSettings = useCallback((next: LauncherSettingsData) => {
    settingsRef.current = next;
    setSettings(next);
  }, []);

  const persistSettings = useCallback(
    async (snapshot: LauncherSettingsData) => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
        saveTimerRef.current = null;
      }
      settingsRef.current = snapshot;
      if (!isTauri()) return;
      try {
        await invoke("save_launcher_settings", { settings: snapshot });
        clearSaveError();
      } catch (e) {
        setSaveError(`Save failed: ${e}`);
      }
    },
    [clearSaveError],
  );

  const saveSettingsNow = useCallback(async () => {
    await persistSettings(settingsRef.current);
  }, [persistSettings]);

  const scheduleSaveSettings = useCallback(() => {
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => {
      saveTimerRef.current = null;
      void persistSettings(settingsRef.current);
    }, 300);
  }, [persistSettings]);

  const updateSettings = useCallback(
    (patch: Partial<LauncherSettingsData>) => {
      commitSettings(mergeSettings(settingsRef.current, patch));
    },
    [commitSettings],
  );

  const setThemeMode = useCallback(
    (mode: ThemeMode) => {
      const next = mergeSettings(settingsRef.current, { theme_mode: mode });
      commitSettings(next);
      void persistSettings(next);
    },
    [commitSettings, persistSettings],
  );

  const setThemePreset = useCallback(
    (presetId: ThemePresetId) => {
      const next = mergeSettings(settingsRef.current, {
        theme_preset: presetId,
        theme_overrides: {},
      });
      commitSettings(next);
      void persistSettings(next);
    },
    [commitSettings, persistSettings],
  );

  const setThemeOverrides = useCallback(
    (overrides: ThemeOverrides) => {
      commitSettings(mergeSettings(settingsRef.current, { theme_overrides: overrides }));
      scheduleSaveSettings();
    },
    [commitSettings, scheduleSaveSettings],
  );

  const resetThemeOverrides = useCallback(() => {
    const next = mergeSettings(settingsRef.current, { theme_overrides: {} });
    commitSettings(next);
    void persistSettings(next);
  }, [commitSettings, persistSettings]);

  const saveCustomPreset = useCallback(
    (name: string, description?: string) => {
      const { settings: nextSettings } = buildCustomPresetFromSettings(
        settingsRef.current,
        settingsRef.current.custom_theme_presets,
        name,
        description,
      );
      commitSettings(nextSettings);
      void persistSettings(nextSettings);
    },
    [commitSettings, persistSettings],
  );

  const deleteCustomPreset = useCallback(
    (id: string) => {
      const { settings: nextSettings } = deleteCustomPresetFromSettings(
        settingsRef.current,
        settingsRef.current.custom_theme_presets,
        id,
      );
      commitSettings(nextSettings);
      void persistSettings(nextSettings);
    },
    [commitSettings, persistSettings],
  );

  useEffect(() => {
    if (!isTauri()) {
      const migrated = migrateSettingsFromLegacyStorage(settingsRef.current);
      const repaired = repairThemeSettings(migrated.settings, migrated.settings.custom_theme_presets);
      if (migrated.migrated || repaired.repaired) {
        commitSettings(repaired.settings);
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
          migrated.settings.custom_theme_presets,
        );
        commitSettings(repaired.settings);
        setPresetRepaired(repaired.repaired);
        if (migrated.migrated || repaired.repaired) {
          void persistSettings(repaired.settings);
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

  const value = useMemo<LauncherSettingsContextValue>(
    () => ({
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
    }),
    [
      settings,
      loaded,
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
    ],
  );

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
