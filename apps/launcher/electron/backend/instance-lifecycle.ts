import fs from "node:fs/promises";
import path from "node:path";
import { shell } from "electron";
import { getInstanceGroup, removeInstanceFromGroups, setInstanceGroup } from "./groups";
import { copyTree, dirSize, exists, listFiles, mapConcurrent, removeIfExists } from "./fs-utils";
import { installDefaultInstanceIcon, resolveIconPath } from "./instance-icons";
import { backupPlayerData, preserveDirName, restorePlayerData, wipeInstanceForReinstall } from "./migration";
import { applyPersistentMinecraft } from "./minecraft-files";
import {
  applyPersistentCustomMods,
  buildUpdatePreview,
  downloadAndExtractToStaging,
  flattenNestedPack,
  installStagingContents,
  persistentCustomModsDir,
  prepareInstanceConfigs,
  removeCustomModsExcept,
  resolveModsDir,
} from "./pack";
import { instanceBackupsDir, instanceDir, sanitizeName, validateInstanceId } from "./paths";
import { loadInstanceSettings, saveInstanceSettings } from "./settings";
import { defaultInstanceSettings, type DownloadProgress } from "./types";
import type { BackendContext, CopyInstanceArgs, DownloadInstallArgs, PreviewUpdateArgs, ReinstallInstanceArgs, UpdateInstanceArgs } from "./backend-context";

const COPY_PROGRESS_STEP = 0.01;

export async function refreshSizes(ctx: BackendContext, ids: string[] | null | undefined): Promise<Record<string, number>> {
  const target = ids?.map((id) => sanitizeName(id.trim())) ?? [...(await ctx.knownInstanceIds())];
  const sizes = await mapConcurrent(
    target,
    async (id) => {
      const [size, settings] = await Promise.all([dirSize(instanceDir(id)), loadInstanceSettings(id)]);
      settings.cached_size_bytes = size;
      await saveInstanceSettings(id, settings);
      return [id, size] as const;
    },
    4,
  );
  return Object.fromEntries(sizes);
}

export async function deleteInstance(ctx: BackendContext, rawId: string): Promise<void> {
  const id = sanitizeName(rawId.trim());
  if (ctx.isDeleteInProgress(id)) throw new Error("delete already in progress for this instance");
  const cancel = { cancelled: false };
  ctx.state.deleteCancel.set(id, cancel);
  try {
    const files = await listFiles(instanceDir(id));
    ctx.emitProgress({ stage: "deleting", operation: "delete", pct: 0, id });
    const total = Math.max(files.length, 1);
    for (let index = 0; index < files.length; index += 1) {
      if (cancel.cancelled) throw new Error("deletion cancelled");
      await fs.rm(files[index], { force: true });
      ctx.emitProgress({ stage: "deleting", operation: "delete", pct: (index + 1) / total, id });
    }
    await removeIfExists(instanceDir(id));
    await removeInstanceFromGroups(id, await ctx.knownInstanceIds());
    ctx.emitProgress({ stage: "done", operation: "delete", pct: 1, id });
  } finally {
    ctx.state.deleteCancel.delete(id);
    ctx.notifyInstanceOperationWaiters();
  }
}

export function cancelDelete(ctx: BackendContext, rawId: string): void {
  const entry = ctx.getDeleteCancel(sanitizeName(rawId.trim()));
  if (!entry) throw new Error("no deletion in progress for this instance");
  entry.cancelled = true;
}

