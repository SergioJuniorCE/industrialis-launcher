import { describe, expect, it, beforeEach } from "vitest";
import {
  THEME_OVERRIDE_CSS_VARS,
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

describe("theme helpers", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.removeAttribute("data-theme-preset");
    for (const cssVar of Object.values(THEME_TOKEN_CSS_VARS)) {
      document.documentElement.style.removeProperty(cssVar);
    }
  });

  it("maps override keys to kebab-case css vars", () => {
    expect(THEME_OVERRIDE_CSS_VARS.muted_foreground).toBe("--theme-muted-foreground");
    expect(THEME_OVERRIDE_CSS_VARS.background).toBe("--theme-background");
    expect(THEME_OVERRIDE_CSS_VARS.accent).toBe("--theme-accent");
  });

  it("validates hex and radius", () => {
    expect(validateHexColor("#0a0a0a")).toBe(true);
    expect(validateHexColor("red")).toBe(false);
    expect(validateRadius("0.375rem")).toBe(true);
    expect(validateRadius("12px")).toBe(true);
    expect(validateRadius("bad")).toBe(false);
  });

  it("applyTheme sets data attributes and full token vars", () => {
    applyTheme("light", "industrialis", { background: "#ffffff" });
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(document.documentElement.getAttribute("data-theme-preset")).toBe("industrialis");
    expect(document.documentElement.style.getPropertyValue("--theme-background")).toBe("#ffffff");
    expect(document.documentElement.style.getPropertyValue("--theme-accent")).toBe("#b8921f");
    applyTheme("dark", "monochrome", {});
    expect(document.documentElement.getAttribute("data-theme-preset")).toBe("monochrome");
    expect(document.documentElement.style.getPropertyValue("--theme-accent")).toBe("#262626");
  });

  it("computeThemeCssVars uses industrialis bronze accent", () => {
    const vars = computeThemeCssVars("dark", "industrialis");
    expect(vars["--theme-accent"]).toBe("#c9a227");
    expect(vars["--theme-ring"]).toBe("#c9a227");
  });

  it("reads and writes theme cache with preset and vars", () => {
    writeThemeCache("dark", "midnight", { muted_foreground: "#b0b0b0" });
    const cache = readThemeCache();
    expect(cache?.mode).toBe("dark");
    expect(cache?.preset).toBe("midnight");
    expect(cache?.overrides.muted_foreground).toBe("#b0b0b0");
    expect(cache?.vars["--theme-muted-foreground"]).toBe("#b0b0b0");
    expect(localStorage.getItem(THEME_CACHE_KEY)).toContain("midnight");
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
});