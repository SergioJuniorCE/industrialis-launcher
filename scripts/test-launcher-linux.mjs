import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const imageName = "industrialis-launcher-linux-test";
const requestedEngine = process.env.CONTAINER_ENGINE?.trim();
const candidates = requestedEngine ? [requestedEngine] : ["podman", "docker"];

function run(engine, args, stdio = "inherit") {
  return spawnSync(engine, args, {
    cwd: repoRoot,
    shell: false,
    stdio,
  });
}

function isInstalled(engine) {
  const result = run(engine, ["--version"], "ignore");
  return !result.error && result.status === 0;
}

function isRunning(engine) {
  const result = run(engine, ["info"], "ignore");
  return !result.error && result.status === 0;
}

const installedEngines = candidates.filter(isInstalled);
const engine = installedEngines.find(isRunning);

if (!engine) {
  if (installedEngines.includes("podman")) {
    console.error("Podman is installed but not running. Start it with `podman machine start`.");
  } else if (installedEngines.includes("docker")) {
    console.error("Docker is installed but its engine is not running. Start Docker Desktop.");
  } else {
    console.error("Install Podman or Docker to run Linux launcher packaging locally.");
  }
  process.exit(1);
}

console.log(`Building Linux packaging test image with ${engine}...`);
const build = run(engine, ["build", "--file", path.join(repoRoot, "scripts", "launcher-linux.Containerfile"), "--tag", imageName, repoRoot]);

if (build.error) throw build.error;
if (build.status !== 0) process.exit(build.status ?? 1);

console.log("Running Electron Forge Linux ZIP, DEB, and RPM makers...");
const make = run(engine, ["run", "--rm", imageName]);

if (make.error) throw make.error;
process.exit(make.status ?? 1);
