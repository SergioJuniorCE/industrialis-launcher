import { useState } from "react";
import { ExternalLink } from "lucide-react";
import { Button } from "./ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./ui/dialog";
import { Select } from "./ui/select";
import { compareVersionsByReleaseDate } from "../lib/pack-version-status";

interface GtnhVersion {
  title: string;
  releaseDate: string;
  maxJavaVersion: number;
}

const PRESERVED_ITEMS = [
  "World saves (saves/)",
  "JourneyMap data (journeymap/)",
  "Ore vein & node data (visualprospecting/, TCNodeTracker/)",
  "NEI / player options (options.txt, optionsnf.txt)",
  "Schematics, screenshots, shader packs",
  "Server list (servers.dat), local config (localconfig.cfg)",
  "Selected config overrides (lwjgl3ify, tectech, etc.)",
  "Launcher overlays (persistent-minecraft/)",
  "Instance settings, play time, and custom icon",
] as const;

const WIKI_URL = "https://wiki.gtnewhorizons.com/wiki/Installing_and_Migrating";

export function ReinstallInstanceDialog({
  instanceName,
  currentPackVersion,
  defaultJavaType,
  versions,
  onClose,
  onReinstall,
}: {
  instanceName: string;
  currentPackVersion: string;
  defaultJavaType: string;
  versions: Record<string, GtnhVersion> | null;
  onClose: () => void;
  onReinstall: (packVersion: string, javaType: string) => void;
}) {
  const sorted = versions
    ? Object.entries(versions).sort(([a], [b]) => compareVersionsByReleaseDate(a, b, versions))
    : [];
  const [packVersion, setPackVersion] = useState(
    sorted.some(([v]) => v === currentPackVersion)
      ? currentPackVersion
      : sorted.at(-1)?.[0] ?? currentPackVersion,
  );
  const [javaType, setJavaType] = useState(defaultJavaType || "java17+");

  const confirm = () => {
    if (!packVersion) return;
    onReinstall(packVersion, javaType);
  };

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="max-w-lg">
        <DialogTitle>Clean reinstall</DialogTitle>
        <DialogDescription>
          Downloads a fresh pack for {instanceName} while keeping your player data.
        </DialogDescription>
        <div className="space-y-2 text-sm text-muted-foreground">
          <p>
            Use this to fix a broken or mixed install (for example after a partial update). Preserved paths follow the{" "}
            <a
              href={WIKI_URL}
              className="inline-flex items-center gap-1 text-primary hover:underline"
              onClick={(e) => {
                e.preventDefault();
                void import("@tauri-apps/plugin-opener").then(({ openUrl }) => openUrl(WIKI_URL));
              }}
            >
              GTNH migration guide
              <ExternalLink className="size-3" />
            </a>
            .
          </p>
        </div>

        <div className="space-y-3">
          <div className="rounded-lg border border-border/70 bg-muted/20 px-3 py-2">
            <div className="text-xs font-semibold text-foreground mb-1.5">Kept on reinstall</div>
            <ul className="text-xs text-muted-foreground space-y-0.5 list-disc pl-4">
              {PRESERVED_ITEMS.map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
          </div>

          <div className="grid gap-2 sm:grid-cols-2">
            <label className="space-y-1">
              <span className="text-xs font-medium">Pack version</span>
              <Select
                value={packVersion}
                onChange={(e) => setPackVersion(e.target.value)}
                disabled={sorted.length === 0}
              >
                {sorted.length === 0 ? (
                  <option value={currentPackVersion}>{currentPackVersion}</option>
                ) : (
                  sorted.map(([version, meta]) => (
                    <option key={version} value={version}>
                      {meta.title || version}
                    </option>
                  ))
                )}
              </Select>
            </label>
            <label className="space-y-1">
              <span className="text-xs font-medium">Java pack</span>
              <Select value={javaType} onChange={(e) => setJavaType(e.target.value)}>
                <option value="java17+">Java 17+</option>
                <option value="java8">Java 8</option>
              </Select>
            </label>
          </div>

          <p className="text-[11px] text-muted-foreground leading-snug">
            Mods, scripts, and most config files are replaced with a clean install. Custom mods in persistent-minecraft/mods are restored afterward.
          </p>
        </div>

        <div className="flex justify-end gap-2 mt-2">
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={!packVersion}>
            Reinstall in background
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}