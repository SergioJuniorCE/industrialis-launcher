import crypto from "node:crypto";
import { existsSync } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";
import { exists, mapConcurrent, readJson } from "./fs-utils";
import { flattenNestedPack } from "./pack";
import type { LaunchConfig, PatchAssetIndex } from "./types";

const defaultLibraryRepo = "https://libraries.minecraft.net/";

interface GradleSpec {
  group: string;
  artifact: string;
  version: string;
  classifier?: string;
  extension: string;
}

interface PatchLibraryEntry {
  name: string;
  url?: string;
  "MMC-hint"?: string;
  "MMC-absoluteUrl"?: string;
  downloads?: { artifact?: { url: string } };
  rules?: Array<{ action: string; os?: { name?: string } }>;
  natives?: unknown;
}

interface PatchJson {
  order?: number;
  mainClass?: string;
  "+mainClass"?: string;
  minecraftArguments?: string;
  assetIndex?: PatchAssetIndex;
  mainJar?: { name: string; downloads: { artifact?: { url: string } } };
  libraries?: PatchLibraryEntry[];
  "+jvmArgs"?: string[];
  "+args"?: string[];
  "+tweakers"?: string[];
}

interface MmcPackJson {
  components: Array<{ uid: string; version?: string; cachedVersion?: string }>;
}

export type EmitLaunchLog = (id: string, stream: string, line: string) => void;

function currentOsName(): string {
  if (process.platform === "win32") return "windows";
  if (process.platform === "darwin") return "osx";
  return "linux";
}

function parseGradleSpec(value: string): GradleSpec | null {
  const [coordinates, extension = "jar"] = value.split("@", 2);
  const parts = coordinates.split(":");
  if (parts.length < 3) return null;
  return { group: parts[0], artifact: parts[1], version: parts[2], classifier: parts[3], extension };
}

function filename(spec: GradleSpec): string {
  return `${spec.artifact}-${spec.version}${spec.classifier ? `-${spec.classifier}` : ""}.${spec.extension}`;
}

function storagePath(spec: GradleSpec): string {
  return path.join(spec.group.replaceAll(".", "/"), spec.artifact, spec.version, filename(spec));
}

function libraryAllowed(rules?: PatchLibraryEntry["rules"]): boolean {
  if (!rules?.length) return true;
  let allowed = false;
  for (const rule of rules) {
    const applies = !rule.os || rule.os.name === currentOsName();
    if (applies && rule.action !== "defer") allowed = rule.action === "allow";
  }
  return allowed;
}

function isNativeOnly(entry: PatchLibraryEntry): boolean {
  return entry.natives !== undefined && !entry.downloads?.artifact;
}

function resolveLibraryUrl(entry: PatchLibraryEntry, spec: GradleSpec): string {
  const absolute = entry["MMC-absoluteUrl"]?.trim();
  if (absolute) return absolute;
  const artifact = entry.downloads?.artifact?.url.trim();
  if (artifact) return artifact;
  const repo = entry.url?.trim() || defaultLibraryRepo;
  return `${repo.replace(/\/$/u, "")}/${storagePath(spec).replaceAll("\\", "/")}`;
}

async function downloadToFile(url: string, destination: string): Promise<void> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`download failed (${url}): HTTP ${response.status}`);
  await fs.mkdir(path.dirname(destination), { recursive: true });
  await fs.writeFile(destination, Buffer.from(await response.arrayBuffer()));
}

function sha1File(content: Buffer): string {
  return crypto.createHash("sha1").update(content).digest("hex");
}

function assetDigest(hash: string): string {
  return hash.replace(/^sha1:/u, "");
}
function assetPath(assetsDir: string, hash: string): string {
  const digest = assetDigest(hash);
  return path.join(assetsDir, "objects", digest.slice(0, 2), digest);
}
function assetUrl(hash: string): string {
  const digest = assetDigest(hash);
  return `https://resources.download.minecraft.net/${digest.slice(0, 2)}/${digest}`;
}

