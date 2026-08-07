import fs from "node:fs/promises";
import path from "node:path";
import { app } from "electron";
import { exists } from "./fs-utils";
import { persistentMinecraftDir } from "./minecraft-files";

const adityaFiles = [
  "config/salisarcana.cfg",
  "config/salisarcana/addons.cfg",
  "config/salisarcana/bugfixes.cfg",
  "config/salisarcana/commands.cfg",
  "config/salisarcana/enhancements.cfg",
  "config/salisarcana/mod_integrations.cfg",
  "config/salisarcana/thaumcraft_configuration.cfg",
  "config/Betterloadingscreen/betterloadingscreen.cfg",
  "config/RandomThings.cfg",
];

function bundledConfigDir(): string {
  const packaged = path.join(process.resourcesPath, "config");
  return app.isPackaged ? packaged : path.join(app.getAppPath(), "electron", "config");
}

function gamePath(instance: string, rel: string): string {
  return path.join(instance, ".minecraft", rel);
}

async function copyBundledToGame(instance: string, rel: string): Promise<void> {
  const source = path.join(bundledConfigDir(), rel.replace(/^config[\\/]/u, ""));
  if (!(await exists(source))) throw new Error(`bundled preset file missing: ${source}`);
  const destination = gamePath(instance, rel);
  await fs.mkdir(path.dirname(destination), { recursive: true });
  await fs.copyFile(source, destination);
}

async function copyGameToPersistent(instance: string, rel: string): Promise<void> {
  const source = gamePath(instance, rel);
  if (!(await exists(source))) return;
  const destination = path.join(persistentMinecraftDir(instance), rel);
  await fs.mkdir(path.dirname(destination), { recursive: true });
  await fs.copyFile(source, destination);
}

async function patchGadomancy(instance: string): Promise<void> {
  const target = gamePath(instance, "config/gadomancy.cfg");
  if (!(await exists(target))) return;
  const content = await fs.readFile(target, "utf8");
  await fs.writeFile(target, content.replaceAll("B:ancientStoneRecipes=false", "B:ancientStoneRecipes=true"), "utf8");
}

export async function applyConfigPreset(id: string, instance: string, enabled: boolean): Promise<void> {
  if (id !== "aditya") throw new Error(`unknown config preset: ${id}`);
  if (enabled) {
    for (const rel of adityaFiles) await copyBundledToGame(instance, rel);
    await patchGadomancy(instance);
    for (const rel of adityaFiles) await copyGameToPersistent(instance, rel);
    if (await exists(gamePath(instance, "config/gadomancy.cfg"))) {
      await patchGadomancy(instance);
      await copyGameToPersistent(instance, "config/gadomancy.cfg");
    }
    return;
  }
  for (const rel of [...adityaFiles, "config/gadomancy.cfg"]) {
    await fs.rm(path.join(persistentMinecraftDir(instance), rel), { force: true });
  }
}

export async function getConfigPresetStatus(id: string, instance: string): Promise<boolean> {
  if (id !== "aditya") throw new Error(`unknown config preset: ${id}`);
  for (const rel of [...adityaFiles, "config/gadomancy.cfg"]) {
    if (await exists(path.join(persistentMinecraftDir(instance), rel))) return true;
  }
  return false;
}
