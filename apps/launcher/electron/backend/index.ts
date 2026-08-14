import fs from "node:fs/promises";
import path from "node:path";
import { existsSync } from "node:fs";
import { app, BrowserWindow, dialog, shell } from "electron";
import { accountToInfo, cancelMicrosoftLogin, createOfflineAccount, ensureFreshToken, handleOauthCallback, loadAccounts, startMicrosoftLogin } from "./auth";
import { applyConfigPreset, getConfigPresetStatus } from "./config-presets";
import {
  deleteGroup,
  getGroupsState,
  getInstanceGroup,
  moveInstanceInGroup,
  removeInstanceFromGroups,
  renameGroup,
  setGroupCollapsed,
  setGroupInstanceOrder,
  setInstanceGroup,
} from "./groups";
import { copyTree, dirSize, exists, listFiles, removeIfExists } from "./fs-utils";
import { defaultInstanceIconPath, ensureInstanceIconLibrary, importInstanceIcon, instanceIconLibraryPath, listInstanceIcons } from "./instance-icons";
import {
  buildClasspath,
  buildLaunchConfig,
  expandMinecraftArguments,
  instanceCommandVars,
  runShellCommand,
  splitCommandArgs,
  substituteCommandVars,
  syncAssets,
  writeLaunchArgfile,
} from "./launch";
import { detectJava, javaGuiExecutable, javaPath, testJava } from "./java";
import { backupPlayerData, preserveDirName, restorePlayerData, wipeInstanceForReinstall } from "./migration";
import {
  applyPersistentMinecraft,
  deletePersistentFile,
  listMinecraftEntries,
  listPersistentFiles,
  readMinecraftFile,
  writeMinecraftFile,
} from "./minecraft-files";
import {
  addCustomMod,
  applyPersistentCustomMods,
  buildUpdatePreview,
  downloadAndExtractToStaging,
  flattenNestedPack,
  installStagingContents,
  prepareInstanceConfigs,
  listCustomMods,
  persistentCustomModsDir,
  removeCustomMod,
  removeCustomModsExcept,
} from "./pack";
import { evictExpiredPackCache } from "./pack-cache";
import { consoleLogPath, iconsDir, instanceDir, instancesDir, sanitizeName, validateInstanceId } from "./paths";
import { loadInstanceSettings, loadLauncherSettings, saveInstanceSettings, saveLauncherSettings } from "./settings";
import { killGameProcess, spawnGameProcess, waitForGameProcess, type RunningProcess } from "./process-manager";
import type { AccountData, DownloadProgress, InstanceInfo, InstanceSettings, LauncherSettings, LauncherUpdateState, LaunchLogLine } from "./types";
import { MAX_RETAINED_LOG_LINES, takeLogTail } from "../../src/lib/log-buffer";
import { ConsoleLogWriter } from "./console-log-writer";

export interface BackendHost {
  emit(event: string, payload: unknown): void;
}

interface LaunchArgs {
  id: string;
  packVersion?: string;
  javaType?: string;
  keepModIdentities?: string[];
  group?: string;
  name?: string;
  sourceId?: string;
  newId?: string;
  newName?: string;
  settings?: InstanceSettings;
  username?: string;
  accountId?: string;
  pathOverride?: string;
  javaPath?: string;
  subpath?: string | null;
  relPath?: string;
  content?: string;
  persist?: boolean;
  sourcePath?: string;
  iconId?: string;
  identity?: string;
  oldName?: string;
  order?: string[];
  collapsed?: boolean;
  direction?: string;
  ids?: string[] | null;
  full?: boolean;
  groupName?: string;
  idPreset?: string;
  instanceId?: string;
  enabled?: boolean;
  launcherSettings?: LauncherSettings;
}

interface LaunchState {
  running: Map<string, RunningProcess>;
  updateInProgress: Set<string>;
  reinstallInProgress: Set<string>;
  copyInProgress: Set<string>;
  deleteCancel: Map<string, { cancelled: boolean }>;
}

const CONSOLE_LOG_TAIL_BYTES = 4 * 1024 * 1024;

async function readConsoleLogTail(filePath: string): Promise<string> {
  const file = await fs.open(filePath, "r").catch(() => null);
  if (!file) return "";

  try {
    const size = (await file.stat()).size;
    const start = Math.max(0, size - CONSOLE_LOG_TAIL_BYTES);
    const buffer = Buffer.alloc(size - start);
    const { bytesRead } = await file.read(buffer, 0, buffer.length, start);
    return buffer.subarray(0, bytesRead).toString("utf8");
  } finally {
    await file.close();
  }
}

function isLaunchLogLine(value: unknown): value is LaunchLogLine {
  if (typeof value !== "object" || value === null) return false;
  const entry = value as Record<string, unknown>;
  return typeof entry.stream === "string" && typeof entry.line === "string";
}

