import { useEffect, useState, useCallback, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Plus, Settings, Users, Boxes, Play, Square, Trash2, FolderInput, FolderOpen, Info, Terminal, SlidersHorizontal } from "lucide-react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "./components/ui/card";
import { Input } from "./components/ui/input";
import { Badge } from "./components/ui/badge";
import { Progress } from "./components/ui/progress";

import { Select } from "./components/ui/select";
import { ScrollArea } from "./components/ui/scroll-area";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "./components/ui/tabs";
import { ThemeEditor } from "./components/ThemeEditor";
import { ThemePresetPicker } from "./components/ThemePresetPicker";
import { ThemeSwitcher } from "./components/ThemeSwitcher";
import {
  classifyLaunchLogLine,
  formatLaunchLog,
  launchLogLevelClass,
  type LaunchLogLine,
} from "./lib/launch-log";
import {
  formatPlayTime,
  mergeInstanceSettings,
  type InstanceSettings,
} from "./lib/instance-settings";
import { InstanceSettingsPanel } from "./components/InstanceSettingsPanel";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "./components/ui/context-menu";
import "./App.css";

// ── Types ──

interface GtnhVersion {
  title: string;
  description: string;
  releaseDate: string;
  maxJavaVersion: number;
  mmc: { java8Url: string; java17_2XUrl: string };
  client: { java8Url: string };
}

interface InstanceInfo {
  id: string;
  installed: boolean;
  size_bytes: number;
  settings: InstanceSettings;
  group: string;
}

interface InstanceGroupsState {
  collapsed: Record<string, boolean>;
  groups: string[];
}

interface JavaInfo {
  path: string;
  version: number;
}

interface DlProgress {
  stage: string;
  pct: number;
  id?: string;
  name?: string;
}

interface LaunchLogEvent extends LaunchLogLine {
  id: string;
}

interface AccountInfo {
  id: string;
  username: string;
  uuid: string;
  skin_png_base64?: string;
  owns_minecraft?: boolean;
}

