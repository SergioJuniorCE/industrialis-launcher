import { DEFAULT_THEME_PRESET_ID } from "./theme-presets";
import { computeThemeCssVars } from "./theme";

/** Canonical cold-start payload — injected into index.html at build time. */
export const THEME_BOOT_DEFAULT = {
  mode: "dark" as const,
  preset: DEFAULT_THEME_PRESET_ID,
  effect: "grid" as const,
  vars: computeThemeCssVars("dark", DEFAULT_THEME_PRESET_ID),
};

export function themeBootInlineScript(): string {
  const payload = JSON.stringify(THEME_BOOT_DEFAULT);
  return `(function(){try{var c=JSON.parse(localStorage.getItem("industrialis-theme-cache")||"{}");
var d=${payload};
var mode=c.mode==="light"?"light":d.mode;
var preset=typeof c.preset==="string"&&c.preset?c.preset:d.preset;
var effect=c.effect==="grid"||c.effect==="none"?c.effect:d.effect;
var root=document.documentElement;
root.setAttribute("data-theme",mode);
root.setAttribute("data-theme-preset",preset);
root.setAttribute("data-theme-effect",effect);
root.style.colorScheme=mode;
var vars=c.vars&&typeof c.vars==="object"?c.vars:d.vars;
Object.keys(vars).forEach(function(k){var v=vars[k];if(v)root.style.setProperty(k,v);});
}catch(e){}})();`;
}