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
} from "../lib/launcher-settings";
import {
  applyTheme,
  mergeThemeCacheIntoSettings,
  readThemeCache,
  writeThemeCache,
} from "../lib/theme";

interface LauncherSettingsContextValue {
  settings: LauncherSettingsData;
  loaded: boolean;
  updateSettings: (patch: Partial<LauncherSettingsData>) => void;
  saveSettingsNow: () => Promise<void>;
  scheduleSaveSettings: () => void;
  saveError: string | null;
  clearSaveError: () => void;
  setThemeMode: (mode: ThemeMode) => void;
  setThemeOverrides: (overrides: ThemeOverrides) => void;
  resetThemeOverrides: () => void;
}

const LauncherSettingsContext = createContext<LauncherSettingsContextValue | null>(null);

export function useLauncherSettings(): LauncherSettingsContextValue {
  const ctx = useContext(LauncherSettingsContext);
  if (!ctx) {
    throw new Error("useLauncherSettings must be used within LauncherSettingsProvider");
  }
  return ctx;
}

export function LauncherSettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<LauncherSettingsData>(() =>
    mergeThemeCacheIntoSettings(DEFAULT_LAUNCHER_SETTINGS, readThemeCache())
  );
  const [loaded, setLoaded] = useState(false);
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
    }));
  }, []);

  const setThemeMode = useCallback(
    (mode: ThemeMode) => {
      updateSettings({ theme_mode: mode });
      void saveSettingsNow();
    },
    [updateSettings, saveSettingsNow]
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

  useEffect(() => {
    if (!isTauri()) {
      setLoaded(true);
      return;
    }
    invoke<LauncherSettingsData>("get_launcher_settings")
      .then((disk) => {
        setSettings(disk);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  useEffect(() => {
    applyTheme(settings.theme_mode, settings.theme_overrides);
    writeThemeCache(settings.theme_mode, settings.theme_overrides);
  }, [settings.theme_mode, settings.theme_overrides]);

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
    updateSettings,
    saveSettingsNow,
    scheduleSaveSettings,
    saveError,
    clearSaveError,
    setThemeMode,
    setThemeOverrides,
    resetThemeOverrides,
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