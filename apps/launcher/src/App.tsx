import { useEffect, useState, useCallback, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Plus, Settings, Users, Boxes, Play, Square, Trash2, FolderInput, FolderOpen, Info, Terminal, SlidersHorizontal, ArrowUpCircle, Files, Package, Loader2, X, Activity, ChevronDown, ChevronRight, RefreshCw } from "lucide-react";
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
import { AccountSwitcher } from "./components/AccountSwitcher";
import { AccountsTab } from "./components/AccountsTab";
import { useLauncherSettings } from "./context/LauncherSettingsContext";
import { resolveDefaultAccountId } from "./lib/launcher-settings";
import { ProcessesDropdown } from "./components/ProcessesDropdown";
import { ProcessesTab } from "./components/ProcessesTab";
import {
  applyDlProgressEvent,
  createProcess,
  dismissProcess,
  formatDownloadProgress,
  getInstanceProcess,
  isInstanceBusy,
  markProcessFailed,
  operationLabel,
  processKey,
  resolveOperation,
  runningProcessCount,
  sortedProcesses,
  stageLabel,
  type BackgroundProcess,
  type DlProgressEvent,
  type ProcessOperation,
} from "./lib/background-processes";
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
import { InstanceMinecraftEditor } from "./components/InstanceMinecraftEditor";
import { CustomModsPanel } from "./components/CustomModsPanel";
import { UpdatePackDialog } from "./components/UpdatePackDialog";
import { ReinstallInstanceDialog } from "./components/ReinstallInstanceDialog";
import { PackVersionStatus } from "./components/PackVersionStatus";
import { InstanceAvatar } from "./components/InstanceAvatar";
import { compareVersionsByReleaseDate } from "./lib/pack-version-status";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { Dialog, DialogContent } from "./components/ui/dialog";
import { Label } from "./components/ui/label";
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
  icon_path?: string | null;
}

interface InstanceGroupsState {
  collapsed: Record<string, boolean>;
  groups: string[];
}

interface JavaInfo {
  path: string;
  version: number;
}

interface LaunchLogEvent extends LaunchLogLine {
  id: string;
}