async function ensureAssets(assetsDir: string, index: PatchAssetIndex, id: string, emit: EmitLaunchLog): Promise<void> {
  emit(id, "system", `Syncing Minecraft assets (${index.id})…`);
  const indexPath = path.join(assetsDir, "indexes", `${index.id}.json`);
  let indexBuffer = await fs.readFile(indexPath).catch(() => null);
  if (indexBuffer && index.sha1 && sha1File(indexBuffer) !== index.sha1) indexBuffer = null;
  if (!indexBuffer) {
    if (!index.url) throw new Error(`asset index ${index.id} has no download URL`);
    await downloadToFile(index.url, indexPath);
    indexBuffer = await fs.readFile(indexPath);
    if (index.sha1 && sha1File(indexBuffer) !== index.sha1) {
      await fs.rm(indexPath, { force: true });
      throw new Error(`asset index ${index.id} checksum mismatch`);
    }
  }
  const parsed = JSON.parse(indexBuffer.toString("utf8")) as { objects?: Record<string, { hash: string; size: number }> };
  const objects = Object.values(parsed.objects ?? {});
  const missing = (
    await mapConcurrent(objects, async (object) => {
      const target = assetPath(assetsDir, object.hash);
      const stat = await fs.stat(target).catch(() => null);
      return !stat || stat.size !== object.size ? object : null;
    })
  ).filter((object): object is (typeof objects)[number] => object !== null);
  if (!missing.length) {
    emit(id, "system", `Assets up to date (${objects.length} objects)`);
    return;
  }
  emit(id, "system", `Downloading ${missing.length} of ${objects.length} asset objects…`);
  await mapConcurrent(missing, async (object) => {
    const target = assetPath(assetsDir, object.hash);
    await downloadToFile(assetUrl(object.hash), target);
    const stat = await fs.stat(target).catch(() => null);
    if (!stat || stat.size !== object.size) throw new Error(`asset object ${assetDigest(object.hash)} did not verify after download`);
  });
  emit(id, "system", "Asset sync complete");
}

async function ensureLibrary(packDir: string, entry: PatchLibraryEntry, id: string, emit: EmitLaunchLog): Promise<string | null> {
  if (!libraryAllowed(entry.rules) || isNativeOnly(entry)) return null;
  const spec = parseGradleSpec(entry.name);
  if (!spec) throw new Error(`bad library name: ${entry.name}`);
  const root = path.join(packDir, "libraries");
  const candidates = [path.join(root, storagePath(spec))];
  if (entry["MMC-hint"] === "local") candidates.unshift(path.join(root, filename(spec)));
  for (const candidate of candidates) if (await exists(candidate)) return candidate;
  const destination = entry["MMC-hint"] === "local" ? path.join(root, filename(spec)) : path.join(root, storagePath(spec));
  emit(id, "system", `Downloading ${entry.name}`);
  await downloadToFile(resolveLibraryUrl(entry, spec), destination);
  return destination;
}

async function ensureMainJar(
  packDir: string,
  jar: NonNullable<PatchJson["mainJar"]>,
  minecraftVersion: string,
  id: string,
  emit: EmitLaunchLog,
): Promise<string> {
  const destination = path.join(packDir, ".minecraft", "versions", minecraftVersion, `${minecraftVersion}.jar`);
  if (await exists(destination)) return destination;
  const url = jar.downloads.artifact?.url;
  if (!url) throw new Error("minecraft mainJar has no download URL");
  emit(id, "system", `Downloading Minecraft ${minecraftVersion}`);
  await downloadToFile(url, destination);
  return destination;
}

function effectiveVersion(component: MmcPackJson["components"][number]): string {
  return component.version ?? component.cachedVersion ?? "";
}

