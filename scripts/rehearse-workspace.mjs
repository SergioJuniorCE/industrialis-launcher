// Rehearse workspace dependency pruning in an isolated source snapshot, never in place.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, lstat, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const cache = path.join(repo, "apps/launcher/target");
const verify = process.argv.includes("--verify");
const online = process.argv.includes("--online");
if (process.argv.slice(2).some((arg) => !["--verify", "--online"].includes(arg)))
  throw new Error("Usage: node scripts/rehearse-workspace.mjs [--verify] [--online]");
await mkdir(cache, { recursive: true });
const snapshot = await mkdtemp(path.join(cache, "workspace-removal-"));
const env = { ...process.env, CARGO_TARGET_DIR: cache };
delete env.INDUSTRIALIS_DATA_DIR;
delete env.INDUSTRIALIS_GOOGLE_DRIVE_CLIENT_ID;
function run(command, args, cwd = snapshot, capture = false) {
  console.log(`Workspace rehearsal: ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, {
    cwd,
    env,
    windowsHide: true,
    shell: false,
    stdio: capture ? "pipe" : "inherit",
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Workspace rehearsal failed (${result.status}): ${result.stderr ?? command}`);
  return result.stdout;
}
// Explicit source roots exclude profiles, attachments, tool state and generated outputs.
const roots = ["apps/launcher/", "apps/server/", "apps/dashboard/", "apps/website/", "packages/", "scripts/", "docs/", ".github/"];
const rootFiles = new Set([
  "package.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "turbo.json",
  "README.md",
  "LICENSE",
  ".gitignore",
  ".oxlintrc.json",
  ".oxfmtrc.json",
  "tsconfig.json",
]);
const retired = new Set([
  "scripts/build-launcher.ps1",
  "scripts/build-launcher.sh",
  "scripts/run-build-launcher.mjs",
  "scripts/check-launcher-node.mjs",
  ".github/workflows/launcher-release.yml",
]);
const candidates = [...new Set(run("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], repo, true).split("\0").filter(Boolean))];
const inventory = [];
for (const relative of candidates.sort()) {
  if ((!rootFiles.has(relative) && !roots.some((root) => relative.startsWith(root))) || retired.has(relative)) continue;
  if (relative.split("/").some((part) => ["target", "dist", "node_modules", ".next", ".astro", ".git"].includes(part) || part.startsWith(".env"))) continue;
  if (relative.startsWith("scripts/extract-zip-node24/")) continue;
  const source = path.join(repo, relative);
  const info = await lstat(source);
  if (!info.isFile() || info.isSymbolicLink()) throw new Error(`Unsupported source input: ${relative}`);
  const destination = path.join(snapshot, relative);
  await mkdir(path.dirname(destination), { recursive: true });
  await copyFile(source, destination);
  inventory.push({
    path: relative,
    sha256: createHash("sha256")
      .update(await readFile(source))
      .digest("hex"),
  });
}
const packageFile = path.join(snapshot, "package.json");
const manifest = JSON.parse(await readFile(packageFile, "utf8"));
for (const [name, command] of Object.entries(manifest.scripts)) {
  if (name.includes(":electron") || command.includes("run-build-launcher.mjs")) delete manifest.scripts[name];
}
manifest.scripts.dev = "turbo dev";
await writeFile(packageFile, `${JSON.stringify(manifest, null, 2)}\n`);
const workspaceFile = path.join(snapshot, "pnpm-workspace.yaml");
const workspace = (await readFile(workspaceFile, "utf8"))
  .replace(/^overrides:\r?\n  "@electron\/packager>extract-zip":.*\r?\n/m, "")
  .replace(/^  electron-winstaller: true\r?\n/m, "");
await writeFile(workspaceFile, workspace);
await writeFile(
  path.join(snapshot, "scripts/set-launcher-version.mjs"),
  'import { setVersion } from "./launcher-version.mjs";\nimport { fileURLToPath } from "node:url";\nawait setVersion(fileURLToPath(new URL("../apps/launcher", import.meta.url)), process.argv[2] ?? "");\n',
);
await writeFile(path.join(snapshot, "source-inventory.json"), `${JSON.stringify(inventory, null, 2)}\n`);
console.log(`Isolated workspace without Electron: ${snapshot}`);
// Give source scanners their own boundary: the parent ignores the entire target tree.
// This empty local repository has no remotes and is never committed or published.
run("git", ["init", "--quiet"]);
// The native Corepack JS entry works with spawnSync on Windows, unlike corepack.cmd.
const corepack = process.env.LAUNCHER_REHEARSAL_COREPACK ?? path.join(path.dirname(process.execPath), "node_modules/corepack/dist/corepack.js");
if (!(await lstat(corepack).catch(() => null))?.isFile())
  throw new Error("Set LAUNCHER_REHEARSAL_COREPACK to the local corepack/dist/corepack.js file; no global tooling is modified.");
const pnpm = (...args) => run(process.execPath, [corepack, "pnpm", ...args]);
const network = online ? [] : ["--offline"];
pnpm("install", "--lockfile-only", ...network, "--ignore-scripts");
const lock = await readFile(path.join(snapshot, "pnpm-lock.yaml"), "utf8");
if (/^  ['"]?(?:@electron\/[^:'"\n]+|electron|electron-winstaller|electron-packager|electron-forge)@/m.test(lock)) {
  throw new Error("Electron importer/runtime/build dependencies remain in the pruned lockfile");
}
if (verify) {
  pnpm("install", "--frozen-lockfile", ...network, "--ignore-scripts");
  pnpm("lint");
  pnpm("--filter", "@industrialis/server-contracts", "build");
  pnpm("--recursive", "--if-present", "typecheck");
  pnpm("--recursive", "--if-present", "test");
  pnpm("--filter", "@industrialis/server", "--filter", "@industrialis/dashboard", "--filter", "@industrialis/website", "build");
  run(process.execPath, ["--test", "scripts/packaging.test.mjs", "scripts/notices.test.mjs", "scripts/windows-runtime.test.mjs"]);
}
const report = {
  date: new Date().toISOString(),
  sourceFiles: inventory.length,
  originalLauncherPresent: false,
  lockfilePruned: true,
  electronDependenciesRemaining: false,
  workspaceChecks: verify ? "passed" : "not run",
  dependencyNetworkAllowed: online,
  changesAppliedOnlyToSnapshot: true,
  boundary:
    "Repository/dependency rehearsal only. No production workflow/feed/download switch, account migration, installed replacement, signing, publication or Electron removal in the working checkout.",
};
await writeFile(path.join(snapshot, "result.json"), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
