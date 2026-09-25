import fs from "node:fs/promises";
import path from "node:path";
import { app, BrowserWindow, dialog } from "electron";
import { accountToInfo, createOfflineAccount, loadAccounts, startMicrosoftLogin } from "./auth";
import { BackupService } from "./backups";
import { BackupManager, type BackupInstancePolicy } from "./backup-manager";
import { GoogleDriveAdapter } from "./google-drive";
import { applyConfigPreset, getConfigPresetStatus } from "./config-presets";
import {
  deleteGroup,
  getGroupsState,
  getInstanceGroup,
  moveInstanceInGroup,
  renameGroup,
  setGroupCollapsed,
  setGroupInstanceOrder,
  setInstanceGroup,
} from "./groups";
import { dirSize, exists } from "./fs-utils";
import {
  cancelDelete,
  copyInstance,
  deleteInstance,
  downloadInstall,
  openBackupsFolder,
  openInstanceFolder,
  openModsFolder,
  previewUpdate,
  refreshSizes,
  reinstallInstance,
  updateInstance,
} from "./instance-lifecycle";
import { getConsoleLog } from "./console-logs";
import { killInstance, launchInstance } from "./game-launch";
import {
  clearInstanceIcon,
  importInstanceIcon,
  listInstanceIcons,
  openInstanceIconsFolder,
  resolveIconPath,
  setInstanceIcon,
  setInstanceIconFromLibrary,
} from "./instance-icons";
import { ReleaseUpdater } from "./release-updater";
import type { BackendContext, LaunchArgs, LaunchState } from "./backend-context";
import { detectJava, testJava } from "./java";
import { deletePersistentFile, listMinecraftEntries, listPersistentFiles, readMinecraftFile, writeMinecraftFile } from "./minecraft-files";
import { addCustomMod, listCustomMods, removeCustomMod } from "./pack";
import { evictExpiredPackCache } from "./pack-cache";
import { backupStatePath, consoleLogPath, instanceDir, instancesDir, sanitizeName } from "./paths";
import { loadRunningGamePids, saveRunningGamePids, type RunningGamePid } from "./running-game-pids";
import { loadInstanceSettings, loadLauncherSettings, saveInstanceSettings, saveLauncherSettings } from "./settings";
import { getProcessCreationId, isProcessAlive, normalizeProcessCreationId, waitForGameProcess, type RunningProcess } from "./process-manager";
import {
  defaultInstanceSettings,
  defaultLauncherSettings,
  type AccountData,
  type DownloadProgress,
  type InstanceInfo,
  type InstanceSettings,
  type LauncherSettings,
  type LaunchLogLine,
} from "./types";
import { ConsoleLogWriter } from "./console-log-writer";

export interface BackendHost {
  emit(event: string, payload: unknown): void;
}

export class LauncherBackend {
  private readonly state: LaunchState = {
    running: new Map(),
    installInProgress: new Set(),
    updateInProgress: new Set(),
    reinstallInProgress: new Set(),
    copyInProgress: new Set(),
    deleteCancel: new Map(),
  };
  private readonly consoleLogWriter = new ConsoleLogWriter(consoleLogPath);
  private readonly runningProcessReady: Promise<void>;
  private runningProcessPersistence: Promise<void> = Promise.resolve();
  private runningProcessEventsPublished = false;
  private readonly restoredRunningInstanceIds = new Set<string>();
  private readonly backupService = new BackupService();
  private readonly googleDrive = new GoogleDriveAdapter();
  private readonly backupManager: BackupManager;
  private readonly backupManagerReady: Promise<void>;
  private readonly instanceOperationWaiters = new Set<() => void>();
  private activeFilesystemOperations = 0;
  private readonly ctx: BackendContext;
  private readonly releaseUpdater: ReleaseUpdater;
  private disposing = false;

