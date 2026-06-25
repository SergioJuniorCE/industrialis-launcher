import type { ThemeMode } from "./launcher-settings";

export interface ThemeTokens {
  background: string;
  foreground: string;
  card: string;
  card_foreground: string;
  popover: string;
  popover_foreground: string;
  primary: string;
  primary_foreground: string;
  secondary: string;
  secondary_foreground: string;
  muted: string;
  muted_foreground: string;
  accent: string;
  accent_foreground: string;
  destructive: string;
  destructive_foreground: string;
  border: string;
  input: string;
  ring: string;
  radius: string;
}

export interface ThemePreset {
  id: string;
  name: string;
  description: string;
  builtin: boolean;
  dark: ThemeTokens;
  light: ThemeTokens;
}

export type BuiltinThemePresetId = "monochrome" | "industrialis" | "midnight" | "sandstone";

export interface SavedThemePreset extends ThemePreset {
  builtin: false;
}

const MONOCHROME_DARK: ThemeTokens = {
  background: "#0a0a0a",
  foreground: "#fafafa",
  card: "#141414",
  card_foreground: "#fafafa",
  popover: "#141414",
  popover_foreground: "#fafafa",
  primary: "#fafafa",
  primary_foreground: "#0a0a0a",
  secondary: "#1f1f1f",
  secondary_foreground: "#fafafa",
  muted: "#1a1a1a",
  muted_foreground: "#a3a3a3",
  accent: "#262626",
  accent_foreground: "#fafafa",
  destructive: "#991b1b",
  destructive_foreground: "#fafafa",
  border: "#2a2a2a",
  input: "#2a2a2a",
  ring: "#d4d4d4",
  radius: "0.375rem",
};

const MONOCHROME_LIGHT: ThemeTokens = {
  background: "#fafafa",
  foreground: "#0a0a0a",
  card: "#ffffff",
  card_foreground: "#0a0a0a",
  popover: "#ffffff",
  popover_foreground: "#0a0a0a",
  primary: "#171717",
  primary_foreground: "#fafafa",
  secondary: "#f5f5f5",
  secondary_foreground: "#171717",
  muted: "#f5f5f5",
  muted_foreground: "#737373",
  accent: "#f5f5f5",
  accent_foreground: "#171717",
  destructive: "#dc2626",
  destructive_foreground: "#fafafa",
  border: "#e5e5e5",
  input: "#e5e5e5",
  ring: "#525252",
  radius: "0.375rem",
};

/** Matches apps/website — dark neutral base with bronze accent. */
const INDUSTRIALIS_DARK: ThemeTokens = {
  ...MONOCHROME_DARK,
  accent: "#c9a227",
  accent_foreground: "#0a0a0a",
  primary: "#fafafa",
  primary_foreground: "#0a0a0a",
  ring: "#c9a227",
  radius: "0.5rem",
};

const INDUSTRIALIS_LIGHT: ThemeTokens = {
  ...MONOCHROME_LIGHT,
  accent: "#b8921f",
  accent_foreground: "#fafafa",
  ring: "#b8921f",
  radius: "0.5rem",
};

const MIDNIGHT_DARK: ThemeTokens = {
  background: "#070b14",
  foreground: "#e8edf8",
  card: "#0f1623",
  card_foreground: "#e8edf8",
  popover: "#0f1623",
  popover_foreground: "#e8edf8",
  primary: "#60a5fa",
  primary_foreground: "#070b14",
  secondary: "#152033",
  secondary_foreground: "#e8edf8",
  muted: "#121a28",
  muted_foreground: "#8fa3bf",
  accent: "#3b82f6",
  accent_foreground: "#f8fafc",
  destructive: "#b91c1c",
  destructive_foreground: "#fafafa",
  border: "#243044",
  input: "#243044",
  ring: "#60a5fa",
  radius: "0.5rem",
};

const MIDNIGHT_LIGHT: ThemeTokens = {
  background: "#f4f7fc",
  foreground: "#0c1524",
  card: "#ffffff",
  card_foreground: "#0c1524",
  popover: "#ffffff",
  popover_foreground: "#0c1524",
  primary: "#1d4ed8",
  primary_foreground: "#f8fafc",
  secondary: "#e8eef8",
  secondary_foreground: "#0c1524",
  muted: "#e8eef8",
  muted_foreground: "#5b6b82",
  accent: "#2563eb",
  accent_foreground: "#f8fafc",
  destructive: "#dc2626",
  destructive_foreground: "#fafafa",
  border: "#d5deed",
  input: "#d5deed",
  ring: "#2563eb",
  radius: "0.5rem",
};

