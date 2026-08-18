import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { desktopPlatform, invoke, openUrl } from "./lib/desktop";
import {
  Plus,
  Settings,
  Users,
  Boxes,
  Play,
  Square,
  Trash2,
  FolderInput,
  Info,
  Terminal,
  SlidersHorizontal,
  ArrowUpCircle,
  Files,
  Package,
  Loader2,
  X,
  Activity,
  ChevronDown,
  ChevronRight,
  RefreshCw,
  Copy,
  ExternalLink,
  Pencil,
} from "lucide-react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "./components/ui/card";
import { Input } from "./components/ui/input";
import { Badge } from "./components/ui/badge";
import { Progress } from "./components/ui/progress";
import { Select } from "./components/ui/select";
import { ScrollArea } from "./components/ui/scroll-area";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "./components/ui/tabs";
import { ThemePresetPicker } from "./components/ThemePresetPicker";
import { ThemeSwitcher } from "./components/ThemeSwitcher";
import { AccountSwitcher } from "./components/AccountSwitcher";
import { AccountsTab } from "./components/AccountsTab";
import { useLauncherSettings } from "./context/launcher-settings-context";
import { resolveDefaultAccountId } from "./lib/launcher-settings";
import { ProcessesDropdown } from "./components/ProcessesDropdown";
import { ProcessesTab } from "./components/ProcessesTab";
import {
  formatDownloadProgress,
  getInstanceProcess,
  isInstanceBusy,
  processKey,
  runningProcessCount,
  sortedProcesses,
  stageLabel,
  type BackgroundProcess,
} from "./lib/background-processes";
import { formatLaunchLog, type LaunchLogLine } from "./lib/launch-log";
import { formatPlayTime, mergeInstanceSettings, type InstanceSettings } from "./lib/instance-settings";
import { InstanceSettingsPanel } from "./components/InstanceSettingsPanel";
import { InstanceMinecraftEditor } from "./components/InstanceMinecraftEditor";
import { CustomModsPanel } from "./components/CustomModsPanel";
import { UpdatePackDialog } from "./components/UpdatePackDialog";
import { ReinstallInstanceDialog } from "./components/ReinstallInstanceDialog";
import { PackVersionStatus } from "./components/PackVersionStatus";
import { InstanceAvatar } from "./components/InstanceAvatar";
import { InstanceGridCard, type InstanceGridCardCommands } from "./components/InstanceGridCard";
import { LauncherUpdateDialog } from "./components/LauncherUpdateDialog";
import { VirtualizedLogList } from "./components/VirtualizedLogList";
import { WindowControls } from "./components/WindowControls";
import { JavaInstallationPicker } from "./components/JavaInstallationPicker";
import { compareVersionsByReleaseDate } from "./lib/pack-version-status";
import { validateAndSelectJava } from "./lib/java-selection";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { Dialog, DialogContent } from "./components/ui/dialog";
import { Label } from "./components/ui/label";
import { MAX_RETAINED_LOG_LINES } from "./lib/log-buffer";
import { cn } from "./lib/utils";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger } from "./components/ui/context-menu";
import { useLauncherStore, type GtnhVersion, type InstanceInfo, type LauncherAccount } from "./stores/launcher-store";
import { useLauncherSession } from "./lib/launcher-session";
import type { JavaInfo } from "./lib/java-installations";
import "./App.css";

const GITHUB_URL = "https://github.com/SergioJuniorCE/industrialis-launcher";

const PRIMARY_NAV_TABS = [
  { key: "instances", label: "Instances", Icon: Boxes },
  { key: "processes", label: "Processes", Icon: Activity },
  { key: "accounts", label: "Accounts", Icon: Users },
] as const;

// ── Types ──

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

