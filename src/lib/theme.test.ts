import { describe, expect, it, beforeEach } from "vitest";
import {
  THEME_OVERRIDE_CSS_VARS,
  applyTheme,
  mergeThemeCacheIntoSettings,
  readThemeCache,
  validateHexColor,
  validateRadius,
  writeThemeCache,
  THEME_CACHE_KEY,
} from "./theme";
import { DEFAULT_LAUNCHER_SETTINGS } from "./launcher-settings";

describe("theme helpers", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    for (const cssVar of Object.values(THEME_OVERRIDE_CSS_VARS)) {
      document.documentElement.style.removeProperty(cssVar);
    }
  });

  it("maps override keys to kebab-case css vars", () => {
    expect(THEME_OVERRIDE_CSS_VARS.muted_foreground).toBe("--theme-muted-foreground");
    expect(THEME_OVERRIDE_CSS_VARS.background).toBe("--theme-background");
  });

  it("validates hex and radius", () => {
    expect(validateHexColor("#0a0a0a")).toBe(true);
    expect(validateHexColor("red")).toBe(false);
    expect(validateRadius("0.375rem")).toBe(true);
    expect(validateRadius("12px")).toBe(true);
    expect(validateRadius("bad")).toBe(false);
  });

  it("applyTheme sets data-theme and inline styles", () => {
    applyTheme("light", { background: "#ffffff" });
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(document.documentElement.style.getPropertyValue("--theme-background")).toBe("#ffffff");
    applyTheme("dark", {});
    expect(document.documentElement.style.getPropertyValue("--theme-background")).toBe("");
  });

  it("reads and writes theme cache", () => {
    writeThemeCache("dark", { muted_foreground: "#b0b0b0" });
    const cache = readThemeCache();
    expect(cache?.mode).toBe("dark");
    expect(cache?.overrides.muted_foreground).toBe("#b0b0b0");
    expect(localStorage.getItem(THEME_CACHE_KEY)).toContain("muted_foreground");
  });

  it("mergeThemeCacheIntoSettings hydrates defaults", () => {
    writeThemeCache("light", { background: "#fafafa" });
    const merged = mergeThemeCacheIntoSettings(
      DEFAULT_LAUNCHER_SETTINGS,
      readThemeCache()
    );
    expect(merged.theme_mode).toBe("light");
    expect(merged.theme_overrides.background).toBe("#fafafa");
  });
});