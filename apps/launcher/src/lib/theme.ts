import type { ThemeMode, ThemeOverrides } from "./launcher-settings";

export const THEME_CACHE_KEY = "industrialis-theme-cache";

export const THEME_OVERRIDE_CSS_VARS = {
  background: "--theme-background",
  foreground: "--theme-foreground",
  primary: "--theme-primary",
  card: "--theme-card",
  border: "--theme-border",
  muted: "--theme-muted",
  muted_foreground: "--theme-muted-foreground",
  radius: "--theme-radius",
} as const satisfies Record<keyof ThemeOverrides, string>;

export interface ThemeCache {
  mode: ThemeMode;
  overrides: ThemeOverrides;
}

const HEX_RE = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
const RADIUS_RE = /^\d+(\.\d+)?(rem|px)$/;

export function validateHexColor(value: string): boolean {
  return value.length <= 9 && HEX_RE.test(value);
}

export function validateRadius(value: string): boolean {
  return value.length <= 16 && RADIUS_RE.test(value);
}

export function applyTheme(mode: ThemeMode, overrides: ThemeOverrides = {}): void {
  const root = document.documentElement;
  root.setAttribute("data-theme", mode);
  for (const [key, cssVar] of Object.entries(THEME_OVERRIDE_CSS_VARS)) {
    const value = overrides[key as keyof ThemeOverrides];
    if (value) {
      root.style.setProperty(cssVar, value);
    } else {
      root.style.removeProperty(cssVar);
    }
  }
  if (import.meta.env.DEV) {
    const overrideCount = Object.values(overrides).filter(Boolean).length;
    console.debug("[theme] applied", { mode, overrideCount });
  }
}

export function readThemeCache(): ThemeCache | null {
  try {
    const raw = localStorage.getItem(THEME_CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<ThemeCache>;
    const mode: ThemeMode = parsed.mode === "light" ? "light" : "dark";
    const overrides =
      parsed.overrides && typeof parsed.overrides === "object" ? parsed.overrides : {};
    return { mode, overrides };
  } catch {
    return null;
  }
}

export function writeThemeCache(mode: ThemeMode, overrides: ThemeOverrides): void {
  const payload: ThemeCache = { mode, overrides };
  localStorage.setItem(THEME_CACHE_KEY, JSON.stringify(payload));
}

export function mergeThemeCacheIntoSettings<T extends { theme_mode: ThemeMode; theme_overrides: ThemeOverrides }>(
  defaults: T,
  cache: ThemeCache | null
): T {
  if (!cache) return defaults;
  return {
    ...defaults,
    theme_mode: cache.mode,
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