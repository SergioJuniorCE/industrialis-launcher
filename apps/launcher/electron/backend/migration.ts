import fs from "node:fs/promises";
import path from "node:path";
import { copyTree, exists, removeIfExists } from "./fs-utils";

export const preserveDirName = ".reinstall-preserve";
const dirs = ["backups", "ESM", "journeymap", "resourcepacks", "saves", "schematics", "screenshots", "shaderpacks", "TCNodeTracker", "visualprospecting", "serverutilities"];
const files = ["shaders.properties", "BotaniaVars.dat", "localconfig.cfg", "options.txt", "optionsnf.txt", "optionsof.txt", "optionsshaders.txt", "servers.dat"];
const configs = ["vendingmachine/favourites", "GregTech/Pollution.cfg", "txloader/load/minecraft/sounds/music/menu", "gtnhintergalactic.cfg", "lwjgl3ify.cfg", "tectech.cfg"];

async function copyIfExists(source: string, destination: string): Promise<void> {
  if (!(await exists(source))) return;
  await removeIfExists(destination);
  await copyTree(source, destination);
}

export async function backupPlayerData(instance: string, preserve: string): Promise<void> {
  await removeIfExists(preserve);
  await fs.mkdir(preserve, { recursive: true });
  await copyIfExists(path.join(instance, "instance.json"), path.join(preserve, "instance.json"));
  await copyIfExists(path.join(instance, "persistent-minecraft"), path.join(preserve, "persistent-minecraft"));
  const icons = path.join(preserve, "launcher-icons");
  await fs.mkdir(icons, { recursive: true });
  for (const entry of await fs.readdir(instance, { withFileTypes: true }).catch(() => [])) {
    if (entry.name.startsWith("instance-icon")) await copyIfExists(path.join(instance, entry.name), path.join(icons, entry.name));
  }
  const minecraft = path.join(instance, ".minecraft");
  const preserveMinecraft = path.join(preserve, "minecraft");
  if (!(await exists(minecraft))) return;
  for (const name of dirs) await copyIfExists(path.join(minecraft, name), path.join(preserveMinecraft, name));
  for (const name of files) await copyIfExists(path.join(minecraft, name), path.join(preserveMinecraft, name));
  for (const rel of configs) await copyIfExists(path.join(minecraft, "config", rel), path.join(preserveMinecraft, "config", rel));
}

export async function restorePlayerData(instance: string, preserve: string): Promise<void> {
  await copyIfExists(path.join(preserve, "instance.json"), path.join(instance, "instance.json"));
  await copyIfExists(path.join(preserve, "persistent-minecraft"), path.join(instance, "persistent-minecraft"));
  const icons = path.join(preserve, "launcher-icons");
  for (const entry of await fs.readdir(icons, { withFileTypes: true }).catch(() => [])) await copyIfExists(path.join(icons, entry.name), path.join(instance, entry.name));
  const minecraft = path.join(instance, ".minecraft");
  const preserveMinecraft = path.join(preserve, "minecraft");
  await fs.mkdir(minecraft, { recursive: true });
  if (!(await exists(preserveMinecraft))) return;
  for (const name of dirs) await copyIfExists(path.join(preserveMinecraft, name), path.join(minecraft, name));
  for (const name of files) await copyIfExists(path.join(preserveMinecraft, name), path.join(minecraft, name));
  for (const rel of configs) await copyIfExists(path.join(preserveMinecraft, "config", rel), path.join(minecraft, "config", rel));
}

export async function wipeInstanceForReinstall(instance: string, preserve: string): Promise<void> {
  for (const entry of await fs.readdir(instance, { withFileTypes: true }).catch(() => [])) {
    const target = path.join(instance, entry.name);
    if (target === preserve) continue;
    await removeIfExists(target);
  }
}