export async function copyInstance(ctx: BackendContext, args: CopyInstanceArgs): Promise<void> {
  const sourceId = sanitizeName(String(args.sourceId ?? "").trim());
  const newId = validateInstanceId(String(args.newId ?? ""));
  const newName = String(args.newName ?? "").trim();
  if (!newName) throw new Error("instance name cannot be empty");
  if (sourceId === newId) throw new Error("new instance id must differ from the source");
  if (ctx.isRunning(sourceId)) throw new Error("cannot copy while instance is running");
  if (ctx.isCopyInProgress(sourceId)) throw new Error("copy already in progress for this instance");
  ctx.state.copyInProgress.add(sourceId);
  let ownsDestination = false;
  try {
    const known = await ctx.knownInstanceIds();
    if (!known.has(sourceId)) throw new Error("source instance not found");
    if (known.has(newId)) throw new Error("an instance with that id already exists");
    const source = instanceDir(sourceId);
    const destination = instanceDir(newId);
    ownsDestination = true;
    ctx.emitProgress({ stage: "copying", operation: "copy", pct: 0, id: newId, name: newName });
    let lastProgress = 0;
    await copyTree(source, destination, ({ completedFiles, totalFiles, completedBytes, totalBytes }) => {
      const ratio = totalBytes > 0 ? completedBytes / totalBytes : totalFiles > 0 ? completedFiles / totalFiles : 0;
      const pct = 0.05 + Math.min(1, ratio) * 0.85;
      if (pct < 0.9 && pct - lastProgress < COPY_PROGRESS_STEP && completedFiles < totalFiles) return;
      lastProgress = pct;
      ctx.emitProgress({ stage: "copying", operation: "copy", pct, id: newId, name: newName });
    });
    ctx.emitProgress({ stage: "finalizing", operation: "copy", pct: 0.95, id: newId, name: newName, log_line: "Finalizing copied instance" });
    const settings = await loadInstanceSettings(newId);
    settings.name = newName;
    if (!(await resolveIconPath(newId, settings))) settings.custom_icon = await installDefaultInstanceIcon(destination);
    settings.cached_size_bytes = await dirSize(destination);
    await saveInstanceSettings(newId, settings);
    const group = await getInstanceGroup(sourceId, known);
    if (group) await setInstanceGroup(newId, group, new Set([...known, newId]));
    ctx.emitProgress({ stage: "done", operation: "copy", pct: 1, id: newId, name: newName });
  } catch (error) {
    if (ownsDestination) await removeIfExists(instanceDir(newId));
    throw error;
  } finally {
    ctx.state.copyInProgress.delete(sourceId);
    ctx.notifyInstanceOperationWaiters();
  }
}

async function requireInstalledInstance(rawId: string): Promise<{ id: string; instance: string }> {
  const id = sanitizeName(rawId);
  const instance = instanceDir(id);
  if (!(await exists(instance))) throw new Error("instance not installed");
  return { id, instance };
}

async function openPathOrThrow(target: string, label: string): Promise<void> {
  const error = await shell.openPath(target);
  if (error) throw new Error(`failed to open ${label} folder: ${error}`);
}

export async function openInstanceFolder(rawId: string): Promise<void> {
  const { instance } = await requireInstalledInstance(rawId);
  await flattenNestedPack(instance);
  await openPathOrThrow(instance, "instance");
}

export async function openModsFolder(rawId: string): Promise<void> {
  const { instance } = await requireInstalledInstance(rawId);
  await flattenNestedPack(instance);
  const mods = await resolveModsDir(instance);
  await fs.mkdir(mods, { recursive: true });
  await openPathOrThrow(mods, "mods");
}

export async function openBackupsFolder(rawId: string): Promise<void> {
  const { id } = await requireInstalledInstance(rawId);
  const backups = instanceBackupsDir(id);
  await fs.mkdir(backups, { recursive: true });
  await openPathOrThrow(backups, "backups");
}

export async function downloadInstall(ctx: BackendContext, args: DownloadInstallArgs): Promise<void> {
  const id = validateInstanceId(String(args.id));
  const instance = instanceDir(id);
  const packVersion = String(args.packVersion);
  const javaType = String(args.javaType ?? "java17+");
  if (ctx.isInstallInProgress(id)) throw new Error("install already in progress for this instance");
  ctx.state.installInProgress.add(id);
  try {
    const known = await ctx.knownInstanceIds();
    if (known.has(id)) throw new Error("an instance with that id already exists");
    await fs.mkdir(instance, { recursive: true });
    const staging = await downloadAndExtractToStaging(
      (payload) => ctx.emitProgress({ ...payload, id, operation: payload.operation ?? "install" } as DownloadProgress),
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
      ...defaultInstanceSettings(),
      name: String(args.name ?? "").trim() || `GTNH ${packVersion}`,
      pack_version: packVersion,
      pack_java_type: javaType,
      custom_icon: customIcon,
    };
    await ctx.saveAndRefreshSize(id, settings);
    if (args.group) await setInstanceGroup(id, args.group, await ctx.knownInstanceIds());
    ctx.emitProgress({ stage: "done", pct: 1, id, operation: "install" });
  } finally {
    ctx.state.installInProgress.delete(id);
    ctx.notifyInstanceOperationWaiters();
  }
}

