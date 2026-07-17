import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronRight, FileText, Folder, Save, Undo2 } from "lucide-react";
import { Button } from "./ui/button";
import { ScrollArea } from "./ui/scroll-area";
import { Badge } from "./ui/badge";
import { Tabs, TabsList, TabsTrigger } from "./ui/tabs";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "./ui/resizable";
import { useLauncherSettings } from "../context/launcher-settings-context";
import { cn } from "../lib/utils";
import { ConfigCodeEditor } from "./ConfigCodeEditor";
import { ForgeConfigEasyEditor } from "./ForgeConfigEasyEditor";
import {
  isForgeConfigFile,
  parseForgeConfig,
  serializeForgeConfig,
} from "../lib/forge-cfg";
import {
  readMinecraftEditorMode,
  writeMinecraftEditorMode,
  type MinecraftEditorMode,
} from "../lib/minecraft-editor-storage";

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
  const [editorMode, setEditorMode] = useState<MinecraftEditorMode>(() => readMinecraftEditorMode());
  const saveRef = useRef<() => Promise<void>>(async () => {});

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

  const forgeDoc = useMemo(() => {
    if (!selectedPath || !isForgeConfigFile(selectedPath)) return null;
    return parseForgeConfig(content);
  }, [content, selectedPath]);

  const canUseEasyMode = forgeDoc !== null;

  const save = useCallback(async () => {
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
  }, [selectedPath, content, instanceId, loadDir, cwd]);

  useEffect(() => {
    saveRef.current = save;
  }, [save]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (selectedPath && dirty && !loading) {
          void saveRef.current();
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selectedPath, dirty, loading]);

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
      if (
        editorMode === "easy" &&
        isForgeConfigFile(entry.rel_path) &&
        !parseForgeConfig(text)
      ) {
        setEditorMode("advanced");
        writeMinecraftEditorMode("advanced");
      }
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

  const handleModeChange = (mode: MinecraftEditorMode) => {
    if (mode === "easy" && !canUseEasyMode) return;
    setEditorMode(mode);
    writeMinecraftEditorMode(mode);
  };

  const crumbs = cwd ? cwd.split("/") : [];

  return (
    <div className="flex flex-col gap-1.5 h-full min-h-[260px]">
      <p className="text-[11px] text-muted-foreground leading-snug">
        Edit <span className="font-mono">.minecraft/</span> files. Saves persist across pack updates.
        <span className="ml-1 text-muted-foreground/80">Ctrl+S to save.</span>
      </p>

      <ResizablePanelGroup
        orientation="horizontal"
        className="flex-1 min-h-0 overflow-hidden rounded-md border border-border bg-card"
      >
        <ResizablePanel defaultSize="38%" minSize="15%" maxSize="60%">
          <div className="flex h-full flex-col bg-card">
            <div className="px-2 py-1.5 border-b border-border text-xs flex items-center gap-1 flex-wrap text-left">
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
              <div className="p-1 text-left">
                {cwd && (
                  <button
                    type="button"
                    className="flex h-auto w-full items-center justify-start gap-1.5 rounded-sm px-2 py-1 text-left text-xs font-normal hover:bg-primary/14"
                    onClick={() => {
                      const parent = cwd.includes("/") ? cwd.replace(/\/[^/]+$/, "") : "";
                      void loadDir(parent);
                    }}
                  >
                    <Folder className="size-3.5 shrink-0 text-muted-foreground" />
                    <span className="truncate text-left">..</span>
                  </button>
                )}
                {entries.map((entry) => (
                  <button
                    key={entry.rel_path}
                    type="button"
                    className={cn(
                      "flex h-auto w-full items-center justify-start gap-1.5 rounded-sm px-2 py-1 text-left text-xs font-normal hover:bg-primary/14",
                      selectedPath === entry.rel_path && "bg-primary/15 text-foreground",
                    )}
                    onClick={() => void openFile(entry)}
                  >
                    {entry.is_dir ? (
                      <Folder className="size-3.5 shrink-0 text-muted-foreground" />
                    ) : (
                      <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                    )}
                    <span className="min-w-0 flex-1 truncate text-left">{entry.name}</span>
                    {entry.has_persistent_override && (
                      <Badge variant="secondary" className="h-4 shrink-0 px-1 text-[10px]">
                        saved
                      </Badge>
                    )}
                  </button>
                ))}
              </div>
            </ScrollArea>
          </div>
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize="62%" minSize="30%">
          <div className="flex h-full min-w-0 flex-col bg-card">
            <div className="flex flex-wrap items-center gap-2 border-b border-border px-2 py-1.5 shrink-0">
              <span className="min-w-0 flex-1 truncate text-left text-xs font-mono text-muted-foreground">
                {selectedPath ?? "Select a file"}
              </span>
              <Tabs value={editorMode} onValueChange={(v) => handleModeChange(v as MinecraftEditorMode)}>
                <TabsList>
                  <TabsTrigger value="easy" disabled={!canUseEasyMode || !selectedPath}>
                    Easy
                  </TabsTrigger>
                  <TabsTrigger value="advanced" disabled={!selectedPath}>
                    Advanced
                  </TabsTrigger>
                </TabsList>
              </Tabs>
              <Button
                size="sm"
                variant="ghost"
                disabled={!selectedPath || loading}
                onClick={() => void revertOverride()}
              >
                <Undo2 className="size-3.5" />
                Revert override
              </Button>
              <Button size="sm" disabled={!selectedPath || !dirty || loading} onClick={() => void save()}>
                <Save className="size-3.5" />
                {saved ? "Saved" : "Save"}
              </Button>
            </div>
            {error && <p className="text-xs text-destructive px-2 py-1">{error}</p>}
            {editorMode === "easy" && forgeDoc && selectedPath ? (
              <ForgeConfigEasyEditor
                document={forgeDoc}
                serialize={serializeForgeConfig}
                onChange={(next) => {
                  setContent(next);
                  setDirty(true);
                }}
              />
            ) : (
              <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                {editorMode === "easy" && selectedPath && !canUseEasyMode && (
                  <p className="px-2 py-1 text-xs text-muted-foreground">
                    Easy mode is only available for Forge .cfg files. Use Advanced mode for this file.
                  </p>
                )}
                <ConfigCodeEditor
                  value={content}
                  disabled={!selectedPath || loading}
                  isDark={isDark}
                  className="flex-1"
                  onChange={(next) => {
                    setContent(next);
                    setDirty(true);
                  }}
                />
              </div>
            )}
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
