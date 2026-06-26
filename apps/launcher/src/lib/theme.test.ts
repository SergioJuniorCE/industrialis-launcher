import { describe, expect, it, beforeEach } from "vitest";
import {
  THEME_TOKEN_CSS_VARS,
  applyTheme,
  computeThemeCssVars,
  mergeThemeCacheIntoSettings,
  readThemeCache,
  validateHexColor,
  validateRadius,
  writeThemeCache,
  THEME_CACHE_KEY,
} from "./theme";
import { DEFAULT_LAUNCHER_SETTINGS } from "./launcher-settings";
import { parseSavedThemePresets } from "./theme-store";

describe("theme helpers", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.removeAttribute("data-theme-preset");
    document.documentElement.removeAttribute("data-theme-effect");
    for (const cssVar of Object.values(THEME_TOKEN_CSS_VARS)) {
      document.documentElement.style.removeProperty(cssVar);
    }
  });

  it("maps token keys to kebab-case css vars", () => {
    expect(THEME_TOKEN_CSS_VARS.muted_foreground).toBe("--theme-muted-foreground");
    expect(THEME_TOKEN_CSS_VARS.background).toBe("--theme-background");
    expect(THEME_TOKEN_CSS_VARS.accent).toBe("--theme-accent");
  });

  it("validates hex and radius", () => {
    expect(validateHexColor("#0a0a0a")).toBe(true);
    expect(validateHexColor("red")).toBe(false);
    expect(validateRadius("0.375rem")).toBe(true);
    expect(validateRadius("12px")).toBe(true);
    expect(validateRadius("bad")).toBe(false);
  });

  it("applyTheme sets data attributes, effect, and full token vars", () => {
    applyTheme("light", "industrialis", { background: "#ffffff" });
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(document.documentElement.getAttribute("data-theme-preset")).toBe("industrialis");
    expect(document.documentElement.getAttribute("data-theme-effect")).toBe("grid");
    expect(document.documentElement.style.getPropertyValue("--theme-background")).toBe("#ffffff");
    expect(document.documentElement.style.getPropertyValue("--theme-accent")).toBe("#ebe4d4");
    applyTheme("dark", "monochrome", {});
    expect(document.documentElement.getAttribute("data-theme-effect")).toBe("none");
    expect(document.documentElement.style.getPropertyValue("--theme-accent")).toBe("#262626");
  });

  it("computeThemeCssVars uses industrialis bronze as primary", () => {
    const vars = computeThemeCssVars("dark", "industrialis");
    expect(vars["--theme-primary"]).toBe("#c9a227");
    expect(vars["--theme-accent"]).toBe("#1a1610");
    expect(vars["--theme-ring"]).toBe("#c9a227");
  });

  it("reads and writes theme cache with preset, effect, and vars", () => {
    writeThemeCache("dark", "midnight", { muted_foreground: "#b0b0b0" });
    const cache = readThemeCache();
    expect(cache?.version).toBe(2);
    expect(cache?.mode).toBe("dark");
    expect(cache?.preset).toBe("midnight");
    expect(cache?.effect).toBe("none");
    expect(cache?.overrides.muted_foreground).toBe("#b0b0b0");
    expect(cache?.vars["--theme-muted-foreground"]).toBe("#b0b0b0");
    expect(localStorage.getItem(THEME_CACHE_KEY)).toContain("midnight");
  });

  it("ignores stale theme cache without version", () => {
    localStorage.setItem(
      THEME_CACHE_KEY,
      JSON.stringify({
        mode: "dark",
        preset: "industrialis",
        effect: "grid",
        overrides: {},
        vars: { "--theme-accent": "#c9a227" },
      })
    );
    expect(readThemeCache()).toBeNull();
  });

  it("mergeThemeCacheIntoSettings hydrates defaults", () => {
    writeThemeCache("light", "sandstone", { background: "#fafafa" });
    const merged = mergeThemeCacheIntoSettings(
      DEFAULT_LAUNCHER_SETTINGS,
      readThemeCache()
    );
    expect(merged.theme_mode).toBe("light");
    expect(merged.theme_preset).toBe("sandstone");
    expect(merged.theme_overrides.background).toBe("#fafafa");
  });

  it("parseSavedThemePresets rejects invalid token payloads", () => {
    const valid = parseSavedThemePresets([
      {
        id: "custom-abc12345",
        name: "Mine",
        description: "x",
        background_effect: "grid",
        dark: { background: "#0a0a0a" },
        light: { background: "#fafafa" },
      },
    ]);
    expect(valid).toHaveLength(0);
  });
});