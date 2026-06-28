import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ExternalLink, Loader2 } from "lucide-react";
import { Button } from "./ui/button";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "./ui/card";
import { Dialog, DialogContent } from "./ui/dialog";
import { Select } from "./ui/select";
import { ScrollArea } from "./ui/scroll-area";
import { Badge } from "./ui/badge";
import { Checkbox } from "./ui/checkbox";
import { Progress } from "./ui/progress";
import { formatDownloadProgress, formatDownloadSpeed, type DlProgressEvent } from "../lib/background-processes";
import { compareVersionsByReleaseDate } from "../lib/pack-version-status";

interface GtnhVersion {
  title: string;
  releaseDate: string;
  maxJavaVersion: number;
}

interface ModPreviewEntry {
  identity: string;
  filename: string;
  size_bytes: number;
  in_persistent_overlay: boolean;
}

interface UpdateModPreview {
  current_pack_version: string;
  target_pack_version: string;
  custom_mods: ModPreviewEntry[];
  new_pack_mods_count: number;
  updated_pack_mods_count: number;
  removed_from_pack_count: number;
}

const WIKI_URL = "https://wiki.gtnewhorizons.com/wiki/Installing_and_Migrating";

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

type Step = "version" | "mods" | "confirm";

export function UpdatePackDialog({
  instanceId,
  instanceName,
  currentPackVersion,
  defaultJavaType,
  versions,
  onClose,
  onUpdate,
}: {
  instanceId: string;
  instanceName: string;
  currentPackVersion: string;
  defaultJavaType: string;
  versions: Record<string, GtnhVersion> | null;
  onClose: () => void;
  onUpdate: (
    packVersion: string,
    javaType: string,
    keepModIdentities: string[],
  ) => void;
}) {
  const [step, setStep] = useState<Step>("version");
  const [targetVersion, setTargetVersion] = useState<string | null>(null);
  const [javaType, setJavaType] = useState(defaultJavaType || "java17+");
  const [preview, setPreview] = useState<UpdateModPreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewLogs, setPreviewLogs] = useState<string[]>([]);
  const [previewStage, setPreviewStage] = useState<string | null>(null);
  const [previewPct, setPreviewPct] = useState(0);
  const [previewSpeedMbps, setPreviewSpeedMbps] = useState<number | undefined>();
  const [previewDownloadedMb, setPreviewDownloadedMb] = useState<number | undefined>();
  const [previewTotalMb, setPreviewTotalMb] = useState<number | undefined>();
  const previewLogRef = useRef<HTMLDivElement>(null);
  const [keepMods, setKeepMods] = useState<Record<string, boolean>>({});
  const [handoff, setHandoff] = useState(false);

  const dialogLocked = previewLoading || handoff;

  const sorted = versions
    ? Object.entries(versions).sort(([a], [b]) => compareVersionsByReleaseDate(a, b, versions))
    : [];

  const requestClose = () => {
    if (dialogLocked) return;
    onClose();
  };

  useEffect(() => {
    if (!preview) return;
    const next: Record<string, boolean> = {};
    for (const mod of preview.custom_mods) {
      next[mod.identity] = true;
    }
    setKeepMods(next);
  }, [preview]);

  useEffect(() => {
    if (!previewLoading) return;
    const unlisten = listen<DlProgressEvent>("dl-progress", (e) => {
      const p = e.payload;
      if (p.operation !== "preview" || p.id !== instanceId) return;
      if (p.log_line) {
        setPreviewLogs((prev) => [...prev, p.log_line!]);
      }
      setPreviewStage(p.stage);
      if (p.stage === "downloading") {
        setPreviewPct(p.pct * 0.6);
        setPreviewSpeedMbps(p.speed_mbps);
        setPreviewDownloadedMb(p.downloaded_mb);
        setPreviewTotalMb(p.total_mb);
      } else if (p.stage === "extracting") {
        setPreviewPct(0.6 + p.pct * 0.25);
        setPreviewSpeedMbps(undefined);
        setPreviewDownloadedMb(undefined);
        setPreviewTotalMb(undefined);
      } else if (p.stage === "preview") {
        setPreviewPct(Math.max(0.85, p.pct));
        setPreviewSpeedMbps(undefined);
        setPreviewDownloadedMb(undefined);
        setPreviewTotalMb(undefined);
      }
    });
    return () => { unlisten.then((f) => f()); };
  }, [previewLoading, instanceId]);

  useEffect(() => {
    const el = previewLogRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [previewLogs]);

  const previewStageLabel = (() => {
    switch (previewStage) {
      case "downloading": {
        const speed = formatDownloadSpeed(previewSpeedMbps);
        if (previewDownloadedMb != null && previewTotalMb != null) {
          return `Downloading ${previewDownloadedMb.toFixed(1)} / ${previewTotalMb.toFixed(1)} MB${speed ? ` · ${speed}` : "…"}`;
        }
        if (previewDownloadedMb != null) {
          return `Downloading ${previewDownloadedMb.toFixed(1)} MB${speed ? ` · ${speed}` : "…"}`;
        }
        return speed ? `Downloading target pack · ${speed}` : "Downloading target pack…";
      }
      case "extracting":
        return "Extracting pack…";
      case "preview":
        return "Analyzing mods…";
      default:
        return "Starting analysis…";
    }
  })();

  const previewProgressLabel = previewStage === "downloading"
    ? formatDownloadProgress({
        stage: "downloading",
        pct: previewPct / 0.6,
        speedMbps: previewSpeedMbps,
        downloadedMb: previewDownloadedMb,
        totalMb: previewTotalMb,
      })
    : `${(previewPct * 100).toFixed(0)}%`;

  const loadPreview = async (packVersion: string) => {
    setPreviewLoading(true);
    setPreviewError(null);
    setPreviewLogs([`Starting mod analysis for ${packVersion}…`]);
    setPreviewStage(null);
    setPreviewPct(0);
    setPreviewSpeedMbps(undefined);
    setPreviewDownloadedMb(undefined);
    setPreviewTotalMb(undefined);
    try {
      const result = await invoke<UpdateModPreview>("preview_update_mods", {
        id: instanceId,
        packVersion,
        javaType,
      });
      setPreview(result);
      if (result.custom_mods.length > 0) {
        setStep("mods");
      } else {
        setStep("confirm");
      }
    } catch (e) {
      setPreviewError(String(e));
    } finally {
      setPreviewLoading(false);
    }
  };

  const goNextFromVersion = () => {
    if (!targetVersion) return;
    void loadPreview(targetVersion);
  };

  const confirmUpdate = () => {
    if (!targetVersion) return;
    setHandoff(true);
    onUpdate(targetVersion, javaType, keepIdentities);
  };

  const keepIdentities = Object.entries(keepMods)
    .filter(([, v]) => v)
    .map(([k]) => k);

  return (
    <Dialog open onOpenChange={(open) => { if (!open) requestClose(); }}>
      <DialogContent className="flex max-h-[85vh] max-w-lg flex-col overflow-hidden p-0">
      <Card className="flex max-h-[85vh] flex-col overflow-hidden border-0 shadow-none">
        <CardHeader className="shrink-0 pb-2">
          <div className="flex items-center justify-between">
            <CardTitle>Update Pack</CardTitle>
            <Button variant="ghost" size="sm" onClick={requestClose} disabled={dialogLocked}>
              ✕
            </Button>
          </div>
          <CardDescription>
            {instanceName} · {currentPackVersion}
            {targetVersion ? ` → ${targetVersion}` : ""}
          </CardDescription>
        </CardHeader>

        <CardContent className="flex flex-col flex-1 min-h-0 gap-4 overflow-hidden pb-6">
          {dialogLocked && (
            <div className="rounded-md border border-primary/30 bg-primary/10 px-3 py-2 text-xs text-muted-foreground">
              {handoff ? (
                <span className="inline-flex items-center gap-2">
                  <Loader2 className="size-3.5 animate-spin" />
                  Starting background update…
                </span>
              ) : (
                "Analyzing the target pack. Keep this dialog open until analysis finishes."
              )}
            </div>
          )}

          {step === "version" && (
            <>
              <Select value={javaType} onChange={(e) => setJavaType(e.target.value)}>
                <option value="java17+">Java 17+</option>
                <option value="java8">Java 8</option>
              </Select>
              <ScrollArea className="flex-1 min-h-0 rounded-md border border-border">
                <div className="space-y-2 p-2">
                  {!versions && <p className="text-sm text-muted-foreground p-2">Loading versions…</p>}
                  {sorted.map(([key, v]) => (
                    <Button
                      key={key}
                      type="button"
                      variant={targetVersion === key ? "secondary" : "outline"}
                      className="h-auto w-full justify-between p-3 text-left font-normal"
                      onClick={() => setTargetVersion(key)}
                      disabled={previewLoading}
                    >
                      <div>
                        <div className="font-medium">{key}</div>
                        <div className="text-xs text-muted-foreground">
                          {v.releaseDate} · Max Java {v.maxJavaVersion}
                        </div>
                      </div>
                      {key === currentPackVersion && <Badge variant="secondary">Current</Badge>}
                    </Button>
                  ))}
                </div>
              </ScrollArea>
              {previewLoading && (
                <div className="space-y-2 shrink-0">
                  <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
                    <span className="truncate">{previewStageLabel}</span>
                    <span className="shrink-0">{previewProgressLabel}</span>
                  </div>
                  <Progress value={previewPct * 100} />
                  {previewLogs.length > 0 && (
                    <div
                      ref={previewLogRef}
                      className="max-h-32 overflow-y-auto rounded-md border border-border bg-black/50 p-2 font-mono text-xs space-y-0.5"
                    >
                      {previewLogs.map((line, i) => (
                        <div key={i} className="text-muted-foreground">{line}</div>
                      ))}
                    </div>
                  )}
                </div>
              )}
              {previewError && <p className="text-sm text-destructive">{previewError}</p>}
              <Button
                className="w-full"
                disabled={!targetVersion || previewLoading}
                onClick={goNextFromVersion}
              >
                {previewLoading ? "Analyzing mods…" : "Next"}
              </Button>
            </>
          )}

          {step === "mods" && preview && (
            <>
              <p className="text-sm text-muted-foreground">
                These are custom mods you added in the Mods tab. Uncheck any you want removed before updating.
                The fresh pack install replaces all pack mods ({preview.updated_pack_mods_count} updated,{" "}
                {preview.new_pack_mods_count} new from pack).
              </p>
              <ScrollArea className="flex-1 min-h-0 rounded-md border border-border">
                <div className="p-2 space-y-2">
                  {preview.custom_mods.map((mod) => (
                    <Checkbox
                      key={mod.identity}
                      checked={keepMods[mod.identity] ?? true}
                      onChange={(e) =>
                        setKeepMods((prev) => ({ ...prev, [mod.identity]: e.target.checked }))
                      }
                      label={
                        <span>
                          <span className="font-medium">{mod.filename}</span>
                          <span className="block text-xs text-muted-foreground font-mono">
                            {mod.identity} · {formatBytes(mod.size_bytes)}
                          </span>
                        </span>
                      }
                    />
                  ))}
                </div>
              </ScrollArea>
              <div className="flex gap-2">
                <Button variant="outline" className="flex-1" onClick={() => setStep("version")}>
                  Back
                </Button>
                <Button className="flex-1" onClick={() => setStep("confirm")}>
                  Next
                </Button>
              </div>
            </>
          )}

          {step === "confirm" && (
            <>
              <div className="text-sm space-y-2">
                <p>
                  Update <strong>{instanceName}</strong> from{" "}
                  <span className="font-mono">{currentPackVersion}</span> to{" "}
                  <span className="font-mono">{targetVersion}</span>?
                </p>
                <p className="text-muted-foreground text-xs">
                  The update runs in the background. You can keep using the launcher while it progresses in
                  Processes.
                </p>
                <p className="text-muted-foreground text-xs">
                  Installs a fresh copy of the target pack, then restores your saves, JourneyMap, options, and other
                  player data per the{" "}
                  <a
                    href={WIKI_URL}
                    className="inline-flex items-center gap-0.5 text-primary hover:underline"
                    onClick={(e) => {
                      e.preventDefault();
                      void import("@tauri-apps/plugin-opener").then(({ openUrl }) => openUrl(WIKI_URL));
                    }}
                  >
                    GTNH migration guide
                    <ExternalLink className="size-3" />
                  </a>
                  . Files you saved in the Files tab are re-applied afterward.
                </p>
                <p className="text-muted-foreground text-xs">
                  GTNH recommends updating one major version at a time. Back up your instance folder if unsure.
                </p>
                {preview && preview.custom_mods.length > 0 && (
                  <p className="text-xs text-muted-foreground">
                    Keeping {keepIdentities.length} custom mod{keepIdentities.length === 1 ? "" : "s"}.
                  </p>
                )}
              </div>
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  className="flex-1"
                  onClick={() => setStep(preview?.custom_mods.length ? "mods" : "version")}
                  disabled={handoff}
                >
                  Back
                </Button>
                <Button className="flex-1" disabled={!targetVersion || handoff} onClick={confirmUpdate}>
                  {handoff ? (
                    <>
                      <Loader2 className="size-3.5 animate-spin" />
                      Starting…
                    </>
                  ) : (
                    "Update in background"
                  )}
                </Button>
              </div>
            </>
          )}
        </CardContent>
      </Card>
      </DialogContent>
    </Dialog>
  );
}