  constructor(private readonly host: BackendHost) {
    this.runningProcessReady = this.restoreRunningProcesses();
    this.backupManager = new BackupManager({
      service: this.backupService,
      providers: [
        {
          id: "google-drive",
          label: "Google Drive",
          store: this.googleDrive,
          getStatus: () => this.googleDrive.getStatus(),
        },
      ],
      listInstances: () => this.backupInstancePolicies(),
      statePath: backupStatePath(),
      emit: (event, payload) => this.emit(event, payload),
    });
    void evictExpiredPackCache();
    // Tracked (rather than fire-and-forget) so dispose() can quiesce the
    // initial run and polling timer before temporary data is cleaned up.
    this.backupManagerReady = this.backupManager.start().catch(() => undefined);
    this.ctx = {
      state: this.state,
      emit: (event, payload) => this.emit(event, payload),
      emitProgress: (payload) => this.emitProgress(payload),
      emitLog: (id, stream, line) => this.emitLog(id, stream, line),
      knownInstanceIds: () => this.knownInstanceIds(),
      saveAndRefreshSize: (id, settings) => this.saveAndRefreshSize(id, settings),
      notifyInstanceOperationWaiters: () => this.notifyInstanceOperationWaiters(),
      waitForInstanceOperations: () => this.waitForInstanceOperations(),
      persistRunningProcesses: () => this.persistRunningProcesses(),
      flushConsoleLog: (id) => this.flushConsoleLog(id),
      compactConsoleLog: (id) => this.compactConsoleLog(id),
      isRunning: (id) => this.state.running.has(id),
      isInstallInProgress: (id) => this.state.installInProgress.has(id),
      isUpdateInProgress: (id) => this.state.updateInProgress.has(id),
      isReinstallInProgress: (id) => this.state.reinstallInProgress.has(id),
      isCopyInProgress: (id) => this.state.copyInProgress.has(id),
      isDeleteInProgress: (id) => this.state.deleteCancel.has(id),
      getDeleteCancel: (id) => this.state.deleteCancel.get(id),
    };
    this.releaseUpdater = new ReleaseUpdater(this.ctx);
  }

  private emit(event: string, payload: unknown): void {
    this.host.emit(event, payload);
  }

  private emitProgress(payload: DownloadProgress): void {
    this.emit("dl-progress", payload);
  }

  private hasActiveInstanceOperations(): boolean {
    return (
      this.activeFilesystemOperations > 0 ||
      this.state.installInProgress.size > 0 ||
      this.state.updateInProgress.size > 0 ||
      this.state.reinstallInProgress.size > 0 ||
      this.state.copyInProgress.size > 0 ||
      this.state.deleteCancel.size > 0
    );
  }

  private waitForInstanceOperations(): Promise<void> {
    if (!this.hasActiveInstanceOperations()) return Promise.resolve();
    return new Promise((resolve) => this.instanceOperationWaiters.add(resolve));
  }

  private async trackFilesystemOperation<T>(operation: () => Promise<T>): Promise<T> {
    this.activeFilesystemOperations += 1;
    try {
      return await operation();
    } finally {
      this.activeFilesystemOperations -= 1;
      this.notifyInstanceOperationWaiters();
    }
  }

  private notifyInstanceOperationWaiters(): void {
    if (this.hasActiveInstanceOperations()) return;
    const waiters = [...this.instanceOperationWaiters];
    this.instanceOperationWaiters.clear();
    for (const resolve of waiters) resolve();
  }

  private emitLog(id: string, stream: string, line: string): void {
    const entry: LaunchLogLine = { stream, line };
    this.consoleLogWriter.append(id, entry);
    this.emit("launch-log", { id, ...entry });
  }

  private flushConsoleLog(id: string): Promise<void> {
    return this.consoleLogWriter.flush(id);
  }

  private compactConsoleLog(id: string): Promise<void> {
    return this.consoleLogWriter.compact(id);
  }

  private async knownInstanceIds(): Promise<Set<string>> {
    const entries = await fs.readdir(instancesDir(), { withFileTypes: true }).catch(() => []);
    const ids = await Promise.all(
      entries.map(async (entry) => {
        if (!entry.isDirectory()) return null;
        const instance = path.join(instancesDir(), entry.name);
        if (await exists(path.join(instance, "mmc-pack.json"))) return entry.name;
        const children = await fs.readdir(instance, { withFileTypes: true }).catch(() => []);
        const nested = await Promise.all(
          children.map((child) => (child.isDirectory() ? exists(path.join(instance, child.name, "mmc-pack.json")) : Promise.resolve(false))),
        );
        return nested.some(Boolean) ? entry.name : null;
      }),
    );
    return new Set(ids.filter((id): id is string => id !== null));
  }

  private async saveAndRefreshSize(id: string, settings: InstanceSettings): Promise<void> {
    settings.cached_size_bytes = await dirSize(instanceDir(id));
    await saveInstanceSettings(id, settings);
  }