function orderInstancesInGroup(items: InstanceInfo[], groupKey: string, instanceOrder: Record<string, string[]>): InstanceInfo[] {
  const order = instanceOrder[groupKey] ?? [];
  const byId = new Map(items.map((inst) => [inst.id, inst]));
  const result: InstanceInfo[] = [];
  const seen = new Set<string>();
  for (const id of order) {
    if (seen.has(id)) continue;
    const inst = byId.get(id);
    if (inst) {
      result.push(inst);
      seen.add(id);
    }
  }
  for (const inst of items) {
    if (!seen.has(inst.id)) result.push(inst);
  }
  return result;
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

function accountDisplayName(account: LauncherAccount): string {
  if (account.username.trim()) return account.username;
  return account.account_type === "offline" ? "Offline account" : "Microsoft account";
}

function isInstanceActive(id: string, launching: string | null, runningInstanceIds: Set<string>): boolean {
  return launching === id || runningInstanceIds.has(id);
}

function resolveLaunchAccount(
  accountList: LauncherAccount[],
  defaultAccountId: string | null | undefined,
  instanceSettings: InstanceSettings,
): LauncherAccount | null {
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

function formatUpdateProgress(proc: BackgroundProcess): string {
  return `${stageLabel(proc.stage)} · ${formatDownloadProgress(proc)}`;
}

// eslint-disable-next-line react-doctor/no-giant-component -- Desktop orchestration remains centralized while tab views are extracted incrementally.
export default function App() {
  const { settings: launcherSettings, loaded: launcherSettingsLoaded, updateSettings, saveSettingsNow } = useLauncherSettings();
  const defaultAccountId = resolveDefaultAccountId(launcherSettings);
  const { error, javaOptions, javaRefreshing, instanceLogs, launcherUpdate, session } = useLauncherSession();
  const {
    tab,
    selectedProcessKey,
    selectedInstanceId,
    showNewInstance,
    detailTab,
    instances,
    sizesRefreshing,
    processes,
    launching,
    runningInstanceIds,
    groupsState,
    accounts,
    accountsLoaded,
    gtnhVersions,
    updatePackInstanceId,
    reinstallInstanceId,
    copyInstanceId,
    changeGroupInstanceId,
    renameInstanceId,
    setTab,
    setSelectedProcessKey,
    setSelectedInstanceId,
    setShowNewInstance,
    setDetailTab,
    setInstances,
    setGroupsState,
    setAccounts,
    setUpdatePackInstanceId,
    setReinstallInstanceId,
    setCopyInstanceId,
    setChangeGroupInstanceId,
    setRenameInstanceId,
  } = useLauncherStore();
  const setError = session.setError;
  const loadGroups = session.loadGroups;
  const loadInstances = session.loadInstances;
  const refreshJava = session.refreshJava;
  const registerProcess = session.startProcess;
  const handleProcessFailed = session.failProcess;
  const handleKill = session.kill;
  const [notice, setNotice] = useState<string | null>(null);
  const [lastUsedGroup, setLastUsedGroup] = useState("");
  const [accountsLaunchRedirect, setAccountsLaunchRedirect] = useState<{
    instanceId: string;
    instanceName: string;
  } | null>(null);
  const [deleteInstanceConfirm, setDeleteInstanceConfirm] = useState<{
    id: string;
    name: string;
  } | null>(null);
  const installLauncherUpdate = useCallback(() => {
    void session.installLauncherUpdate();
  }, [session]);

  const retryLauncherUpdate = useCallback(() => {
    void session.retryLauncherUpdate();
  }, [session]);

  useEffect(() => {
    if (!launcherSettingsLoaded || !accountsLoaded) return;
    if (defaultAccountId && !accounts.some((a) => a.id === defaultAccountId)) {
      updateSettings({ default_account_id: null });
      void saveSettingsNow();
      return;
    }
    if (!defaultAccountId && accounts.length === 1) {
      updateSettings({ default_account_id: accounts[0].id });
      void saveSettingsNow();
    }
  }, [accounts, accountsLoaded, defaultAccountId, launcherSettingsLoaded, updateSettings, saveSettingsNow]);

  const handleSetDefaultAccount = useCallback(
    (id: string | null) => {
      updateSettings({ default_account_id: id });
      void saveSettingsNow();
    },
    [updateSettings, saveSettingsNow],
  );

  const handleSetDefaultJava = useCallback(
    (path: string | null) => {
      updateSettings({ default_java_path: path });
      void saveSettingsNow();
    },
    [updateSettings, saveSettingsNow],
  );

  const startPackUpdate = useCallback(
    (id: string, name: string, packVersion: string, javaType: string, keepModIdentities: string[]) => {
      const key = processKey("update-pack", id);
      setError(null);
      setNotice(`${name} is updating in the background. Follow its progress in Processes.`);
      setUpdatePackInstanceId(null);
      registerProcess("update-pack", id, name, `Preparing pack update to ${packVersion}...`);
      setTab("processes");
      setSelectedProcessKey(key);
      void invoke("update_instance", {
        id,
        packVersion,
        javaType,
        keepModIdentities,
      }).catch((e) => handleProcessFailed("update-pack", id, e));
    },
    [handleProcessFailed, registerProcess, setError, setNotice, setSelectedProcessKey, setTab, setUpdatePackInstanceId],
  );

  const startCleanReinstall = useCallback(
    (id: string, name: string, packVersion: string, javaType: string) => {
      const key = processKey("reinstall", id);
      setError(null);
      setReinstallInstanceId(null);
      registerProcess("reinstall", id, name, `Starting clean reinstall to ${packVersion}…`);
      setTab("processes");
      setSelectedProcessKey(key);
      void invoke("reinstall_instance", {
        id,
        packVersion,
        javaType,
      }).catch((e) => handleProcessFailed("reinstall", id, e));
    },
    [handleProcessFailed, registerProcess, setError, setReinstallInstanceId, setSelectedProcessKey, setTab],
  );

  const handleDismissProcess = useCallback(
    (key: string) => {
      session.dismissProcess(key);
      setSelectedProcessKey((current) => (current === key ? null : current));
    },
    [session, setSelectedProcessKey],
  );

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
    [processes, setSelectedProcessKey, setTab],
  );

  const instanceBusy = useCallback((id: string) => isInstanceBusy(processes, id), [processes]);

  useEffect(() => {
    if (selectedInstanceId) void session.loadLogs(selectedInstanceId);
  }, [selectedInstanceId, session]);

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

  const handleReorderInstances = async (group: string, order: string[]) => {
    setGroupsState((prev) => ({
      ...prev,
      instance_order: { ...prev.instance_order, [group]: order },
    }));
    try {
      await invoke("set_group_instance_order", { group, order });
      loadGroups();
    } catch (e) {
      loadGroups();
      setError(`Reorder failed: ${e}`);
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

  const handleLaunch = async (id: string) => {
    if (useLauncherStore.getState().launching !== null) return;

    let accountList = accounts;
    try {
      accountList = await invoke<LauncherAccount[]>("get_accounts");
      setAccounts(accountList);
    } catch {
      accountList = accounts;
    }

    const inst = instances.find((i) => i.id === id);
    if (!inst) return;

    const launchAccount = resolveLaunchAccount(accountList, resolveDefaultAccountId(launcherSettings), inst.settings);
    if (!launchAccount) {
      setAccountsLaunchRedirect({
        instanceId: id,
        instanceName: instanceDisplayName(inst),
      });
      setTab("accounts");
      return;
    }

    await session.launch(id, mergeInstanceSettings(inst.settings));
  };

  const handleOpenInstanceFolder = async (id: string) => {
    try {
      await invoke("open_instance_folder", { id });
    } catch (e) {
      setError(`Open folder failed: ${e}`);
    }
  };

  const handleClearConsole = async (id: string) => {
    await session.clearConsole(id);
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
        session.dismissProcess(processKey("delete", id));
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

  const handleCopyInstance = (sourceId: string, newId: string, newName: string) => {
    setError(null);
    setCopyInstanceId(null);
    registerProcess("copy", newId, newName);
    void invoke("copy_instance", {
      sourceId,
      newId,
      newName,
    }).catch((e) => handleProcessFailed("copy", newId, e));
  };

  const handleSaveSettings = useCallback(
    async (id: string, settings: InstanceSettings) => {
      setInstances((current) => current.map((instance) => (instance.id === id ? { ...instance, settings } : instance)));
      try {
        await invoke("save_settings", { id, settings });
      } catch (e) {
        setError(`Save failed: ${e}`);
      }
    },
    [setError, setInstances],
  );

  const handleRenameInstance = async (id: string, newName: string) => {
    const inst = instances.find((i) => i.id === id);
    if (!inst) return;
    const trimmed = newName.trim();
    if (trimmed === (inst.settings.name ?? "")) {
      setRenameInstanceId(null);
      return;
    }
    try {
      await invoke("save_settings", {
        id,
        settings: { ...mergeInstanceSettings(inst.settings), name: trimmed },
      });
      loadInstances();
      setRenameInstanceId(null);
    } catch (e) {
      setError(`Rename failed: ${e}`);
    }
  };

  const sel = instances.find((i) => i.id === selectedInstanceId) ?? null;
  const selectedDeleteProcess = selectedInstanceId ? getInstanceProcess(processes, "delete", selectedInstanceId) : undefined;
  const selectedUpdateProcess = selectedInstanceId ? getInstanceProcess(processes, "update-pack", selectedInstanceId) : undefined;
  const selectedReinstallProcess = selectedInstanceId ? getInstanceProcess(processes, "reinstall", selectedInstanceId) : undefined;
  const isDeletingSelected = selectedDeleteProcess?.status === "running";
  const isUpdatingSelected = selectedUpdateProcess?.status === "running";
  const isReinstallingSelected = selectedReinstallProcess?.status === "running";
  const selectedInstanceActive = selectedInstanceId ? isInstanceActive(selectedInstanceId, launching, runningInstanceIds) : false;
  const selectedInstanceRunning = selectedInstanceId ? runningInstanceIds.has(selectedInstanceId) : false;
  const selectedInstanceStarting = selectedInstanceId ? launching === selectedInstanceId : false;

  return (
    <div className={cn("app-shell h-screen flex flex-col overflow-hidden", desktopPlatform() === "darwin" && "app-shell-macos")}>
      {/* Toolbar */}
      <header className="app-toolbar h-11 shrink-0 flex items-center px-3 gap-1.5">
        <div className="flex items-center gap-2 pr-1.5">
          <span className="brand-mark size-5 rounded-md" aria-hidden="true" />
          <span className="toolbar-brand-name font-semibold text-sm tracking-tight">Industrialis</span>
        </div>
        <Button
          variant="default"
          size="sm"
          className="h-7"
          aria-label="Add instance"
          title="Add instance"
          onClick={() => {
            setTab("instances");
            setShowNewInstance(true);
          }}
        >
          <Plus className="size-3.5" /> <span className="toolbar-label">Add</span>
        </Button>
        <div className="w-px h-5 bg-border/80 mx-1" />
        <div className="primary-nav inline-flex h-8 items-center rounded-lg border border-border/70 bg-muted/70 p-0.5 gap-0.5 shadow-inner">
          {PRIMARY_NAV_TABS.map(({ key, label, Icon }) => (
            <Button
              key={key}
              variant={tab === key ? "secondary" : "ghost"}
              size="sm"
              className="primary-nav-button h-6 px-2"
              data-active={tab === key}
              aria-label={label}
              title={label}
              onClick={() => (key === "processes" ? openProcesses() : setTab(key))}
            >
              <Icon className="size-3.5" /> <span className="toolbar-label">{label}</span>
              {key === "processes" && runningProcessCount(processes) > 0 && (
                <Badge variant="secondary" className="h-4 min-w-4 justify-center px-1 text-[10px]">
                  {runningProcessCount(processes)}
                </Badge>
              )}
            </Button>
          ))}
        </div>
        <div className="app-toolbar-actions ml-auto flex items-center gap-0.5">
          <AccountSwitcher
            accounts={accounts}
            defaultAccountId={defaultAccountId}
            onSelectDefaultAccount={handleSetDefaultAccount}
            onManageAccounts={() => setTab("accounts")}
          />
          <ProcessesDropdown processes={processes} onDismiss={handleDismissProcess} onCancelDelete={handleCancelDelete} onOpenProcesses={openProcesses} />
          <Button
            variant={tab === "settings" ? "secondary" : "ghost"}
            size="icon"
            className="size-7"
            data-active={tab === "settings"}
            aria-label="Settings"
            title="Settings"
            onClick={() => setTab("settings")}
          >
            <Settings className="size-4" />
          </Button>
          <ThemeSwitcher />
        </div>
        <WindowControls />
      </header>

      {tab === "instances" ? (
        <div className="instance-workspace min-h-0 flex-1 flex overflow-hidden p-2 gap-2">
          {/* Instance list */}
          <div className="surface-panel workspace-panel workspace-panel-library min-h-0 flex-[1.15] min-w-[300px] max-w-[58%] shrink-0 overflow-hidden flex flex-col rounded-lg border border-border/80 shadow-sm">
            {instances.length === 0 ? (
              <div className="empty-state m-2 flex-1 rounded-lg border border-dashed border-border/80 bg-muted/30 p-4 text-sm">
                <div className="font-medium text-foreground">No instances installed</div>
                <p className="mt-1 text-xs text-muted-foreground">Add a pack instance to start building your launcher library.</p>
              </div>
            ) : (
              <InstanceGroupList
                commands={{
                  launch: handleLaunch,
                  kill: handleKill,
                  openFolder: handleOpenInstanceFolder,
                  delete: handleDelete,
                  cancelDelete: handleCancelDelete,
                  iconChanged: loadInstances,
                  iconError: (message) => setError(`Icon update failed: ${message}`),
                  toggleGroupCollapsed: handleToggleGroupCollapsed,
                  renameGroup: handleRenameGroup,
                  deleteGroup: handleDeleteGroup,
                  reorderInstances: handleReorderInstances,
                }}
              />
            )}
          </div>

          {/* Details panel */}
          <div className="surface-panel workspace-panel flex-1 flex flex-col overflow-hidden rounded-lg border border-border/80 shadow-sm">
            {sel ? (
              <>
                <Tabs value={detailTab} onValueChange={setDetailTab} className="flex-1 flex flex-col overflow-hidden">
                  <div className="detail-header shrink-0 px-4 py-3 flex flex-col gap-2 min-h-16">
                    <div className="flex items-center gap-3 min-w-0">
                      <InstanceAvatar
                        instanceId={sel.id}
                        name={instanceDisplayName(sel)}
                        iconPath={sel.icon_path}
                        size="md"
                        loading={isDeletingSelected || isUpdatingSelected || isReinstallingSelected}
                        onIconChanged={loadInstances}
                        onError={(message) => setError(`Icon update failed: ${message}`)}
                        onOpenFolder={() => handleOpenInstanceFolder(sel.id)}
                      />
                      <div className="min-w-0 flex-1">
                        <div className="text-base font-semibold leading-tight break-words">{instanceDisplayName(sel)}</div>
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
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7 shrink-0"
                        title="Close instance details"
                        aria-label="Close instance details"
                        onClick={() => setSelectedInstanceId(null)}
                      >
                        <X />
                      </Button>
                    </div>
                    <TabsList className="w-full min-w-0 justify-start overflow-hidden h-8 rounded-lg border border-border/70 bg-background/50">
                      <TabsTrigger value="info" className="flex-1">
                        <Info className="size-3 mr-0.5" />
                        Info
                      </TabsTrigger>
                      <TabsTrigger value="files" className="flex-1">
                        <Files className="size-3 mr-0.5" />
                        Files
                      </TabsTrigger>
                      <TabsTrigger value="mods" className="flex-1">
                        <Package className="size-3 mr-0.5" />
                        Mods
                      </TabsTrigger>
                      <TabsTrigger value="settings" className="flex-1">
                        <SlidersHorizontal className="size-3 mr-0.5" />
                        Settings
                      </TabsTrigger>
                      <TabsTrigger value="logs" className="flex-1">
                        <Terminal className="size-3 mr-0.5" />
                        Logs
                      </TabsTrigger>
                    </TabsList>
                  </div>

                  <TabsContent value="info" className="flex-1 overflow-auto px-4 pb-4 pt-3 mt-0 space-y-3">
                    <div className="detail-row flex items-center justify-between gap-3 rounded-lg border border-border/60 bg-card/45 px-3 py-2">
                      <PackVersionStatus
                        currentVersion={instancePackVersion(sel)}
                        versions={gtnhVersions}
                        onUpdate={() => setUpdatePackInstanceId(selectedInstanceId!)}
                        disabled={selectedInstanceActive || instanceBusy(selectedInstanceId!)}
                      />
                    </div>
                    <div className="detail-row flex items-center justify-between gap-3 rounded-lg border border-border/60 bg-card/45 px-3 py-2">
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
                        { label: "Group", value: sel.group || groupsState.ungrouped_name },
                        {
                          label: "Java",
                          value: mergeInstanceSettings(sel.settings).override_java_location
                            ? sel.settings.java_path || "Invalid instance override"
                            : launcherSettings.default_java_path || "Auto-detect",
                        },
                        {
                          label: "RAM",
                          value: sel.settings.override_memory ? `${sel.settings.min_ram_mb}-${sel.settings.max_ram_mb} MB` : "Default (4096-6144 MB)",
                        },
                        ...(mergeInstanceSettings(sel.settings).show_game_time && (sel.settings.override_game_time || sel.settings.total_play_seconds > 0)
                          ? [{ label: "Play time", value: formatPlayTime(sel.settings.total_play_seconds) }]
                          : []),
                        {
                          label: "Account",
                          value: (() => {
                            const launchAccount = resolveLaunchAccount(accounts, defaultAccountId, sel.settings);
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
                      javaRefreshing={javaRefreshing}
                      accounts={accounts}
                      onOpenLauncherSettings={() => setTab("settings")}
                      onRefreshJava={refreshJava}
                      onSave={handleSaveSettings}
                    />
                  </TabsContent>

                  <TabsContent value="logs" className="flex-1 overflow-hidden flex flex-col mt-0">
                    <LogView
                      log={instanceLogs[selectedInstanceId!] ?? []}
                      onClear={() => handleClearConsole(selectedInstanceId!)}
                      disableClear={selectedInstanceActive}
                      onCopy={async () => {
                        if (selectedInstanceActive) return instanceLogs[selectedInstanceId!] ?? [];
                        try {
                          return await session.getConsoleLog(selectedInstanceId!);
                        } catch {
                          return instanceLogs[selectedInstanceId!] ?? [];
                        }
                      }}
                    />
                  </TabsContent>
                </Tabs>

                {/* Action bar */}
                <div className="detail-action-bar shrink-0 border-t border-border/80 bg-card/60 px-4 py-3 flex items-center gap-2">
                  {isReinstallingSelected && selectedReinstallProcess ? (
                    <>
                      <div className="flex-1 min-w-0 space-y-1.5">
                        <div className="flex items-center gap-2 text-sm min-w-0">
                          <Loader2 className="size-4 shrink-0 animate-spin text-muted-foreground" />
                          <span className="truncate">{formatUpdateProgress(selectedReinstallProcess)}</span>
                        </div>
                        <Progress value={selectedReinstallProcess.pct * 100} className="h-1.5" />
                      </div>
                      <Button variant="outline" onClick={() => openProcesses(processKey("reinstall", selectedInstanceId!))}>
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
                      <Button variant="outline" onClick={() => openProcesses(processKey("update-pack", selectedInstanceId!))}>
                        <Activity className="size-3.5" />
                        View log
                      </Button>
                    </>
                  ) : isDeletingSelected && selectedDeleteProcess ? (
                    <>
                      <div className="flex-1 min-w-0 space-y-1.5">
                        <div className="flex items-center gap-2 text-sm">
                          <Loader2 className="size-4 shrink-0 animate-spin text-muted-foreground" />
                          <span>Deleting... {(selectedDeleteProcess.pct * 100).toFixed(0)}%</span>
                        </div>
                        <Progress value={selectedDeleteProcess.pct * 100} className="h-1.5" />
                      </div>
                      <Button variant="outline" onClick={() => handleCancelDelete(selectedInstanceId!)}>
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
                          <>
                            <Loader2 className="animate-spin" /> Launching...
                          </>
                        ) : (
                          <>
                            <Square className="fill-current" /> Stop
                          </>
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
                        title="Copy instance"
                        disabled={selectedInstanceActive || instanceBusy(selectedInstanceId!)}
                        onClick={() => setCopyInstanceId(selectedInstanceId)}
                      >
                        <Copy />
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
                        title="Copy instance"
                        disabled={selectedInstanceActive || instanceBusy(selectedInstanceId!)}
                        onClick={() => setCopyInstanceId(selectedInstanceId)}
                      >
                        <Copy />
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
                <div className="empty-state max-w-sm rounded-lg border border-dashed border-border/80 bg-muted/30 p-6 text-center">
                  <div className="mx-auto mb-3 instance-avatar size-11 rounded-lg flex items-center justify-center">
                    <Boxes className="size-5 text-muted-foreground" />
                  </div>
                  <div className="font-medium">Select an instance</div>
                  <p className="mt-1 text-xs text-muted-foreground">Pick a pack from the library to view files, mods, settings, and launch logs.</p>
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
        <main className="content-shell min-w-0 flex-1 overflow-auto">
          {tab === "settings" && (
            <div className="settings-page mx-auto w-full max-w-5xl p-4">
              <SettingsTab
                javaOptions={javaOptions}
                javaRefreshing={javaRefreshing}
                onRefreshJava={refreshJava}
                defaultJavaPath={launcherSettings.default_java_path ?? null}
                onDefaultJavaChange={handleSetDefaultJava}
                onError={(message) => setError(`Settings failed: ${message}`)}
              />
            </div>
          )}
          {tab === "accounts" && (
            <div className="accounts-page mx-auto w-full max-w-2xl p-4">
              <AccountsTab
                onSetDefaultAccount={handleSetDefaultAccount}
                defaultAccountId={defaultAccountId}
                launchRedirect={accountsLaunchRedirect ? { instanceName: accountsLaunchRedirect.instanceName } : null}
                onDismissRedirect={() => setAccountsLaunchRedirect(null)}
              />
            </div>
          )}
        </main>
      )}

      {/* Status bar */}
      <footer className="h-6 shrink-0 border-t border-border/80 flex items-center px-3 gap-3 text-[11px] text-muted-foreground bg-card/80">
        <span>
          {instances.length} instance{instances.length === 1 ? "" : "s"}
        </span>
        {sel && <span className="truncate">{instanceDisplayName(sel)}</span>}
        {launching && <span>Launching {launching}…</span>}
        {runningInstanceIds.size > 0 && <span>Running {Array.from(runningInstanceIds).join(", ")}</span>}
        {runningProcessCount(processes) > 0 && (
          <span>
            {runningProcessCount(processes)} background process{runningProcessCount(processes) === 1 ? "" : "es"}
          </span>
        )}
      </footer>

      {updatePackInstanceId &&
        (() => {
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
              onUpdate={(packVersion, javaType, keepModIdentities) => {
                startPackUpdate(updatePackInstanceId, instanceDisplayName(inst), packVersion, javaType, keepModIdentities);
              }}
            />
          );
        })()}

      {reinstallInstanceId &&
        (() => {
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
                startCleanReinstall(reinstallInstanceId, instanceDisplayName(inst), packVersion, javaType);
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

      {copyInstanceId &&
        (() => {
          const source = instances.find((i) => i.id === copyInstanceId);
          if (!source) return null;
          return (
            <CopyInstanceDialog
              source={source}
              existingInstanceIds={new Set(instances.map((i) => i.id))}
              onClose={() => setCopyInstanceId(null)}
              onCopy={(newId, newName) => handleCopyInstance(copyInstanceId, newId, newName)}
            />
          );
        })()}

      {renameInstanceId &&
        (() => {
          const inst = instances.find((i) => i.id === renameInstanceId);
          if (!inst) return null;
          return (
            <RenameInstanceDialog
              instance={inst}
              onClose={() => setRenameInstanceId(null)}
              onSave={(newName) => handleRenameInstance(renameInstanceId, newName)}
            />
          );
        })()}

      {changeGroupInstanceId && (
        <ChangeGroupDialog
          instanceName={
            instances.find((i) => i.id === changeGroupInstanceId)
              ? instanceDisplayName(instances.find((i) => i.id === changeGroupInstanceId)!)
              : changeGroupInstanceId
          }
          currentGroup={instances.find((i) => i.id === changeGroupInstanceId)?.group ?? ""}
          existingGroups={groupsState.groups}
          ungroupedName={groupsState.ungrouped_name}
          onClose={() => setChangeGroupInstanceId(null)}
          onSave={(group) => {
            handleSetInstanceGroup(changeGroupInstanceId, group);
            setChangeGroupInstanceId(null);
          }}
        />
      )}

      <LauncherUpdateDialog
        state={launcherUpdate}
        onInstall={installLauncherUpdate}
        onDismiss={() => session.dismissLauncherUpdate()}
        onRetry={retryLauncherUpdate}
      />

      <ConfirmDialog
        open={deleteInstanceConfirm !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteInstanceConfirm(null);
        }}
        title="Delete instance?"
        description={deleteInstanceConfirm ? `Delete "${deleteInstanceConfirm.name}"? This removes all instance files and cannot be undone.` : ""}
        confirmLabel="Delete"
        destructive
        onConfirm={confirmDeleteInstance}
      />

      {notice && (
        <div className="fixed bottom-10 right-4 z-50 max-w-sm rounded bg-secondary p-3 text-secondary-foreground shadow-lg">
          <p className="text-sm">{notice}</p>
          <Button size="sm" variant="ghost" className="mt-1" onClick={() => setNotice(null)}>
            Dismiss
          </Button>
        </div>
      )}

      {/* Error toast */}
      {error && (
        <div className="fixed bottom-10 right-4 bg-destructive text-destructive-foreground p-3 rounded shadow-lg max-w-sm z-50">
          <p className="text-sm">{error}</p>
          <Button size="sm" variant="ghost" className="mt-1" onClick={() => setError(null)}>
            Dismiss
          </Button>
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

interface InstanceListCommands extends InstanceGridCardCommands {
  toggleGroupCollapsed: (group: string, collapsed: boolean) => void;
  renameGroup: (oldName: string, newName: string) => void;
  deleteGroup: (name: string) => void;
  reorderInstances: (group: string, order: string[]) => void;
}

function buildGroupSections(instances: InstanceInfo[], groupNames: string[], instanceOrder: Record<string, string[]>, ungroupedName: string): GroupSection[] {
  const buckets = new Map<string, InstanceInfo[]>();
  for (const inst of instances) {
    const key = inst.group || "";
    if (!buckets.has(key)) buckets.set(key, []);
    buckets.get(key)!.push(inst);
  }

  const sections: GroupSection[] = [];
  const sortedNames = [...groupNames].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));

  for (const name of sortedNames) {
    const items = buckets.get(name);
    if (items && items.length > 0) {
      sections.push({
        id: name,
        label: name,
        items: orderInstancesInGroup(items, name, instanceOrder),
      });
      buckets.delete(name);
    }
  }

  for (const [key, items] of buckets) {
    if (key && items.length > 0) {
      sections.push({
        id: key,
        label: key,
        items: orderInstancesInGroup(items, key, instanceOrder),
      });
    }
  }

  const ungrouped = buckets.get("") ?? [];
  if (ungrouped.length > 0) {
    sections.push({
      id: "",
      label: ungroupedName || "Ungrouped",
      items: orderInstancesInGroup(ungrouped, "", instanceOrder),
    });
  }

  return sections;
}

function InstanceGroupList({ commands }: { commands: InstanceListCommands }) {
  const instances = useLauncherStore((state) => state.instances);
  const groupsState = useLauncherStore((state) => state.groupsState);
  const sections = useMemo(
    () => buildGroupSections(instances, groupsState.groups, groupsState.instance_order, groupsState.ungrouped_name),
    [instances, groupsState.groups, groupsState.instance_order, groupsState.ungrouped_name],
  );

  if (sections.length === 0) return null;

  return (
    <div className="min-h-0 flex-1 space-y-2 overflow-y-auto p-2">
      {sections.map((section) => (
        <InstanceGroupSection key={section.id || "__ungrouped__"} section={section} commands={commands} />
      ))}
    </div>
  );
}

function InstanceGroupSection({ section, commands }: { section: GroupSection; commands: InstanceListCommands }) {
  const collapsed = useLauncherStore((state) => state.groupsState.collapsed[section.id] ?? false);
  const ungroupedName = useLauncherStore((state) => state.groupsState.ungrouped_name);

  const [renaming, setRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState("");
  const [deleteGroupOpen, setDeleteGroupOpen] = useState(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropTargetId, setDropTargetId] = useState<string | null>(null);

  const reorderByDrop = (fromId: string, toId: string) => {
    if (fromId === toId || draggingId !== fromId) return;
    const ids = section.items.map((item) => item.id);
    const fromIndex = ids.indexOf(fromId);
    const toIndex = ids.indexOf(toId);
    if (fromIndex < 0 || toIndex < 0) return;
    const next = [...ids];
    next.splice(fromIndex, 1);
    next.splice(toIndex, 0, fromId);
    commands.reorderInstances(section.id, next);
  };

  const startRename = () => {
    setRenameDraft(section.label);
    setRenaming(true);
  };

  const commitRename = () => {
    setRenaming(false);
    const trimmed = renameDraft.trim();
    if (!trimmed || trimmed === section.label) {
      return;
    }
    commands.renameGroup(section.id, trimmed);
  };

  const confirmDeleteGroup = () => {
    if (!section.id) return;
    commands.deleteGroup(section.id);
  };

  return (
    <section className="space-y-1">
      <div className="flex items-center gap-1.5 px-1 py-1 group/header sticky top-0 z-10 bg-card/90 backdrop-blur">
        <Button
          type="button"
          variant="ghost"
          onClick={() => commands.toggleGroupCollapsed(section.id, !collapsed)}
          className="flex h-7 flex-1 min-w-0 items-center gap-2 justify-start rounded-md px-2 py-1 font-normal hover:text-foreground"
          aria-expanded={!collapsed}
        >
          {collapsed ? <ChevronRight className="size-3.5 text-muted-foreground" /> : <ChevronDown className="size-3.5 text-muted-foreground" />}
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
            <ContextMenu>
              <ContextMenuTrigger asChild>
                <h2
                  className="cursor-text truncate text-[11px] font-semibold uppercase text-muted-foreground"
                  onDoubleClick={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    startRename();
                  }}
                  title="Double-click or right-click to rename"
                >
                  {section.label}
                </h2>
              </ContextMenuTrigger>
              <ContextMenuContent className="w-44">
                <ContextMenuItem onSelect={startRename}>
                  <Pencil />
                  Rename
                </ContextMenuItem>
                {section.id && (
                  <>
                    <ContextMenuSeparator />
                    <ContextMenuItem onSelect={() => setDeleteGroupOpen(true)} className="text-destructive focus:text-destructive">
                      <Trash2 />
                      Delete
                    </ContextMenuItem>
                  </>
                )}
              </ContextMenuContent>
            </ContextMenu>
          )}
          <Badge variant="secondary" className="shrink-0 h-5 rounded-md">
            {section.items.length}
          </Badge>
        </Button>
        {!renaming && (
          <div className="flex items-center gap-1 opacity-0 group-hover/header:opacity-100 transition-opacity">
            <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={startRename}>
              Rename
            </Button>
            {section.id && (
              <Button variant="ghost" size="sm" className="h-6 px-2 text-xs text-destructive hover:text-destructive" onClick={() => setDeleteGroupOpen(true)}>
                Delete
              </Button>
            )}
          </div>
        )}
      </div>
      <ConfirmDialog
        open={deleteGroupOpen}
        onOpenChange={setDeleteGroupOpen}
        title="Delete group?"
        description={`Delete group "${section.label}"? Instances will be moved to ${ungroupedName}.`}
        confirmLabel="Delete"
        destructive
        onConfirm={confirmDeleteGroup}
      />
      {!collapsed && (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(9rem,9rem))] justify-start gap-2 px-1 pb-1">
          {section.items.map((inst) => (
            <InstanceGridCard
              key={inst.id}
              inst={inst}
              commands={commands}
              isDragging={draggingId === inst.id}
              isDragOver={dropTargetId === inst.id && draggingId !== inst.id}
              onDragHandleStart={(event) => {
                event.dataTransfer.setData("text/plain", inst.id);
                event.dataTransfer.effectAllowed = "move";
                setDraggingId(inst.id);
              }}
              onDragEnd={() => {
                setDraggingId(null);
                setDropTargetId(null);
              }}
              onDragOver={(event) => {
                if (!draggingId || draggingId === inst.id) return;
                event.preventDefault();
                event.dataTransfer.dropEffect = "move";
                setDropTargetId(inst.id);
              }}
              onDragLeave={() => {
                if (dropTargetId === inst.id) setDropTargetId(null);
              }}
              onDrop={(event) => {
                event.preventDefault();
                const fromId = event.dataTransfer.getData("text/plain") || draggingId;
                if (fromId) reorderByDrop(fromId, inst.id);
                setDraggingId(null);
                setDropTargetId(null);
              }}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function RenameInstanceDialog({ instance, onClose, onSave }: { instance: InstanceInfo; onClose: () => void; onSave: (newName: string) => void }) {
  const packVersion = instancePackVersion(instance);
  const placeholder = `GTNH ${packVersion}`;
  const [name, setName] = useState(instance.settings.name);
  const trimmed = name.trim();
  const unchanged = trimmed === (instance.settings.name ?? "");

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="max-w-sm p-0">
        <Card className="border-0 shadow-none">
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle>Rename Instance</CardTitle>
              <Button variant="ghost" size="icon" className="size-7" onClick={onClose} aria-label="Close">
                <X className="size-3.5" />
              </Button>
            </div>
            <CardDescription>Change the display name shown in the launcher. The instance folder id stays the same.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="rename-instance-name">Display name</Label>
              <Input
                id="rename-instance-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={placeholder}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !unchanged) onSave(name);
                }}
                autoFocus
              />
            </div>
            <p className="text-xs text-muted-foreground">
              Folder: <span className="font-mono">{instance.id}</span>
            </p>
            <div className="flex gap-2">
              <Button className="flex-1" disabled={unchanged} onClick={() => onSave(name)}>
                Save
              </Button>
              <Button variant="outline" onClick={onClose}>
                Cancel
              </Button>
            </div>
          </CardContent>
        </Card>
      </DialogContent>
    </Dialog>
  );
}

function CopyInstanceDialog({
  source,
  existingInstanceIds,
  onClose,
  onCopy,
}: {
  source: InstanceInfo;
  existingInstanceIds: Set<string>;
  onClose: () => void;
  onCopy: (newId: string, newName: string) => void;
}) {
  const sourceName = instanceDisplayName(source);
  const packVersion = instancePackVersion(source);
  const defaultName = `${sourceName} (copy)`;
  const [instanceName, setInstanceName] = useState(defaultName);
  const resolvedName = instanceName.trim() || defaultName;
  const resolvedId = makeInstanceId(resolvedName, packVersion, existingInstanceIds);
  const idConflict = existingInstanceIds.has(resolvedId);

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="max-w-sm p-0">
        <Card className="border-0 shadow-none">
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle>Copy Instance</CardTitle>
              <Button variant="ghost" size="icon" className="size-7" onClick={onClose} aria-label="Close">
                <X className="size-3.5" />
              </Button>
            </div>
            <CardDescription>Create a duplicate of {sourceName} with a new name and folder.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="copy-instance-name">Instance name</Label>
              <Input
                id="copy-instance-name"
                value={instanceName}
                onChange={(e) => setInstanceName(e.target.value)}
                placeholder={defaultName}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !idConflict && resolvedName) {
                    onCopy(resolvedId, resolvedName);
                  }
                }}
                autoFocus
              />
            </div>
            <div className="space-y-1">
              <p className="text-xs text-muted-foreground">
                Folder: <span className="font-mono">{resolvedId}</span>
              </p>
              {idConflict && <p className="text-xs text-destructive">An instance with this folder name already exists.</p>}
            </div>
            <p className="text-xs text-muted-foreground">All files, mods, saves, and settings are copied. Only the name and folder id change.</p>
            <div className="flex gap-2">
              <Button className="flex-1" disabled={idConflict || !resolvedName} onClick={() => onCopy(resolvedId, resolvedName)}>
                Copy
              </Button>
              <Button variant="outline" onClick={onClose}>
                Cancel
              </Button>
            </div>
          </CardContent>
        </Card>
      </DialogContent>
    </Dialog>
  );
}

function ChangeGroupDialog({
  instanceName,
  currentGroup,
  existingGroups,
  ungroupedName,
  onClose,
  onSave,
}: {
  instanceName: string;
  currentGroup: string;
  existingGroups: string[];
  ungroupedName: string;
  onClose: () => void;
  onSave: (group: string) => void;
}) {
  const [group, setGroup] = useState(currentGroup);
  const listId = "change-group-options";

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
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
              <p className="text-xs text-muted-foreground">Pick an existing group or type a new name. Leave empty for {ungroupedName}.</p>
            </div>
            <div className="flex gap-2">
              <Button className="flex-1" onClick={() => onSave(group.trim())}>
                Save
              </Button>
              <Button variant="outline" onClick={onClose}>
                Cancel
              </Button>
            </div>
          </CardContent>
        </Card>
      </DialogContent>
    </Dialog>
  );
}

function GroupPicker({ value, onChange, existingGroups, id }: { value: string; onChange: (value: string) => void; existingGroups: string[]; id: string }) {
  const listId = `${id}-list`;

  return (
    <div className="space-y-2">
      <Label htmlFor={id}>Group</Label>
      <Input id={id} value={value} onChange={(e) => onChange(e.target.value)} placeholder="No group" list={listId} />
      <datalist id={listId}>
        {existingGroups.map((g) => (
          <option key={g} value={g} />
        ))}
      </datalist>
    </div>
  );
}

// ── Details helpers ──

function InfoGrid({ items }: { items: { label: string; value: string }[] }) {
  return (
    <div className="grid sm:grid-cols-2 gap-2 pt-1">
      {items.map((item) => (
        <div key={item.label} className="info-grid-row flex min-w-0 justify-between gap-3 rounded-lg border border-border/60 bg-card/45 px-3 py-2 text-xs">
          <span className="text-muted-foreground shrink-0">{item.label}</span>
          <span className="font-medium text-right truncate">{item.value}</span>
        </div>
      ))}
    </div>
  );
}

function LogView({
  log,
  onClear,
  disableClear,
  onCopy,
}: {
  log: LaunchLogLine[];
  onClear: () => void;
  disableClear: boolean;
  onCopy?: () => Promise<LaunchLogLine[]>;
}) {
  const [copied, setCopied] = useState(false);
  const [copying, setCopying] = useState(false);
  const [copyFailed, setCopyFailed] = useState(false);

  const copy = async () => {
    setCopying(true);
    setCopied(false);
    setCopyFailed(false);
    try {
      const source = onCopy ? await onCopy() : log;
      const text = formatLaunchLog(source.length > 0 ? source : log);
      if (!text) {
        setCopyFailed(true);
        return;
      }
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setCopyFailed(false);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
      setCopyFailed(true);
    } finally {
      setCopying(false);
    }
  };

  return (
    <div className="flex flex-col flex-1 overflow-hidden">
      <div className="flex gap-0.5 px-3 pb-0.5 shrink-0">
        <Button size="sm" variant="ghost" onClick={() => void copy()} disabled={copying || log.length === 0}>
          {copying ? "Preparing..." : copied ? "Copied" : copyFailed ? "Copy failed" : "Copy"}
        </Button>
        <Button size="sm" variant="ghost" onClick={onClear} disabled={disableClear || log.length === 0}>
          Clear
        </Button>
        {log.length >= MAX_RETAINED_LOG_LINES && (
          <span className="self-center px-2 text-[10px] text-muted-foreground">Latest {MAX_RETAINED_LOG_LINES.toLocaleString()} lines retained</span>
        )}
      </div>
      <VirtualizedLogList lines={log} />
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
  onInstall: (id: string, packVersion: string, javaType: string, group: string, name: string) => void;
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

  const sorted = versions ? Object.entries(versions).sort(([a], [b]) => compareVersionsByReleaseDate(a, b, versions)) : [];

  const filtered =
    filter === "stable"
      ? sorted.filter(([, v]) => v.title === "Stable release")
      : filter === "beta"
        ? sorted.filter(([, v]) => v.title !== "Stable release")
        : sorted;

  const resolvedName = instanceName.trim() || (sel ? `GTNH ${sel}` : "");
  const resolvedId = sel ? makeInstanceId(resolvedName, sel, existingInstanceIds) : "";

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
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
                {!versions && <p className="text-muted-foreground text-sm px-1 py-2">Loading versions...</p>}
                {versions && filtered.length === 0 && <p className="text-muted-foreground text-sm px-1 py-2">No versions match this filter.</p>}
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
                    <Badge variant={v.title === "Stable release" ? "success" : "warning"}>{v.title === "Stable release" ? "Stable" : "Beta"}</Badge>
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

              <GroupPicker value={group} onChange={setGroup} existingGroups={existingGroups} id="new-instance-group-options" />

              <Button className="w-full" disabled={!sel} onClick={() => sel && onInstall(resolvedId, sel, javaType, group.trim(), resolvedName)}>
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

function SettingsTab({
  javaOptions,
  javaRefreshing,
  onRefreshJava,
  defaultJavaPath,
  onDefaultJavaChange,
  onError,
}: {
  javaOptions: JavaInfo[];
  javaRefreshing: boolean;
  onRefreshJava: () => Promise<JavaInfo[]>;
  defaultJavaPath: string | null;
  onDefaultJavaChange: (path: string | null) => void;
  onError: (message: string) => void;
}) {
  const [settingsTab, setSettingsTab] = useState("java");
  const javaBrowseAttempt = useRef(0);

  const browseDefaultJava = async () => {
    const attempt = ++javaBrowseAttempt.current;
    const isCurrentAttempt = () => attempt === javaBrowseAttempt.current;

    try {
      const picked = await invoke<string | null>("browse_java_executable");
      await validateAndSelectJava(
        picked,
        (javaPath) => invoke("test_java", { javaPath }),
        (javaPath) => {
          if (isCurrentAttempt()) onDefaultJavaChange(javaPath);
        },
        (message) => {
          if (isCurrentAttempt()) onError(message);
        },
        isCurrentAttempt,
      );
    } catch (error) {
      if (isCurrentAttempt()) onError(`Browse Java failed: ${error}`);
    }
  };
  return (
    <Tabs value={settingsTab} onValueChange={setSettingsTab}>
      <TabsList aria-label="Settings sections" className="grid h-auto w-full max-w-2xl grid-cols-3 gap-1 rounded-lg border border-border/70 bg-muted/60 p-1">
        <TabsTrigger value="java" className="h-9 rounded-md text-sm">
          Java
        </TabsTrigger>
        <TabsTrigger value="appearance" className="h-9 rounded-md text-sm">
          Appearance
        </TabsTrigger>
        <TabsTrigger value="about" className="h-9 rounded-md text-sm">
          About
        </TabsTrigger>
      </TabsList>

      <TabsContent value="java" className="mt-4">
        <Card>
          <CardHeader className="flex-row items-center justify-between gap-2">
            <CardTitle>Java Detection</CardTitle>
            <Button type="button" variant="outline" size="sm" disabled={javaRefreshing} onClick={() => void onRefreshJava()}>
              <RefreshCw className={javaRefreshing ? "animate-spin" : ""} />
              {javaRefreshing ? "Scanning..." : "Refresh"}
            </Button>
          </CardHeader>
          <CardContent className="space-y-3">
            <JavaInstallationPicker
              installations={javaOptions}
              refreshing={javaRefreshing}
              selectedPath={defaultJavaPath}
              onBrowse={browseDefaultJava}
              onSelect={onDefaultJavaChange}
            />
          </CardContent>
        </Card>
      </TabsContent>

      <TabsContent value="appearance" className="mt-4">
        <ThemePresetPicker />
      </TabsContent>

      <TabsContent value="about" className="mt-4">
        <Card>
          <CardHeader>
            <CardTitle>About</CardTitle>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground space-y-1">
            <p>Industrialis Launcher v0.1.0</p>
            <p>GT New Horizons modpack manager built with Electron.</p>
            <a
              href={GITHUB_URL}
              className="inline-flex items-center gap-1.5 pt-1 text-primary hover:underline"
              onClick={(e) => {
                e.preventDefault();
                void openUrl(GITHUB_URL).catch(() => undefined);
              }}
            >
              <ExternalLink className="size-3.5" />
              View on GitHub
            </a>
          </CardContent>
        </Card>
      </TabsContent>
    </Tabs>
  );
}
