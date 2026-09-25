import type { ForgeConfigDocument } from "../lib/forge-cfg";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke, openMinecraftEditorWindow } from "../lib/desktop";
import { ChevronRight, ExternalLink, FileText, Folder, Save, Undo2 } from "lucide-react";
import { Button } from "./ui/button";
import { ScrollArea } from "./ui/scroll-area";
import { Badge } from "./ui/badge";
import { Tabs, TabsList, TabsTrigger } from "./ui/tabs";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "./ui/resizable";
import { useLauncherSettings } from "../context/launcher-settings-context";
import { cn } from "../lib/utils";
import { ConfigCodeEditor } from "./ConfigCodeEditor";
import { ForgeConfigEasyEditor } from "./ForgeConfigEasyEditor";
import { isForgeConfigFile, parseForgeConfig, serializeForgeConfig } from "../lib/forge-cfg";
import { readMinecraftEditorMode, writeMinecraftEditorMode, type MinecraftEditorMode } from "../lib/minecraft-editor-storage";

interface MinecraftDirEntry {
  name: string;
  rel_path: string;
  is_dir: boolean;
  has_persistent_override: boolean;
  editable: boolean;
  too_large_to_edit: boolean;
}

export function InstanceMinecraftEditor({ instanceId, initialPath, standalone = false }: { instanceId: string; initialPath?: string; standalone?: boolean }) {
  const { settings } = useLauncherSettings();
  const isDark = settings.theme_mode === "dark";
  const [cwd, setCwd] = useState("");
  const [entries, setEntries] = useState<MinecraftDirEntry[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [dirty, setDirty] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [largeFile, setLargeFile] = useState(false);
  const [saved, setSaved] = useState(false);
  const [externalOpened, setExternalOpened] = useState(false);
  const [lineWrapping, setLineWrapping] = useState(true);
  const [editorMode, setEditorMode] = useState<MinecraftEditorMode>(() => readMinecraftEditorMode());
  const saveRef = useRef<() => Promise<boolean>>(async () => false);
  const directoryRequestRef = useRef(0);
  const fileRequestRef = useRef(0);
  const initialPathOpenedRef = useRef(false);

  const loadDir = useCallback(
    async (subpath: string) => {
      const request = ++directoryRequestRef.current;
      setError(null);
      try {
        const list = await invoke<MinecraftDirEntry[]>("list_minecraft_entries", {
          id: instanceId,
          subpath: subpath || null,
        });
        if (request !== directoryRequestRef.current) return;
        setEntries(list);
        setCwd(subpath);
      } catch (e) {
        if (request === directoryRequestRef.current) setError(String(e));
      }
    },
    [instanceId],
  );

  useEffect(() => {
    void loadDir(initialPath?.includes("/") ? initialPath.slice(0, initialPath.lastIndexOf("/")) : "");
  }, [initialPath, loadDir]);

  const forgeDoc = useMemo(() => {
    if (!selectedPath || !isForgeConfigFile(selectedPath)) return null;
    return parseForgeConfig(content);
  }, [content, selectedPath]);

  const canUseEasyMode = forgeDoc !== null && !largeFile;

  const save = useCallback(async (): Promise<boolean> => {
    if (!selectedPath || largeFile) return false;
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
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setLoading(false);
    }
  }, [selectedPath, content, instanceId, loadDir, cwd, largeFile]);

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

  const openFile = useCallback(
    async (entry: MinecraftDirEntry) => {
      if (entry.is_dir) {
        fileRequestRef.current += 1;
        setLoading(false);
        setSelectedPath(null);
        setContent("");
        setDirty(false);
        setExternalOpened(false);
        setLargeFile(false);
        await loadDir(entry.rel_path);
        return;
      }
      if (!entry.editable) {
        fileRequestRef.current += 1;
        setLoading(false);
        setLargeFile(false);
        setSelectedPath(null);
        setContent("");
        setDirty(false);
        setExternalOpened(false);
        setError("This file type cannot be edited in the launcher.");
        return;
      }
      if (entry.too_large_to_edit) {
        fileRequestRef.current += 1;
        setLoading(false);
        setError(null);
        setSelectedPath(entry.rel_path);
        setContent("");
        setDirty(false);
        setExternalOpened(false);
        setLargeFile(true);
        return;
      }
      const request = ++fileRequestRef.current;
      setLoading(true);
      setError(null);
      setLargeFile(false);
      setSelectedPath(null);
      setContent("");
      setDirty(false);
      setExternalOpened(false);
      try {
        const text = await invoke<string>("read_minecraft_file", {
          id: instanceId,
          relPath: entry.rel_path,
        });
        if (request !== fileRequestRef.current) return;
        setSelectedPath(entry.rel_path);
        setContent(text);
        setDirty(false);
        setExternalOpened(false);
        setLargeFile(false);
        if (editorMode === "easy" && isForgeConfigFile(entry.rel_path) && !parseForgeConfig(text)) {
          setEditorMode("advanced");
          writeMinecraftEditorMode("advanced");
        }
      } catch (e) {
        if (request === fileRequestRef.current) {
          if (String(e).includes("file too large to edit in launcher")) {
            setSelectedPath(entry.rel_path);
            setContent("");
            setDirty(false);
            setExternalOpened(false);
            setLargeFile(true);
          } else {
            setError(String(e));
          }
        }
      } finally {
        if (request === fileRequestRef.current) setLoading(false);
      }
    },
    [editorMode, instanceId, loadDir],
  );

  useEffect(() => {
    if (!initialPath || initialPathOpenedRef.current) return;
    const entry = entries.find((item) => item.rel_path === initialPath && !item.is_dir);
    if (!entry) return;
    initialPathOpenedRef.current = true;
    void openFile(entry);
  }, [entries, initialPath, openFile]);

  const revertOverride = async () => {
    if (!selectedPath || largeFile) return;
    fileRequestRef.current += 1;
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
      setExternalOpened(false);
      setLargeFile(false);
      await loadDir(cwd);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const openInSeparateWindow = async () => {
    if (dirty && !(await saveRef.current())) return;
    try {
      await openMinecraftEditorWindow(instanceId, selectedPath ?? undefined);
    } catch (e) {
      setError(String(e));
    }
  };

  const openInDefaultApp = async () => {
    if (!selectedPath || (dirty && !(await saveRef.current()))) return;
    setLoading(true);
    setError(null);
    try {
      await invoke("open_minecraft_file_external", { id: instanceId, relPath: selectedPath });
      setExternalOpened(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const reloadFromDisk = async () => {
    if (!selectedPath || dirty || largeFile) return;
    const request = ++fileRequestRef.current;
    setLoading(true);
    setError(null);
    try {
      const text = await invoke<string>("read_minecraft_file", { id: instanceId, relPath: selectedPath });
      if (request !== fileRequestRef.current) return;
      setContent(text);
      setDirty(false);
      await loadDir(cwd);
    } catch (e) {
      if (request === fileRequestRef.current) setError(String(e));
    } finally {
      if (request === fileRequestRef.current) setLoading(false);
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
        <span className="ml-1 text-muted-foreground/80">Ctrl+S to save. Open with the default app or pop out for more space.</span>
      </p>

      <ResizablePanelGroup orientation="horizontal" className="flex-1 min-h-0 overflow-hidden rounded-md border border-border bg-card">
        <ResizablePanel defaultSize="28%" minSize="15%" maxSize="60%">
          <div className="flex h-full flex-col bg-card">
            <div className="px-2 py-1.5 border-b border-border text-xs flex items-center gap-1 flex-wrap text-left">
              <Button type="button" variant="link" className="h-auto p-0 text-xs" disabled={loading} onClick={() => void loadDir("")}>
                .minecraft
              </Button>
              {crumbs.map((part, i) => {
                const path = crumbs.slice(0, i + 1).join("/");
                return (
                  <span key={path} className="flex items-center gap-1">
                    <ChevronRight className="size-3 text-muted-foreground" />
                    <Button type="button" variant="link" className="h-auto p-0 text-xs" disabled={loading} onClick={() => void loadDir(path)}>
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
                    disabled={loading}
                    aria-label="Go to parent directory"
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
                    disabled={loading}
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

        <MinecraftFileEditorPane
          selectedPath={selectedPath}
          editorMode={editorMode}
          handleModeChange={handleModeChange}
          canUseEasyMode={canUseEasyMode}
          largeFile={largeFile}
          loading={loading}
          revertOverride={revertOverride}
          dirty={dirty}
          save={save}
          saved={saved}
          openInSeparateWindow={openInSeparateWindow}
          openInDefaultApp={openInDefaultApp}
          reloadFromDisk={reloadFromDisk}
          externalOpened={externalOpened}
          lineWrapping={lineWrapping}
          setLineWrapping={setLineWrapping}
          standalone={standalone}
          error={error}
          forgeDoc={forgeDoc}
          setContent={setContent}
          setDirty={setDirty}
          content={content}
          isDark={isDark}
        />
      </ResizablePanelGroup>
    </div>
  );
}

function MinecraftFileEditorPane({
  selectedPath,
  editorMode,
  handleModeChange,
  canUseEasyMode,
  largeFile,
  loading,
  revertOverride,
  dirty,
  save,
  saved,
  openInSeparateWindow,
  openInDefaultApp,
  reloadFromDisk,
  externalOpened,
  lineWrapping,
  setLineWrapping,
  standalone,
  error,
  forgeDoc,
  setContent,
  setDirty,
  content,
  isDark,
}: {
  selectedPath: string | null;
  editorMode: MinecraftEditorMode;
  handleModeChange: (mode: MinecraftEditorMode) => void;
  canUseEasyMode: boolean;
  largeFile: boolean;
  loading: boolean;
  revertOverride: () => Promise<void>;
  dirty: boolean;
  save: () => Promise<boolean>;
  saved: boolean;
  openInSeparateWindow: () => Promise<void>;
  openInDefaultApp: () => Promise<void>;
  reloadFromDisk: () => Promise<void>;
  externalOpened: boolean;
  lineWrapping: boolean;
  setLineWrapping: (enabled: boolean) => void;
  standalone: boolean;
  error: string | null;
  forgeDoc: ForgeConfigDocument | null;
  setContent: React.Dispatch<React.SetStateAction<string>>;
  setDirty: React.Dispatch<React.SetStateAction<boolean>>;
  content: string;
  isDark: boolean;
}) {
  const canEditInLauncher = Boolean(selectedPath) && !largeFile;

  return (
    <ResizablePanel defaultSize="72%" minSize="30%">
      <div className="flex h-full min-w-0 flex-col bg-card">
        <div className="flex flex-wrap items-center gap-2 border-b border-border px-2 py-1.5 shrink-0">
          <span className="min-w-0 flex-1 truncate text-left text-xs font-mono text-muted-foreground">{selectedPath ?? "Select a file"}</span>
          <Tabs value={editorMode} onValueChange={(v) => handleModeChange(v as MinecraftEditorMode)}>
            <TabsList>
              <TabsTrigger value="easy" disabled={!canUseEasyMode || !canEditInLauncher || loading}>
                Easy
              </TabsTrigger>
              <TabsTrigger value="advanced" disabled={!canEditInLauncher || loading}>
                Advanced
              </TabsTrigger>
            </TabsList>
          </Tabs>
          {editorMode === "advanced" && !largeFile && (
            <Button
              size="sm"
              variant={lineWrapping ? "secondary" : "ghost"}
              aria-pressed={lineWrapping}
              onClick={() => setLineWrapping(!lineWrapping)}
              title="Wrap long lines"
            >
              Wrap lines
            </Button>
          )}
          {externalOpened && !largeFile && (
            <Button size="sm" variant="ghost" disabled={loading || dirty} onClick={() => void reloadFromDisk()}>
              Reload from disk
            </Button>
          )}
          {!standalone && (
            <Button size="sm" variant="ghost" disabled={loading} onClick={() => void openInSeparateWindow()}>
              <ExternalLink className="size-3.5" />
              Pop out
            </Button>
          )}
          <Button
            size="sm"
            variant="ghost"
            disabled={!selectedPath || loading}
            onClick={() => void openInDefaultApp()}
            title="Open with your operating system's default app for this file type"
          >
            <ExternalLink className="size-3.5" />
            Open in default app
          </Button>
          <Button size="sm" variant="ghost" disabled={!canEditInLauncher || loading} onClick={() => void revertOverride()}>
            <Undo2 className="size-3.5" />
            Revert override
          </Button>
          <Button size="sm" disabled={!canEditInLauncher || !dirty || loading} onClick={() => void save()}>
            <Save className="size-3.5" />
            {saved ? "Saved" : "Save"}
          </Button>
        </div>
        {error && (
          <p role="alert" className="text-xs text-destructive px-2 py-1">
            {error}
          </p>
        )}
        {largeFile ? (
          <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
            <FileText className="size-8 text-muted-foreground" />
            <p className="text-sm font-medium">This file is too large for the integrated editor.</p>
            <p className="max-w-md text-xs text-muted-foreground">
              Files over 2 MiB cannot be loaded here. Open it with your default app to edit it; changes will still persist across pack updates.
            </p>
          </div>
        ) : editorMode === "easy" && forgeDoc && selectedPath ? (
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
              <p className="px-2 py-1 text-xs text-muted-foreground">Easy mode is only available for Forge .cfg files. Use Advanced mode for this file.</p>
            )}
            <ConfigCodeEditor
              value={content}
              disabled={!canEditInLauncher || loading}
              isDark={isDark}
              className="flex-1"
              lineWrapping={lineWrapping}
              onChange={(next) => {
                setContent(next);
                setDirty(true);
              }}
            />
          </div>
        )}
      </div>
    </ResizablePanel>
  );
}