export async function buildLaunchConfig(instance: string, id: string, emit: EmitLaunchLog): Promise<LaunchConfig> {
  await flattenNestedPack(instance);
  const mmcPath = path.join(instance, "mmc-pack.json");
  const mmc = await readJson<MmcPackJson>(mmcPath);
  if (!mmc) throw new Error(`invalid Minecraft pack metadata: ${mmcPath}`);
  const patches = (
    await mapConcurrent(mmc.components, async (component) => {
      const patchPath = path.join(instance, "patches", `${component.uid}.json`);
      if (!(await exists(patchPath))) return null;
      const patch = await readJson<PatchJson>(patchPath);
      if (!patch) throw new Error(`invalid Minecraft patch metadata: ${patchPath}`);
      return { order: Number(patch.order ?? 0), uid: component.uid, patch };
    })
  ).filter((patch): patch is { order: number; uid: string; patch: PatchJson } => patch !== null);
  patches.sort((a, b) => a.order - b.order);
  let mainClass = "net.minecraft.launchwrapper.Launch";
  let minecraftVersion = "1.12.2";
  let jvmArgs = ["-Duser.language=en"];
  const programArgs: string[] = [];
  const libraries: Array<{ path: string; version: string; key: string }> = [];
  const index = new Map<string, number>();
  let minecraftArgumentsTemplate: string | undefined;
  let assetIndex: PatchAssetIndex | undefined;
  let mainJar: NonNullable<PatchJson["mainJar"]> | undefined;
  const componentByUid = new Map(mmc.components.map((component) => [component.uid, component]));
  const libraryPromises = new Map<string, Promise<string | null>>();
  const resolveLibrary = (entry: PatchLibraryEntry): Promise<string | null> => {
    const key = [entry.name, entry["MMC-hint"] ?? "", entry["MMC-absoluteUrl"] ?? "", entry.url ?? ""].join("\0");
    const existing = libraryPromises.get(key);
    if (existing) return existing;
    const promise = ensureLibrary(instance, entry, id, emit);
    libraryPromises.set(key, promise);
    return promise;
  };

  for (const loaded of patches) {
    const { patch } = loaded;
    mainClass = patch["+mainClass"] ?? patch.mainClass ?? mainClass;
    jvmArgs.push(...(patch["+jvmArgs"] ?? []));
    for (const tweaker of patch["+tweakers"] ?? []) programArgs.push("--tweakClass", tweaker);
    const extraArgs = patch["+args"] ?? [];
    for (let i = 0; i < extraArgs.length; i += 1) {
      if (extraArgs[i] === "--tweakClass" && extraArgs[i + 1]) {
        programArgs.push("--tweakClass", extraArgs[i + 1]);
        i += 1;
      } else if (extraArgs[i].startsWith("--tweakClass=")) programArgs.push(extraArgs[i]);
      else jvmArgs.push(extraArgs[i]);
    }
    if (loaded.uid === "net.minecraft") {
      const component = componentByUid.get(loaded.uid);
      if (component) minecraftVersion = effectiveVersion(component);
      mainJar = patch.mainJar;
      minecraftArgumentsTemplate = patch.minecraftArguments;
      assetIndex = patch.assetIndex;
    }
  }
  const resolvedLibraries = await mapConcurrent(
    patches.flatMap(({ patch }) => patch.libraries ?? []),
    async (entry) => {
      const resolved = await resolveLibrary(entry);
      const spec = parseGradleSpec(entry.name);
      if (!resolved || !spec) return null;
      return { resolved, spec };
    },
  );
  for (const library of resolvedLibraries) {
    if (!library) continue;
    const { resolved, spec } = library;
    const key = `${spec.group}:${spec.artifact}`;
    const previous = index.get(key);
    if (previous === undefined) {
      index.set(key, libraries.length);
      libraries.push({ path: resolved, version: spec.version, key });
    } else if (spec.version > libraries[previous].version) libraries[previous] = { path: resolved, version: spec.version, key };
  }
  if (mainJar) {
    const resolved = await ensureMainJar(instance, mainJar, minecraftVersion, id, emit);
    const spec = parseGradleSpec(mainJar.name);
    if (spec) {
      const key = `${spec.group}:${spec.artifact}`;
      const previous = index.get(key);
      if (previous === undefined) {
        index.set(key, libraries.length);
        libraries.push({ path: resolved, version: minecraftVersion, key });
      } else libraries[previous] = { path: resolved, version: minecraftVersion, key };
    } else libraries.push({ path: resolved, version: minecraftVersion, key: mainJar.name });
  }
  if (!libraries.length) throw new Error("no libraries resolved for launch");
  const gameDir = path.join(instance, ".minecraft");
  const assetsDir = path.join(gameDir, "assets");
  const nativesDir = path.join(instance, "natives");
  await fs.mkdir(nativesDir, { recursive: true });
  jvmArgs.push(`-Djava.library.path=${nativesDir}`);
  return {
    mainClass,
    minecraftVersion,
    libraries: libraries.map((entry) => entry.path),
    gameDir,
    assetsDir,
    jvmArgs,
    programArgs,
    minecraftArgumentsTemplate,
    assetIndex,
  };
}

export async function syncAssets(config: LaunchConfig, id: string, emit: EmitLaunchLog): Promise<void> {
  if (config.assetIndex) await ensureAssets(config.assetsDir, config.assetIndex, id, emit);
}

export function buildClasspath(libraries: string[]): string {
  return libraries.join(path.delimiter);
}

export function expandMinecraftArguments(template: string, tokens: Record<string, string>): string[] {
  return template
    .split(/\s+/u)
    .filter(Boolean)
    .map((part) => part.replace(/\$\{([^}]+)\}/gu, (_match, key: string) => tokens[key] ?? ""));
}

export function splitCommandArgs(command: string): string[] {
  const args: string[] = [];
  let current = "";
  let quoted = false;
  for (const char of command) {
    if (char === '"') quoted = !quoted;
    else if ((char === " " || char === "\t") && !quoted) {
      if (current) {
        args.push(current);
        current = "";
      }
    } else current += char;
  }
  if (current) args.push(current);
  return args;
}

