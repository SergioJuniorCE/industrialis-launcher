import { useLauncherSettings } from "../context/LauncherSettingsContext";
import type { ThemeOverrides } from "../lib/launcher-settings";
import { resolveThemePreset } from "../lib/theme-presets";
import {
  effectiveOverrideValue,
  hasLowContrast,
  validateHexColor,
  validateRadius,
} from "../lib/theme";
import { Button } from "./ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./ui/card";
import { Input } from "./ui/input";

const TOKEN_FIELDS: {
  key: keyof ThemeOverrides;
  label: string;
  type: "color" | "radius";
}[] = [
  { key: "background", label: "Background", type: "color" },
  { key: "foreground", label: "Text", type: "color" },
  { key: "primary", label: "Primary", type: "color" },
  { key: "accent", label: "Accent", type: "color" },
  { key: "card", label: "Cards", type: "color" },
  { key: "border", label: "Borders", type: "color" },
  { key: "muted", label: "Muted surface", type: "color" },
  { key: "muted_foreground", label: "Muted text", type: "color" },
  { key: "accent_foreground", label: "Accent text", type: "color" },
  { key: "radius", label: "Corner radius", type: "radius" },
];

export function ThemeEditor() {
  const { settings, customPresets, setThemeOverrides, resetThemeOverrides } = useLauncherSettings();
  const overrides = settings.theme_overrides;
  const preset = resolveThemePreset(settings.theme_preset, customPresets);

  const updateField = (key: keyof ThemeOverrides, raw: string) => {
    const field = TOKEN_FIELDS.find((f) => f.key === key);
    if (!field) return;
    if (field.type === "color" && raw && !validateHexColor(raw)) return;
    if (field.type === "radius" && raw && !validateRadius(raw)) return;

    const next: ThemeOverrides = { ...overrides };
    if (!raw) {
      delete next[key];
    } else {
      next[key] = raw;
    }
    setThemeOverrides(next);
  };

  const effectiveForeground = effectiveOverrideValue(
    "foreground",
    settings.theme_mode,
    settings.theme_preset,
    overrides,
    customPresets
  );
  const effectiveBackground = effectiveOverrideValue(
    "background",
    settings.theme_mode,
    settings.theme_preset,
    overrides,
    customPresets
  );
  const lowContrast = hasLowContrast(effectiveForeground, effectiveBackground);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Theme Editor</CardTitle>
        <CardDescription>
          Base: {preset.name} ({settings.theme_mode === "dark" ? "dark" : "light"}). Leave a field
          empty to use the preset default. Secondary, destructive, ring, and input tokens follow the
          preset.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {lowContrast && (
          <p className="text-sm text-amber-500">
            Low contrast — text may be hard to read.
          </p>
        )}
        <div className="grid gap-3">
          {TOKEN_FIELDS.map(({ key, label, type }) => {
            const value = overrides[key] ?? "";
            const presetDefault = effectiveOverrideValue(
              key,
              settings.theme_mode,
              settings.theme_preset,
              {},
              customPresets
            );
            return (
              <div key={key} className="flex items-center gap-3">
                <label className="text-sm w-32 shrink-0">{label}</label>
                {type === "color" ? (
                  <>
                    <input
                      type="color"
                      value={
                        (value || presetDefault || "#0a0a0a").startsWith("#") &&
                        (value || presetDefault || "#0a0a0a").length >= 7
                          ? (value || presetDefault || "#0a0a0a").slice(0, 7)
                          : "#0a0a0a"
                      }
                      onChange={(e) => updateField(key, e.target.value)}
                      className="h-8 w-10 rounded border border-input bg-transparent cursor-pointer"
                    />
                    <Input
                      value={value}
                      placeholder={presetDefault ?? "Preset default"}
                      onChange={(e) => updateField(key, e.target.value)}
                      className="font-mono text-xs"
                    />
                  </>
                ) : (
                  <Input
                    value={value}
                    placeholder={presetDefault ?? "0.375rem"}
                    onChange={(e) => updateField(key, e.target.value)}
                    className="font-mono text-xs"
                  />
                )}
              </div>
            );
          })}
        </div>
        <Button variant="outline" onClick={resetThemeOverrides}>
          Reset to preset
        </Button>
      </CardContent>
    </Card>
  );
}