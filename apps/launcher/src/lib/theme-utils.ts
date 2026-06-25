import type { ThemeOverrides } from "./launcher-settings";
import type { ThemeTokens } from "./theme-presets";

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

export const THEME_OVERRIDE_KEYS = [
  "background",
  "foreground",
  "primary",
  "card",
  "border",
  "muted",
  "muted_foreground",
  "accent",
  "accent_foreground",
  "radius",
] as const satisfies readonly (keyof ThemeOverrides)[];

const HEX_RE = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
const RADIUS_RE = /^\d+(\.\d+)?(rem|px)$/;

export function validateHexColor(value: string): boolean {
  return value.length <= 9 && HEX_RE.test(value);
}

export function validateRadius(value: string): boolean {
  return value.length <= 16 && RADIUS_RE.test(value);
}

export function deriveTokens(
  base: ThemeTokens,
  patch: Partial<ThemeTokens>
): ThemeTokens {
  return { ...base, ...patch };
}

export function mergeOverridesIntoTokens(
  tokens: ThemeTokens,
  overrides: ThemeOverrides
): ThemeTokens {
  const next = { ...tokens };
  for (const key of THEME_OVERRIDE_KEYS) {
    const value = overrides[key];
    if (value) next[key] = value;
  }
  return next;
}

export function tokensToCssVars(
  tokens: ThemeTokens,
  overrides: ThemeOverrides = {}
): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const [key, cssVar] of Object.entries(THEME_TOKEN_CSS_VARS)) {
    const tokenKey = key as keyof ThemeTokens;
    const override = overrides[tokenKey as keyof ThemeOverrides];
    vars[cssVar] = override ?? tokens[tokenKey];
  }
  return vars;
}

export function isValidThemeTokens(tokens: unknown): tokens is ThemeTokens {
  if (!tokens || typeof tokens !== "object") return false;
  const record = tokens as Record<string, unknown>;
  for (const key of Object.keys(THEME_TOKEN_CSS_VARS)) {
    const value = record[key];
    if (typeof value !== "string") return false;
    if (key === "radius") {
      if (!validateRadius(value)) return false;
    } else if (!validateHexColor(value)) {
      return false;
    }
  }
  return true;
}