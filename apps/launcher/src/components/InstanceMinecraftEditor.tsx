import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronRight, FileText, Folder, Save, Undo2 } from "lucide-react";
import { Button } from "./ui/button";
import { ScrollArea } from "./ui/scroll-area";
import { Textarea } from "./ui/textarea";
import { Badge } from "./ui/badge";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "./ui/resizable";
import { useLauncherSettings } from "../context/LauncherSettingsContext";
import { cn } from "../lib/utils";

interface MinecraftDirEntry {
  name: string;
  rel_path: string;
  is_dir: boolean;
  has_persistent_override: boolean;
  editable: boolean;
}

export function InstanceMinecraftEditor({ instanceId }: { instanceId: string }) {
  const { settings } = useLauncherSettings();
  const isDark = settings.theme_mode === "dark";
  const [cwd, setCwd] = useState("");
  const [entries, setEntries] = useState<MinecraftDirEntry[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [dirty, setDirty] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const loadDir = useCallback(async (subpath: string) => {
    setError(null);
    try {
      const list = await invoke<MinecraftDirEntry[]>("list_minecraft_entries", {
        id: instanceId,
        subpath: subpath || null,
      });
      setEntries(list);
      setCwd(subpath);
    } catch (e) {
      setError(String(e));
    }
  }, [instanceId]);

  useEffect(() => {
    void loadDir("");
  }, [loadDir]);

  const openFile = async (entry: MinecraftDirEntry) => {
    if (entry.is_dir) {
      await loadDir(entry.rel_path);
      setSelectedPath(null);
      setContent("");
      setDirty(false);
      return;
    }
    if (!entry.editable) {
      setError("This file type cannot be edited in the launcher.");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const text = await invoke<string>("read_minecraft_file", {
        id: instanceId,
        relPath: entry.rel_path,
      });
      setSelectedPath(entry.rel_path);
      setContent(text);
      setDirty(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const save = async () => {
    if (!selectedPath) return;
    setLoading(true);
    setError(null);
    try {
      await invoke("write_minecraft_file", {
        id: instanceId,
        relPath: selectedPath,
        content,
        persist: true,
      });
      setDirty(false);
      setSaved(true);
      window.setTimeout(() => setSaved(false), 1500);
      await loadDir(cwd);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const revertOverride = async () => {
    if (!selectedPath) return;
    setLoading(true);
    setError(null);
    try {
      await invoke("delete_persistent_file", {
        id: instanceId,
        relPath: selectedPath,
      });
      const text = await invoke<string>("read_minecraft_file", {
        id: instanceId,
        relPath: selectedPath,
      });
      setContent(text);
      setDirty(false);
      await loadDir(cwd);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const crumbs = cwd ? cwd.split("/") : [];

  return (
    <div className="flex flex-col gap-1.5 h-full min-h-[260px]">
      <p className="text-[11px] text-muted-foreground leading-snug">
        Edit <span className="font-mono">.minecraft/</span> files. Saves persist across pack updates.
      </p>

      <ResizablePanelGroup
        orientation="horizontal"
        className="flex-1 min-h-0 overflow-hidden rounded-md border border-border bg-card"
      >
        <ResizablePanel defaultSize="38%" minSize="15%" maxSize="60%">
          <div className="flex h-full flex-col bg-card">
            <div className="px-2 py-1.5 border-b border-border text-xs flex items-center gap-1 flex-wrap">
            <Button
              type="button"
              variant="link"
              className="h-auto p-0 text-xs"
              onClick={() => void loadDir("")}
            >
              .minecraft
            </Button>
            {crumbs.map((part, i) => {
              const path = crumbs.slice(0, i + 1).join("/");
              return (
                <span key={path} className="flex items-center gap-1">
                  <ChevronRight className="size-3 text-muted-foreground" />
                  <Button
                    type="button"
                    variant="link"
                    className="h-auto p-0 text-xs"
                    onClick={() => void loadDir(path)}
                  >
                    {part}
                  </Button>
                </span>
              );
            })}
            </div>
            <ScrollArea className="flex-1">
              <div className="p-1">
                {cwd && (
                  <Button
                    type="button"
                    variant="ghost"
                    className="h-auto w-full justify-start gap-1.5 px-2 py-1 text-xs font-normal"
                    onClick={() => {
                      const parent = cwd.includes("/") ? cwd.replace(/\/[^/]+$/, "") : "";
                      void loadDir(parent);
                    }}
                  >
                    <Folder className="size-3.5 shrink-0" />
                    ..
                  </Button>
                )}
                {entries.map((entry) => (
                  <Button
                    key={entry.rel_path}
                    type="button"
                    variant="ghost"
                    className={cn(
                      "h-auto w-full justify-start gap-1.5 px-2 py-1 text-xs font-normal",
                      selectedPath === entry.rel_path && "bg-primary/15 text-foreground",
                    )}
                    onClick={() => void openFile(entry)}
                  >
                    {entry.is_dir ? (
                      <Folder className="size-3.5 shrink-0 text-muted-foreground" />
                    ) : (
                      <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                    )}
                    <span className="truncate flex-1">{entry.name}</span>
                    {entry.has_persistent_override && (
                      <Badge variant="secondary" className="text-[10px] h-4 px-1 shrink-0">
                        saved
                      </Badge>
                    )}
                  </Button>
                ))}
              </div>
            </ScrollArea>
          </div>
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize="62%" minSize="30%">
          <div className="flex h-full min-w-0 flex-col bg-card">
            <div className="flex items-center gap-2 px-2 py-1.5 border-b border-border shrink-0">
              <span className="text-xs font-mono truncate flex-1 text-muted-foreground">
                {selectedPath ?? "Select a file"}
              </span>
              <Button size="sm" variant="ghost" disabled={!selectedPath || loading} onClick={() => void revertOverride()}>
                <Undo2 className="size-3.5" />
                Revert override
              </Button>
              <Button size="sm" disabled={!selectedPath || !dirty || loading} onClick={() => void save()}>
                <Save className="size-3.5" />
                {saved ? "Saved" : "Save"}
              </Button>
            </div>
            {error && <p className="text-xs text-destructive px-2 py-1">{error}</p>}
            <Textarea
              className={cn(
                "min-h-[160px] flex-1 resize-none rounded-none border-0 font-mono text-[11px] leading-relaxed shadow-none focus-visible:ring-0 disabled:opacity-100 disabled:cursor-default",
                isDark ? "bg-black/40" : "bg-card",
              )}
              value={content}
              disabled={!selectedPath || loading}
              onChange={(e) => {
                setContent(e.target.value);
                setDirty(true);
              }}
              spellCheck={false}
            />
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}