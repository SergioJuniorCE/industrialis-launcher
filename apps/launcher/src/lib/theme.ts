import type { ThemeMode, ThemeOverrides, ThemePresetId } from "./launcher-settings";
import {
  DEFAULT_THEME_PRESET_ID,
  resolveThemePreset,
  tokensForPreset,
  type SavedThemePreset,
  type ThemeTokens,
} from "./theme-presets";

export const THEME_CACHE_KEY = "industrialis-theme-cache";

export const THEME_TOKEN_CSS_VARS = {
  background: "--theme-background",
  foreground: "--theme-foreground",
  card: "--theme-card",
  card_foreground: "--theme-card-foreground",
  popover: "--theme-popover",
  popover_foreground: "--theme-popover-foreground",
  primary: "--theme-primary",
  primary_foreground: "--theme-primary-foreground",
  secondary: "--theme-secondary",
  secondary_foreground: "--theme-secondary-foreground",
  muted: "--theme-muted",
  muted_foreground: "--theme-muted-foreground",
  accent: "--theme-accent",
  accent_foreground: "--theme-accent-foreground",
  destructive: "--theme-destructive",
  destructive_foreground: "--theme-destructive-foreground",
  border: "--theme-border",
  input: "--theme-input",
  ring: "--theme-ring",
  radius: "--theme-radius",
} as const satisfies Record<keyof ThemeTokens, string>;

export const THEME_OVERRIDE_CSS_VARS = {
  background: "--theme-background",
  foreground: "--theme-foreground",
  primary: "--theme-primary",
  card: "--theme-card",
  border: "--theme-border",
  muted: "--theme-muted",
  muted_foreground: "--theme-muted-foreground",
  accent: "--theme-accent",
  accent_foreground: "--theme-accent-foreground",
  radius: "--theme-radius",
} as const satisfies Record<keyof ThemeOverrides, string>;

export interface ThemeCache {
  mode: ThemeMode;
  preset: ThemePresetId;
  overrides: ThemeOverrides;
  vars: Record<string, string>;
}

const HEX_RE = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
const RADIUS_RE = /^\d+(\.\d+)?(rem|px)$/;

export function validateHexColor(value: string): boolean {
  return value.length <= 9 && HEX_RE.test(value);
}

export function validateRadius(value: string): boolean {
  return value.length <= 16 && RADIUS_RE.test(value);
}

export function computeThemeCssVars(
  mode: ThemeMode,
  presetId: string,
  overrides: ThemeOverrides = {},
  customPresets: SavedThemePreset[] = []
): Record<string, string> {
  const preset = resolveThemePreset(presetId, customPresets);
  const tokens = tokensForPreset(preset, mode);
  const vars: Record<string, string> = {};

  for (const [key, cssVar] of Object.entries(THEME_TOKEN_CSS_VARS)) {
    const tokenKey = key as keyof ThemeTokens;
    const override = overrides[tokenKey as keyof ThemeOverrides];
    vars[cssVar] = override ?? tokens[tokenKey];
  }

  return vars;
}

export function applyTheme(
  mode: ThemeMode,
  presetId: string = DEFAULT_THEME_PRESET_ID,
  overrides: ThemeOverrides = {},
  customPresets: SavedThemePreset[] = []
): void {
  const root = document.documentElement;
  const preset = resolveThemePreset(presetId, customPresets);
  const vars = computeThemeCssVars(mode, preset.id, overrides, customPresets);

  root.setAttribute("data-theme", mode);
  root.setAttribute("data-theme-preset", preset.id);
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
    const vars =
      parsed.vars && typeof parsed.vars === "object"
        ? parsed.vars
        : computeThemeCssVars(mode, preset, overrides);
    return { mode, preset, overrides, vars };
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
  const vars = computeThemeCssVars(mode, presetId, overrides, customPresets);
  const payload: ThemeCache = { mode, preset: presetId, overrides, vars };
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

export function effectiveOverrideValue(
  key: keyof ThemeOverrides,
  mode: ThemeMode,
  presetId: string,
  overrides: ThemeOverrides,
  customPresets: SavedThemePreset[] = []
): string | undefined {
  if (overrides[key]) return overrides[key];
  const preset = resolveThemePreset(presetId, customPresets);
  const tokens = tokensForPreset(preset, mode);
  const tokenKey = key as keyof ThemeTokens;
  if (tokenKey in tokens) return tokens[tokenKey];
  return undefined;
}