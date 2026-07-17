import { useState } from "react";
import { Check, Palette, Save, Trash2 } from "lucide-react";
import { useLauncherSettings } from "../context/launcher-settings-context";
import { BUILTIN_THEME_PRESETS } from "../lib/theme-presets";
import { Button } from "./ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./ui/card";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./ui/dialog";
import { Input } from "./ui/input";
import { Label } from "./ui/label";

function PresetSwatch({ primary, background }: { primary: string; background: string }) {
  return (
    <span
      className="inline-flex h-8 w-8 shrink-0 overflow-hidden rounded-md border border-border"
      aria-hidden
    >
      <span className="flex-1" style={{ backgroundColor: background }} />
      <span className="w-2.5" style={{ backgroundColor: primary }} />
    </span>
  );
}

export function ThemePresetPicker() {
  const {
    settings,
    customPresets,
    setThemePreset,
    saveCustomPreset,
    deleteCustomPreset,
  } = useLauncherSettings();
  const [saveOpen, setSaveOpen] = useState(false);
  const [presetName, setPresetName] = useState("");
  const [presetDescription, setPresetDescription] = useState("");

  const handleSave = () => {
    const name = presetName.trim();
    if (!name) return;
    saveCustomPreset(name, presetDescription.trim() || undefined);
    setPresetName("");
    setPresetDescription("");
    setSaveOpen(false);
  };

  const activeId = settings.theme_preset;
  const mode = settings.theme_mode;

  const renderPresetButton = (
    id: string,
    name: string,
    description: string,
    primary: string,
    background: string,
    builtin: boolean
  ) => {
    const selected = activeId === id;
    return (
      <Button
        key={id}
        type="button"
        variant={selected ? "secondary" : "outline"}
        onClick={() => setThemePreset(id)}
        className={`h-auto w-full items-start gap-3 p-3 text-left font-normal ${
          selected ? "border-primary bg-primary/10" : ""
        }`}
      >
        <PresetSwatch primary={primary} background={background} />
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2 text-sm font-medium">
            {name}
            {selected && <Check className="size-3.5 text-primary" />}
          </span>
          <span className="mt-0.5 block text-xs text-muted-foreground">{description}</span>
          {!builtin && (
            <span className="mt-1 block text-[10px] uppercase tracking-wide text-muted-foreground">
              Custom
            </span>
          )}
        </span>
      </Button>
    );
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Palette className="size-4" />
          Theme Presets
        </CardTitle>
        <CardDescription>
          Choose a built-in palette or save your own. Overrides in the editor layer on top of the
          active preset.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-2">
          {BUILTIN_THEME_PRESETS.map((preset) => {
            const tokens = mode === "dark" ? preset.dark : preset.light;
            return renderPresetButton(
              preset.id,
              preset.name,
              preset.description,
              tokens.primary,
              tokens.background,
              true
            );
          })}
        </div>

        {customPresets.length > 0 && (
          <div className="space-y-2">
            <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Your presets
            </p>
            <div className="grid gap-2">
              {customPresets.map((preset) => {
                const tokens = mode === "dark" ? preset.dark : preset.light;
                return (
                  <div key={preset.id} className="flex items-stretch gap-2">
                    <div className="min-w-0 flex-1">
                      {renderPresetButton(
                        preset.id,
                        preset.name,
                        preset.description,
                        tokens.primary,
                        tokens.background,
                        false
                      )}
                    </div>
                    <Button
                      variant="outline"
                      size="icon"
                      className="shrink-0"
                      onClick={() => deleteCustomPreset(preset.id)}
                      aria-label={`Delete ${preset.name}`}
                      title={`Delete ${preset.name}`}
                    >
                      <Trash2 className="size-4" />
                    </Button>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        <Button variant="outline" className="w-full" onClick={() => setSaveOpen(true)}>
          <Save className="size-4" />
          Save current look as preset
        </Button>
        <Dialog open={saveOpen} onOpenChange={setSaveOpen}>
          <DialogContent>
          <DialogTitle>Save theme preset</DialogTitle>
          <DialogDescription>
            Saves the active preset plus overrides for the current light/dark mode. The other mode
            keeps the base preset palette.
          </DialogDescription>
          <div className="mt-4 space-y-3">
            <div className="space-y-2">
              <Label htmlFor="preset-name">Name</Label>
              <Input
                id="preset-name"
                value={presetName}
                onChange={(e) => setPresetName(e.target.value)}
                placeholder="My GTNH theme"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="preset-description">Description (optional)</Label>
              <Input
                id="preset-description"
                value={presetDescription}
                onChange={(e) => setPresetDescription(e.target.value)}
                placeholder="Warm bronze accents"
              />
            </div>
            <Button onClick={handleSave} disabled={!presetName.trim()} className="w-full">
              Save preset
            </Button>
          </div>
          </DialogContent>
        </Dialog>
      </CardContent>
    </Card>
  );
}