interface AccountInfo {
  id: string;
  username: string;
  uuid: string;
  account_type: string;
  skin_png_base64?: string;
  owns_minecraft?: boolean;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatInstanceSize(bytes: number, refreshing: boolean): string {
  if (bytes === 0 && refreshing) return "…";
  return formatBytes(bytes);
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
  return value.replace(/[/\\:*?"<>|\s]/g, "_");
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

function accountDisplayName(account: AccountInfo): string {
  if (account.username.trim()) return account.username;
  return account.account_type === "offline" ? "Offline account" : "Microsoft account";
}

function isInstanceActive(
  id: string,
  launching: string | null,
  runningInstanceIds: Set<string>,
): boolean {
  return launching === id || runningInstanceIds.has(id);
}

function resolveLaunchAccount(
  accountList: AccountInfo[],
  defaultAccountId: string | null | undefined,
  instanceSettings: InstanceSettings,
): AccountInfo | null {
  const merged = mergeInstanceSettings(instanceSettings);
  if (merged.override_account && merged.account_id) {
    return accountList.find((a) => a.id === merged.account_id) ?? null;
  }
  if (defaultAccountId) {
    return accountList.find((a) => a.id === defaultAccountId) ?? null;
  }
  if (accountList.length === 1) {
    return accountList[0];
  }
  return null;
}

// ponytail: one file, no router, no zustand

export default function App() {
  const { settings: launcherSettings, loaded: launcherSettingsLoaded, updateSettings, saveSettingsNow } = useLauncherSettings();
  const defaultAccountId = resolveDefaultAccountId(launcherSettings);
  const [tab, setTab] = useState("instances");
  const [selectedProcessKey, setSelectedProcessKey] = useState<string | null>(null);
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [sizesRefreshing, setSizesRefreshing] = useState(false);
  const [processes, setProcesses] = useState<Map<string, BackgroundProcess>>(() => new Map());
  const [error, setError] = useState<string | null>(null);
  const [selectedInstanceId, setSelectedInstanceId] = useState<string | null>(null);
  const [detailTab, setDetailTab] = useState("info");
  const [javaOptions, setJavaOptions] = useState<JavaInfo[]>([]);
  const [instanceLogs, setInstanceLogs] = useState<Record<string, LaunchLogLine[]>>({});
  const [launching, setLaunching] = useState<string | null>(null);
  const [runningInstanceIds, setRunningInstanceIds] = useState<Set<string>>(() => new Set());
  const launchingRef = useRef<string | null>(null);

  useEffect(() => {
    launchingRef.current = launching;
  }, [launching]);
  const [showNewInstance, setShowNewInstance] = useState(false);
  const [updatePackInstanceId, setUpdatePackInstanceId] = useState<string | null>(null);
  const [reinstallInstanceId, setReinstallInstanceId] = useState<string | null>(null);
  const [groupsState, setGroupsState] = useState<InstanceGroupsState>({ collapsed: {}, groups: [] });
  const [changeGroupInstanceId, setChangeGroupInstanceId] = useState<string | null>(null);
  const [lastUsedGroup, setLastUsedGroup] = useState("");
  const [accounts, setAccounts] = useState<AccountInfo[]>([]);
  const [gtnhVersions, setGtnhVersions] = useState<Record<string, GtnhVersion> | null>(null);
  const [accountsLaunchRedirect, setAccountsLaunchRedirect] = useState<{
    instanceId: string;
    instanceName: string;
  } | null>(null);
  const [deleteInstanceConfirm, setDeleteInstanceConfirm] = useState<{
    id: string;
    name: string;
  } | null>(null);

  const loadGroups = useCallback(() => {
    invoke<InstanceGroupsState>("get_instance_groups").then(setGroupsState).catch(() => {});
  }, []);

  const loadAccounts = useCallback(() => {
    invoke<AccountInfo[]>("get_accounts").then(setAccounts).catch(() => setAccounts([]));
  }, []);

  const loadInstanceSizes = useCallback(() => {
    setSizesRefreshing(true);
    void invoke<Record<string, number>>("refresh_instance_sizes", { ids: null })
      .then((sizes) => {
        setInstances((prev) =>
          prev.map((inst) => ({
            ...inst,
            size_bytes: sizes[inst.id] ?? inst.size_bytes,
          })),
        );
      })
      .catch(() => {})
      .finally(() => setSizesRefreshing(false));
  }, []);

  const loadInstances = useCallback(() => {
    invoke<InstanceInfo[]>("get_instances")
      .then((list) => {
        setInstances(list);
        loadGroups();
        if (list.some((inst) => inst.size_bytes === 0)) {
          loadInstanceSizes();
        }
      })
      .catch(() => {});
  }, [loadGroups, loadInstanceSizes]);

  useEffect(() => {
    loadInstances();
    loadAccounts();
    invoke<JavaInfo[]>("detect_java").then(setJavaOptions).catch(() => {});
    invoke<Record<string, GtnhVersion>>("get_versions").then(setGtnhVersions).catch(() => {});
  }, [loadAccounts, loadInstances]);

  useEffect(() => {
    if (!launcherSettingsLoaded) return;
    if (defaultAccountId && !accounts.some((a) => a.id === defaultAccountId)) {
      updateSettings({ default_account_id: null });
      void saveSettingsNow();
      return;
    }
    if (!defaultAccountId && accounts.length === 1) {
      updateSettings({ default_account_id: accounts[0].id });
      void saveSettingsNow();
    }
  }, [accounts, defaultAccountId, launcherSettingsLoaded, updateSettings, saveSettingsNow]);

  const handleSetDefaultAccount = useCallback(
    (id: string | null) => {
      updateSettings({ default_account_id: id });
      void saveSettingsNow();
    },
    [updateSettings, saveSettingsNow],
  );

  const registerProcess = useCallback(
    (operation: ProcessOperation, id: string, name: string, initialLog?: string) => {
      const proc = createProcess(operation, id, name, initialLog);
      setProcesses((prev) => {
        const next = new Map(prev);
        next.set(proc.key, proc);
        return next;
      });
    },
    [],
  );

  const handleProcessFailed = useCallback(
    (operation: ProcessOperation, id: string, error: unknown) => {
      setProcesses((prev) => markProcessFailed(prev, operation, id, error));
      setError(`${operationLabel(operation)} failed: ${error}`);
    },
    [],
  );

  const startPackUpdate = useCallback(
    (
      id: string,
      name: string,
      packVersion: string,
      javaType: string,
      overwritePackConfigs: boolean,
      keepModIdentities: string[],
    ) => {
      const key = processKey("update-pack", id);
      setError(null);
      setUpdatePackInstanceId(null);
      setProcesses((prev) => {
        const next = new Map(prev);
        next.set(
          key,
          createProcess("update-pack", id, name, `Preparing pack update to ${packVersion}...`),
        );
        return next;
      });
      setTab("processes");
      setSelectedProcessKey(key);
      void invoke("update_instance", {
        id,
        packVersion,
        javaType,
        overwritePackConfigs,
        keepModIdentities,
      }).catch((e) => handleProcessFailed("update-pack", id, e));
    },
    [handleProcessFailed],
  );

  const startCleanReinstall = useCallback(
    (id: string, name: string, packVersion: string, javaType: string) => {
      const key = processKey("reinstall", id);
      setError(null);
      setReinstallInstanceId(null);
      setProcesses((prev) => {
        const next = new Map(prev);
        next.set(
          key,
          createProcess("reinstall", id, name, `Starting clean reinstall to ${packVersion}…`),
        );
        return next;
      });
      setTab("processes");
      setSelectedProcessKey(key);
      void invoke("reinstall_instance", {
        id,
        packVersion,
        javaType,
      }).catch((e) => handleProcessFailed("reinstall", id, e));
    },
    [handleProcessFailed],
  );

  const handleDismissProcess = useCallback((key: string) => {
    setProcesses((prev) => dismissProcess(prev, key));
    setSelectedProcessKey((current) => (current === key ? null : current));
  }, []);

  const openProcesses = useCallback(
    (key?: string) => {
      setTab("processes");
      if (key) {
        setSelectedProcessKey(key);
        return;
      }
      setSelectedProcessKey((current) => {
        if (current && processes.has(current)) return current;
        return sortedProcesses(processes)[0]?.key ?? null;
      });
    },
    [processes],
  );

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

  useEffect(() => {
    const clearLaunching = (id: string) => {
      if (launchingRef.current === id) {
        launchingRef.current = null;
        setLaunching(null);
      }
    };

    const unlistenStarted = listen<{ id: string }>("instance-started", (e) => {
      const { id } = e.payload;
      setRunningInstanceIds((prev) => new Set(prev).add(id));
      clearLaunching(id);
    });
    const unlistenStopped = listen<{ id: string }>("instance-stopped", (e) => {
      const { id } = e.payload;
      setRunningInstanceIds((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
      clearLaunching(id);
      loadInstances();
    });
    return () => {
      unlistenStarted.then((f) => f());
      unlistenStopped.then((f) => f());
    };
  }, [loadInstances]);

  useEffect(() => {
    const unlisten = listen<DlProgressEvent>("dl-progress", (e) => {
      const p = e.payload;
      setProcesses((prev) => {
        const operation = resolveOperation(prev, p);
        const next = applyDlProgressEvent(prev, p);
        if (p.stage === "failed" && p.id && operation) {
          const message = p.log_line?.replace(/^Error:\s*/, "") ?? "Unknown error";
          setError(`${operationLabel(operation)} failed: ${message}`);
        }
        if (p.stage === "done" && p.id) {
          if (operation === "delete") {
            setSelectedInstanceId((current) => (current === p.id ? null : current));
          } else if (operation === "install") {
            setSelectedInstanceId(p.id);
            setShowNewInstance(false);
          } else if (operation === "update-pack" || operation === "reinstall") {
            setSelectedInstanceId(p.id);
            setTab("instances");
            setSelectedProcessKey(null);
          }
          loadInstances();
        }
        return next;
      });
    });
    return () => { unlisten.then((f) => f()); };
  }, [loadInstances]);

  const instanceBusy = useCallback(
    (id: string) => isInstanceBusy(processes, id),
    [processes],
  );

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

    const launchAccount = resolveLaunchAccount(
      accountList,
      resolveDefaultAccountId(launcherSettings),
      inst.settings,
    );
    if (!launchAccount) {
      setAccountsLaunchRedirect({
        instanceId: id,
        instanceName: instanceDisplayName(inst),
      });
      setTab("accounts");
      return;
    }

    await runLaunch(id);
  };

  const handleKill = async (id: string) => {
    try {
      await invoke("kill_instance", { id });
    } catch (e) {
      setError(`Stop failed: ${e}`);
    } finally {
      setRunningInstanceIds((prev) => {
        if (!prev.has(id)) return prev;
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
      if (launchingRef.current === id) {
        launchingRef.current = null;
        setLaunching(null);
      }
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

  const handleDelete = (id: string) => {
    const inst = instances.find((i) => i.id === id);
    const name = inst ? instanceDisplayName(inst) : id;
    setDeleteInstanceConfirm({ id, name });
  };

  const confirmDeleteInstance = () => {
    if (!deleteInstanceConfirm) return;
    const { id, name } = deleteInstanceConfirm;
    setError(null);
    registerProcess("delete", id, name);
    void invoke("delete_instance", { id }).catch((e) => {
      const message = String(e);
      if (message.toLowerCase().includes("cancelled")) {
        setProcesses((prev) => dismissProcess(prev, processKey("delete", id)));
        loadInstances();
        return;
      }
      handleProcessFailed("delete", id, e);
      loadInstances();
    });
  };

  const handleCancelDelete = (id: string) => {
    void invoke("cancel_delete_instance", { id }).catch((e) => {
      setError(`Cancel delete failed: ${e}`);
    });
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
  const selectedDeleteProcess = selectedInstanceId
    ? getInstanceProcess(processes, "delete", selectedInstanceId)
    : undefined;
  const selectedUpdateProcess = selectedInstanceId
    ? getInstanceProcess(processes, "update-pack", selectedInstanceId)
    : undefined;
  const selectedReinstallProcess = selectedInstanceId
    ? getInstanceProcess(processes, "reinstall", selectedInstanceId)
    : undefined;
  const isDeletingSelected = selectedDeleteProcess?.status === "running";
  const isUpdatingSelected = selectedUpdateProcess?.status === "running";
  const isReinstallingSelected = selectedReinstallProcess?.status === "running";
  const selectedInstanceActive = selectedInstanceId
    ? isInstanceActive(selectedInstanceId, launching, runningInstanceIds)
    : false;
  const selectedInstanceRunning = selectedInstanceId
    ? runningInstanceIds.has(selectedInstanceId)
    : false;
  const selectedInstanceStarting = selectedInstanceId
    ? launching === selectedInstanceId
    : false;

  const formatUpdateProgress = (proc: BackgroundProcess) => {
    const stage = stageLabel(proc.stage);
    const progress = formatDownloadProgress(proc);
    return progress === `${(proc.pct * 100).toFixed(0)}%`
      ? `${stage} · ${progress}`
      : `${stage} · ${progress}`;
  };

  return (
    <div className="app-shell h-screen flex flex-col overflow-hidden">
      {/* Toolbar */}
      <header className="app-toolbar h-11 shrink-0 flex items-center px-3 gap-1.5">
        <div className="flex items-center gap-2 pr-1.5">
          <span className="brand-mark size-5 rounded-md" aria-hidden="true" />
          <span className="font-semibold text-sm tracking-tight">Industrialis</span>
        </div>
        <Button variant="default" size="sm" className="h-7" onClick={() => { setTab("instances"); setShowNewInstance(true); }}>
          <Plus className="size-3.5" /> Add
        </Button>
        <div className="w-px h-5 bg-border/80 mx-1" />
        <div className="inline-flex h-8 items-center rounded-lg border border-border/70 bg-muted/70 p-0.5 gap-0.5 shadow-inner">
          <Button variant={tab === "instances" ? "secondary" : "ghost"} size="sm" className="h-6 px-2" onClick={() => setTab("instances")}><Boxes className="size-3.5" /> Instances</Button>
          <Button
            variant={tab === "processes" ? "secondary" : "ghost"}
            size="sm"
            className="h-6 px-2"
            onClick={() => openProcesses()}
          >
            <Activity className="size-3.5" /> Processes
            {runningProcessCount(processes) > 0 && (
              <Badge variant="secondary" className="h-4 min-w-4 justify-center px-1 text-[10px]">
                {runningProcessCount(processes)}
              </Badge>
            )}
          </Button>
          <Button variant={tab === "settings" ? "secondary" : "ghost"} size="sm" className="h-6 px-2" onClick={() => setTab("settings")}><Settings className="size-3.5" /> Settings</Button>
          <Button variant={tab === "accounts" ? "secondary" : "ghost"} size="sm" className="h-6 px-2" onClick={() => setTab("accounts")}><Users className="size-3.5" /> Accounts</Button>
        </div>
        <div className="ml-auto flex items-center gap-0.5">
          <AccountSwitcher
            accounts={accounts}
            defaultAccountId={defaultAccountId}
            onSelectDefaultAccount={handleSetDefaultAccount}
            onManageAccounts={() => setTab("accounts")}
          />
          <ProcessesDropdown
            processes={processes}
            onDismiss={handleDismissProcess}
            onCancelDelete={handleCancelDelete}
            onOpenProcesses={openProcesses}
          />
          <ThemeSwitcher />
        </div>
      </header>

      {tab === "instances" ? (
        <div className="flex-1 flex overflow-hidden p-2 gap-2">
          {/* Instance list */}
          <div className="surface-panel w-[min(320px,35%)] shrink-0 overflow-auto flex flex-col rounded-lg border border-border/80 shadow-sm">
            {instances.length === 0 ? (
              <div className="m-2 rounded-lg border border-dashed border-border/80 bg-muted/30 p-4 text-sm">
                <div className="font-medium text-foreground">No instances installed</div>
                <p className="mt-1 text-xs text-muted-foreground">Add a pack instance to start building your launcher library.</p>
              </div>
            ) : (
              <InstanceGroupList
                instances={instances}
                groupsState={groupsState}
                sizesRefreshing={sizesRefreshing}
                onToggleCollapsed={handleToggleGroupCollapsed}
                onRenameGroup={handleRenameGroup}
                onDeleteGroup={handleDeleteGroup}
                selectedInstanceId={selectedInstanceId}
                onSelect={setSelectedInstanceId}
                launching={launching}
                runningInstanceIds={runningInstanceIds}
                onLaunch={handleLaunch}
                onKill={handleKill}
                onOpenSettings={handleOpenInstanceSettings}
                onOpenFolder={handleOpenInstanceFolder}
                onDelete={handleDelete}
                isInstanceBusy={instanceBusy}
                processes={processes}
                onCancelDelete={handleCancelDelete}
                versions={gtnhVersions}
                onUpdatePack={setUpdatePackInstanceId}
                onReinstall={setReinstallInstanceId}
                onIconChanged={loadInstances}
                onIconError={(message) => setError(`Icon update failed: ${message}`)}
              />
            )}
          </div>

          {/* Details panel */}
          <div className="surface-panel flex-1 flex flex-col overflow-hidden rounded-lg border border-border/80 shadow-sm">
            {sel ? (
              <>
                <Tabs value={detailTab} onValueChange={setDetailTab} className="flex-1 flex flex-col overflow-hidden">
                  <div className="detail-header shrink-0 px-4 py-3 flex items-center gap-3 min-h-16">
                    <InstanceAvatar
                      instanceId={sel.id}
                      name={instanceDisplayName(sel)}
                      iconPath={sel.icon_path}
                      size="md"
                      loading={isDeletingSelected || isUpdatingSelected || isReinstallingSelected}
                      onIconChanged={loadInstances}
                      onError={(message) => setError(`Icon update failed: ${message}`)}
                    />
                    <div className="min-w-0 flex-1">
                      <div className="text-base font-semibold truncate leading-tight">{instanceDisplayName(sel)}</div>
                      <div className="mt-0.5 text-xs text-muted-foreground truncate leading-tight">
                        {isDeletingSelected && selectedDeleteProcess ? (
                          <>Deleting... {(selectedDeleteProcess.pct * 100).toFixed(0)}%</>
                        ) : isReinstallingSelected && selectedReinstallProcess ? (
                          <>{formatUpdateProgress(selectedReinstallProcess)}</>
                        ) : isUpdatingSelected && selectedUpdateProcess ? (
                          <>{formatUpdateProgress(selectedUpdateProcess)}</>
                        ) : (
                          <span className="inline-flex items-center gap-2 min-w-0">
                            <span className="truncate">
                              {instancePackVersion(sel)} / {formatInstanceSize(sel.size_bytes, sizesRefreshing)}
                              {sel.group ? ` / ${sel.group}` : ""}
                            </span>
                            <PackVersionStatus
                              currentVersion={instancePackVersion(sel)}
                              versions={gtnhVersions}
                              onUpdate={() => setUpdatePackInstanceId(selectedInstanceId!)}
                              disabled={selectedInstanceActive || instanceBusy(selectedInstanceId!)}
                              compact
                            />
                          </span>
                        )}
                      </div>
                    </div>
                    <TabsList className="shrink-0 h-8 rounded-lg border border-border/70 bg-background/50">
                      <TabsTrigger value="info"><Info className="size-3 mr-0.5" />Info</TabsTrigger>
                      <TabsTrigger value="files"><Files className="size-3 mr-0.5" />Files</TabsTrigger>
                      <TabsTrigger value="mods"><Package className="size-3 mr-0.5" />Mods</TabsTrigger>
                      <TabsTrigger value="settings"><SlidersHorizontal className="size-3 mr-0.5" />Settings</TabsTrigger>
                      <TabsTrigger value="logs"><Terminal className="size-3 mr-0.5" />Logs</TabsTrigger>
                    </TabsList>
                  </div>

                  <TabsContent value="info" className="flex-1 overflow-auto px-4 pb-4 pt-3 mt-0 space-y-3">
                    <div className="flex items-center justify-between gap-3 rounded-lg border border-border/60 bg-card/45 px-3 py-2">
                      <PackVersionStatus
                        currentVersion={instancePackVersion(sel)}
                        versions={gtnhVersions}
                        onUpdate={() => setUpdatePackInstanceId(selectedInstanceId!)}
                        disabled={selectedInstanceActive || instanceBusy(selectedInstanceId!)}
                      />
                    </div>
                    <div className="flex items-center justify-between gap-3 rounded-lg border border-border/60 bg-card/45 px-3 py-2">
                      <div className="min-w-0">
                        <div className="text-sm font-medium">Clean reinstall</div>
                        <p className="text-xs text-muted-foreground mt-0.5 leading-snug">
                          Fresh pack install while keeping saves, JourneyMap, options, and launcher overlays.
                        </p>
                      </div>
                      <Button
                        variant="outline"
                        size="sm"
                        className="shrink-0"
                        disabled={selectedInstanceActive || instanceBusy(selectedInstanceId!)}
                        onClick={() => setReinstallInstanceId(selectedInstanceId!)}
                      >
                        <RefreshCw className="size-3.5" />
                        Reinstall
                      </Button>
                    </div>
                    <InfoGrid
                      items={[
                        { label: "Pack version", value: instancePackVersion(sel) },
                        { label: "Instance ID", value: sel.id },
                        { label: "Size", value: formatInstanceSize(sel.size_bytes, sizesRefreshing) },
                        { label: "Group", value: sel.group || "Ungrouped" },
                        { label: "Java", value: sel.settings.java_path || "Auto-detect" },
                        {
                          label: "RAM",
                          value: sel.settings.override_memory
                            ? `${sel.settings.min_ram_mb}-${sel.settings.max_ram_mb} MB`
                            : "Default (4096-6144 MB)",
                        },
                        ...(mergeInstanceSettings(sel.settings).show_game_time &&
                        (sel.settings.override_game_time || sel.settings.total_play_seconds > 0)
                          ? [{ label: "Play time", value: formatPlayTime(sel.settings.total_play_seconds) }]
                          : []),
                        {
                          label: "Account",
                          value: (() => {
                            const launchAccount = resolveLaunchAccount(
                              accounts,
                              defaultAccountId,
                              sel.settings,
                            );
                            if (launchAccount) {
                              const isOverride = mergeInstanceSettings(sel.settings).override_account;
                              const suffix = launchAccount.account_type === "offline" ? "offline" : "Microsoft";
                              return isOverride
                                ? `${accountDisplayName(launchAccount)} (${suffix}, instance override)`
                                : `${accountDisplayName(launchAccount)} (${suffix}, default)`;
                            }
                            return "No default account - set one in Accounts";
                          })(),
                        },
                      ]}
                    />
                  </TabsContent>

                  <TabsContent value="files" className="flex-1 overflow-auto px-4 pb-4 pt-3 mt-0">
                    <InstanceMinecraftEditor instanceId={selectedInstanceId!} />
                  </TabsContent>

                  <TabsContent value="mods" className="flex-1 overflow-auto px-4 pb-4 pt-3 mt-0">
                    <CustomModsPanel instanceId={selectedInstanceId!} />
                  </TabsContent>

                  <TabsContent value="settings" className="flex-1 overflow-auto px-4 pb-4 pt-3 mt-0">
                    <InstanceSettingsPanel
                      instanceId={selectedInstanceId!}
                      packVersion={instancePackVersion(sel)}
                      javaOptions={javaOptions}
                      accounts={accounts}
                      onOpenLauncherSettings={() => setTab("settings")}
                      onSave={(v, s) => handleSaveSettings(v, s)}
                    />
                  </TabsContent>

                  <TabsContent value="logs" className="flex-1 overflow-hidden flex flex-col mt-0">
                    <LogView
                      log={instanceLogs[selectedInstanceId!] ?? []}
                      onClear={() => handleClearConsole(selectedInstanceId!)}
                      disableClear={selectedInstanceActive}
                    />
                  </TabsContent>
                </Tabs>

                {/* Action bar */}
                <div className="shrink-0 border-t border-border/80 bg-card/60 px-4 py-3 flex items-center gap-2">
                  {isReinstallingSelected && selectedReinstallProcess ? (
                    <>
                      <div className="flex-1 min-w-0 space-y-1.5">
                        <div className="flex items-center gap-2 text-sm min-w-0">
                          <Loader2 className="size-4 shrink-0 animate-spin text-muted-foreground" />
                          <span className="truncate">{formatUpdateProgress(selectedReinstallProcess)}</span>
                        </div>
                        <Progress value={selectedReinstallProcess.pct * 100} className="h-1.5" />
                      </div>
                      <Button
                        variant="outline"
                        onClick={() => openProcesses(processKey("reinstall", selectedInstanceId!))}
                      >
                        <Activity className="size-3.5" />
                        View log
                      </Button>
                    </>
                  ) : isUpdatingSelected && selectedUpdateProcess ? (
                    <>
                      <div className="flex-1 min-w-0 space-y-1.5">
                        <div className="flex items-center gap-2 text-sm min-w-0">
                          <Loader2 className="size-4 shrink-0 animate-spin text-muted-foreground" />
                          <span className="truncate">{formatUpdateProgress(selectedUpdateProcess)}</span>
                        </div>
                        <Progress value={selectedUpdateProcess.pct * 100} className="h-1.5" />
                      </div>
                      <Button
                        variant="outline"
                        onClick={() => openProcesses(processKey("update-pack", selectedInstanceId!))}
                      >
                        <Activity className="size-3.5" />
                        View log
                      </Button>
                    </>
                  ) : isDeletingSelected && selectedDeleteProcess ? (
                    <>
                      <div className="flex-1 min-w-0 space-y-1.5">
                        <div className="flex items-center gap-2 text-sm">
                          <Loader2 className="size-4 shrink-0 animate-spin text-muted-foreground" />
                          <span>
                            Deleting... {(selectedDeleteProcess.pct * 100).toFixed(0)}%
                          </span>
                        </div>
                        <Progress value={selectedDeleteProcess.pct * 100} className="h-1.5" />
                      </div>
                      <Button
                        variant="outline"
                        onClick={() => handleCancelDelete(selectedInstanceId!)}
                      >
                        <X className="size-3.5" />
                        Cancel
                      </Button>
                    </>
                  ) : selectedInstanceRunning || selectedInstanceStarting ? (
                    <>
                      <Button
                        className="flex-1"
                        variant={selectedInstanceRunning ? "destructive" : "default"}
                        onClick={() => selectedInstanceRunning && handleKill(selectedInstanceId!)}
                        disabled={selectedInstanceStarting}
                      >
                        {selectedInstanceStarting ? (
                          <><Loader2 className="animate-spin" /> Launching...</>
                        ) : (
                          <><Square className="fill-current" /> Stop</>
                        )}
                      </Button>
                      <Button
                        variant="outline"
                        size="icon"
                        title="Update pack"
                        disabled={selectedInstanceActive || instanceBusy(selectedInstanceId!)}
                        onClick={() => setUpdatePackInstanceId(selectedInstanceId)}
                      >
                        <ArrowUpCircle />
                      </Button>
                      <Button variant="outline" size="icon" title="Change group" onClick={() => setChangeGroupInstanceId(selectedInstanceId)}>
                        <FolderInput />
                      </Button>
                      <Button
                        variant="outline"
                        size="icon"
                        title="Delete"
                        disabled={instanceBusy(selectedInstanceId!)}
                        onClick={() => handleDelete(selectedInstanceId!)}
                      >
                        <Trash2 />
                      </Button>
                    </>
                  ) : (
                    <>
                      <Button
                        className="flex-1"
                        onClick={() => handleLaunch(selectedInstanceId!)}
                        disabled={launching !== null || instanceBusy(selectedInstanceId!)}
                      >
                        <Play /> {launching ? "Busy" : "Launch"}
                      </Button>
                      <Button
                        variant="outline"
                        size="icon"
                        title="Update pack"
                        disabled={selectedInstanceActive || instanceBusy(selectedInstanceId!)}
                        onClick={() => setUpdatePackInstanceId(selectedInstanceId)}
                      >
                        <ArrowUpCircle />
                      </Button>
                      <Button variant="outline" size="icon" title="Change group" onClick={() => setChangeGroupInstanceId(selectedInstanceId)}>
                        <FolderInput />
                      </Button>
                      <Button
                        variant="outline"
                        size="icon"
                        title="Delete"
                        disabled={instanceBusy(selectedInstanceId!)}
                        onClick={() => handleDelete(selectedInstanceId!)}
                      >
                        <Trash2 />
                      </Button>
                    </>
                  )}
                </div>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center p-6">
                <div className="max-w-sm rounded-lg border border-dashed border-border/80 bg-muted/30 p-6 text-center">
                  <div className="mx-auto mb-3 instance-avatar size-11 rounded-lg flex items-center justify-center">
                    <Boxes className="size-5 text-muted-foreground" />
                  </div>
                  <div className="font-medium">Select an instance</div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    Pick a pack from the library to view files, mods, settings, and launch logs.
                  </p>
                </div>
              </div>
            )}
          </div>
        </div>
      ) : tab === "processes" ? (
        <ProcessesTab
          processes={processes}
          selectedKey={selectedProcessKey}
          onSelect={setSelectedProcessKey}
          onDismiss={handleDismissProcess}
          onCancelDelete={handleCancelDelete}
          onViewInstance={(id) => {
            setSelectedInstanceId(id);
            setTab("instances");
            setSelectedProcessKey(null);
          }}
        />
      ) : (
        <main className="flex-1 overflow-auto p-4 max-w-2xl">
          {tab === "settings" && <SettingsTab javaOptions={javaOptions} />}
          {tab === "accounts" && (
            <AccountsTab
              onAccountsChanged={loadAccounts}
              onSetDefaultAccount={handleSetDefaultAccount}
              defaultAccountId={defaultAccountId}
              launchRedirect={
                accountsLaunchRedirect
                  ? { instanceName: accountsLaunchRedirect.instanceName }
                  : null
              }
              onDismissRedirect={() => setAccountsLaunchRedirect(null)}
            />
          )}
        </main>
      )}

      {/* Status bar */}
      <footer className="h-6 shrink-0 border-t border-border/80 flex items-center px-3 gap-3 text-[11px] text-muted-foreground bg-card/80">
        <span>{instances.length} instance{instances.length === 1 ? "" : "s"}</span>
        {sel && <span className="truncate">{instanceDisplayName(sel)}</span>}
        {launching && <span>Launching {launching}…</span>}
        {runningInstanceIds.size > 0 && (
          <span>
            Running {Array.from(runningInstanceIds).join(", ")}
          </span>
        )}
        {runningProcessCount(processes) > 0 && (
          <span>{runningProcessCount(processes)} background process{runningProcessCount(processes) === 1 ? "" : "es"}</span>
        )}
      </footer>

      {updatePackInstanceId && (() => {
        const inst = instances.find((i) => i.id === updatePackInstanceId);
        if (!inst) return null;
        return (
          <UpdatePackDialog
            instanceId={updatePackInstanceId}
            instanceName={instanceDisplayName(inst)}
            currentPackVersion={instancePackVersion(inst)}
            defaultJavaType={inst.settings.pack_java_type || "java17+"}
            versions={gtnhVersions}
            onClose={() => setUpdatePackInstanceId(null)}
            onUpdate={(packVersion, javaType, overwritePackConfigs, keepModIdentities) => {
              startPackUpdate(
                updatePackInstanceId,
                instanceDisplayName(inst),
                packVersion,
                javaType,
                overwritePackConfigs,
                keepModIdentities,
              );
            }}
          />
        );
      })()}

      {reinstallInstanceId && (() => {
        const inst = instances.find((i) => i.id === reinstallInstanceId);
        if (!inst) return null;
        return (
          <ReinstallInstanceDialog
            instanceName={instanceDisplayName(inst)}
            currentPackVersion={instancePackVersion(inst)}
            defaultJavaType={inst.settings.pack_java_type || "java17+"}
            versions={gtnhVersions}
            onClose={() => setReinstallInstanceId(null)}
            onReinstall={(packVersion, javaType) => {
              startCleanReinstall(
                reinstallInstanceId,
                instanceDisplayName(inst),
                packVersion,
                javaType,
              );
            }}
          />
        );
      })()}

      {showNewInstance && (
        <NewInstanceDialog
          onClose={() => setShowNewInstance(false)}
          onInstall={(id, packVersion, javaType, group, name) => {
            setError(null);
            setShowNewInstance(false);
            registerProcess("install", id, name || `GTNH ${packVersion}`);
            void invoke("download_install", {
              id,
              packVersion,
              javaType,
              group: group || null,
              name: name || null,
            }).catch((e) => handleProcessFailed("install", id, e));
            if (group) setLastUsedGroup(group);
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

      <ConfirmDialog
        open={deleteInstanceConfirm !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteInstanceConfirm(null);
        }}
        title="Delete instance?"
        description={
          deleteInstanceConfirm
            ? `Delete "${deleteInstanceConfirm.name}"? This removes all instance files and cannot be undone.`
            : ""
        }
        confirmLabel="Delete"
        destructive
        onConfirm={confirmDeleteInstance}
      />

      {/* Error toast */}
      {error && (
        <div className="fixed bottom-10 right-4 bg-destructive text-destructive-foreground p-3 rounded shadow-lg max-w-sm z-50">
          <p className="text-sm">{error}</p>
          <Button size="sm" variant="ghost" className="mt-1" onClick={() => setError(null)}>Dismiss</Button>
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
  sizesRefreshing,
  onToggleCollapsed,
  onRenameGroup,
  onDeleteGroup,
  selectedInstanceId,
  onSelect,
  launching,
  runningInstanceIds,
  onLaunch,
  onKill,
  onOpenSettings,
  onOpenFolder,
  onDelete,
  isInstanceBusy,
  processes,
  onCancelDelete,
  versions,
  onUpdatePack,
  onReinstall,
  onIconChanged,
  onIconError,
}: {
  instances: InstanceInfo[];
  groupsState: InstanceGroupsState;
  sizesRefreshing: boolean;
  onToggleCollapsed: (group: string, collapsed: boolean) => void;
  onRenameGroup: (oldName: string, newName: string) => void;
  onDeleteGroup: (name: string) => void;
  selectedInstanceId: string | null;
  onSelect: (id: string) => void;
  launching: string | null;
  runningInstanceIds: Set<string>;
  onLaunch: (id: string) => void;
  onKill: (id: string) => void;
  onOpenSettings: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onDelete: (id: string) => void;
  isInstanceBusy: (id: string) => boolean;
  processes: Map<string, BackgroundProcess>;
  onCancelDelete: (id: string) => void;
  versions: Record<string, GtnhVersion> | null;
  onUpdatePack: (id: string) => void;
  onReinstall: (id: string) => void;
  onIconChanged: () => void;
  onIconError: (message: string) => void;
}) {
  const sections = useMemo(
    () => buildGroupSections(instances, groupsState.groups),
    [instances, groupsState.groups],
  );

  if (sections.length === 0) return null;

  return (
    <div className="space-y-2 p-2">
      {sections.map((section) => (
        <InstanceGroupSection
          key={section.id || "__ungrouped__"}
          section={section}
          collapsed={groupsState.collapsed[section.id] ?? false}
          sizesRefreshing={sizesRefreshing}
          onToggleCollapsed={(collapsed) => onToggleCollapsed(section.id, collapsed)}
          onRenameGroup={section.id ? onRenameGroup : undefined}
          onDeleteGroup={section.id ? onDeleteGroup : undefined}
          selectedInstanceId={selectedInstanceId}
          onSelect={onSelect}
          launching={launching}
          runningInstanceIds={runningInstanceIds}
          onLaunch={onLaunch}
          onKill={onKill}
          onOpenSettings={onOpenSettings}
          onOpenFolder={onOpenFolder}
          onDelete={onDelete}
          isInstanceBusy={isInstanceBusy}
          processes={processes}
          onCancelDelete={onCancelDelete}
          versions={versions}
          onUpdatePack={onUpdatePack}
          onReinstall={onReinstall}
          onIconChanged={onIconChanged}
          onIconError={onIconError}
        />
      ))}
    </div>
  );
}

function InstanceGroupSection({
  section,
  collapsed,
  sizesRefreshing,
  onToggleCollapsed,
  onRenameGroup,
  onDeleteGroup,
  selectedInstanceId,
  onSelect,
  launching,
  runningInstanceIds,
  onLaunch,
  onKill,
  onOpenSettings,
  onOpenFolder,
  onDelete,
  isInstanceBusy,
  processes,
  onCancelDelete,
  versions,
  onUpdatePack,
  onReinstall,
  onIconChanged,
  onIconError,
}: {
  section: GroupSection;
  collapsed: boolean;
  sizesRefreshing: boolean;
  onToggleCollapsed: (collapsed: boolean) => void;
  onRenameGroup?: (oldName: string, newName: string) => void;
  onDeleteGroup?: (name: string) => void;
  selectedInstanceId: string | null;
  onSelect: (id: string) => void;
  launching: string | null;
  runningInstanceIds: Set<string>;
  onLaunch: (id: string) => void;
  onKill: (id: string) => void;
  onOpenSettings: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onDelete: (id: string) => void;
  isInstanceBusy: (id: string) => boolean;
  processes: Map<string, BackgroundProcess>;
  onCancelDelete: (id: string) => void;
  versions: Record<string, GtnhVersion> | null;
  onUpdatePack: (id: string) => void;
  onReinstall: (id: string) => void;
  onIconChanged: () => void;
  onIconError: (message: string) => void;
}) {
  const [renaming, setRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState("");
  const [deleteGroupOpen, setDeleteGroupOpen] = useState(false);

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

  const confirmDeleteGroup = () => {
    if (!onDeleteGroup || !section.id) return;
    onDeleteGroup(section.id);
  };

  return (
    <section className="space-y-1">
      <div className="flex items-center gap-1.5 px-1 py-1 group/header sticky top-0 z-10 bg-card/90 backdrop-blur">
        <Button
          type="button"
          variant="ghost"
          onClick={() => onToggleCollapsed(!collapsed)}
          className="flex h-7 flex-1 min-w-0 items-center gap-2 justify-start rounded-md px-2 py-1 font-normal hover:text-foreground"
          aria-expanded={!collapsed}
        >
          {collapsed ? (
            <ChevronRight className="size-3.5 text-muted-foreground" />
          ) : (
            <ChevronDown className="size-3.5 text-muted-foreground" />
          )}
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
            <h2 className="text-[11px] font-semibold uppercase truncate text-muted-foreground">{section.label}</h2>
          )}
          <Badge variant="secondary" className="shrink-0 h-5 rounded-md">
            {section.items.length}
          </Badge>
        </Button>
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
              onClick={() => setDeleteGroupOpen(true)}
            >
              Delete
            </Button>
          </div>
        )}
      </div>
      <ConfirmDialog
        open={deleteGroupOpen}
        onOpenChange={setDeleteGroupOpen}
        title="Delete group?"
        description={`Delete group "${section.label}"? Instances will be moved to Ungrouped.`}
        confirmLabel="Delete"
        destructive
        onConfirm={confirmDeleteGroup}
      />
      {!collapsed && section.items.map((inst) => (
        <InstanceRow
          key={inst.id}
          inst={inst}
          selected={selectedInstanceId === inst.id}
          sizesRefreshing={sizesRefreshing}
          onSelect={() => onSelect(inst.id)}
          running={runningInstanceIds.has(inst.id)}
          starting={launching === inst.id}
          busy={launching !== null}
          onLaunch={() => onLaunch(inst.id)}
          onKill={() => onKill(inst.id)}
          onOpenSettings={() => onOpenSettings(inst.id)}
          onOpenFolder={() => onOpenFolder(inst.id)}
          onDelete={() => onDelete(inst.id)}
          isInstanceBusy={isInstanceBusy}
          deleteProcess={getInstanceProcess(processes, "delete", inst.id)}
          updateProcess={getInstanceProcess(processes, "update-pack", inst.id)}
          reinstallProcess={getInstanceProcess(processes, "reinstall", inst.id)}
          onCancelDelete={() => onCancelDelete(inst.id)}
          versions={versions}
          onUpdatePack={() => onUpdatePack(inst.id)}
          onReinstall={() => onReinstall(inst.id)}
          onIconChanged={onIconChanged}
          onIconError={onIconError}
        />
      ))}
    </section>
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
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="max-w-sm p-0">
        <Card className="border-0 shadow-none">
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle>Change Group</CardTitle>
              <Button variant="ghost" size="icon" className="size-7" onClick={onClose} aria-label="Close">
                <X className="size-3.5" />
              </Button>
            </div>
            <CardDescription>{instanceName}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="change-group-input">Group</Label>
              <Input
                id="change-group-input"
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
              <p className="text-xs text-muted-foreground">
                Pick an existing group or type a new name. Leave empty for Ungrouped.
              </p>
            </div>
            <div className="flex gap-2">
              <Button className="flex-1" onClick={() => onSave(group.trim())}>Save</Button>
              <Button variant="outline" onClick={onClose}>Cancel</Button>
            </div>
          </CardContent>
        </Card>
      </DialogContent>
    </Dialog>
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
  const listId = `${id}-list`;

  return (
    <div className="space-y-2">
      <Label htmlFor={id}>Group</Label>
      <Input
        id={id}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="No group"
        list={listId}
      />
      <datalist id={listId}>
        {existingGroups.map((g) => (
          <option key={g} value={g} />
        ))}
      </datalist>
    </div>
  );
}

// ── Instance Row ──

function formatRowUpdateProgress(proc: BackgroundProcess): string {
  const progress = formatDownloadProgress(proc);
  return `${stageLabel(proc.stage)} · ${progress}`;
}

function InstanceRow({
  inst,
  selected,
  sizesRefreshing,
  onSelect,
  running,
  starting,
  busy,
  onLaunch,
  onKill,
  onOpenSettings,
  onOpenFolder,
  onDelete,
  isInstanceBusy,
  deleteProcess,
  updateProcess,
  reinstallProcess,
  onCancelDelete,
  versions,
  onUpdatePack,
  onReinstall,
  onIconChanged,
  onIconError,
}: {
  inst: InstanceInfo;
  selected: boolean;
  sizesRefreshing: boolean;
  onSelect: () => void;
  running: boolean;
  starting: boolean;
  busy: boolean;
  onLaunch: () => void;
  onKill: () => void;
  onOpenSettings: () => void;
  onOpenFolder: () => void;
  onDelete: () => void;
  isInstanceBusy: (id: string) => boolean;
  deleteProcess?: BackgroundProcess;
  updateProcess?: BackgroundProcess;
  reinstallProcess?: BackgroundProcess;
  onCancelDelete: () => void;
  versions: Record<string, GtnhVersion> | null;
  onUpdatePack: () => void;
  onReinstall: () => void;
  onIconChanged: () => void;
  onIconError: (message: string) => void;
}) {
  const name = instanceDisplayName(inst);
  const deleting = deleteProcess?.status === "running";
  const updating = updateProcess?.status === "running";
  const reinstalling = reinstallProcess?.status === "running";
  const packBusy = deleting || updating || reinstalling;
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          className={`group/row flex flex-col rounded-lg border transition-colors ${
            packBusy ? "opacity-80" : ""
          } ${selected ? "instance-row-selected border-primary/40" : "border-transparent bg-card/25 hover:border-primary/30 hover:bg-primary/10"}`}
        >
          <div className="flex items-center gap-1 pl-1 pr-1">
            <InstanceAvatar
              instanceId={inst.id}
              name={name}
              iconPath={inst.icon_path}
              loading={packBusy}
              onIconChanged={onIconChanged}
              onError={onIconError}
            />
            <Button
              type="button"
              variant="ghost"
              onClick={onSelect}
              className="h-auto min-w-0 flex-1 items-center justify-start gap-2 rounded-lg px-2 py-2 font-normal hover:bg-transparent"
            >
              <div className="min-w-0 flex-1">
                <div className="text-xs font-semibold truncate leading-tight">{name}</div>
                <div className="text-[11px] text-muted-foreground truncate leading-tight">
                  {deleting ? (
                    <>Deleting... {(deleteProcess.pct * 100).toFixed(0)}%</>
                  ) : updating ? (
                    <>{formatRowUpdateProgress(updateProcess)}</>
                  ) : reinstalling ? (
                    <>{formatRowUpdateProgress(reinstallProcess!)}</>
                  ) : (
                    <>{instancePackVersion(inst)} / {formatInstanceSize(inst.size_bytes, sizesRefreshing)}</>
                  )}
                </div>
              </div>
              {running && !packBusy && (
                <span
                  className="status-running size-2 rounded-full animate-pulse shrink-0"
                  title="Running"
                />
              )}
              {starting && !running && !packBusy && (
                <span title="Launching" className="shrink-0">
                  <Loader2 className="size-2.5 animate-spin text-muted-foreground" />
                </span>
              )}
              {!packBusy && (
                <PackVersionStatus
                  currentVersion={instancePackVersion(inst)}
                  versions={versions}
                  onUpdate={onUpdatePack}
                  disabled={busy || running || starting || isInstanceBusy(inst.id)}
                  compact
                />
              )}
            </Button>
            <div className="flex items-center gap-0 pr-1 shrink-0">
              {deleting ? (
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-6 text-destructive hover:text-destructive"
                  title="Cancel deletion"
                  aria-label={`Cancel deletion of ${name}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    onCancelDelete();
                  }}
                >
                  <X className="size-3.5" />
                </Button>
              ) : running ? (
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-6 text-destructive hover:text-destructive"
                  title="Stop"
                  aria-label={`Stop ${name}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    onKill();
                  }}
                >
                  <Square className="size-3.5 fill-current" />
                </Button>
              ) : starting ? (
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-6"
                  title="Launching"
                  aria-label={`Launching ${name}`}
                  disabled
                >
                  <Loader2 className="size-3.5 animate-spin" />
                </Button>
              ) : (
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-6"
                  title={busy ? "Another instance is launching" : "Launch"}
                  aria-label={`Launch ${name}`}
                  disabled={busy || isInstanceBusy(inst.id)}
                  onClick={(e) => {
                    e.stopPropagation();
                    onLaunch();
                  }}
                >
                  <Play className="size-3.5" />
                </Button>
              )}
              {!deleting && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-6"
                  title="Instance settings"
                  aria-label={`Settings for ${name}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    onOpenSettings();
                  }}
                >
                  <SlidersHorizontal className="size-3.5" />
                </Button>
              )}
            </div>
          </div>
          {deleting && (
            <div className="px-2 pb-1.5">
              <Progress value={deleteProcess.pct * 100} className="h-1" />
            </div>
          )}
          {updating && (
            <div className="px-2 pb-1.5">
              <Progress value={updateProcess.pct * 100} className="h-1" />
            </div>
          )}
          {reinstalling && (
            <div className="px-2 pb-1.5">
              <Progress value={reinstallProcess!.pct * 100} className="h-1" />
            </div>
          )}
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
        {deleting ? (
          <ContextMenuItem onSelect={onCancelDelete} className="text-destructive focus:text-destructive">
            <X />
            Cancel deletion
          </ContextMenuItem>
        ) : running ? (
          <ContextMenuItem onSelect={onKill} className="text-destructive focus:text-destructive">
            <Square className="fill-current" />
            Stop
          </ContextMenuItem>
        ) : (
          <ContextMenuItem onSelect={onLaunch} disabled={busy || starting || isInstanceBusy(inst.id)}>
            <Play />
            {starting ? "Launching…" : "Launch"}
          </ContextMenuItem>
        )}
        <ContextMenuSeparator />
        <ContextMenuItem
          onSelect={onReinstall}
          disabled={running || starting || packBusy || isInstanceBusy(inst.id)}
        >
          <RefreshCw />
          Clean reinstall…
        </ContextMenuItem>
        <ContextMenuItem
          onSelect={onDelete}
          disabled={running || starting || deleting || isInstanceBusy(inst.id)}
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

function InfoGrid({ items }: { items: { label: string; value: string }[] }) {
  return (
    <div className="grid sm:grid-cols-2 gap-2 pt-1">
      {items.map((item) => (
        <div
          key={item.label}
          className="flex min-w-0 justify-between gap-3 rounded-lg border border-border/60 bg-card/45 px-3 py-2 text-xs"
        >
          <span className="text-muted-foreground shrink-0">{item.label}</span>
          <span className="font-medium text-right truncate">{item.value}</span>
        </div>
      ))}
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
      <div className="flex gap-0.5 px-3 pb-0.5 shrink-0">
        <Button size="sm" variant="ghost" onClick={copy} disabled={log.length === 0}>
          {copied ? "Copied" : "Copy"}
        </Button>
        <Button size="sm" variant="ghost" onClick={onClear} disabled={disableClear || log.length === 0}>
          Clear
        </Button>
      </div>
      <ScrollArea ref={ref} className="flex-1 bg-black/60 px-3 py-2 font-mono text-[11px] leading-relaxed">
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
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="flex max-h-[85vh] max-w-lg flex-col overflow-hidden p-0">
        <Card className="flex max-h-[85vh] flex-col overflow-hidden border-0 shadow-none">
        <CardHeader className="shrink-0 pb-4">
          <div className="flex items-center justify-between">
            <CardTitle>New Instance</CardTitle>
            <Button variant="ghost" size="icon" className="size-7" onClick={onClose} aria-label="Close">
              <X className="size-3.5" />
            </Button>
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
                <Button
                  key={key}
                  type="button"
                  variant={sel === key ? "secondary" : "outline"}
                  className="h-auto w-full justify-between p-3 text-left font-normal"
                  onClick={() => setSel(key)}
                >
                  <div>
                    <div className="font-medium">{key}</div>
                    <div className="text-xs text-muted-foreground">
                      {v.releaseDate} / Max Java {v.maxJavaVersion}
                    </div>
                  </div>
                  <Badge variant={v.title === "Stable release" ? "success" : "warning"}>
                    {v.title === "Stable release" ? "Stable" : "Beta"}
                  </Badge>
                </Button>
              ))}
            </div>
          </ScrollArea>

          <div className="shrink-0 space-y-4 border-t border-border pt-4">
            <div className="space-y-2">
              <Label htmlFor="new-instance-name">Instance name</Label>
              <Input
                id="new-instance-name"
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
      </DialogContent>
    </Dialog>
  );
}

// ── Settings Tab ──

function SettingsTab({ javaOptions }: { javaOptions: JavaInfo[] }) {
  return (
    <div className="space-y-4">
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
