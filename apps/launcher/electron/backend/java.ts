import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";
import type { JavaInfo } from "./types";

interface JavaRuntimeDetails {
  version: string;
  majorVersion: number;
  architecture: string;
  vendor: string;
}

function propertyValue(output: string, property: string): string | null {
  const escaped = property.replaceAll(".", "\\.");
  return output.match(new RegExp(`^\\s*${escaped}\\s*=\\s*(.+?)\\s*$`, "mu"))?.[1]?.trim() ?? null;
}

function javaMajorVersion(version: string): number | null {
  const parts = version.match(/\d+/gu)?.map(Number) ?? [];
  if (parts.length === 0) return null;
  return parts[0] === 1 && parts.length > 1 ? parts[1] : parts[0];
}

function normalizeArchitecture(architecture: string | null): string {
  const value = architecture?.trim().toLowerCase() ?? "";
  if (["x86_64", "x64"].includes(value)) return "amd64";
  if (["i386", "i486", "i586", "i686"].includes(value)) return "x86";
  if (value === "aarch64") return "arm64";
  return value || "unknown";
}

export function parseJavaRuntimeDetails(output: string): JavaRuntimeDetails | null {
  const version = propertyValue(output, "java.version") ?? output.match(/(?:java|openjdk)\s+version\s+"([^"]+)"/iu)?.[1] ?? null;
  if (!version) return null;
  const majorVersion = javaMajorVersion(version);
  if (!majorVersion) return null;
  return {
    version,
    majorVersion,
    architecture: normalizeArchitecture(propertyValue(output, "os.arch")),
    vendor: propertyValue(output, "java.vendor") ?? "Unknown vendor",
  };
}

export function javaFromHome(home: string): string | null {
  const trimmed = home.trim();
  const bin = path.basename(trimmed).toLowerCase() === "bin" ? trimmed : path.join(trimmed, "bin");
  const candidates = process.platform === "win32" ? [path.join(bin, "java.exe"), path.join(bin, "java")] : [path.join(bin, "java")];
  return candidates.find((candidate) => existsSync(candidate)) ?? null;
}

export function javaPath(): string | null {
  const fromHome = process.env.JAVA_HOME ? javaFromHome(process.env.JAVA_HOME) : null;
  if (fromHome) return fromHome;
  const separator = process.platform === "win32" ? ";" : ":";
  for (const directory of (process.env.PATH ?? "").split(separator).filter(Boolean)) {
    const candidate = process.platform === "win32" ? path.join(directory, "java.exe") : path.join(directory, "java");
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

async function addJavaHomes(root: string, depth: number, candidates: Set<string>): Promise<void> {
  if (!root || depth < 0) return;
  const java = javaFromHome(root);
  if (java) {
    candidates.add(java);
    return;
  }
  const entries = await fs.readdir(root, { withFileTypes: true }).catch(() => []);
  await Promise.all(entries.filter((entry) => entry.isDirectory()).map((entry) => addJavaHomes(path.join(root, entry.name), depth - 1, candidates)));
}

function windowsJavaRoots(): Array<{ root: string; depth: number }> {
  const roots: Array<{ root: string; depth: number }> = [];
  const vendors = ["Java", "Eclipse Adoptium", "Microsoft", "Amazon Corretto", "BellSoft", "Zulu", "Semeru"];
  for (const programRoot of [process.env.ProgramFiles, process.env["ProgramFiles(x86)"]]) {
    if (!programRoot) continue;
    roots.push(...vendors.map((vendor) => ({ root: path.join(programRoot, vendor), depth: 2 })));
  }
  if (process.env.LOCALAPPDATA) {
    roots.push(...vendors.map((vendor) => ({ root: path.join(process.env.LOCALAPPDATA!, "Programs", vendor), depth: 2 })));
  }
  if (process.env.APPDATA) {
    roots.push(
      { root: path.join(process.env.APPDATA, ".minecraft", "runtime"), depth: 5 },
      { root: path.join(process.env.APPDATA, "PrismLauncher", "java"), depth: 3 },
    );
  }
  return roots;
}

async function discoverJavaCandidates(): Promise<string[]> {
  const candidates = new Set<string>();
  const addCandidate = (candidate: string | null) => {
    if (candidate) candidates.add(path.resolve(candidate));
  };
  addCandidate(process.env.JAVA_HOME ? javaFromHome(process.env.JAVA_HOME) : null);

  const separator = process.platform === "win32" ? ";" : ":";
  for (const directory of (process.env.PATH ?? "").split(separator).filter(Boolean)) {
    addCandidate(process.platform === "win32" ? path.join(directory, "java.exe") : path.join(directory, "java"));
  }

  if (process.platform === "win32") {
    await Promise.all(windowsJavaRoots().map(({ root, depth }) => addJavaHomes(root, depth, candidates)));
  } else {
    for (const root of ["/usr/lib/jvm", "/Library/Java/JavaVirtualMachines"]) await addJavaHomes(root, 4, candidates);
  }

  return [...candidates].filter((candidate) => existsSync(candidate));
}

function inspectJava(candidate: string): Promise<JavaInfo | null> {
  return new Promise((resolve) => {
    execFile(candidate, ["-XshowSettings:properties", "-version"], { windowsHide: true, timeout: 8_000, maxBuffer: 1024 * 1024 }, (_error, stdout, stderr) => {
      const details = parseJavaRuntimeDetails(`${stderr}\n${stdout}`);
      resolve(details ? { path: candidate, ...details } : null);
    });
  });
}

export async function detectJava(): Promise<JavaInfo[]> {
  const inspected = await Promise.all((await discoverJavaCandidates()).map(inspectJava));
  const unique = new Map<string, JavaInfo>();
  for (const java of inspected) {
    if (!java) continue;
    const key = java.path.toLowerCase();
    if (!unique.has(key)) unique.set(key, java);
  }
  return [...unique.values()].sort(
    (left, right) =>
      right.majorVersion - left.majorVersion || right.version.localeCompare(left.version, undefined, { numeric: true }) || left.path.localeCompare(right.path),
  );
}

export async function testJava(pathOverride?: string): Promise<string> {
  const java = pathOverride?.trim() || javaPath();
  if (!java) throw new Error("no Java configured or found - set JAVA_HOME or pick a Java path");
  return new Promise((resolve, reject) => {
    execFile(java, ["-version"], { windowsHide: true }, (error, stdout, stderr) => {
      const output = `${stderr}${stdout}`;
      if (error) reject(new Error(`Java test failed (${java}):\n${output}`));
      else resolve(`OK - ${java}\n${output}`);
    });
  });
}

export function javaGuiExecutable(java: string): string {
  if (process.platform !== "win32" || !/java\.exe$/iu.test(java)) return java;
  const javaw = java.replace(/java\.exe$/iu, "javaw.exe");
  return existsSync(javaw) ? javaw : java;
}