const SANDSTONE_DARK: ThemeTokens = {
  background: "#12100d",
  foreground: "#f5efe6",
  card: "#1c1814",
  card_foreground: "#f5efe6",
  popover: "#1c1814",
  popover_foreground: "#f5efe6",
  primary: "#e8dcc8",
  primary_foreground: "#12100d",
  secondary: "#2a241e",
  secondary_foreground: "#f5efe6",
  muted: "#221e19",
  muted_foreground: "#b8a998",
  accent: "#c17f3a",
  accent_foreground: "#12100d",
  destructive: "#9f2d2d",
  destructive_foreground: "#fafafa",
  border: "#3a322a",
  input: "#3a322a",
  ring: "#c17f3a",
  radius: "0.5rem",
};

const SANDSTONE_LIGHT: ThemeTokens = {
  background: "#f7f1e8",
  foreground: "#1a1510",
  card: "#fffdf9",
  card_foreground: "#1a1510",
  popover: "#fffdf9",
  popover_foreground: "#1a1510",
  primary: "#5c3d1e",
  primary_foreground: "#fffdf9",
  secondary: "#efe6d8",
  secondary_foreground: "#1a1510",
  muted: "#efe6d8",
  muted_foreground: "#7a6a58",
  accent: "#b87333",
  accent_foreground: "#fffdf9",
  destructive: "#c2410c",
  destructive_foreground: "#fafafa",
  border: "#e0d4c4",
  input: "#e0d4c4",
  ring: "#b87333",
  radius: "0.5rem",
};

export const BUILTIN_THEME_PRESETS: ThemePreset[] = [
  {
    id: "monochrome",
    name: "Monochrome",
    description: "Neutral dark UI — the original launcher look.",
    builtin: true,
    dark: MONOCHROME_DARK,
    light: MONOCHROME_LIGHT,
  },
  {
    id: "industrialis",
    name: "Industrialis",
    description: "Website design — charcoal base with bronze highlights.",
    builtin: true,
    dark: INDUSTRIALIS_DARK,
    light: INDUSTRIALIS_LIGHT,
  },
  {
    id: "midnight",
    name: "Midnight",
    description: "Cool blue industrial palette for late-night sessions.",
    builtin: true,
    dark: MIDNIGHT_DARK,
    light: MIDNIGHT_LIGHT,
  },
  {
    id: "sandstone",
    name: "Sandstone",
    description: "Warm parchment tones inspired by factory lighting.",
    builtin: true,
    dark: SANDSTONE_DARK,
    light: SANDSTONE_LIGHT,
  },
];

export const DEFAULT_THEME_PRESET_ID: BuiltinThemePresetId = "industrialis";

export const CUSTOM_PRESETS_STORAGE_KEY = "industrialis-custom-theme-presets";

export function tokensForPreset(
  preset: ThemePreset,
  mode: ThemeMode
): ThemeTokens {
  return mode === "dark" ? preset.dark : preset.light;
}

export function findBuiltinPreset(id: string): ThemePreset | undefined {
  return BUILTIN_THEME_PRESETS.find((p) => p.id === id);
}

export function resolveThemePreset(
  id: string,
  customPresets: SavedThemePreset[] = []
): ThemePreset {
  const builtin = findBuiltinPreset(id);
  if (builtin) return builtin;
  const custom = customPresets.find((p) => p.id === id);
  if (custom) return custom;
  return findBuiltinPreset(DEFAULT_THEME_PRESET_ID)!;
}

export function readCustomThemePresets(): SavedThemePreset[] {
  try {
    const raw = localStorage.getItem(CUSTOM_PRESETS_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as SavedThemePreset[];
    return Array.isArray(parsed) ? parsed.filter((p) => p.id && p.name) : [];
  } catch {
    return [];
  }
}

export function writeCustomThemePresets(presets: SavedThemePreset[]): void {
  localStorage.setItem(CUSTOM_PRESETS_STORAGE_KEY, JSON.stringify(presets));
}

export function createCustomPresetId(): string {
  return `custom-${crypto.randomUUID().slice(0, 8)}`;
}