  private async saveSettingsAndRefreshBackups(id: string, settings: InstanceSettings): Promise<void> {
    if (
      settings.backup_retention_override !== null &&
      (!Number.isInteger(settings.backup_retention_override) || settings.backup_retention_override < 1 || settings.backup_retention_override > 1_000)
    ) {
      throw new Error("instance backup retention limit must be between 1 and 1000");
    }
    await saveInstanceSettings(id, settings);
    await this.backupManager.wake();
  }

  private async saveLauncherSettingsAndRefreshBackups(settings: LauncherSettings): Promise<void> {
    await saveLauncherSettings(settings);
    await this.backupManager.wake();
  }

  private async backupInstancePolicies(): Promise<BackupInstancePolicy[]> {
    const [known, launcherSettings] = await Promise.all([this.knownInstanceIds(), loadLauncherSettings()]);
    return Promise.all(
      [...known].map(async (instanceId) => {
        const settings = await loadInstanceSettings(instanceId);
        return {
          instance_id: instanceId,
          enabled: settings.backups_enabled,
          retention_limit: settings.backup_retention_override ?? launcherSettings.backup_retention_limit,
        };
      }),
    );
  }

  private async loadInstances(): Promise<InstanceInfo[]> {
    await this.runningProcessReady;
    const known = await this.knownInstanceIds();
    const list = await Promise.all(
      [...known].sort().map(async (id): Promise<InstanceInfo> => {
        const settings = await loadInstanceSettings(id);
        if (!settings.pack_version) {
          settings.pack_version = id;
          await saveInstanceSettings(id, settings);
        }
        const icon = await resolveIconPath(id, settings);
        return { id, installed: true, size_bytes: settings.cached_size_bytes, settings, group: await getInstanceGroup(id, known), icon_path: icon };
      }),
    );
    this.publishRestoredRunningProcesses(known);
    return list;
  }

  private async restoreRunningProcesses(): Promise<void> {
    const persisted = await loadRunningGamePids();
    const restored = await Promise.all(
      [...persisted].map(async ([id, saved]) => {
        if (!isProcessAlive(saved.pid)) return null;
        const creationId = await getProcessCreationId(saved.pid);
        if (!creationId || creationId !== normalizeProcessCreationId(saved.creationId)) return null;
        return { id, running: { pid: saved.pid, creationId } satisfies RunningProcess };
      }),
    );
    for (const entry of restored) {
      if (!entry) continue;
      const { id, running } = entry;
      this.state.running.set(id, running);
      this.restoredRunningInstanceIds.add(id);
      this.monitorRestoredProcess(id, running);
    }
    await this.persistRunningProcesses().catch(() => undefined);
  }

  private publishRestoredRunningProcesses(known: Set<string>): void {
    if (this.runningProcessEventsPublished) return;
    this.runningProcessEventsPublished = true;
    for (const id of this.restoredRunningInstanceIds) {
      if (known.has(id) && this.state.running.has(id)) this.emit("instance-started", { id, restored: true });
    }
  }

  private monitorRestoredProcess(id: string, running: RunningProcess): void {
    void waitForGameProcess(running)
      .then((exitCode) => this.handleRestoredProcessExit(id, running, exitCode))
      .catch(() => this.handleRestoredProcessExit(id, running, 1));
  }

  private async handleRestoredProcessExit(id: string, running: RunningProcess, exitCode: number): Promise<void> {
    if (this.state.running.get(id) !== running) return;
    this.state.running.delete(id);
    await this.persistRunningProcesses().catch(() => undefined);
    this.emit("instance-stopped", { id, exit_code: exitCode });
  }

  private persistRunningProcesses(): Promise<void> {
    const snapshot = new Map<string, RunningGamePid>();
    for (const [id, running] of this.state.running) {
      if (running.creationId && isProcessAlive(running.pid)) snapshot.set(id, { pid: running.pid, creationId: running.creationId });
    }
    const write = this.runningProcessPersistence.then(() => saveRunningGamePids(snapshot));
    this.runningProcessPersistence = write.catch(() => undefined);
    return write;
  }

