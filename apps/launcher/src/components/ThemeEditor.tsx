import { useMemo, useState } from "react";
import { RotateCcw } from "lucide-react";
import { useLauncherSettings } from "../context/launcher-settings-context";
import type { ThemeOverrides } from "../lib/launcher-settings";
import { resolveThemePresetOrDefault } from "../lib/theme-presets";
import { hasLowContrast, validateHexColor, validateRadius } from "../lib/theme";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Label } from "./ui/label";

const TOKEN_FIELDS: {
  key: keyof ThemeOverrides;
  label: string;
  type: "color" | "radius";
}[] = [
  { key: "background", label: "Background", type: "color" },
  { key: "foreground", label: "Text", type: "color" },
  { key: "primary", label: "Primary action", type: "color" },
  { key: "accent", label: "Accent surface", type: "color" },
  { key: "card", label: "Cards", type: "color" },
  { key: "border", label: "Borders", type: "color" },
  { key: "muted", label: "Muted surface", type: "color" },
  { key: "muted_foreground", label: "Muted text", type: "color" },
  { key: "accent_foreground", label: "Accent text", type: "color" },
  { key: "radius", label: "Corner radius", type: "radius" },
];

function ThemeTokenField({
  fieldKey,
  label,
  type,
  value,
  presetDefault,
  onChange,
}: {
  fieldKey: keyof ThemeOverrides;
  label: string;
  type: "color" | "radius";
  value: string;
  presetDefault: string;
  onChange: (key: keyof ThemeOverrides, value: string) => void;
}) {
  const [invalid, setInvalid] = useState(false);
  const inputId = `theme-token-${fieldKey}`;
  const errorId = `${inputId}-error`;
  const displayColor = value || presetDefault;

  const valueIsValid = (nextValue: string) => (type === "color" ? validateHexColor(nextValue) : validateRadius(nextValue));

  const commitDraft = (nextValue: string) => {
    const nextIsValid = valueIsValid(nextValue);
    setInvalid(Boolean(nextValue) && !nextIsValid);
    if (!nextValue || nextIsValid) onChange(fieldKey, nextValue);
  };

  return (
    <div className="space-y-1.5">
      <Label htmlFor={inputId}>{label}</Label>
      <div className="flex items-center gap-2">
        {type === "color" && (
          <Input
            type="color"
            value={displayColor.startsWith("#") && displayColor.length >= 7 ? displayColor.slice(0, 7) : "#0a0a0a"}
            onChange={(event) => {
              setInvalid(false);
              onChange(fieldKey, event.target.value);
            }}
            className="h-9 w-10 shrink-0 cursor-pointer p-1"
            aria-label={`Choose ${label.toLowerCase()} color`}
          />
        )}
        <Input
          id={inputId}
          defaultValue={value}
          placeholder={presetDefault}
          onChange={(event) => {
            const nextValue = event.target.value.trim();
            setInvalid(Boolean(nextValue) && !valueIsValid(nextValue));
          }}
          onBlur={(event) => commitDraft(event.target.value.trim())}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
          }}
          className="font-mono text-xs"
          aria-invalid={invalid}
          aria-describedby={invalid ? errorId : undefined}
          spellCheck={false}
        />
      </div>
      {invalid && (
        <p id={errorId} className="text-xs text-destructive">
          {type === "color" ? "Use a 3, 6, or 8 digit hex color." : "Use rem or px, for example 0.5rem."}
        </p>
      )}
    </div>
  );
}

export function ThemeEditor() {
  const { settings, customPresets, setThemeOverrides, resetThemeOverrides } = useLauncherSettings();
  const [draftRevision, setDraftRevision] = useState(0);
  const overrides = settings.theme_overrides;

  const preset = useMemo(() => resolveThemePresetOrDefault(settings.theme_preset, customPresets), [settings.theme_preset, customPresets]);

  const presetDefaults = useMemo(() => {
    const tokens = settings.theme_mode === "dark" ? preset.dark : preset.light;
    return TOKEN_FIELDS.reduce(
      (acc, { key }) => {
        acc[key] = tokens[key as keyof typeof tokens];
        return acc;
      },
      {} as Record<keyof ThemeOverrides, string>,
    );
  }, [preset, settings.theme_mode]);

  const updateField = (key: keyof ThemeOverrides, raw: string) => {
    const next: ThemeOverrides = { ...overrides };
    if (!raw) delete next[key];
    else next[key] = raw;
    setThemeOverrides(next);
  };

  const effectiveForeground = overrides.foreground ?? presetDefaults.foreground;
  const effectiveBackground = overrides.background ?? presetDefaults.background;
  const lowContrast = hasLowContrast(effectiveForeground, effectiveBackground);
  const overrideCount = Object.keys(overrides).length;

  return (
    <div className="space-y-4">
      {lowContrast && (
        <p className="rounded-md border border-amber-500/35 bg-amber-500/10 px-3 py-2 text-sm text-amber-500" role="alert">
          Text and background contrast is low. Some launcher content may be hard to read.
        </p>
      )}

      <div className="grid gap-3 sm:grid-cols-2">
        {TOKEN_FIELDS.map(({ key, label, type }) => (
          <ThemeTokenField
            key={`${draftRevision}:${key}:${overrides[key] ?? ""}`}
            fieldKey={key}
            label={label}
            type={type}
            value={overrides[key] ?? ""}
            presetDefault={presetDefaults[key]}
            onChange={updateField}
          />
        ))}
      </div>

      <div className="flex items-center justify-between gap-3 border-t border-border/70 pt-3">
        <p className="text-xs text-muted-foreground">
          {overrideCount === 0 ? "Using the preset defaults." : `${overrideCount} custom ${overrideCount === 1 ? "adjustment" : "adjustments"} applied.`}
        </p>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            resetThemeOverrides();
            setDraftRevision((revision) => revision + 1);
          }}
        >
          <RotateCcw />
          Reset adjustments
        </Button>
      </div>
    </div>
  );
}