export function substituteCommandVars(template: string, vars: Record<string, string>): string {
  return Object.entries(vars).reduce((value, [key, replacement]) => value.replaceAll(`$${key}`, replacement), template);
}

export async function writeLaunchArgfile(target: string, args: string[]): Promise<void> {
  await fs.writeFile(target, `${args.map((arg) => JSON.stringify(arg)).join("\n")}\n`, "utf8");
}

export async function runShellCommand(command: string, cwd: string, env: Record<string, string>): Promise<void> {
  const { spawn } = await import("node:child_process");
  const shellName = process.platform === "win32" ? "cmd.exe" : "/bin/sh";
  const shellArgs = process.platform === "win32" ? ["/C", command.trim()] : ["-c", command.trim()];
  await new Promise<void>((resolve, reject) => {
    const child = spawn(shellName, shellArgs, { cwd, env: { ...process.env, ...env }, stdio: "ignore", windowsHide: true });
    child.once("error", (error) => reject(new Error(`failed to run command (${command}): ${error.message}`)));
    child.once("exit", (code) => (code === 0 ? resolve() : reject(new Error(`command failed (${command}): exit ${code}`))));
  });
}

export function javaFromHome(home: string): string | null {
  const bin = path.join(home.trim(), "bin");
  const candidates = process.platform === "win32" ? [path.join(bin, "java.exe"), path.join(bin, "java")] : [path.join(bin, "java")];
  return candidates.find((candidate) => existsSync(candidate)) ?? null;
}

export function javaPath(): string | null {
  const fromHome = process.env.JAVA_HOME ? javaFromHome(process.env.JAVA_HOME) : null;
  if (fromHome) return fromHome;
  const separator = process.platform === "win32" ? ";" : ":";
  for (const directory of (process.env.PATH ?? "").split(separator)) {
    const candidate = process.platform === "win32" ? path.join(directory, "java.exe") : path.join(directory, "java");
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

export async function detectJava(): Promise<Array<{ path: string; version: number }>> {
  const { execFile } = await import("node:child_process");
  const candidates = new Set<string>();
  const addHome = (home: string | undefined) => {
    if (home) {
      const candidate = javaFromHome(home);
      if (candidate) candidates.add(candidate);
    }
  };
  addHome(process.env.JAVA_HOME);
  const pathJava = javaPath();
  if (pathJava) candidates.add(pathJava);
  if (process.platform === "win32") {
    for (const root of [process.env.ProgramFiles, process.env["ProgramFiles(x86)"], process.env.LOCALAPPDATA]) {
      if (!root) continue;
      for (const base of [path.join(root, "Java"), path.join(root, "Eclipse Adoptium"), path.join(root, "Microsoft")]) {
        for (const entry of await fs.readdir(base, { withFileTypes: true }).catch(() => [])) if (entry.isDirectory()) addHome(path.join(base, entry.name));
      }
    }
  }
  const result: Array<{ path: string; version: number }> = [];
  for (const candidate of candidates) {
    const version = await new Promise<number | null>((resolve) => {
      execFile(candidate, ["-version"], { windowsHide: true }, (_error, _stdout, stderr) => {
        const match = `${stderr}`.match(/(?:version\s+|JAVA_VERSION=")([0-9]+)/u);
        resolve(match ? Number(match[1]) : null);
      });
    });
    if (version) result.push({ path: candidate, version });
  }
  return result.sort((a, b) => b.version - a.version || a.path.localeCompare(b.path));
}

export async function testJava(pathOverride?: string): Promise<string> {
  const java = pathOverride?.trim() || javaPath();
  if (!java) throw new Error("no Java configured or found — set JAVA_HOME or pick a Java path");
  const { execFile } = await import("node:child_process");
  return new Promise((resolve, reject) => {
    execFile(java, ["-version"], { windowsHide: true }, (error, stdout, stderr) => {
      const output = `${stderr}${stdout}`;
      if (error) reject(new Error(`Java test failed (${java}):\n${output}`));
      else resolve(`OK — ${java}\n${output}`);
    });
  });
}

export function javaGuiExecutable(java: string): string {
  if (process.platform !== "win32") return java;
  if (!/java\.exe$/iu.test(java)) return java;
  const javaw = java.replace(/java\.exe$/iu, "javaw.exe");
  return existsSync(javaw) ? javaw : java;
}

export function instanceCommandVars(id: string, name: string, instance: string, java: string): Record<string, string> {
  return { INST_NAME: name, INST_ID: id, INST_DIR: instance, INST_MC_DIR: path.join(instance, ".minecraft"), INST_JAVA: java };
}