  async invoke(command: string, rawArgs: unknown): Promise<unknown> {
    await this.runningProcessReady;
    if (this.disposing) throw new Error("launcher is shutting down");
    const args = (rawArgs && typeof rawArgs === "object" ? rawArgs : {}) as LaunchArgs;
    const track = <T>(operation: () => Promise<T>): Promise<T> => this.trackFilesystemOperation(operation);
    switch (command) {
      case "get_versions":
        return this.getVersions();
      case "get_instances":
        return track(() => this.loadInstances());
      case "refresh_instance_sizes":
        return track(() => refreshSizes(this.ctx, args.ids));
      case "get_instance_groups":
        return getGroupsState(await this.knownInstanceIds());
      case "set_instance_group":
        return track(async () => setInstanceGroup(sanitizeName(args.id.trim()), String(args.group ?? ""), await this.knownInstanceIds()));
      case "rename_group":
        return track(async () => renameGroup(String(args.oldName ?? ""), String(args.newName ?? ""), await this.knownInstanceIds()));
      case "delete_group":
        return track(async () => deleteGroup(String(args.name ?? ""), await this.knownInstanceIds()));
      case "move_instance_in_group":
        return track(async () => moveInstanceInGroup(sanitizeName(args.id.trim()), String(args.direction), await this.knownInstanceIds()));
      case "set_group_instance_order":
        return track(async () => setGroupInstanceOrder(String(args.group ?? ""), args.order ?? [], await this.knownInstanceIds()));
      case "set_group_collapsed":
        return track(async () => setGroupCollapsed(String(args.group ?? ""), Boolean(args.collapsed), await this.knownInstanceIds()));
      case "delete_instance":
        return track(() => deleteInstance(this.ctx, args.id));
      case "cancel_delete_instance":
        return cancelDelete(this.ctx, args.id);
      case "copy_instance":
        return track(() => copyInstance(this.ctx, args));
      case "open_instance_folder":
        return track(() => openInstanceFolder(args.id));
      case "open_mods_folder":
        return track(() => openModsFolder(args.id));
      case "open_backups_folder":
        return openBackupsFolder(args.id);
      case "get_google_drive_status":
        return this.googleDrive.getStatus();
      case "configure_google_drive":
        return this.googleDrive.configure(String(args.clientId ?? ""));
      case "connect_google_drive": {
        const result = await this.googleDrive.connect();
        await this.backupManager.wake();
        return result;
      }
      case "disconnect_google_drive": {
        const result = await this.googleDrive.disconnect();
        await this.backupManager.wake();
        return result;
      }
      case "get_backup_dashboard":
        return this.backupManager.getDashboard();
      case "scan_backups":
        return this.backupManager.runOnce();
      case "retry_backup":
        return this.backupManager.retry(args.id || undefined, args.snapshotId, args.providerId);
      case "list_local_backups":
        return this.backupService.listLocalBackups(args.id);
      case "list_cloud_backups":
        return this.backupManager.listSnapshots(args.id);
      case "upload_backup":
        if (!args.fileName) throw new Error("backup file name is required");
        return this.backupManager.uploadNow(sanitizeName(args.id), args.fileName);
      case "download_backup":
        if (!args.snapshotId) throw new Error("backup snapshot id is required");
        return this.backupManager.download(args.id, args.snapshotId, args.providerId);
      case "delete_backup":
        if (!args.snapshotId) throw new Error("backup snapshot id is required");
        return this.backupManager.delete(args.id, args.snapshotId);
      case "save_settings":
        return track(() => this.saveSettingsAndRefreshBackups(sanitizeName(args.id), args.settings ?? defaultInstanceSettings()));
      case "get_settings":
        return loadInstanceSettings(sanitizeName(args.id));
      case "download_install":
        return track(() => downloadInstall(this.ctx, args));
      case "preview_update_mods":
        return track(() => previewUpdate(this.ctx, args));
      case "update_instance":
        return track(() => updateInstance(this.ctx, args));
      case "reinstall_instance":
        return track(() => reinstallInstance(this.ctx, args));
      case "list_minecraft_entries":
        return listMinecraftEntries(instanceDir(sanitizeName(args.id)), args.subpath ?? "");
      case "read_minecraft_file":
        return readMinecraftFile(instanceDir(sanitizeName(args.id)), String(args.relPath ?? ""));
      case "write_minecraft_file":
        return track(() =>
          writeMinecraftFile(instanceDir(sanitizeName(args.id)), String(args.relPath ?? ""), String(args.content ?? ""), Boolean(args.persist)),
        );
      case "delete_persistent_file":
        return track(() => deletePersistentFile(instanceDir(sanitizeName(args.id)), String(args.relPath ?? "")));
      case "list_persistent_files":
        return listPersistentFiles(instanceDir(sanitizeName(args.id)));
      case "apply_config_preset":
        return track(() =>
          applyConfigPreset(String(args.idPreset ?? args.id), instanceDir(sanitizeName(String(args.instanceId ?? args.id))), Boolean(args.enabled)),
        );
      case "get_config_preset_status":
        return getConfigPresetStatus(String(args.idPreset ?? args.id), instanceDir(sanitizeName(String(args.instanceId ?? args.id))));
      case "list_custom_mods":
        return listCustomMods(instanceDir(sanitizeName(args.id)));
      case "browse_custom_mod":
        return this.pickFile("Choose a mod", [{ name: "Mods", extensions: ["jar", "zip"] }]);
      case "add_custom_mod":
        return track(() => addCustomMod(instanceDir(sanitizeName(args.id)), String(args.sourcePath)));
      case "remove_custom_mod":
        return track(() => removeCustomMod(instanceDir(sanitizeName(args.id)), String(args.identity)));
      case "browse_instance_icon_file":
        return this.pickFile("Choose an instance icon", [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp", "ico"] }]);
      case "list_instance_icons":
        return listInstanceIcons();
      case "import_instance_icon":
        return track(() => importInstanceIcon(String(args.sourcePath ?? "")));
      case "set_instance_icon_from_library":
        return track(() => setInstanceIconFromLibrary(args));
      case "open_instance_icons_folder":
        return track(() => openInstanceIconsFolder());
      case "set_instance_icon":
        return track(() => setInstanceIcon(args));
      case "clear_instance_icon":
        return track(() => clearInstanceIcon(args.id));
      case "detect_java":
        return detectJava();
      case "browse_java_executable":
        return this.pickFile("Choose Java executable", process.platform === "win32" ? [{ name: "Java", extensions: ["exe"] }] : []);
      case "test_java":
        return testJava(args.javaPath ?? args.pathOverride);
      case "launch_instance":
        return launchInstance(this.ctx, args.id);
      case "exit_launcher":
        app.quit();
        return undefined;
      case "kill_instance":
        return killInstance(this.ctx, args.id);
      case "get_instance_console_log":
        return track(() => getConsoleLog(this.ctx, args.id, Boolean(args.full)));
      case "clear_instance_console_log":
        return track(() => fs.rm(consoleLogPath(sanitizeName(args.id)), { force: true }));
      case "get_accounts":
        return (await loadAccounts()).map(accountToInfo);
      case "add_offline_account":
        return track(() => createOfflineAccount(String(args.username ?? "")));
      case "remove_account":
        return track(() => this.removeAccount(String(args.id)));
      case "get_launcher_settings":
        return loadLauncherSettings();
      case "save_launcher_settings":
        return track(() => this.saveLauncherSettingsAndRefreshBackups(args.launcherSettings ?? defaultLauncherSettings()));
      case "start_microsoft_login":
        return startMicrosoftLogin((event, payload) => this.emit(event, payload));
      case "check_launcher_update":
        return this.releaseUpdater.checkForUpdate();
      case "install_launcher_update":
        return this.releaseUpdater.installUpdate();
      default:
        throw new Error(`Unknown Electron launcher command: ${command}`);
    }
  }

  private async getVersions(): Promise<unknown> {
    const { fetchGtnhVersions } = await import("./pack");
    return fetchGtnhVersions();
  }

  private async pickFile(title: string, filters: Array<{ name: string; extensions: string[] }>): Promise<string | null> {
    const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
    const result = await dialog.showOpenDialog(window, { title, properties: ["openFile"], filters });
    return result.canceled ? null : (result.filePaths[0] ?? null);
  }

  private async removeAccount(id: string): Promise<void> {
    await this.saveAccounts((await loadAccounts()).filter((account) => account.id !== id));
  }
  private async saveAccounts(accounts: AccountData[]): Promise<void> {
    const { saveAccounts } = await import("./auth");
    await saveAccounts(accounts);
  }

  handleDeepLinks(urls: string[]): void {
    for (const url of urls) {
      this.emit("oauth-deep-link", { url });
    }
  }
  async dispose(): Promise<void> {
    /* Detached game processes intentionally remain alive when Electron exits. */
    await this.backupManagerReady;
    await this.backupManager.stop();
    this.disposing = true;
    await this.runningProcessReady;
    await this.waitForInstanceOperations();
    await this.persistRunningProcesses();
  }
}
