import { useLauncherSettings } from "../context/LauncherSettingsContext";
import type { ThemeOverrides } from "../lib/launcher-settings";
import {
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
  { key: "primary", label: "Accent / Primary", type: "color" },
  { key: "card", label: "Cards", type: "color" },
  { key: "border", label: "Borders", type: "color" },
  { key: "muted", label: "Muted surface", type: "color" },
  { key: "muted_foreground", label: "Muted text", type: "color" },
  { key: "radius", label: "Corner radius", type: "radius" },
];

export function ThemeEditor() {
  const { settings, setThemeOverrides, resetThemeOverrides } = useLauncherSettings();
  const overrides = settings.theme_overrides;

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

  const lowContrast = hasLowContrast(overrides.foreground, overrides.background);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Theme Editor</CardTitle>
        <CardDescription>
          Base: {settings.theme_mode === "dark" ? "Dark" : "Light"} monochrome preset.
          Secondary, accent, destructive, ring, and input tokens follow the preset.
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
            return (
              <div key={key} className="flex items-center gap-3">
                <label className="text-sm w-32 shrink-0">{label}</label>
                {type === "color" ? (
                  <>
                    <input
                      type="color"
                      value={value.startsWith("#") && value.length >= 7 ? value.slice(0, 7) : "#0a0a0a"}
                      onChange={(e) => updateField(key, e.target.value)}
                      className="h-8 w-10 rounded border border-input bg-transparent cursor-pointer"
                    />
                    <Input
                      value={value}
                      placeholder="Preset default"
                      onChange={(e) => updateField(key, e.target.value)}
                      className="font-mono text-xs"
                    />
                  </>
                ) : (
                  <Input
                    value={value}
                    placeholder="0.375rem"
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