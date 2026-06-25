import {
  DEFAULT_LAUNCHER_SETTINGS,
  type LauncherSettingsData,
  type ThemeMode,
  type ThemeOverrides,
  type ThemePresetId,
} from "./launcher-settings";
import {
  createCustomPresetId,
  DEFAULT_THEME_PRESET_ID,
  isBuiltinPresetId,
  LEGACY_CUSTOM_PRESETS_STORAGE_KEY,
  resolveThemePreset,
  type SavedThemePreset,
  type ThemePreset,
} from "./theme-presets";
import { isValidThemeTokens, mergeOverridesIntoTokens } from "./theme-utils";

export function parseSavedThemePresets(raw: unknown): SavedThemePreset[] {
  if (!Array.isArray(raw)) return [];
  const valid: SavedThemePreset[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== "object") continue;
    const record = entry as Record<string, unknown>;
    if (typeof record.id !== "string" || !record.id.startsWith("custom-")) continue;
    if (typeof record.name !== "string" || !record.name.trim()) continue;
    if (!isValidThemeTokens(record.dark) || !isValidThemeTokens(record.light)) continue;
    const effect = record.background_effect === "grid" ? "grid" : "none";
    valid.push({
      id: record.id,
      name: record.name.trim(),
      description:
        typeof record.description === "string" && record.description.trim()
          ? record.description.trim()
          : "Custom theme",
      builtin: false,
      background_effect: effect,
      dark: record.dark,
      light: record.light,
    });
  }
  return valid;
}

export function readLegacyCustomPresetsFromLocalStorage(): SavedThemePreset[] {
  try {
    const raw = localStorage.getItem(LEGACY_CUSTOM_PRESETS_STORAGE_KEY);
    if (!raw) return [];
    return parseSavedThemePresets(JSON.parse(raw));
  } catch {
    return [];
  }
}

export function clearLegacyCustomPresetsLocalStorage(): void {
  localStorage.removeItem(LEGACY_CUSTOM_PRESETS_STORAGE_KEY);
}

export function repairThemeSettings(
  settings: LauncherSettingsData,
  customPresets: SavedThemePreset[]
): { settings: LauncherSettingsData; customPresets: SavedThemePreset[]; repaired: boolean } {
  const validPresets = parseSavedThemePresets(customPresets);
  let repaired = validPresets.length !== customPresets.length;
  let nextSettings = { ...settings, custom_theme_presets: validPresets };

  if (isBuiltinPresetId(nextSettings.theme_preset)) {
    return { settings: nextSettings, customPresets: validPresets, repaired };
  }

  const active = resolveThemePreset(nextSettings.theme_preset, validPresets);
  if (active) {
    return { settings: nextSettings, customPresets: validPresets, repaired };
  }

  repaired = true;
  nextSettings = {
    ...nextSettings,
    theme_preset: DEFAULT_THEME_PRESET_ID,
    theme_overrides: {},
  };
  return { settings: nextSettings, customPresets: validPresets, repaired };
}

export function buildCustomPresetFromSettings(
  settings: LauncherSettingsData,
  customPresets: SavedThemePreset[],
  name: string,
  description?: string
): { preset: SavedThemePreset; settings: LauncherSettingsData; customPresets: SavedThemePreset[] } {
  const base = resolveThemePreset(settings.theme_preset, customPresets);
  const fallback = resolveThemePreset(DEFAULT_THEME_PRESET_ID, customPresets)!;
  const source = base ?? fallback;
  const mode = settings.theme_mode;

  const dark =
    mode === "dark"
      ? mergeOverridesIntoTokens(source.dark, settings.theme_overrides)
      : { ...source.dark };
  const light =
    mode === "light"
      ? mergeOverridesIntoTokens(source.light, settings.theme_overrides)
      : { ...source.light };

  const id = createCustomPresetId();
  const preset: SavedThemePreset = {
    id,
    name: name.trim(),
    description: description?.trim() || `Custom theme based on ${source.name}`,
    builtin: false,
    background_effect: source.background_effect,
    dark,
    light,
  };

  const nextPresets = [...customPresets, preset];
  const nextSettings: LauncherSettingsData = {
    ...settings,
    custom_theme_presets: nextPresets,
    theme_preset: id,
    theme_overrides: {},
  };

  return { preset, settings: nextSettings, customPresets: nextPresets };
}

export function deleteCustomPresetFromSettings(
  settings: LauncherSettingsData,
  customPresets: SavedThemePreset[],
  id: string
): { settings: LauncherSettingsData; customPresets: SavedThemePreset[] } {
  const nextPresets = customPresets.filter((p) => p.id !== id);
  if (settings.theme_preset !== id) {
    return {
      settings: { ...settings, custom_theme_presets: nextPresets },
      customPresets: nextPresets,
    };
  }
  return {
    settings: {
      ...settings,
      custom_theme_presets: nextPresets,
      theme_preset: DEFAULT_LAUNCHER_SETTINGS.theme_preset,
      theme_overrides: {},
    },
    customPresets: nextPresets,
  };
}

export function migrateSettingsFromLegacyStorage(
  disk: LauncherSettingsData
): { settings: LauncherSettingsData; migrated: boolean } {
  const legacy = readLegacyCustomPresetsFromLocalStorage();
  if (legacy.length === 0) return { settings: disk, migrated: false };

  const merged = parseSavedThemePresets([...(disk.custom_theme_presets ?? []), ...legacy]);
  clearLegacyCustomPresetsLocalStorage();
  return {
    settings: { ...disk, custom_theme_presets: merged },
    migrated: true,
  };
}

export function effectiveOverrideValue(
  key: keyof ThemeOverrides,
  mode: ThemeMode,
  presetId: ThemePresetId,
  overrides: ThemeOverrides,
  customPresets: SavedThemePreset[] = [],
  preset?: ThemePreset
): string | undefined {
  if (overrides[key]) return overrides[key];
  const resolved = preset ?? resolveThemePreset(presetId, customPresets);
  if (!resolved) return undefined;
  const tokens = mode === "dark" ? resolved.dark : resolved.light;
  if (key in tokens) return tokens[key as keyof typeof tokens];
  return undefined;
}