import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error("Usage: node scripts/set-launcher-version.mjs <major.minor.patch>");
}

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriConfigPath = path.join(repoRoot, "apps", "launcher", "src-tauri", "tauri.conf.json");
const cargoManifestPath = path.join(repoRoot, "apps", "launcher", "src-tauri", "Cargo.toml");

const tauriConfig = JSON.parse(await readFile(tauriConfigPath, "utf8"));
tauriConfig.version = version;
await writeFile(tauriConfigPath, `${JSON.stringify(tauriConfig, null, 2)}\n`);

const cargoManifest = await readFile(cargoManifestPath, "utf8");
const packageVersionPattern = /(^\[package\][\s\S]*?^version = ")[^"]+("\r?$)/m;
if (!packageVersionPattern.test(cargoManifest)) {
  throw new Error("Could not find the launcher Cargo package version");
}
const versionedManifest = cargoManifest.replace(
  packageVersionPattern,
  `$1${version}$2`,
);
await writeFile(cargoManifestPath, versionedManifest);

console.log(`Launcher version set to ${version}`);
