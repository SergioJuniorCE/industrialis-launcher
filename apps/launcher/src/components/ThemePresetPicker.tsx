import { useMemo, useState, type CSSProperties, type FormEvent } from "react";
import { Check, ChevronDown, Moon, Palette, Save, SlidersHorizontal, Sun, Trash2 } from "lucide-react";
import { useLauncherSettings } from "../context/launcher-settings-context";
import { BUILTIN_THEME_PRESETS, resolveThemePresetOrDefault, tokensForPreset, type ThemePreset, type ThemeTokens } from "../lib/theme-presets";
import { mergeOverridesIntoTokens } from "../lib/theme-utils";
import { ThemeEditor } from "./ThemeEditor";
import { Button } from "./ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./ui/card";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "./ui/dialog";
import { Input } from "./ui/input";
import { Label } from "./ui/label";

function ThemePreview({ tokens, compact = false }: { tokens: ThemeTokens; compact?: boolean }) {
  const style = {
    backgroundColor: tokens.background,
    color: tokens.foreground,
    borderColor: tokens.border,
    borderRadius: tokens.radius,
  } satisfies CSSProperties;

  return (
    <div className={`shrink-0 overflow-hidden border ${compact ? "h-14 w-20" : "h-24 w-full"}`} style={style} aria-hidden="true">
      <div className="flex h-5 items-center gap-1.5 border-b px-2" style={{ borderColor: tokens.border, backgroundColor: tokens.card }}>
        <span className="size-1.5 rounded-full" style={{ backgroundColor: tokens.primary }} />
        <span className="h-1 w-6 rounded-full" style={{ backgroundColor: tokens.muted_foreground }} />
      </div>
      <div className={compact ? "space-y-1 p-1.5" : "grid grid-cols-[1fr_auto] gap-2 p-2.5"}>
        <div
          className={compact ? "h-4 rounded-sm border" : "space-y-1.5 rounded border p-2"}
          style={{ borderColor: tokens.border, backgroundColor: tokens.card }}
        >
          {!compact && (
            <>
              <span className="block h-1.5 w-3/4 rounded-full" style={{ backgroundColor: tokens.foreground }} />
              <span className="block h-1 w-1/2 rounded-full" style={{ backgroundColor: tokens.muted_foreground }} />
            </>
          )}
        </div>
        <span className={compact ? "block h-2 rounded-sm" : "block h-7 w-12 rounded-sm"} style={{ backgroundColor: tokens.primary }} />
      </div>
    </div>
  );
}

function PresetOption({
  preset,
  selected,
  tokens,
  onSelect,
  onDelete,
}: {
  preset: ThemePreset;
  selected: boolean;
  tokens: ThemeTokens;
  onSelect: () => void;
  onDelete?: () => void;
}) {
  return (
    <div
      className={`relative rounded-lg border transition-colors ${
        selected ? "border-primary bg-primary/10" : "border-border/80 bg-background/35 hover:border-primary/55"
      }`}
    >
      <button
        type="button"
        aria-pressed={selected}
        onClick={onSelect}
        className="flex min-h-24 w-full items-start gap-3 rounded-lg p-3 pr-10 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
      >
        <ThemePreview tokens={tokens} compact />
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-1.5 text-sm font-medium">
            {preset.name}
            {selected && <Check className="size-3.5 text-primary" aria-hidden="true" />}
          </span>
          <span className="mt-1 block text-xs leading-relaxed text-muted-foreground">{preset.description}</span>
        </span>
      </button>
      {onDelete && (
        <Button
          variant="ghost"
          size="icon"
          className="absolute right-1.5 top-1.5"
          onClick={onDelete}
          aria-label={`Delete ${preset.name}`}
          title={`Delete ${preset.name}`}
        >
          <Trash2 />
        </Button>
      )}
    </div>
  );
}

