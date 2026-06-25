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
  createCustomPresetId,
  readCustomThemePresets,
  resolveThemePreset,
  writeCustomThemePresets,
  type SavedThemePreset,
  type ThemeTokens,
} from "../lib/theme-presets";
import {
  applyTheme,
  mergeThemeCacheIntoSettings,
  readThemeCache,
  writeThemeCache,
} from "../lib/theme";

interface LauncherSettingsContextValue {
  settings: LauncherSettingsData;
  loaded: boolean;
  customPresets: SavedThemePreset[];
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

function applyOverridesToTokens(
  tokens: ThemeTokens,
  overrides: ThemeOverrides
): ThemeTokens {
  const next = { ...tokens };
  for (const [key, value] of Object.entries(overrides)) {
    if (value && key in next) {
      (next as Record<string, string>)[key] = value;
    }
  }
  return next;
}

export function LauncherSettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<LauncherSettingsData>(() =>
    mergeThemeCacheIntoSettings(DEFAULT_LAUNCHER_SETTINGS, readThemeCache())
  );
  const [customPresets, setCustomPresets] = useState<SavedThemePreset[]>(() =>
    readCustomThemePresets()
  );
  const [loaded, setLoaded] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const settingsRef = useRef(settings);
  const customPresetsRef = useRef(customPresets);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  useEffect(() => {
    customPresetsRef.current = customPresets;
  }, [customPresets]);

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
      const snapshot = settingsRef.current;
      const presets = customPresetsRef.current;
      const base = resolveThemePreset(snapshot.theme_preset, presets);
      const dark = applyOverridesToTokens(base.dark, snapshot.theme_overrides);
      const light = applyOverridesToTokens(base.light, snapshot.theme_overrides);
      const id = createCustomPresetId();
      const saved: SavedThemePreset = {
        id,
        name,
        description: description ?? `Custom theme based on ${base.name}`,
        builtin: false,
        dark,
        light,
      };
      const next = [...presets, saved];
      setCustomPresets(next);
      writeCustomThemePresets(next);
      setSettings((prev) => ({
        ...prev,
        theme_preset: id,
        theme_overrides: {},
      }));
      void saveSettingsNow();
    },
    [saveSettingsNow]
  );

  const deleteCustomPreset = useCallback(
    (id: string) => {
      const presets = customPresetsRef.current;
      const next = presets.filter((p) => p.id !== id);
      setCustomPresets(next);
      writeCustomThemePresets(next);
      setSettings((prev) => {
        if (prev.theme_preset !== id) return prev;
        return {
          ...prev,
          theme_preset: DEFAULT_LAUNCHER_SETTINGS.theme_preset,
          theme_overrides: {},
        };
      });
      void saveSettingsNow();
    },
    [saveSettingsNow]
  );

  useEffect(() => {
    if (!isTauri()) {
      setLoaded(true);
      return;
    }
    invoke<LauncherSettingsData>("get_launcher_settings")
      .then((disk) => {
        setSettings((prev) => ({
          ...prev,
          ...disk,
          theme_preset: disk.theme_preset ?? DEFAULT_LAUNCHER_SETTINGS.theme_preset,
        }));
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  useEffect(() => {
    applyTheme(
      settings.theme_mode,
      settings.theme_preset,
      settings.theme_overrides,
      customPresets
    );
    writeThemeCache(
      settings.theme_mode,
      settings.theme_preset,
      settings.theme_overrides,
      customPresets
    );
  }, [settings.theme_mode, settings.theme_preset, settings.theme_overrides, customPresets]);

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
    customPresets,
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