function parseConsoleLog(contents: string, full: boolean): LaunchLogLine[] {
  const entries = contents
    .split(/\r?\n/u)
    .filter(Boolean)
    .flatMap((line) => {
      try {
        const value: unknown = JSON.parse(line);
        return isLaunchLogLine(value) ? [value] : [];
      } catch {
        return [];
      }
    });
  return full ? entries : takeLogTail(entries, MAX_RETAINED_LOG_LINES);
}

export class LauncherBackend {
  private readonly state: LaunchState = {
    running: new Map(),
    updateInProgress: new Set(),
    reinstallInProgress: new Set(),
    copyInProgress: new Set(),
    deleteCancel: new Map(),
  };
  private readonly consoleLogWriter = new ConsoleLogWriter(consoleLogPath);

  constructor(private readonly host: BackendHost) {
    void evictExpiredPackCache();
  }

  private emit(event: string, payload: unknown): void {
    this.host.emit(event, payload);
  }

  private emitProgress(payload: DownloadProgress): void {
    this.emit("dl-progress", payload);
  }

  private emitLog(id: string, stream: string, line: string): void {
    const entry: LaunchLogLine = { stream, line };
    this.consoleLogWriter.append(id, entry);
    this.emit("launch-log", { id, ...entry });
  }

  private flushConsoleLog(id: string): Promise<void> {
    return this.consoleLogWriter.flush(id);
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

  private async loadInstances(): Promise<InstanceInfo[]> {
    const known = await this.knownInstanceIds();
    const list = await Promise.all(
      [...known].sort().map(async (id): Promise<InstanceInfo> => {
        const settings = await loadInstanceSettings(id);
        if (!settings.pack_version) {
          settings.pack_version = id;
          await saveInstanceSettings(id, settings);
        }
        const icon = await this.resolveIconPath(id, settings);
        return { id, installed: true, size_bytes: settings.cached_size_bytes, settings, group: await getInstanceGroup(id, known), icon_path: icon };
      }),
    );
    return list;
  }

  private async resolveIconPath(id: string, settings: InstanceSettings): Promise<string | null> {
    const instance = instanceDir(id);
    if (settings.custom_icon && (await exists(path.join(instance, settings.custom_icon)))) return path.join(instance, settings.custom_icon);
    for (const entry of await fs.readdir(instance, { withFileTypes: true }).catch(() => [])) {
      if (entry.name.startsWith("instance-icon") && /\.(png|jpe?g|webp|gif|bmp|ico)$/iu.test(entry.name)) return path.join(instance, entry.name);
    }
    return null;
  }

  async invoke(command: string, rawArgs: unknown): Promise<unknown> {
    const args = (rawArgs && typeof rawArgs === "object" ? rawArgs : {}) as LaunchArgs;
    switch (command) {
      case "get_versions":
        return this.getVersions();
      case "get_instances":
        return this.loadInstances();
      case "refresh_instance_sizes":
        return this.refreshSizes(args.ids);
      case "get_instance_groups":
        return getGroupsState(await this.knownInstanceIds());
      case "set_instance_group":
        return setInstanceGroup(sanitizeName(args.id.trim()), String(args.group ?? ""), await this.knownInstanceIds());
      case "rename_group":
        return renameGroup(String(args.oldName ?? ""), String(args.newName ?? ""), await this.knownInstanceIds());
      case "delete_group":
        return deleteGroup(String(args.name ?? ""), await this.knownInstanceIds());
      case "move_instance_in_group":
        return moveInstanceInGroup(sanitizeName(args.id.trim()), String(args.direction), await this.knownInstanceIds());
      case "set_group_instance_order":
        return setGroupInstanceOrder(String(args.group ?? ""), args.order ?? [], await this.knownInstanceIds());
      case "set_group_collapsed":
        return setGroupCollapsed(String(args.group ?? ""), Boolean(args.collapsed), await this.knownInstanceIds());
      case "delete_instance":
        return this.deleteInstance(args.id);
      case "cancel_delete_instance":
        return this.cancelDelete(args.id);
      case "copy_instance":
        return this.copyInstance(args);
      case "open_instance_folder":
        return this.openInstanceFolder(args.id);
      case "save_settings":
        return saveInstanceSettings(sanitizeName(args.id), args.settings ?? defaultSettings());
      case "get_settings":
        return loadInstanceSettings(sanitizeName(args.id));
      case "download_install":
        return this.downloadInstall(args);
      case "preview_update_mods":
        return this.previewUpdate(args);
      case "update_instance":
        return this.updateInstance(args);
      case "reinstall_instance":
        return this.reinstallInstance(args);
      case "list_minecraft_entries":
        return listMinecraftEntries(instanceDir(sanitizeName(args.id)), args.subpath ?? "");
      case "read_minecraft_file":
        return readMinecraftFile(instanceDir(sanitizeName(args.id)), String(args.relPath ?? ""));
      case "write_minecraft_file":
        return writeMinecraftFile(instanceDir(sanitizeName(args.id)), String(args.relPath ?? ""), String(args.content ?? ""), Boolean(args.persist));
      case "delete_persistent_file":
        return deletePersistentFile(instanceDir(sanitizeName(args.id)), String(args.relPath ?? ""));
      case "list_persistent_files":
        return listPersistentFiles(instanceDir(sanitizeName(args.id)));
      case "apply_config_preset":
        return applyConfigPreset(String(args.idPreset ?? args.id), instanceDir(sanitizeName(String(args.instanceId ?? args.id))), Boolean(args.enabled));
      case "get_config_preset_status":
        return getConfigPresetStatus(String(args.idPreset ?? args.id), instanceDir(sanitizeName(String(args.instanceId ?? args.id))));
      case "list_custom_mods":
        return listCustomMods(instanceDir(sanitizeName(args.id)));
      case "browse_custom_mod":
        return this.pickFile("Choose a mod", [{ name: "Mods", extensions: ["jar", "zip"] }]);
      case "add_custom_mod":
        return addCustomMod(instanceDir(sanitizeName(args.id)), String(args.sourcePath));
      case "remove_custom_mod":
        return removeCustomMod(instanceDir(sanitizeName(args.id)), String(args.identity));
      case "browse_instance_icon_file":
        return this.pickFile("Choose an instance icon", [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp", "ico"] }]);
      case "list_instance_icons":
        return listInstanceIcons();
      case "import_instance_icon":
        return importInstanceIcon(String(args.sourcePath ?? ""));
      case "set_instance_icon_from_library":
        return this.setInstanceIconFromLibrary(args);
      case "open_instance_icons_folder":
        return this.openInstanceIconsFolder();
      case "set_instance_icon":
        return this.setInstanceIcon(args);
      case "clear_instance_icon":
        return this.clearInstanceIcon(args.id);
      case "detect_java":
        return detectJava();
      case "browse_java_executable":
        return this.pickFile("Choose Java executable", process.platform === "win32" ? [{ name: "Java", extensions: ["exe"] }] : []);
      case "test_java":
        return testJava(args.javaPath ?? args.pathOverride);
      case "launch_instance":
        return this.launchInstance(args.id);
      case "exit_launcher":
        app.quit();
        return undefined;
      case "kill_instance":
        return this.killInstance(args.id);
      case "get_instance_console_log":
        return this.getConsoleLog(args.id, Boolean(args.full));
      case "clear_instance_console_log":
        return fs.rm(consoleLogPath(sanitizeName(args.id)), { force: true });
      case "get_accounts":
        return (await loadAccounts()).map(accountToInfo);
      case "add_offline_account":
        return createOfflineAccount(String(args.username ?? ""));
      case "remove_account":
        return this.removeAccount(String(args.id));
      case "get_launcher_settings":
        return loadLauncherSettings();
      case "save_launcher_settings":
        return saveLauncherSettings(args.launcherSettings ?? defaultLauncherSettings());
      case "start_microsoft_login":
        return startMicrosoftLogin((event, payload) => this.emit(event, payload));
      case "cancel_microsoft_login":
        cancelMicrosoftLogin();
        return undefined;
      case "check_launcher_update":
        return this.checkLauncherUpdate();
      case "install_launcher_update":
        return this.installLauncherUpdate();
      default:
        throw new Error(`Unknown Electron launcher command: ${command}`);
    }
  }

  private async getVersions(): Promise<unknown> {
    const { fetchGtnhVersions } = await import("./pack");
    return fetchGtnhVersions();
  }

  private async refreshSizes(ids: string[] | null | undefined): Promise<Record<string, number>> {
    const target = ids?.map((id) => sanitizeName(id.trim())) ?? [...(await this.knownInstanceIds())];
    const sizes = await Promise.all(
      target.map(async (id) => {
        const [size, settings] = await Promise.all([dirSize(instanceDir(id)), loadInstanceSettings(id)]);
        settings.cached_size_bytes = size;
        await saveInstanceSettings(id, settings);
        return [id, size] as const;
      }),
    );
    return Object.fromEntries(sizes);
  }

  private async deleteInstance(rawId: string): Promise<void> {
    const id = sanitizeName(rawId.trim());
    const cancel = { cancelled: false };
    this.state.deleteCancel.set(id, cancel);
    try {
      const files = await listFiles(instanceDir(id));
      this.emitProgress({ stage: "deleting", operation: "delete", pct: 0, id });
      const total = Math.max(files.length, 1);
      for (let index = 0; index < files.length; index += 1) {
        if (cancel.cancelled) throw new Error("deletion cancelled");
        await fs.rm(files[index], { force: true });
        this.emitProgress({ stage: "deleting", operation: "delete", pct: (index + 1) / total, id });
      }
      await removeIfExists(instanceDir(id));
      await removeInstanceFromGroups(id, await this.knownInstanceIds());
      this.emitProgress({ stage: "done", operation: "delete", pct: 1, id });
    } finally {
      this.state.deleteCancel.delete(id);
    }
  }

  private cancelDelete(rawId: string): void {
    const entry = this.state.deleteCancel.get(sanitizeName(rawId.trim()));
    if (!entry) throw new Error("no deletion in progress for this instance");
    entry.cancelled = true;
  }

  private async copyInstance(args: LaunchArgs): Promise<void> {
    const sourceId = sanitizeName(String(args.sourceId ?? "").trim());
    const newId = validateInstanceId(String(args.newId ?? ""));
    const newName = String(args.newName ?? "").trim();
    if (!newName) throw new Error("instance name cannot be empty");
    if (sourceId === newId) throw new Error("new instance id must differ from the source");
    if (this.state.running.has(sourceId)) throw new Error("cannot copy while instance is running");
    if (this.state.copyInProgress.has(sourceId)) throw new Error("copy already in progress for this instance");
    const known = await this.knownInstanceIds();
    if (!known.has(sourceId)) throw new Error("source instance not found");
    if (known.has(newId)) throw new Error("an instance with that id already exists");
    this.state.copyInProgress.add(sourceId);
    try {
      const source = instanceDir(sourceId);
      const destination = instanceDir(newId);
      this.emitProgress({ stage: "copying", operation: "copy", pct: 0, id: newId, name: newName });
      await copyTree(source, destination);
      const settings = await loadInstanceSettings(newId);
      settings.name = newName;
      if (!(await this.resolveIconPath(newId, settings))) settings.custom_icon = await installDefaultInstanceIcon(destination);
      settings.cached_size_bytes = await dirSize(destination);
      await saveInstanceSettings(newId, settings);
      const group = await getInstanceGroup(sourceId, known);
      if (group) await setInstanceGroup(newId, group, new Set([...known, newId]));
      this.emitProgress({ stage: "done", operation: "copy", pct: 1, id: newId, name: newName });
    } catch (error) {
      await removeIfExists(instanceDir(newId));
      throw error;
    } finally {
      this.state.copyInProgress.delete(sourceId);
    }
  }

  private async openInstanceFolder(rawId: string): Promise<void> {
    const id = sanitizeName(rawId);
    const instance = instanceDir(id);
    if (!(await exists(instance))) throw new Error("instance not installed");
    await flattenNestedPack(instance);
    const error = await shell.openPath(instance);
    if (error) throw new Error(`failed to open instance folder: ${error}`);
  }

  private async downloadInstall(args: LaunchArgs): Promise<void> {
    const id = validateInstanceId(String(args.id));
    const known = await this.knownInstanceIds();
    if (known.has(id)) throw new Error("an instance with that id already exists");
    const instance = instanceDir(id);
    const packVersion = String(args.packVersion);
    const javaType = String(args.javaType ?? "java17+");
    await fs.mkdir(instance, { recursive: true });
    const staging = await downloadAndExtractToStaging(
      (payload) => this.emitProgress({ ...payload, id, operation: payload.operation ?? "install" } as DownloadProgress),
      packVersion,
      javaType,
      instance,
      "install",
      id,
    );
    await installStagingContents(staging, instance);
    await removeIfExists(staging);
    await flattenNestedPack(instance);
    await prepareInstanceConfigs(instance, true);
    const customIcon = await installDefaultInstanceIcon(instance);
    const settings = {
      ...defaultSettings(),
      name: String(args.name ?? "").trim() || `GTNH ${packVersion}`,
      pack_version: packVersion,
      pack_java_type: javaType,
      custom_icon: customIcon,
    };
    await this.saveAndRefreshSize(id, settings);
    if (args.group) await setInstanceGroup(id, args.group, await this.knownInstanceIds());
    this.emitProgress({ stage: "done", pct: 1, id, operation: "install" });
  }

  private async previewUpdate(args: LaunchArgs): Promise<unknown> {
    const id = sanitizeName(String(args.id).trim());
    const known = await this.knownInstanceIds();
    if (!known.has(id)) throw new Error("instance not found");
    const instance = instanceDir(id);
    const settings = await loadInstanceSettings(id);
    const target = String(args.packVersion);
    const javaType = String(args.javaType ?? settings.pack_java_type);
    this.emitProgress({ stage: "preview", pct: 0, operation: "preview", id, log_line: `Preparing mod analysis: ${settings.pack_version || id} → ${target}` });
    const previewDir = path.join(instance, ".update-preview");
    await removeIfExists(previewDir);
    try {
      return await buildUpdatePreview(instance, target, javaType, (payload) => this.emitProgress({ ...payload, id, operation: "preview" } as DownloadProgress));
    } finally {
      await removeIfExists(previewDir);
    }
  }

  private async updateInstance(args: LaunchArgs): Promise<void> {
    const id = sanitizeName(String(args.id).trim());
    if (this.state.running.has(id)) throw new Error("cannot update while instance is running");
    if (this.state.updateInProgress.has(id)) throw new Error("update already in progress for this instance");
    if (this.state.reinstallInProgress.has(id)) throw new Error("reinstall already in progress for this instance");
    this.state.updateInProgress.add(id);
    try {
      await this.reinstallCore(id, String(args.packVersion), String(args.javaType ?? "java17+"), args.keepModIdentities ?? [], "update-pack");
    } finally {
      this.state.updateInProgress.delete(id);
    }
  }

  private async reinstallInstance(args: LaunchArgs): Promise<void> {
    const id = sanitizeName(String(args.id).trim());
    if (this.state.running.has(id)) throw new Error("cannot reinstall while instance is running");
    if (this.state.reinstallInProgress.has(id)) throw new Error("reinstall already in progress for this instance");
    if (this.state.updateInProgress.has(id)) throw new Error("update already in progress for this instance");
    this.state.reinstallInProgress.add(id);
    try {
      await this.reinstallCore(id, String(args.packVersion), String(args.javaType ?? "java17+"), [], "reinstall");
    } finally {
      this.state.reinstallInProgress.delete(id);
    }
  }

  private async reinstallCore(id: string, packVersion: string, javaType: string, keepIds: string[], operation: "update-pack" | "reinstall"): Promise<void> {
    const known = await this.knownInstanceIds();
    if (!known.has(id)) throw new Error("instance not found");
    const instance = instanceDir(id);
    const preserve = path.join(instance, preserveDirName);
    const persistentMods = persistentCustomModsDir(instance);
    if (operation === "update-pack") {
      const removed = await removeCustomModsExcept(persistentMods, new Set(keepIds));
      if (removed) this.emitProgress({ stage: "updating", pct: 0.05, operation, id, log_line: `Removed ${removed} custom mod(s) not selected to keep` });
    }
    this.emitProgress({
      stage: operation === "update-pack" ? "updating" : "reinstalling",
      pct: 0.05,
      operation,
      id,
      log_line: "Backing up saves, JourneyMap, and player settings",
    });
    await backupPlayerData(instance, preserve);
    await wipeInstanceForReinstall(instance, preserve);
    await fs.mkdir(instance, { recursive: true });
    const staging = await downloadAndExtractToStaging(
      (payload) => this.emitProgress({ ...payload, id, operation } as DownloadProgress),
      packVersion,
      javaType,
      instance,
      operation,
      id,
    );
    this.emitProgress({ stage: operation === "update-pack" ? "updating" : "reinstalling", pct: 0.75, operation, id, log_line: "Installing fresh pack files" });
    await installStagingContents(staging, instance);
    await removeIfExists(staging);
    await flattenNestedPack(instance);
    await prepareInstanceConfigs(instance, true);
    await restorePlayerData(instance, preserve);
    const settings = await loadInstanceSettings(id);
    settings.pack_version = packVersion;
    settings.pack_java_type = javaType;
    await this.saveAndRefreshSize(id, settings);
    await applyPersistentCustomMods(instance);
    await applyPersistentMinecraft(instance);
    await removeIfExists(preserve);
    this.emitProgress({ stage: "done", pct: 1, operation, id, log_line: operation === "update-pack" ? "Update complete" : "Clean reinstall complete" });
  }

  private async pickFile(title: string, filters: Array<{ name: string; extensions: string[] }>): Promise<string | null> {
    const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
    const result = await dialog.showOpenDialog(window, { title, properties: ["openFile"], filters });
    return result.canceled ? null : (result.filePaths[0] ?? null);
  }

  private async setInstanceIcon(args: LaunchArgs): Promise<void> {
    const id = sanitizeName(args.id);
    const source = String(args.sourcePath ?? "");
    const instance = instanceDir(id);
    const stat = await fs.stat(source).catch(() => null);
    if (!stat?.isFile()) throw new Error("image file not found");
    if (stat.size > 4 * 1024 * 1024) throw new Error("image must be under 4 MB");
    const extension = path.extname(source).toLowerCase();
    if (!/\.(png|jpe?g|webp|gif|bmp|ico)$/u.test(extension)) throw new Error("unsupported image type; use PNG, JPG, WebP, GIF, BMP, or ICO");
    await fs.mkdir(instance, { recursive: true });
    await this.clearInstanceIcon(id);
    const filename = `instance-icon${extension}`;
    await fs.copyFile(source, path.join(instance, filename));
    const settings = await loadInstanceSettings(id);
    settings.custom_icon = filename;
    await saveInstanceSettings(id, settings);
  }

  private async setInstanceIconFromLibrary(args: LaunchArgs): Promise<void> {
    const sourcePath = await instanceIconLibraryPath(String(args.iconId ?? ""));
    await this.setInstanceIcon({ ...args, sourcePath });
  }

  private async openInstanceIconsFolder(): Promise<void> {
    await ensureInstanceIconLibrary();
    const error = await shell.openPath(iconsDir());
    if (error) throw new Error(`failed to open icons folder: ${error}`);
  }

  private async clearInstanceIcon(rawId: string): Promise<void> {
    const id = sanitizeName(rawId.trim());
    const instance = instanceDir(id);
    for (const entry of await fs.readdir(instance, { withFileTypes: true }).catch(() => []))
      if (entry.name.startsWith("instance-icon")) await fs.rm(path.join(instance, entry.name), { force: true });
    const settings = await loadInstanceSettings(id);
    settings.custom_icon = null;
    await saveInstanceSettings(id, settings);
  }

  private async killInstance(rawId: string): Promise<void> {
    const id = sanitizeName(rawId);
    const running = this.state.running.get(id);
    if (running) await killGameProcess(running);
  }

  private async getConsoleLog(rawId: string, full: boolean): Promise<LaunchLogLine[]> {
    const filePath = consoleLogPath(sanitizeName(rawId));
    const contents = full ? await fs.readFile(filePath, "utf8").catch(() => "") : await readConsoleLogTail(filePath);
    return parseConsoleLog(contents, full);
  }

  private async removeAccount(id: string): Promise<void> {
    await this.saveAccounts((await loadAccounts()).filter((account) => account.id !== id));
  }
  private async saveAccounts(accounts: AccountData[]): Promise<void> {
    const { saveAccounts } = await import("./auth");
    await saveAccounts(accounts);
  }

  private async launchInstance(rawId: string): Promise<void> {
    const id = sanitizeName(rawId.trim());
    if (this.state.running.has(id)) throw new Error("Instance is already running");
    if (this.state.updateInProgress.has(id)) throw new Error("pack update in progress for this instance");
    if (this.state.reinstallInProgress.has(id)) throw new Error("clean reinstall in progress for this instance");
    const instance = instanceDir(id);
    if (!(await exists(instance))) throw new Error("instance not installed");
    await flattenNestedPack(instance);
    await prepareInstanceConfigs(instance, false);
    await applyPersistentCustomMods(instance);
    await applyPersistentMinecraft(instance);
    const [settings, launcherSettings] = await Promise.all([loadInstanceSettings(id), loadLauncherSettings()]);
    const java = resolveJava(settings, launcherSettings.default_java_path);
    const config = await buildLaunchConfig(instance, id, (entryId, stream, line) => this.emitLog(entryId, stream, line));
    await syncAssets(config, id, (entryId, stream, line) => this.emitLog(entryId, stream, line));
    const classpath = buildClasspath(config.libraries);
    const vars = instanceCommandVars(id, settings.name, instance, java);
    if (settings.override_commands && settings.pre_launch_command.trim()) {
      await runShellCommand(substituteCommandVars(settings.pre_launch_command.trim(), vars), instance, settings.override_env ? settings.env_vars : {});
      this.emitLog(id, "system", "Pre-launch command finished");
    }
    const args = [`-Xms${settings.override_memory ? settings.min_ram_mb : 4096}M`, `-Xmx${settings.override_memory ? settings.max_ram_mb : 6144}M`];
    if (settings.override_memory && settings.perm_gen_mb > 0) args.push(`-XX:PermSize=${settings.perm_gen_mb}M`, `-XX:MaxPermSize=${settings.perm_gen_mb}M`);
    if (settings.override_java_args && settings.jvm_args.trim()) args.push(...splitCommandArgs(settings.jvm_args));
    args.push("-cp", classpath, ...config.jvmArgs, config.mainClass, ...config.programArgs);
    if (settings.override_window && !settings.launch_maximized) args.push("--width", String(settings.window_width), "--height", String(settings.window_height));
    if (settings.join_server_on_launch && settings.join_server_address.trim()) args.push("--server", settings.join_server_address.trim());
    const accounts = await loadAccounts();
    const account = chooseAccount(settings, accounts, launcherSettings.default_account_id);
    const auth = await this.launchAuth(account);
    if (config.minecraftArgumentsTemplate)
      args.push(
        ...expandMinecraftArguments(config.minecraftArgumentsTemplate, {
          auth_player_name: auth.username,
          version_name: config.minecraftVersion,
          game_directory: config.gameDir,
          assets_root: config.assetsDir,
          assets_index_name: config.assetIndex?.id ?? config.minecraftVersion,
          auth_uuid: auth.uuid,
          auth_access_token: auth.accessToken,
          user_properties: "{}",
          user_type: auth.userType,
        }),
      );
    else
      args.push(
        "--username",
        auth.username,
        "--version",
        config.minecraftVersion,
        "--gameDir",
        config.gameDir,
        "--assetsDir",
        config.assetsDir,
        "--accessToken",
        auth.accessToken,
        "--uuid",
        auth.uuid,
        "--userType",
        auth.userType,
      );
    this.emitLog(id, "system", "──────── Launch ────────");
    this.emitLog(id, "system", `Java: ${java}`);
    this.emitLog(id, "system", `Main class: ${config.mainClass}`);
    this.emitLog(id, "system", `Classpath: ${config.libraries.length} libraries (${classpath.length} chars)`);
    await writeLaunchArgfile(path.join(instance, "launch.arg"), args);
    this.emitLog(id, "system", `Launch args saved to ${path.join(instance, "launch.arg")}`);
    const launchJava = javaGuiExecutable(java);
    const command = resolveLaunchCommand(settings, launchJava, args, vars);
    const gameDir = config.gameDir;
    await fs.mkdir(gameDir, { recursive: true });
    const started = Date.now();
    const running = await spawnGameProcess(command.executable, command.args, gameDir, settings.override_env ? settings.env_vars : {}, (stream, line) => {
      for (const part of line.split(/\r?\n/u)) if (part) this.emitLog(id, stream, part);
    });
    this.state.running.set(id, running);
    this.emit("instance-started", { id });
    const exitCode = await waitForGameProcess(running);
    this.state.running.delete(id);
    this.emitLog(id, "system", `Process exited with code ${exitCode}`);
    try {
      if (settings.override_game_time && settings.record_game_time) {
        settings.total_play_seconds += Math.floor((Date.now() - started) / 1000);
        await saveInstanceSettings(id, settings);
      }
      if (settings.override_commands && settings.post_exit_command.trim()) {
        await runShellCommand(substituteCommandVars(settings.post_exit_command.trim(), vars), instance, settings.override_env ? settings.env_vars : {});
        this.emitLog(id, "system", "Post-exit command finished");
      }
    } finally {
      await this.flushConsoleLog(id);
      this.emit("instance-stopped", { id, exit_code: exitCode });
    }
    if (exitCode !== 0) throw new Error(`game exited with code ${exitCode}`);
  }

  private async launchAuth(account: AccountData): Promise<{ username: string; uuid: string; accessToken: string; userType: string }> {
    if (account.account_type === "offline") {
      const username = account.minecraft_profile?.name ?? "";
      const uuid = account.minecraft_profile?.id ?? "";
      if (!username || !uuid) throw new Error("Offline account is missing a username.");
      return { username, uuid, accessToken: "0", userType: "legacy" };
    }
    if (account.minecraft_entitlement && !account.minecraft_entitlement.can_play_minecraft)
      throw new Error("This Microsoft account does not own Minecraft Java Edition.");
    const token = await ensureFreshToken(account);
    const username = account.minecraft_profile?.name ?? "";
    const uuid = account.minecraft_profile?.id ?? "";
    if (!username || !uuid) throw new Error("This Microsoft account has no Minecraft profile yet. Set a username in the official launcher first.");
    return { username, uuid, accessToken: token, userType: "msa" };
  }

  private async checkLauncherUpdate(): Promise<LauncherUpdateState> {
    const current_version = app.getVersion();
    if (!app.isPackaged) return { status: "disabled", current_version };
    try {
      const response = await fetch("https://api.github.com/repos/SergioJuniorCE/industrialis-launcher/releases/latest", {
        headers: { Accept: "application/vnd.github+json", "User-Agent": "industrialis-launcher" },
      });
      if (!response.ok) throw new Error(`GitHub release lookup failed: HTTP ${response.status}`);
      const release = (await response.json()) as {
        tag_name?: string;
        body?: string;
        draft?: boolean;
        prerelease?: boolean;
        html_url?: string;
        assets?: Array<{ name?: string; browser_download_url?: string }>;
      };
      const version = release.tag_name?.replace(/^launcher-v/u, "");
      if (!version || release.draft || release.prerelease || !isNewerVersion(version, current_version)) return { status: "up-to-date", current_version };
      const asset = release.assets?.find((entry) => {
        const name = entry.name?.toLowerCase() ?? "";
        return process.platform === "win32"
          ? name.endsWith("setup.exe")
          : process.platform === "darwin"
            ? name.endsWith(".dmg")
            : name.endsWith(".deb") || name.endsWith(".rpm");
      });
      const state = {
        status: "available",
        current_version,
        version,
        body: release.body ?? "",
        release_url: release.html_url ?? "https://github.com/SergioJuniorCE/industrialis-launcher/releases/latest",
        ...(asset?.browser_download_url ? { download_url: asset.browser_download_url } : {}),
      } satisfies LauncherUpdateState;
      this.emit("launcher-update", state);
      return state;
    } catch (error) {
      return { status: "failed", current_version, error: String(error) };
    }
  }

  private async installLauncherUpdate(): Promise<LauncherUpdateState> {
    const state = await this.checkLauncherUpdate();
    if (state.status === "available") {
      const releaseUrl = state.release_url ?? "https://github.com/SergioJuniorCE/industrialis-launcher/releases/latest";
      await shell.openExternal(releaseUrl);
      const manual = { ...state, status: "manual" } satisfies LauncherUpdateState;
      this.emit("launcher-update", manual);
      return manual;
    }
    return state;
  }

  handleDeepLinks(urls: string[]): void {
    for (const url of urls) {
      handleOauthCallback(url);
      this.emit("oauth-deep-link", { url });
    }
  }
  dispose(): void {
    /* detached game processes intentionally remain alive when Electron exits */
  }
}

const defaultInstanceIconFilename = "instance-icon.png";

async function installDefaultInstanceIcon(instance: string): Promise<string> {
  const source = await defaultInstanceIconPath();
  await fs.copyFile(source, path.join(instance, defaultInstanceIconFilename));
  return defaultInstanceIconFilename;
}

function defaultSettings(): InstanceSettings {
  return {
    name: "",
    pack_version: "",
    pack_java_type: "java17+",
    java_path: null,
    min_ram_mb: 4096,
    max_ram_mb: 6144,
    perm_gen_mb: 128,
    jvm_args: "",
    auth_mode: "offline",
    username: "",
    offline_username_confirmed: false,
    override_window: false,
    launch_maximized: false,
    window_width: 854,
    window_height: 480,
    close_after_launch: false,
    quit_after_game_stop: false,
    override_console: false,
    show_console_on_launch: false,
    show_console_on_error: true,
    auto_close_console: false,
    override_game_time: false,
    show_game_time: true,
    record_game_time: true,
    total_play_seconds: 0,
    override_account: false,
    account_id: null,
    join_server_on_launch: false,
    join_server_address: "",
    override_java_location: false,
    skip_java_compat: false,
    override_memory: false,
    override_java_args: false,
    override_commands: false,
    pre_launch_command: "",
    wrapper_command: "",
    post_exit_command: "",
    override_env: false,
    env_vars: {},
    cached_size_bytes: 0,
    custom_icon: null,
  };
}
function defaultLauncherSettings(): LauncherSettings {
  return {
    theme_mode: "dark",
    theme_preset: "industrialis",
    theme_overrides: {},
    custom_theme_presets: [],
    default_account_id: null,
    default_java_path: null,
    instance_grid_columns: 3,
  };
}

function resolveJava(settings: InstanceSettings, defaultPath: string | null): string {
  if (settings.override_java_location && settings.java_path?.trim()) {
    if (!existsSync(settings.java_path)) throw new Error(`configured Java not found: ${settings.java_path}`);
    return settings.java_path;
  }
  if (defaultPath?.trim()) {
    if (!existsSync(defaultPath)) throw new Error(`default Java not found: ${defaultPath}`);
    return defaultPath;
  }
  const detected = javaPath();
  if (!detected)
    throw new Error("no Java configured or found — choose a default Java in launcher settings, set JAVA_HOME, or pick a Java in instance settings");
  return detected;
}

function chooseAccount(settings: InstanceSettings, accounts: AccountData[], defaultAccountId: string | null): AccountData {
  if (!accounts.length) throw new Error("Add an account in Accounts before launching.");
  if (settings.override_account && settings.account_id) {
    const account = accounts.find((entry) => entry.id === settings.account_id);
    if (!account) throw new Error("The account selected in instance settings was not found.");
    return account;
  }
  if (defaultAccountId) {
    const account = accounts.find((entry) => entry.id === defaultAccountId);
    if (account) return account;
  }
  if (accounts.length === 1) return accounts[0];
  throw new Error("Set a default account in Accounts before launching.");
}

function resolveLaunchCommand(settings: InstanceSettings, java: string, args: string[], vars: Record<string, string>): { executable: string; args: string[] } {
  if (!settings.override_commands || !settings.wrapper_command.trim()) return { executable: java, args };
  const wrapper = substituteCommandVars(settings.wrapper_command.trim(), vars);
  const parts = splitCommandArgs(wrapper);
  const executable = parts.shift();
  if (!executable) throw new Error("wrapper command is empty");
  return { executable, args: [...parts, java, ...args] };
}

function isNewerVersion(candidate: string, current: string): boolean {
  const parse = (value: string) => value.split(/[.-]/u).map((part) => Number(part.replace(/\D.*$/u, "")) || 0);
  const a = parse(candidate);
  const b = parse(current);
  for (let i = 0; i < Math.max(a.length, b.length); i += 1) if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  return false;
}
