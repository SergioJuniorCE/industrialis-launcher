import type { ThemeMode, ThemeOverrides, ThemePresetId } from "./launcher-settings";
import {
  DEFAULT_THEME_PRESET_ID,
  resolveThemePresetOrDefault,
  tokensForPreset,
  type SavedThemePreset,
  type ThemeBackgroundEffect,
  type ThemeTokens,
} from "./theme-presets";
import { THEME_TOKEN_CSS_VARS, tokensToCssVars, validateHexColor } from "./theme-utils";

export { THEME_TOKEN_CSS_VARS, validateHexColor, validateRadius } from "./theme-utils";

export const THEME_CACHE_KEY = "industrialis-theme-cache";
export const THEME_CACHE_VERSION = 2;

export interface ThemeCache {
  version: number;
  mode: ThemeMode;
  preset: ThemePresetId;
  effect: ThemeBackgroundEffect;
  overrides: ThemeOverrides;
  vars: Record<string, string>;
}

export function computeThemeCssVars(
  mode: ThemeMode,
  presetId: string,
  overrides: ThemeOverrides = {},
  customPresets: SavedThemePreset[] = []
): Record<string, string> {
  const preset = resolveThemePresetOrDefault(presetId, customPresets);
  const tokens = tokensForPreset(preset, mode);
  return tokensToCssVars(tokens, overrides);
}

export function applyTheme(
  mode: ThemeMode,
  presetId: string = DEFAULT_THEME_PRESET_ID,
  overrides: ThemeOverrides = {},
  customPresets: SavedThemePreset[] = []
): void {
  const root = document.documentElement;
  const preset = resolveThemePresetOrDefault(presetId, customPresets);
  const vars = tokensToCssVars(tokensForPreset(preset, mode), overrides);

  root.setAttribute("data-theme", mode);
  root.setAttribute("data-theme-preset", preset.id);
  root.setAttribute("data-theme-effect", preset.background_effect);
  root.style.colorScheme = mode;

  for (const cssVar of Object.values(THEME_TOKEN_CSS_VARS)) {
    const value = vars[cssVar];
    if (value) {
      root.style.setProperty(cssVar, value);
    } else {
      root.style.removeProperty(cssVar);
    }
  }

  if (import.meta.env.DEV) {
    const overrideCount = Object.values(overrides).filter(Boolean).length;
    console.debug("[theme] applied", { mode, preset: preset.id, overrideCount });
  }
}

export function readThemeCache(): ThemeCache | null {
  try {
    const raw = localStorage.getItem(THEME_CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<ThemeCache>;
    const mode: ThemeMode = parsed.mode === "light" ? "light" : "dark";
    const preset =
      typeof parsed.preset === "string" && parsed.preset.length > 0
        ? parsed.preset
        : DEFAULT_THEME_PRESET_ID;
    const overrides =
      parsed.overrides && typeof parsed.overrides === "object" ? parsed.overrides : {};
    const version = typeof parsed.version === "number" ? parsed.version : 0;
    const vars = computeThemeCssVars(mode, preset, overrides);
    const effect: ThemeBackgroundEffect =
      parsed.effect === "grid" ? "grid" : preset === "industrialis" ? "grid" : "none";
    if (version !== THEME_CACHE_VERSION) return null;
    return { version, mode, preset, effect, overrides, vars };
  } catch {
    return null;
  }
}

export function writeThemeCache(
  mode: ThemeMode,
  presetId: string,
  overrides: ThemeOverrides,
  customPresets: SavedThemePreset[] = []
): void {
  const preset = resolveThemePresetOrDefault(presetId, customPresets);
  const vars = tokensToCssVars(tokensForPreset(preset, mode), overrides);
  const payload: ThemeCache = {
    version: THEME_CACHE_VERSION,
    mode,
    preset: presetId,
    effect: preset.background_effect,
    overrides,
    vars,
  };
  localStorage.setItem(THEME_CACHE_KEY, JSON.stringify(payload));
}

export function mergeThemeCacheIntoSettings<T extends {
  theme_mode: ThemeMode;
  theme_preset: ThemePresetId;
  theme_overrides: ThemeOverrides;
}>(
  defaults: T,
  cache: ThemeCache | null
): T {
  if (!cache) return defaults;
  return {
    ...defaults,
    theme_mode: cache.mode,
    theme_preset: cache.preset,
    theme_overrides: cache.overrides ?? {},
  };
}

function channel(value: string): number {
  const v = parseInt(value, 16) / 255;
  return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

function luminance(hex: string): number {
  const normalized =
    hex.length === 4
      ? `#${hex[1]}${hex[1]}${hex[2]}${hex[2]}${hex[3]}${hex[3]}`
      : hex.length === 9
        ? hex.slice(0, 7)
        : hex;
  const r = channel(normalized.slice(1, 3));
  const g = channel(normalized.slice(3, 5));
  const b = channel(normalized.slice(5, 7));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(foreground: string, background: string): number {
  const l1 = luminance(foreground);
  const l2 = luminance(background);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

export function hasLowContrast(foreground?: string, background?: string): boolean {
  if (!foreground || !background) return false;
  if (!validateHexColor(foreground) || !validateHexColor(background)) return false;
  return contrastRatio(foreground, background) < 4.5;
}

export type { ThemeTokens };