interface DeviceCodeInfo {
  user_code: string;
  verification_uri: string;
  message: string;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function parseReleaseDate(value: string): number {
  const parts = value.trim().split(/[/-]/).map((part) => Number.parseInt(part, 10));
  if (parts.length !== 3 || parts.some((part) => Number.isNaN(part))) {
    return 0;
  }
  const [year, month, day] = parts;
  return Date.UTC(year, month - 1, day);
}

function compareVersionsByReleaseDate(
  leftKey: string,
  rightKey: string,
  versions: Record<string, GtnhVersion> | null,
): number {
  const leftDate = parseReleaseDate(versions?.[leftKey]?.releaseDate ?? "");
  const rightDate = parseReleaseDate(versions?.[rightKey]?.releaseDate ?? "");
  if (leftDate !== rightDate) {
    return rightDate - leftDate;
  }
  return rightKey.localeCompare(leftKey, undefined, { numeric: true });
}

function instanceDisplayName(inst: InstanceInfo): string {
  return inst.settings.name || `GTNH ${inst.settings.pack_version || inst.id}`;
}

function instancePackVersion(inst: InstanceInfo): string {
  return inst.settings.pack_version || inst.id;
}

function sortInstancesByName(items: InstanceInfo[]): InstanceInfo[] {
  return [...items].sort((a, b) =>
    instanceDisplayName(a).localeCompare(instanceDisplayName(b), undefined, { sensitivity: "base" }),
  );
}

function sanitizeInstanceId(value: string): string {
  return value.replace(/[/\\:*?"<>|]/g, "_");
}

function makeInstanceId(name: string, packVersion: string, existing: Set<string>): string {
  const trimmed = name.trim();
  let base = sanitizeInstanceId(trimmed || `gtnh-${packVersion}`).slice(0, 48);
  if (!base) base = `gtnh-${packVersion}`;
  let candidate = base;
  let n = 2;
  while (existing.has(candidate)) {
    candidate = `${base}-${n}`;
    n += 1;
  }
  return candidate;
}

const OFFLINE_USERNAME_RE = /^[a-zA-Z0-9_]{1,16}$/;

function isValidOfflineUsername(value: string): boolean {
  return OFFLINE_USERNAME_RE.test(value.trim());
}

function hasLinkedMicrosoftAccount(accounts: AccountInfo[]): boolean {
  return accounts.some((a) => a.username.trim().length > 0);
}

function hasConfirmedOfflineUsername(settings: InstanceSettings): boolean {
  const username = settings.username.trim();
  return Boolean(settings.offline_username_confirmed) && isValidOfflineUsername(username);
}

// ponytail: one file, no router, no zustand

export default function App() {
  const [tab, setTab] = useState("instances");
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [dlProgress, setDlProgress] = useState<DlProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedInstanceId, setSelectedInstanceId] = useState<string | null>(null);
  const [detailTab, setDetailTab] = useState("info");
  const [javaOptions, setJavaOptions] = useState<JavaInfo[]>([]);
  const [instanceLogs, setInstanceLogs] = useState<Record<string, LaunchLogLine[]>>({});
  const [launching, setLaunching] = useState<string | null>(null);
  const launchingRef = useRef<string | null>(null);

  useEffect(() => {
    launchingRef.current = launching;
  }, [launching]);
  const [showNewInstance, setShowNewInstance] = useState(false);
  const [groupsState, setGroupsState] = useState<InstanceGroupsState>({ collapsed: {}, groups: [] });
  const [changeGroupInstanceId, setChangeGroupInstanceId] = useState<string | null>(null);
  const [lastUsedGroup, setLastUsedGroup] = useState("");
  const [accounts, setAccounts] = useState<AccountInfo[]>([]);
  const [gtnhVersions, setGtnhVersions] = useState<Record<string, GtnhVersion> | null>(null);
  const [offlineLaunchPrompt, setOfflineLaunchPrompt] = useState<{
    id: string;
    settings: InstanceSettings;
    username: string;
  } | null>(null);

  const loadGroups = useCallback(() => {
    invoke<InstanceGroupsState>("get_instance_groups").then(setGroupsState).catch(() => {});
  }, []);

  const loadAccounts = useCallback(() => {
    invoke<AccountInfo[]>("get_accounts").then(setAccounts).catch(() => setAccounts([]));
  }, []);

  useEffect(() => {
    loadInstances();
    loadGroups();
    loadAccounts();
    invoke<JavaInfo[]>("detect_java").then(setJavaOptions).catch(() => {});
    invoke<Record<string, GtnhVersion>>("get_versions").then(setGtnhVersions).catch(() => {});
  }, [loadGroups, loadAccounts]);

  useEffect(() => {
    const unlisten = listen<DlProgress>("dl-progress", (e) => {
      const p = e.payload;
      if (p.stage === "done") {
        setDlProgress(null);
        setShowNewInstance(false);
        if (p.id) {
          setSelectedInstanceId((current) => (current === p.id ? null : current));
        }
        loadInstances();
      } else {
        setDlProgress((current) => ({
          ...p,
          name: p.name ?? current?.name,
          id: p.id ?? current?.id,
        }));
      }
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  useEffect(() => {
    const unlisten = listen<LaunchLogEvent>("launch-log", (e) => {
      const { id, stream, line } = e.payload;
      setInstanceLogs((prev) => ({
        ...prev,
        [id]: [...(prev[id] ?? []), { stream, line }],
      }));
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const loadInstances = useCallback(() => {
    invoke<InstanceInfo[]>("get_instances")
      .then((list) => {
        setInstances(list);
        loadGroups();
      })
      .catch(() => {});
  }, [loadGroups]);

  const handleSetInstanceGroup = async (id: string, group: string) => {
    try {
      await invoke("set_instance_group", { id, group });
      setLastUsedGroup(group);
      loadInstances();
    } catch (e) {
      setError(`Change group failed: ${e}`);
    }
  };

  const handleRenameGroup = async (oldName: string, newName: string) => {
    try {
      await invoke("rename_group", { oldName, newName });
      loadInstances();
    } catch (e) {
      setError(`Rename group failed: ${e}`);
    }
  };

  const handleDeleteGroup = async (name: string) => {
    try {
      await invoke("delete_group", { name });
      loadInstances();
    } catch (e) {
      setError(`Delete group failed: ${e}`);
    }
  };

  const handleToggleGroupCollapsed = async (group: string, collapsed: boolean) => {
    setGroupsState((prev) => ({
      ...prev,
      collapsed: { ...prev.collapsed, [group]: collapsed },
    }));
    try {
      await invoke("set_group_collapsed", { group, collapsed });
    } catch (e) {
      setError(`Failed to save group state: ${e}`);
      loadGroups();
    }
  };

  const loadLogs = useCallback(async (id: string) => {
    try {
      const persisted = await invoke<LaunchLogLine[]>("get_instance_console_log", { id });
      setInstanceLogs((prev) => {
        if (launchingRef.current === id && (prev[id]?.length ?? 0) > 0) {
          return prev;
        }
        return { ...prev, [id]: persisted };
      });
    } catch {
      // keep in-memory logs if file read fails
    }
  }, []);

  useEffect(() => {
    if (selectedInstanceId) loadLogs(selectedInstanceId);
  }, [selectedInstanceId, loadLogs]);

  const runLaunch = useCallback(async (id: string) => {
    if (launchingRef.current !== null) return;
    launchingRef.current = id;
    setError(null);
    setSelectedInstanceId(id);
    setLaunching(id);

    const inst = instances.find((i) => i.id === id);
    const settings = inst ? mergeInstanceSettings(inst.settings) : null;
    const consoleCfg = settings?.override_console
      ? {
          showOnLaunch: settings.show_console_on_launch,
          showOnError: settings.show_console_on_error,
          autoClose: settings.auto_close_console,
        }
      : { showOnLaunch: false, showOnError: true, autoClose: false };

    if (consoleCfg.showOnLaunch) {
      setDetailTab("logs");
    }

    if (settings?.override_window && settings.close_after_launch) {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().hide();
      } catch {
        // not running inside Tauri (e.g. vite-only dev)
      }
    }

    try {
      await invoke("launch_instance", { id });
      if (settings?.override_window && settings.quit_after_game_stop) {
        try {
          await invoke("exit_launcher");
        } catch {
          // not running inside Tauri
        }
      }
      if (consoleCfg.autoClose) {
        setDetailTab("info");
      }
      loadInstances();
    } catch (e) {
      if (consoleCfg.showOnError) {
        setDetailTab("logs");
      }
      setError(`Launch failed: ${e}`);
    } finally {
      launchingRef.current = null;
      setLaunching(null);
    }
  }, [instances, loadInstances]);

  const handleLaunch = async (id: string) => {
    if (launchingRef.current !== null) return;

    let accountList = accounts;
    try {
      accountList = await invoke<AccountInfo[]>("get_accounts");
      setAccounts(accountList);
    } catch {
      accountList = accounts;
    }

    const inst = instances.find((i) => i.id === id);
    if (!inst) return;

    const merged = mergeInstanceSettings(inst.settings);
    if (merged.override_account && merged.account_id) {
      const picked = accountList.find((a) => a.id === merged.account_id);
      if (picked?.username.trim()) {
        await runLaunch(id);
        return;
      }
    }

    if (hasLinkedMicrosoftAccount(accountList)) {
      const settings: InstanceSettings = { ...inst.settings, auth_mode: "microsoft" };
      if (inst.settings.auth_mode !== "microsoft") {
        await invoke("save_settings", { id, settings });
        loadInstances();
      }
      await runLaunch(id);
      return;
    }

    if (hasConfirmedOfflineUsername(inst.settings)) {
      const savedUsername = inst.settings.username.trim();
      const settings: InstanceSettings = {
        ...inst.settings,
        auth_mode: "offline",
        username: savedUsername,
        offline_username_confirmed: true,
      };
      if (inst.settings.auth_mode !== "offline") {
        await invoke("save_settings", { id, settings });
        loadInstances();
      }
      await runLaunch(id);
      return;
    }

    const savedUsername = inst.settings.username.trim();
    setOfflineLaunchPrompt({
      id,
      settings: inst.settings,
      username: isValidOfflineUsername(savedUsername) ? savedUsername : "",
    });
  };

  const handleOfflineLaunchConfirm = async (username: string) => {
    if (!offlineLaunchPrompt) return;
    const trimmed = username.trim();
    if (!isValidOfflineUsername(trimmed)) {
      setError("Username must be 1–16 characters: letters, numbers, and underscores only.");
      return;
    }
    const { id, settings } = offlineLaunchPrompt;
    const next: InstanceSettings = {
      ...settings,
      auth_mode: "offline",
      username: trimmed,
      offline_username_confirmed: true,
    };
    setOfflineLaunchPrompt(null);
    try {
      await invoke("save_settings", { id, settings: next });
      loadInstances();
      await runLaunch(id);
    } catch (e) {
      setError(`Save failed: ${e}`);
    }
  };

  const handleKill = async (id: string) => {
    try {
      await invoke("kill_instance", { id });
    } catch (e) {
      setError(`Stop failed: ${e}`);
    }
  };

  const handleOpenInstanceSettings = (id: string) => {
    setSelectedInstanceId(id);
    setDetailTab("settings");
  };

  const handleOpenInstanceFolder = async (id: string) => {
    try {
      await invoke("open_instance_folder", { id });
    } catch (e) {
      setError(`Open folder failed: ${e}`);
    }
  };

  const handleClearConsole = async (id: string) => {
    try {
      await invoke("clear_instance_console_log", { id });
      setInstanceLogs((prev) => ({ ...prev, [id]: [] }));
    } catch (e) {
      setError(`Clear console failed: ${e}`);
    }
  };

  const handleDelete = async (id: string) => {
    const inst = instances.find((i) => i.id === id);
    const name = inst ? instanceDisplayName(inst) : id;
    if (!window.confirm(`Delete "${name}"? This removes all instance files and cannot be undone.`)) {
      return;
    }
    setError(null);
    setDlProgress({ stage: "deleting", pct: 0, id, name });
    try {
      await invoke("delete_instance", { id });
    } catch (e) {
      setDlProgress(null);
      setError(`Delete failed: ${e}`);
    }
  };

  const handleSaveSettings = async (id: string, settings: InstanceSettings) => {
    try {
      await invoke("save_settings", { id, settings });
      loadInstances();
    } catch (e) {
      setError(`Save failed: ${e}`);
    }
  };

  const sel = instances.find((i) => i.id === selectedInstanceId) ?? null;

  return (
    <div className="h-screen flex flex-col overflow-hidden">
      {/* Toolbar */}
      <header className="h-12 shrink-0 border-b border-border flex items-center px-2 gap-1 bg-card">
        <span className="font-semibold text-sm px-2 mr-1">Industrialis</span>
        <Button variant="ghost" size="sm" onClick={() => { setTab("instances"); setShowNewInstance(true); }} disabled={dlProgress !== null}>
          <Plus /> Add Instance
        </Button>
        <div className="w-px h-6 bg-border mx-1" />
        <Button variant={tab === "instances" ? "secondary" : "ghost"} size="sm" onClick={() => setTab("instances")}><Boxes /> Instances</Button>
        <Button variant={tab === "settings" ? "secondary" : "ghost"} size="sm" onClick={() => setTab("settings")}><Settings /> Settings</Button>
        <Button variant={tab === "accounts" ? "secondary" : "ghost"} size="sm" onClick={() => setTab("accounts")}><Users /> Accounts</Button>
        <div className="ml-auto flex items-center">
          <ThemeSwitcher />
        </div>
      </header>

      {tab === "instances" ? (
        <div className="flex-1 flex overflow-hidden">
          {/* Instance list */}
          <div className="w-1/2 max-w-xl border-r border-border overflow-auto flex flex-col">
            {instances.length === 0 ? (
              <p className="text-muted-foreground text-sm p-4">No instances installed. Click &ldquo;Add Instance&rdquo;.</p>
            ) : (
              <InstanceGroupList
                instances={instances}
                groupsState={groupsState}
                onToggleCollapsed={handleToggleGroupCollapsed}
                onRenameGroup={handleRenameGroup}
                onDeleteGroup={handleDeleteGroup}
                selectedInstanceId={selectedInstanceId}
                onSelect={setSelectedInstanceId}
                launching={launching}
                onLaunch={handleLaunch}
                onKill={handleKill}
                onOpenSettings={handleOpenInstanceSettings}
                onOpenFolder={handleOpenInstanceFolder}
                onDelete={handleDelete}
                operationBusy={dlProgress !== null}
              />
            )}
          </div>

          {/* Details panel */}
          <div className="flex-1 flex flex-col overflow-hidden">
            {sel ? (
              <>
                <div className="flex items-center gap-3 px-4 py-3 border-b border-border shrink-0">
                  <div className="size-10 rounded bg-secondary flex items-center justify-center text-base font-semibold shrink-0">
                    {instanceDisplayName(sel).charAt(0).toUpperCase()}
                  </div>
                  <div className="min-w-0">
                    <div className="font-medium truncate">{instanceDisplayName(sel)}</div>
                    <div className="text-xs text-muted-foreground truncate">
                      {instancePackVersion(sel)} · {formatBytes(sel.size_bytes)}{sel.group ? ` · ${sel.group}` : ""}
                    </div>
                  </div>
                </div>

                <Tabs value={detailTab} onValueChange={setDetailTab} className="flex-1 flex flex-col overflow-hidden">
                  <TabsList className="mx-4 mt-3 self-start">
                    <TabsTrigger value="info"><Info className="size-3.5 mr-1" />Info</TabsTrigger>
                    <TabsTrigger value="settings"><SlidersHorizontal className="size-3.5 mr-1" />Settings</TabsTrigger>
                    <TabsTrigger value="logs"><Terminal className="size-3.5 mr-1" />Logs</TabsTrigger>
                  </TabsList>

                  <TabsContent value="info" className="flex-1 overflow-auto px-4 pb-4 mt-3">
                    <div className="text-sm">
                      <InfoRow label="Pack version" value={instancePackVersion(sel)} />
                      <InfoRow label="Instance ID" value={sel.id} />
                      <InfoRow label="Size" value={formatBytes(sel.size_bytes)} />
                      <InfoRow label="Group" value={sel.group || "Ungrouped"} />
                      <InfoRow label="Java" value={sel.settings.java_path || "Auto-detect"} />
                      <InfoRow
                        label="RAM"
                        value={
                          sel.settings.override_memory
                            ? `${sel.settings.min_ram_mb}–${sel.settings.max_ram_mb} MB`
                            : "Default (4096–6144 MB)"
                        }
                      />
                      {mergeInstanceSettings(sel.settings).show_game_time &&
                        (sel.settings.override_game_time || sel.settings.total_play_seconds > 0) && (
                          <InfoRow
                            label="Play time"
                            value={formatPlayTime(sel.settings.total_play_seconds)}
                          />
                        )}
                      <InfoRow
                        label="Username"
                        value={
                          sel.settings.auth_mode === "microsoft"
                            ? "(Microsoft account)"
                            : sel.settings.offline_username_confirmed && sel.settings.username
                              ? sel.settings.username
                              : "Not set — choose when you first play"
                        }
                      />
                    </div>
                  </TabsContent>

                  <TabsContent value="settings" className="flex-1 overflow-auto px-4 pb-4 mt-3">
                    <InstanceSettingsPanel
                      instanceId={selectedInstanceId!}
                      packVersion={instancePackVersion(sel)}
                      javaOptions={javaOptions}
                      accounts={accounts}
                      onOpenLauncherSettings={() => setTab("settings")}
                      onSave={(v, s) => handleSaveSettings(v, s)}
                    />
                  </TabsContent>

                  <TabsContent value="logs" className="flex-1 overflow-hidden flex flex-col mt-3">
                    <LogView
                      log={instanceLogs[selectedInstanceId!] ?? []}
                      onClear={() => handleClearConsole(selectedInstanceId!)}
                      disableClear={launching === selectedInstanceId}
                    />
                  </TabsContent>
                </Tabs>

                {/* Action bar */}
                <div className="shrink-0 border-t border-border p-3 flex items-center gap-2">
                  <Button className="flex-1" onClick={() => handleLaunch(selectedInstanceId!)} disabled={launching !== null}>
                    <Play /> {launching === selectedInstanceId ? "Launching…" : launching ? "Busy" : "Launch"}
                  </Button>
                  <Button variant="outline" size="icon" title="Change group" onClick={() => setChangeGroupInstanceId(selectedInstanceId)}>
                    <FolderInput />
                  </Button>
                  <Button
                    variant="outline"
                    size="icon"
                    title="Delete"
                    disabled={dlProgress !== null}
                    onClick={() => handleDelete(selectedInstanceId!)}
                  >
                    <Trash2 />
                  </Button>
                </div>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
                Select an instance to view details.
              </div>
            )}
          </div>
        </div>
      ) : (
        <main className="flex-1 overflow-auto p-6">
          {tab === "settings" && <SettingsTab javaOptions={javaOptions} />}
          {tab === "accounts" && <AccountsTab onAccountsChanged={loadAccounts} />}
        </main>
      )}

      {/* Status bar */}
      <footer className="h-6 shrink-0 border-t border-border flex items-center px-3 gap-4 text-xs text-muted-foreground bg-card">
        <span>{instances.length} instance{instances.length === 1 ? "" : "s"}</span>
        {sel && <span className="truncate">{instanceDisplayName(sel)}</span>}
        {launching && <span>Launching {launching}…</span>}
        {dlProgress && dlProgress.stage === "deleting" && (
          <span>
            Deleting {dlProgress.name || dlProgress.id}… {(dlProgress.pct * 100).toFixed(0)}%
          </span>
        )}
        {dlProgress && dlProgress.stage !== "deleting" && (
          <span>Installing… {(dlProgress.pct * 100).toFixed(0)}%</span>
        )}
      </footer>

      {showNewInstance && (
        <NewInstanceDialog
          onClose={() => setShowNewInstance(false)}
          onInstall={async (id, packVersion, javaType, group, name) => {
            setError(null);
            try {
              await invoke("download_install", {
                id,
                packVersion,
                javaType,
                group: group || null,
                name: name || null,
              });
              if (group) setLastUsedGroup(group);
            } catch (e) { setError(`Install failed: ${e}`); setDlProgress(null); }
          }}
          existingInstanceIds={new Set(instances.map((i) => i.id))}
          existingGroups={groupsState.groups}
          initialGroup={lastUsedGroup}
          versions={gtnhVersions}
        />
      )}

      {changeGroupInstanceId && (
        <ChangeGroupDialog
          instanceName={
            instances.find((i) => i.id === changeGroupInstanceId)
              ? instanceDisplayName(instances.find((i) => i.id === changeGroupInstanceId)!)
              : changeGroupInstanceId
          }
          currentGroup={instances.find((i) => i.id === changeGroupInstanceId)?.group ?? ""}
          existingGroups={groupsState.groups}
          onClose={() => setChangeGroupInstanceId(null)}
          onSave={(group) => {
            handleSetInstanceGroup(changeGroupInstanceId, group);
            setChangeGroupInstanceId(null);
          }}
        />
      )}

      {offlineLaunchPrompt && (
        <OfflineUsernameDialog
          key={`${offlineLaunchPrompt.id}:${offlineLaunchPrompt.username}`}
          instanceName={
            instances.find((i) => i.id === offlineLaunchPrompt.id)
              ? instanceDisplayName(instances.find((i) => i.id === offlineLaunchPrompt.id)!)
              : offlineLaunchPrompt.id
          }
          username={offlineLaunchPrompt.username}
          onClose={() => setOfflineLaunchPrompt(null)}
          onConfirm={handleOfflineLaunchConfirm}
        />
      )}

      {/* Error toast */}
      {error && (
        <div className="fixed bottom-10 right-4 bg-destructive text-destructive-foreground p-3 rounded shadow-lg max-w-sm z-50">
          <p className="text-sm">{error}</p>
          <Button size="sm" variant="ghost" className="mt-1" onClick={() => setError(null)}>Dismiss</Button>
        </div>
      )}

      {/* Install / delete progress */}
      {dlProgress && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <Card className="w-80">
            <CardHeader>
              <CardTitle>
                {dlProgress.stage === "downloading" && "Downloading..."}
                {dlProgress.stage === "extracting" && "Extracting..."}
                {dlProgress.stage === "deleting" && "Deleting..."}
              </CardTitle>
              <CardDescription>
                {dlProgress.stage === "deleting"
                  ? `Removing ${dlProgress.name || dlProgress.id || "instance"}`
                  : "Installing instance"}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <Progress value={dlProgress.pct * 100} />
              <p className="text-sm text-muted-foreground mt-2">{(dlProgress.pct * 100).toFixed(0)}%</p>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}

// ── Instance Groups ──

interface GroupSection {
  id: string;
  label: string;
  items: InstanceInfo[];
}

function buildGroupSections(
  instances: InstanceInfo[],
  groupNames: string[],
): GroupSection[] {
  const buckets = new Map<string, InstanceInfo[]>();
  for (const inst of instances) {
    const key = inst.group || "";
    if (!buckets.has(key)) buckets.set(key, []);
    buckets.get(key)!.push(inst);
  }

  const sections: GroupSection[] = [];
  const sortedNames = [...groupNames].sort((a, b) =>
    a.localeCompare(b, undefined, { sensitivity: "base" }),
  );

  for (const name of sortedNames) {
    const items = buckets.get(name);
    if (items && items.length > 0) {
      sections.push({
        id: name,
        label: name,
        items: sortInstancesByName(items),
      });
      buckets.delete(name);
    }
  }

  for (const [key, items] of buckets) {
    if (key && items.length > 0) {
      sections.push({
        id: key,
        label: key,
        items: sortInstancesByName(items),
      });
    }
  }

  const ungrouped = buckets.get("") ?? [];
  if (ungrouped.length > 0) {
    sections.push({
      id: "",
      label: "Ungrouped",
      items: sortInstancesByName(ungrouped),
    });
  }

  return sections;
}

function InstanceGroupList({
  instances,
  groupsState,
  onToggleCollapsed,
  onRenameGroup,
  onDeleteGroup,
  selectedInstanceId,
  onSelect,
  launching,
  onLaunch,
  onKill,
  onOpenSettings,
  onOpenFolder,
  onDelete,
  operationBusy,
}: {
  instances: InstanceInfo[];
  groupsState: InstanceGroupsState;
  onToggleCollapsed: (group: string, collapsed: boolean) => void;
  onRenameGroup: (oldName: string, newName: string) => void;
  onDeleteGroup: (name: string) => void;
  selectedInstanceId: string | null;
  onSelect: (id: string) => void;
  launching: string | null;
  onLaunch: (id: string) => void;
  onKill: (id: string) => void;
  onOpenSettings: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onDelete: (id: string) => void;
  operationBusy: boolean;
}) {
  const sections = useMemo(
    () => buildGroupSections(instances, groupsState.groups),
    [instances, groupsState.groups],
  );

  if (sections.length === 0) return null;

  return (
    <div>
      {sections.map((section) => (
        <InstanceGroupSection
          key={section.id || "__ungrouped__"}
          section={section}
          collapsed={groupsState.collapsed[section.id] ?? false}
          onToggleCollapsed={(collapsed) => onToggleCollapsed(section.id, collapsed)}
          onRenameGroup={section.id ? onRenameGroup : undefined}
          onDeleteGroup={section.id ? onDeleteGroup : undefined}
          selectedInstanceId={selectedInstanceId}
          onSelect={onSelect}
          launching={launching}
          onLaunch={onLaunch}
          onKill={onKill}
          onOpenSettings={onOpenSettings}
          onOpenFolder={onOpenFolder}
          onDelete={onDelete}
          operationBusy={operationBusy}
        />
      ))}
    </div>
  );
}

function InstanceGroupSection({
  section,
  collapsed,
  onToggleCollapsed,
  onRenameGroup,
  onDeleteGroup,
  selectedInstanceId,
  onSelect,
  launching,
  onLaunch,
  onKill,
  onOpenSettings,
  onOpenFolder,
  onDelete,
  operationBusy,
}: {
  section: GroupSection;
  collapsed: boolean;
  onToggleCollapsed: (collapsed: boolean) => void;
  onRenameGroup?: (oldName: string, newName: string) => void;
  onDeleteGroup?: (name: string) => void;
  selectedInstanceId: string | null;
  onSelect: (id: string) => void;
  launching: string | null;
  onLaunch: (id: string) => void;
  onKill: (id: string) => void;
  onOpenSettings: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onDelete: (id: string) => void;
  operationBusy: boolean;
}) {
  const [renaming, setRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState("");

  const startRename = () => {
    setRenameDraft(section.label);
    setRenaming(true);
  };

  const commitRename = () => {
    setRenaming(false);
    const trimmed = renameDraft.trim();
    if (!onRenameGroup || !section.id || !trimmed || trimmed === section.label) {
      return;
    }
    onRenameGroup(section.id, trimmed);
  };

  const handleDelete = () => {
    if (!onDeleteGroup || !section.id) return;
    if (window.confirm(`Delete group "${section.label}"? Instances will be moved to Ungrouped.`)) {
      onDeleteGroup(section.id);
    }
  };

  return (
    <section>
      <div className="flex items-center gap-2 px-3 py-1.5 group/header sticky top-0 bg-background z-10 border-b border-border/50">
        <button
          type="button"
          onClick={() => onToggleCollapsed(!collapsed)}
          className="flex items-center gap-2 flex-1 min-w-0 text-left hover:text-foreground transition-colors"
          aria-expanded={!collapsed}
        >
          <span className="text-muted-foreground text-xs w-3 shrink-0">
            {collapsed ? "▶" : "▼"}
          </span>
          {renaming ? (
            <Input
              value={renameDraft}
              onChange={(e) => setRenameDraft(e.target.value)}
              onBlur={commitRename}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitRename();
                if (e.key === "Escape") setRenaming(false);
              }}
              onClick={(e) => e.stopPropagation()}
              className="h-6 text-xs max-w-xs"
              autoFocus
            />
          ) : (
            <h2 className="text-xs font-semibold tracking-wide uppercase truncate text-muted-foreground">{section.label}</h2>
          )}
          <Badge variant="secondary" className="shrink-0 h-5">
            {section.items.length}
          </Badge>
        </button>
        {section.id && onRenameGroup && onDeleteGroup && !renaming && (
          <div className="flex items-center gap-1 opacity-0 group-hover/header:opacity-100 transition-opacity">
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={startRename}
            >
              Rename
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs text-destructive hover:text-destructive"
              onClick={handleDelete}
            >
              Delete
            </Button>
          </div>
        )}
      </div>
      {!collapsed && section.items.map((inst) => (
        <InstanceRow
          key={inst.id}
          inst={inst}
          selected={selectedInstanceId === inst.id}
          onSelect={() => onSelect(inst.id)}
          running={launching === inst.id}
          busy={launching !== null}
          onLaunch={() => onLaunch(inst.id)}
          onKill={() => onKill(inst.id)}
          onOpenSettings={() => onOpenSettings(inst.id)}
          onOpenFolder={() => onOpenFolder(inst.id)}
          onDelete={() => onDelete(inst.id)}
          operationBusy={operationBusy}
        />
      ))}
    </section>
  );
}

function OfflineUsernameDialog({
  instanceName,
  username,
  onClose,
  onConfirm,
}: {
  instanceName: string;
  username: string;
  onClose: () => void;
  onConfirm: (username: string) => void;
}) {
  const [value, setValue] = useState(username);

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-40">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Choose a username</CardTitle>
            <Button variant="ghost" size="sm" onClick={onClose}>✕</Button>
          </div>
          <CardDescription>
            No Microsoft account is linked. Pick an offline username for {instanceName} — it will be
            saved so you only set it once.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div>
            <label className="text-sm font-medium">Username</label>
            <Input
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder="Steve"
              maxLength={16}
              className="mt-1 font-mono"
              autoFocus
              onKeyDown={(e) => e.key === "Enter" && onConfirm(value)}
            />
            <p className="text-xs text-muted-foreground mt-1">
              Letters, numbers, and underscores. Up to 16 characters.
            </p>
          </div>
          <div className="flex gap-2">
            <Button className="flex-1" onClick={() => onConfirm(value)}>
              <Play className="size-4" />
              Play
            </Button>
            <Button variant="outline" onClick={onClose}>Cancel</Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function ChangeGroupDialog({
  instanceName,
  currentGroup,
  existingGroups,
  onClose,
  onSave,
}: {
  instanceName: string;
  currentGroup: string;
  existingGroups: string[];
  onClose: () => void;
  onSave: (group: string) => void;
}) {
  const [group, setGroup] = useState(currentGroup);
  const listId = "change-group-options";

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-40">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Change Group</CardTitle>
            <Button variant="ghost" size="sm" onClick={onClose}>✕</Button>
          </div>
          <CardDescription>{instanceName}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div>
            <label className="text-sm font-medium">Group</label>
            <Input
              value={group}
              onChange={(e) => setGroup(e.target.value)}
              placeholder="No group"
              list={listId}
              onKeyDown={(e) => e.key === "Enter" && onSave(group.trim())}
              autoFocus
            />
            <datalist id={listId}>
              {existingGroups.map((g) => (
                <option key={g} value={g} />
              ))}
            </datalist>
            <p className="text-xs text-muted-foreground mt-1">
              Pick an existing group or type a new name. Leave empty for Ungrouped.
            </p>
          </div>
          <div className="flex gap-2">
            <Button className="flex-1" onClick={() => onSave(group.trim())}>Save</Button>
            <Button variant="outline" onClick={onClose}>Cancel</Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function GroupPicker({
  value,
  onChange,
  existingGroups,
  id,
}: {
  value: string;
  onChange: (value: string) => void;
  existingGroups: string[];
  id: string;
}) {
  return (
    <div>
      <label className="text-sm font-medium">Group</label>
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="No group"
        list={id}
      />
      <datalist id={id}>
        {existingGroups.map((g) => (
          <option key={g} value={g} />
        ))}
      </datalist>
    </div>
  );
}

// ── Instance Row ──

function InstanceRow({
  inst,
  selected,
  onSelect,
  running,
  busy,
  onLaunch,
  onKill,
  onOpenSettings,
  onOpenFolder,
  onDelete,
  operationBusy,
}: {
  inst: InstanceInfo;
  selected: boolean;
  onSelect: () => void;
  running: boolean;
  busy: boolean;
  onLaunch: () => void;
  onKill: () => void;
  onOpenSettings: () => void;
  onOpenFolder: () => void;
  onDelete: () => void;
  operationBusy: boolean;
}) {
  const name = instanceDisplayName(inst);
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          className={`group/row flex items-center gap-1 border-b border-border/40 transition-colors ${
            selected ? "bg-muted" : "hover:bg-muted/50"
          }`}
        >
          <button
            type="button"
            onClick={onSelect}
            className="flex min-w-0 flex-1 items-center gap-3 px-3 py-2 text-left"
          >
            <div className="size-9 rounded bg-secondary flex items-center justify-center text-sm font-medium shrink-0">
              {name.charAt(0).toUpperCase()}
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium truncate">{name}</div>
              <div className="text-xs text-muted-foreground truncate">
                {instancePackVersion(inst)} · {formatBytes(inst.size_bytes)}
              </div>
            </div>
            {running && (
              <span
                className="size-2 rounded-full bg-green-500 animate-pulse shrink-0"
                title="Running"
              />
            )}
          </button>
          <div className="flex items-center gap-0.5 pr-2 shrink-0">
            {running ? (
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="size-7 text-destructive hover:text-destructive"
                title="Stop"
                aria-label={`Stop ${name}`}
                onClick={(e) => {
                  e.stopPropagation();
                  onKill();
                }}
              >
                <Square className="size-3.5 fill-current" />
              </Button>
            ) : (
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="size-7"
                title={busy ? "Another instance is launching" : "Launch"}
                aria-label={`Launch ${name}`}
                disabled={busy}
                onClick={(e) => {
                  e.stopPropagation();
                  onLaunch();
                }}
              >
                <Play className="size-3.5" />
              </Button>
            )}
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-7"
              title="Instance settings"
              aria-label={`Settings for ${name}`}
              onClick={(e) => {
                e.stopPropagation();
                onOpenSettings();
              }}
            >
              <SlidersHorizontal className="size-3.5" />
            </Button>
          </div>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        <ContextMenuItem onSelect={onOpenFolder}>
          <FolderOpen />
          Open in Explorer
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={onOpenSettings}>
          <SlidersHorizontal />
          Settings
        </ContextMenuItem>
        {running ? (
          <ContextMenuItem onSelect={onKill} className="text-destructive focus:text-destructive">
            <Square className="fill-current" />
            Stop
          </ContextMenuItem>
        ) : (
          <ContextMenuItem onSelect={onLaunch} disabled={busy}>
            <Play />
            Launch
          </ContextMenuItem>
        )}
        <ContextMenuSeparator />
        <ContextMenuItem
          onSelect={onDelete}
          disabled={running || operationBusy}
          className="text-destructive focus:text-destructive"
        >
          <Trash2 />
          Delete instance
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

// ── Details helpers ──

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-4 py-1.5 border-b border-border/50">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium text-right truncate">{value}</span>
    </div>
  );
}

function LogView({ log, onClear, disableClear }: {
  log: LaunchLogLine[];
  onClear: () => void;
  disableClear: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [log]);

  const copy = async () => {
    const text = formatLaunchLog(log);
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="flex flex-col flex-1 overflow-hidden">
      <div className="flex gap-1 px-4 pb-1 shrink-0">
        <Button size="sm" variant="ghost" onClick={copy} disabled={log.length === 0}>
          {copied ? "Copied" : "Copy"}
        </Button>
        <Button size="sm" variant="ghost" onClick={onClear} disabled={disableClear || log.length === 0}>
          Clear
        </Button>
      </div>
      <ScrollArea ref={ref} className="flex-1 bg-black/60 p-3 font-mono text-xs">
        {log.length === 0 ? (
          <div className="text-muted-foreground">No log output yet.</div>
        ) : (
          log.map((entry, i) => (
            <div
              key={i}
              className={launchLogLevelClass(classifyLaunchLogLine(entry))}
            >
              {entry.line}
            </div>
          ))
        )}
      </ScrollArea>
    </div>
  );
}

// ── New Instance Dialog ──

function NewInstanceDialog({
  onClose,
  onInstall,
  existingInstanceIds,
  existingGroups,
  initialGroup,
  versions,
}: {
  onClose: () => void;
  onInstall: (
    id: string,
    packVersion: string,
    javaType: string,
    group: string,
    name: string,
  ) => void;
  existingInstanceIds: Set<string>;
  existingGroups: string[];
  initialGroup: string;
  versions: Record<string, GtnhVersion> | null;
}) {
  const [filter, setFilter] = useState<"all" | "stable" | "beta">("all");
  const [sel, setSel] = useState<string | null>(null);
  const [javaType, setJavaType] = useState("java17+");
  const [group, setGroup] = useState(initialGroup);
  const [instanceName, setInstanceName] = useState("");

  const sorted = versions
    ? Object.entries(versions).sort(([a], [b]) => compareVersionsByReleaseDate(a, b, versions))
    : [];

  const filtered = filter === "stable"
    ? sorted.filter(([, v]) => v.title === "Stable release")
    : filter === "beta"
      ? sorted.filter(([, v]) => v.title !== "Stable release")
      : sorted;

  const resolvedName = instanceName.trim() || (sel ? `GTNH ${sel}` : "");
  const resolvedId = sel
    ? makeInstanceId(resolvedName, sel, existingInstanceIds)
    : "";

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-40 p-4">
      <Card className="w-full max-w-lg max-h-[85vh] flex flex-col overflow-hidden">
        <CardHeader className="shrink-0 pb-4">
          <div className="flex items-center justify-between">
            <CardTitle>New Instance</CardTitle>
            <Button variant="ghost" size="sm" onClick={onClose}>✕</Button>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col flex-1 min-h-0 gap-4 overflow-hidden pb-6">
          <div className="flex gap-2 shrink-0">
            <Select value={filter} onChange={(e) => setFilter(e.target.value as typeof filter)}>
              <option value="all">All versions</option>
              <option value="stable">Stable only</option>
              <option value="beta">Beta only</option>
            </Select>
            <Select value={javaType} onChange={(e) => setJavaType(e.target.value)}>
              <option value="java17+">Java 17+</option>
              <option value="java8">Java 8</option>
            </Select>
          </div>

          <ScrollArea className="flex-1 min-h-0 rounded-md border border-border">
            <div className="space-y-2 p-2">
              {!versions && (
                <p className="text-muted-foreground text-sm px-1 py-2">Loading versions...</p>
              )}
              {versions && filtered.length === 0 && (
                <p className="text-muted-foreground text-sm px-1 py-2">No versions match this filter.</p>
              )}
              {filtered.map(([key, v]) => (
                <div
                  key={key}
                  className={`flex items-center justify-between p-3 rounded cursor-pointer border transition-colors ${
                    sel === key ? "border-foreground/30 bg-muted" : "border-border hover:bg-muted"
                  }`}
                  onClick={() => setSel(key)}
                >
                  <div>
                    <div className="font-medium">{key}</div>
                    <div className="text-xs text-muted-foreground">
                      {v.releaseDate} · Max Java {v.maxJavaVersion}
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <Badge variant={v.title === "Stable release" ? "success" : "warning"}>
                      {v.title === "Stable release" ? "Stable" : "Beta"}
                    </Badge>
                  </div>
                </div>
              ))}
            </div>
          </ScrollArea>

          <div className="shrink-0 space-y-4 border-t border-border pt-4">
            <div>
              <label className="text-sm font-medium">Instance name</label>
              <Input
                className="mt-1"
                value={instanceName}
                onChange={(e) => setInstanceName(e.target.value)}
                placeholder={sel ? `GTNH ${sel}` : "Name this instance"}
              />
              {sel && (
                <p className="text-xs text-muted-foreground mt-1">
                  Folder: <span className="font-mono">{resolvedId}</span>
                </p>
              )}
            </div>

            <GroupPicker
              value={group}
              onChange={setGroup}
              existingGroups={existingGroups}
              id="new-instance-group-options"
            />

            <Button
              className="w-full"
              disabled={!sel}
              onClick={() =>
                sel && onInstall(resolvedId, sel, javaType, group.trim(), resolvedName)
              }
            >
              Install {sel || ""}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

// ── Settings Tab ──

function SettingsTab({ javaOptions }: { javaOptions: JavaInfo[] }) {
  return (
    <div className="space-y-6 max-w-lg">
      <ThemePresetPicker />
      <ThemeEditor />

      <Card>
        <CardHeader><CardTitle>Java Detection</CardTitle></CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground mb-2">Detected Java installations:</p>
          {javaOptions.length === 0 && <p className="text-xs text-muted-foreground">None found</p>}
          <div className="space-y-1">
            {javaOptions.map((j) => (
              <div key={j.path} className="text-sm font-mono bg-muted p-2 rounded">
                <span className="text-foreground font-medium">Java {j.version}</span>
                <span className="text-muted-foreground ml-2 text-xs">{j.path}</span>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle>About</CardTitle></CardHeader>
        <CardContent className="text-sm text-muted-foreground space-y-1">
          <p>Industrialis Launcher v0.1.0</p>
          <p>GT New Horizons modpack manager built with Tauri.</p>
        </CardContent>
      </Card>
    </div>
  );
}

// ── Accounts Tab ──

function AccountsTab({ onAccountsChanged }: { onAccountsChanged?: () => void }) {
  const [accounts, setAccounts] = useState<AccountInfo[]>([]);
  const [loggingIn, setLoggingIn] = useState(false);
  const [deviceCode, setDeviceCode] = useState<DeviceCodeInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    invoke<AccountInfo[]>("get_accounts")
      .then((list) => {
        setAccounts(list);
        onAccountsChanged?.();
      })
      .catch(() => {});
  };
  useEffect(load, []);

  useEffect(() => {
    const unlisten = listen<DeviceCodeInfo>("auth-device-code", (e) => {
      setDeviceCode(e.payload);
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const handleLogin = async () => {
    setLoggingIn(true);
    setError(null);
    setDeviceCode(null);
    try {
      await invoke<AccountInfo>("start_microsoft_login");
      load();
    } catch (e) {
      setError(`${e}`);
    }
    setLoggingIn(false);
    setDeviceCode(null);
  };

  const handleRemove = async (id: string) => {
    await invoke("remove_account", { id });
    load();
  };

  return (
    <div className="space-y-4 max-w-lg">
      <h2 className="text-lg font-semibold">Microsoft Accounts</h2>

      {accounts.length === 0 && (
        <p className="text-muted-foreground">No accounts linked.</p>
      )}

      {accounts.map((acc) => (
        <Card key={acc.id}>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-3 min-w-0">
                {acc.skin_png_base64 && (
                  <img
                    src={`data:image/png;base64,${acc.skin_png_base64}`}
                    alt=""
                    className="w-8 h-8 rounded-sm shrink-0 image-pixelated"
                    style={{ imageRendering: "pixelated" }}
                  />
                )}
                <div className="min-w-0">
                  <CardTitle className="text-base truncate">
                    {acc.username || "(no Minecraft username)"}
                  </CardTitle>
                  {acc.uuid && (
                    <CardDescription className="font-mono text-xs truncate">{acc.uuid}</CardDescription>
                  )}
                </div>
              </div>
              <Button size="sm" variant="destructive" onClick={() => handleRemove(acc.id)}>Remove</Button>
            </div>
            {acc.owns_minecraft === false && (
              <p className="text-xs text-amber-500 mt-2">This account does not own Minecraft Java Edition.</p>
            )}
          </CardHeader>
        </Card>
      ))}

      {loggingIn && deviceCode && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Device code login</CardTitle>
            <CardDescription>
              If the browser did not open, enter this code at{" "}
              <a className="text-foreground underline" href={deviceCode.verification_uri} target="_blank" rel="noreferrer">
                {deviceCode.verification_uri}
              </a>
            </CardDescription>
          </CardHeader>
          <CardContent>
            <p className="text-2xl font-mono tracking-widest">{deviceCode.user_code}</p>
            <p className="text-xs text-muted-foreground mt-2">{deviceCode.message}</p>
          </CardContent>
        </Card>
      )}

      <Button onClick={handleLogin} disabled={loggingIn} className="w-full">
        {loggingIn ? "Logging in..." : "Add Microsoft Account"}
      </Button>

      {error && <p className="text-destructive text-sm">{error}</p>}
    </div>
  );
}
