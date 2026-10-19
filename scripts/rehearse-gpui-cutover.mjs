// Source-removal rehearsal. Copies repository sources only; never profile data.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cp, lstat, mkdir, mkdtemp, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.join(repo, "apps/launcher");
const cache = path.join(source, "target");
await mkdir(cache, { recursive: true });
const snapshot = await mkdtemp(path.join(cache, "source-removal-"));
const app = path.join(snapshot, "apps/launcher");
await mkdir(app, { recursive: true });
await mkdir(path.join(snapshot, "scripts"));
const inventory = [];

async function copyOwned(relative) {
  const from = path.join(source, relative);
  const to = path.join(app, relative);
  async function record(file) {
    const info = await lstat(file);
    if (info.isSymbolicLink()) throw new Error(`Source snapshot refuses a symlink: ${file}`);
    const name = path.basename(file);
    if (["target", "dist", "node_modules", ".git"].includes(name) || name.startsWith(".env")) {
      throw new Error(`Generated or private input in source snapshot: ${file}`);
    }
    if (info.isDirectory()) {
      for (const child of (await readdir(file)).sort()) await record(path.join(file, child));
    } else if (info.isFile()) {
      const bytes = await readFile(file);
      inventory.push({ path: path.relative(source, file).replaceAll("\\", "/"), sha256: createHash("sha256").update(bytes).digest("hex") });
    } else throw new Error(`Unsupported source input: ${file}`);
  }
  await record(from);
  await cp(from, to, { recursive: true, errorOnExist: true, force: false });
}
for (const name of ["Cargo.toml", "Cargo.lock", "package.json", "LICENSE", "crates", "compatibility", "packaging"]) await copyOwned(name);
for (const name of ["gpui-profile-owner.test.mjs", "gpui-game-broker.test.mjs"]) {
  await cp(path.join(repo, "scripts", name), path.join(snapshot, "scripts", name));
}
await writeFile(path.join(snapshot, "source-inventory.json"), JSON.stringify(inventory, null, 2) + "\n");
const env = { ...process.env, CARGO_TARGET_DIR: cache };
// Do not allow a caller's profile override to reach any rehearsal executable.
delete env.INDUSTRIALIS_DATA_DIR;
function run(command, args, cwd = app) {
  console.log(`Rehearsal: ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, { cwd, env, stdio: "inherit", windowsHide: true });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Rehearsal failed with exit ${result.status}`);
}
console.log(`Native source-only snapshot: ${snapshot}`);
const cargo = ["--locked", "--offline", "--jobs", "1"];
run("cargo", ["test", "--workspace", "--lib", "--bins", "--tests", ...cargo, "--quiet"]);
run("cargo", [
  "build",
  "-p",
  "industrialis-app",
  "-p",
  "industrialis-backend",
  "-p",
  "industrialis-cli",
  "--example",
  "profile_ownership_smoke",
  "--example",
  "window_move_bench",
  "--example",
  "profile_owner",
  "--example",
  "game_lifecycle",
  "--bin",
  "industrialis-cli",
  ...cargo,
  "--quiet",
]);
// A distinct rustc invocation forces compilation of the actual app entrypoint
// from this snapshot even if Cargo reused identical workspace source hashes.
run("cargo", ["rustc", "-p", "industrialis-app", "--example", "profile_ownership_smoke", ...cargo, "--", "--cfg", "industrialis_standalone_rehearsal"]);
run(process.execPath, ["--test", "scripts/gpui-profile-owner.test.mjs", "scripts/gpui-game-broker.test.mjs"], snapshot);
run(path.join(cache, "debug/examples", `window_move_bench${process.platform === "win32" ? ".exe" : ""}`), []);
await writeFile(
  path.join(snapshot, "result.json"),
  JSON.stringify(
    {
      date: new Date().toISOString(),
      sourceFiles: inventory.length,
      originalLauncherPresent: false,
      rustTests: "passed",
      nativeProcessTests: "passed",
      hiddenRenderBenchmark: "passed",
      boundary: "Synthetic profiles only; no live providers, production installer or native Mac/Linux acceptance.",
    },
    null,
    2,
  ) + "\n",
);
console.log(`Source-removal rehearsal passed. Evidence: ${path.join(snapshot, "result.json")}`);
