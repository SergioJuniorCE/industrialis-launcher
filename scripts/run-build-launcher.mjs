import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");
const target = process.argv[2] ?? "all";
const outputDirectory = process.argv[3];

const isWindows = process.platform === "win32";
const script = path.join(
  __dirname,
  isWindows ? "build-launcher.ps1" : "build-launcher.sh",
);

/** @type {string[]} */
let command;
/** @type {string[]} */
let args;

if (isWindows) {
  command = "powershell";
  args = [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    script,
    "-Target",
    target,
  ];
  if (outputDirectory) {
    args.push("-OutputDirectory", outputDirectory);
  }
} else {
  command = "bash";
  args = [script, target];
  if (outputDirectory) {
    args.push(outputDirectory);
  }
}

const result = spawnSync(command, args, {
  cwd: repoRoot,
  stdio: "inherit",
  shell: false,
});

if (result.error) {
  console.error(result.error);
  process.exit(1);
}

process.exit(result.status ?? 1);