export async function previewUpdate(ctx: BackendContext, args: PreviewUpdateArgs): Promise<unknown> {
  const id = sanitizeName(String(args.id).trim());
  const known = await ctx.knownInstanceIds();
  if (!known.has(id)) throw new Error("instance not found");
  const instance = instanceDir(id);
  const settings = await loadInstanceSettings(id);
  const target = String(args.packVersion);
  const javaType = String(args.javaType ?? settings.pack_java_type);
  ctx.emitProgress({ stage: "preview", pct: 0, operation: "preview", id, log_line: `Preparing mod analysis: ${settings.pack_version || id} → ${target}` });
  const previewDir = path.join(instance, ".update-preview");
  await removeIfExists(previewDir);
  try {
    return await buildUpdatePreview(instance, target, javaType, (payload) => ctx.emitProgress({ ...payload, id, operation: "preview" } as DownloadProgress));
  } finally {
    await removeIfExists(previewDir);
  }
}

export async function updateInstance(ctx: BackendContext, args: UpdateInstanceArgs): Promise<void> {
  const id = sanitizeName(String(args.id).trim());
  if (ctx.isRunning(id)) throw new Error("cannot update while instance is running");
  if (ctx.isUpdateInProgress(id)) throw new Error("update already in progress for this instance");
  if (ctx.isReinstallInProgress(id)) throw new Error("reinstall already in progress for this instance");
  ctx.state.updateInProgress.add(id);
  try {
    await reinstallCore(ctx, id, String(args.packVersion), String(args.javaType ?? "java17+"), args.keepModIdentities ?? [], "update-pack");
  } finally {
    ctx.state.updateInProgress.delete(id);
    ctx.notifyInstanceOperationWaiters();
  }
}

export async function reinstallInstance(ctx: BackendContext, args: ReinstallInstanceArgs): Promise<void> {
  const id = sanitizeName(String(args.id).trim());
  if (ctx.isRunning(id)) throw new Error("cannot reinstall while instance is running");
  if (ctx.isReinstallInProgress(id)) throw new Error("reinstall already in progress for this instance");
  if (ctx.isUpdateInProgress(id)) throw new Error("update already in progress for this instance");
  ctx.state.reinstallInProgress.add(id);
  try {
    await reinstallCore(ctx, id, String(args.packVersion), String(args.javaType ?? "java17+"), [], "reinstall");
  } finally {
    ctx.state.reinstallInProgress.delete(id);
    ctx.notifyInstanceOperationWaiters();
  }
}

async function reinstallCore(
  ctx: BackendContext,
  id: string,
  packVersion: string,
  javaType: string,
  keepIds: string[],
  operation: "update-pack" | "reinstall",
): Promise<void> {
  const known = await ctx.knownInstanceIds();
  if (!known.has(id)) throw new Error("instance not found");
  const instance = instanceDir(id);
  const preserve = path.join(instance, preserveDirName);
  const persistentMods = persistentCustomModsDir(instance);
  if (operation === "update-pack") {
    const removed = await removeCustomModsExcept(persistentMods, new Set(keepIds));
    if (removed) ctx.emitProgress({ stage: "updating", pct: 0.05, operation, id, log_line: `Removed ${removed} custom mod(s) not selected to keep` });
  }
  ctx.emitProgress({
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
    (payload) => ctx.emitProgress({ ...payload, id, operation } as DownloadProgress),
    packVersion,
    javaType,
    instance,
    operation,
    id,
  );
  ctx.emitProgress({ stage: operation === "update-pack" ? "updating" : "reinstalling", pct: 0.75, operation, id, log_line: "Installing fresh pack files" });
  await installStagingContents(staging, instance);
  await removeIfExists(staging);
  await flattenNestedPack(instance);
  await prepareInstanceConfigs(instance, true);
  await restorePlayerData(instance, preserve);
  const settings = await loadInstanceSettings(id);
  settings.pack_version = packVersion;
  settings.pack_java_type = javaType;
  await ctx.saveAndRefreshSize(id, settings);
  await applyPersistentCustomMods(instance);
  await applyPersistentMinecraft(instance);
  await removeIfExists(preserve);
  ctx.emitProgress({ stage: "done", pct: 1, operation, id, log_line: operation === "update-pack" ? "Update complete" : "Clean reinstall complete" });
}