export function ThemePresetPicker() {
  const { settings, customPresets, setThemeMode, setThemePreset, saveCustomPreset, deleteCustomPreset } = useLauncherSettings();
  const [saveOpen, setSaveOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<ThemePreset | null>(null);
  const [presetName, setPresetName] = useState("");
  const [presetDescription, setPresetDescription] = useState("");

  const activePreset = useMemo(() => resolveThemePresetOrDefault(settings.theme_preset, customPresets), [settings.theme_preset, customPresets]);
  const activeTokens = useMemo(
    () => mergeOverridesIntoTokens(tokensForPreset(activePreset, settings.theme_mode), settings.theme_overrides),
    [activePreset, settings.theme_mode, settings.theme_overrides],
  );
  const overrideCount = Object.keys(settings.theme_overrides).length;

  const closeSaveDialog = () => {
    setSaveOpen(false);
    setPresetName("");
    setPresetDescription("");
  };

  const handleSave = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = presetName.trim();
    if (!name) return;
    saveCustomPreset(name, presetDescription.trim() || undefined);
    closeSaveDialog();
  };

  const renderPreset = (preset: ThemePreset) => {
    const selected = settings.theme_preset === preset.id;
    const baseTokens = tokensForPreset(preset, settings.theme_mode);
    return (
      <PresetOption
        key={preset.id}
        preset={preset}
        selected={selected}
        tokens={selected ? mergeOverridesIntoTokens(baseTokens, settings.theme_overrides) : baseTokens}
        onSelect={() => setThemePreset(preset.id)}
        onDelete={preset.builtin ? undefined : () => setDeleteTarget(preset)}
      />
    );
  };

  return (
    <Card>
      <CardHeader className="gap-3 sm:flex-row sm:items-start sm:justify-between sm:space-y-0">
        <div>
          <CardTitle className="flex items-center gap-2">
            <Palette className="size-4 text-primary" />
            Appearance
          </CardTitle>
          <CardDescription className="mt-1">Pick a theme, preview it live, then fine-tune it only if you need to.</CardDescription>
        </div>
        <div className="inline-flex w-fit rounded-md border border-border bg-muted/45 p-0.5" role="group" aria-label="Color mode">
          <Button
            variant={settings.theme_mode === "light" ? "secondary" : "ghost"}
            size="sm"
            className={settings.theme_mode === "light" ? "bg-background shadow-sm" : ""}
            aria-pressed={settings.theme_mode === "light"}
            onClick={() => setThemeMode("light")}
          >
            <Sun />
            Light
          </Button>
          <Button
            variant={settings.theme_mode === "dark" ? "secondary" : "ghost"}
            size="sm"
            className={settings.theme_mode === "dark" ? "bg-background shadow-sm" : ""}
            aria-pressed={settings.theme_mode === "dark"}
            onClick={() => setThemeMode("dark")}
          >
            <Moon />
            Dark
          </Button>
        </div>
      </CardHeader>

      <CardContent className="space-y-5">
        <section
          className="grid gap-3 rounded-lg border border-border/80 bg-muted/25 p-3 sm:grid-cols-[11rem_1fr] sm:items-center"
          aria-label="Active theme preview"
        >
          <ThemePreview tokens={activeTokens} />
          <div className="min-w-0">
            <p className="text-xs font-medium text-muted-foreground">Active theme</p>
            <p className="mt-0.5 truncate text-base font-semibold">{activePreset.name}</p>
            <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
              {settings.theme_mode === "dark" ? "Dark mode" : "Light mode"}
              {overrideCount > 0 ? ` with ${overrideCount} custom ${overrideCount === 1 ? "adjustment" : "adjustments"}` : " using preset defaults"}. Changes
              apply immediately.
            </p>
          </div>
        </section>

        <section aria-labelledby="built-in-themes-heading">
          <div className="mb-2 flex items-center justify-between gap-3">
            <h3 id="built-in-themes-heading" className="text-sm font-semibold">
              Built-in themes
            </h3>
            <span className="text-xs text-muted-foreground">Select to apply</span>
          </div>
          <div className="grid gap-2 sm:grid-cols-2">{BUILTIN_THEME_PRESETS.map(renderPreset)}</div>
        </section>

        {customPresets.length > 0 && (
          <section aria-labelledby="saved-themes-heading">
            <div className="mb-2 flex items-center justify-between gap-3">
              <h3 id="saved-themes-heading" className="text-sm font-semibold">
                Saved themes
              </h3>
              <span className="text-xs text-muted-foreground">
                {customPresets.length} {customPresets.length === 1 ? "theme" : "themes"}
              </span>
            </div>
            <div className="grid gap-2 sm:grid-cols-2">{customPresets.map(renderPreset)}</div>
          </section>
        )}

        <details className="group rounded-lg border border-border/80 bg-background/30">
          <summary className="flex cursor-pointer list-none items-center gap-3 rounded-lg px-3 py-3 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring [&::-webkit-details-marker]:hidden">
            <SlidersHorizontal className="size-4 text-primary" aria-hidden="true" />
            <span className="min-w-0 flex-1">
              <span className="block text-sm font-medium">Advanced customization</span>
              <span className="block text-xs text-muted-foreground">Edit colors and corner radius for the current look.</span>
            </span>
            {overrideCount > 0 && <span className="text-xs text-muted-foreground">{overrideCount} changed</span>}
            <ChevronDown className="size-4 text-muted-foreground transition-transform group-open:rotate-180" aria-hidden="true" />
          </summary>
          <div className="border-t border-border/70 p-3">
            <ThemeEditor key={`${settings.theme_preset}:${settings.theme_mode}`} />
          </div>
        </details>

        <div className="flex flex-col gap-2 border-t border-border/70 pt-4 sm:flex-row sm:items-center sm:justify-between">
          <p className="text-xs text-muted-foreground">Save the current theme and adjustments for quick reuse.</p>
          <Button variant="outline" onClick={() => setSaveOpen(true)}>
            <Save />
            Save as new theme
          </Button>
        </div>
      </CardContent>

      <Dialog
        open={saveOpen}
        onOpenChange={(open) => {
          if (open) setSaveOpen(true);
          else closeSaveDialog();
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Save as new theme</DialogTitle>
            <DialogDescription>
              The current {settings.theme_mode} appearance is saved with your adjustments. The other mode keeps the {activePreset.name} palette.
            </DialogDescription>
          </DialogHeader>
          <form className="space-y-4" onSubmit={handleSave}>
            <div className="space-y-2">
              <Label htmlFor="preset-name">Theme name</Label>
              <Input
                id="preset-name"
                value={presetName}
                onChange={(event) => setPresetName(event.target.value)}
                placeholder="My launcher theme"
                maxLength={48}
                autoFocus
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="preset-description">Description (optional)</Label>
              <Input
                id="preset-description"
                value={presetDescription}
                onChange={(event) => setPresetDescription(event.target.value)}
                placeholder="Bronze accents with softer surfaces"
                maxLength={100}
              />
            </div>
            <div className="flex justify-end gap-2 pt-1">
              <Button variant="ghost" onClick={closeSaveDialog}>
                Cancel
              </Button>
              <Button type="submit" disabled={!presetName.trim()}>
                Save theme
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog open={deleteTarget !== null} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete {deleteTarget?.name}?</DialogTitle>
            <DialogDescription>
              {deleteTarget?.id === settings.theme_preset
                ? "This theme is active. Industrialis will be applied after deletion."
                : "This removes the saved theme from this launcher."}
            </DialogDescription>
          </DialogHeader>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setDeleteTarget(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                if (deleteTarget) deleteCustomPreset(deleteTarget.id);
                setDeleteTarget(null);
              }}
            >
              Delete theme
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
