import { spawn } from "node:child_process";
import { readdir, stat } from "node:fs/promises";
import path from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const workspace = fileURLToPath(new URL("../apps/launcher/", import.meta.url));
const prefix = "[gpui dev]";
const ignored = new Set(["target", "node_modules", ".git", "dist"]);

// A small, scoped poll also catches editor atomic saves/new directories on all
// supported platforms. Never walk target/ or a personal launcher profile.
export async function sourceSnapshot(root) {
  const files = [];
  async function scan(relative) {
    const absolute = path.join(root, relative);
    let entries;
    try {
      entries = await readdir(absolute, { withFileTypes: true });
    } catch (error) {
      if (error.code === "ENOENT") return;
      throw error;
    }
    await Promise.all(
      entries.map(async (entry) => {
        if (ignored.has(entry.name) || /(?:~|\.sw[op]|\.tmp)$/.test(entry.name)) return;
        const name = path.join(relative, entry.name);
        if (entry.isDirectory()) return scan(name);
        if (entry.isFile()) await record(name);
        // Do not follow symlinks into arbitrary user/build directories.
      }),
    );
  }
  async function record(relative) {
    try {
      const info = await stat(path.join(root, relative), { bigint: true });
      if (info.isFile()) files.push(`${relative}:${info.size}:${info.mtimeNs}`);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  await Promise.all([scan("crates"), scan(".cargo"), ...["Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml"].map(record)]);
  return files.sort().join("\n");
}

export function buildLauncher(root, log = console.log) {
  return new Promise((resolve, reject) => {
    const cargo = spawn(
      "cargo",
      [
        "build",
        "-p",
        "industrialis-app",
        "--bin",
        "industrialis-dev",
        "--features",
        "dev-watch",
        "--locked",
        "--jobs",
        "2",
        "--message-format=json-render-diagnostics",
      ],
      { cwd: root, stdio: ["ignore", "pipe", "inherit"], shell: false },
    );
    let executable;
    const output = createInterface({ input: cargo.stdout });
    output.on("line", (line) => {
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        log(line);
        return;
      }
      if (message.reason === "compiler-message" && message.message.rendered) {
        process.stderr.write(message.message.rendered);
      }
      if (message.reason === "compiler-artifact" && message.target.name === "industrialis-dev" && message.executable) {
        executable = message.executable;
      }
    });
    cargo.once("error", reject);
    cargo.once("close", (code) => {
      output.close();
      if (code !== 0) {
        log(`${prefix} Build failed. Fix the error and save to retry.`);
        resolve(null);
      } else if (!executable) reject(new Error("Cargo did not report the launcher executable."));
      else resolve(executable);
    });
  });
}

export function launchLauncher(executable, root, args, log = console.log) {
  const child = spawn(executable, args, {
    cwd: root,
    env: { ...process.env, INDUSTRIALIS_DEV_WATCH: "1" },
    // Keep terminal Ctrl+C away from the GUI so its normal close flow can run.
    detached: true,
    // SW_HIDE also hides GPUI's first window, not just a child console.
    // The dev-watch Rust target uses the Windows GUI subsystem instead.
    windowsHide: false,
    stdio: ["pipe", "inherit", "inherit"],
    shell: false,
  });
  child.stdin.on("error", () => {}); // The app can close before a queued request.
  const done = new Promise((resolve) => {
    child.once("error", (error) => {
      log(`${prefix} Could not launch: ${error.message}`);
      resolve();
    });
    child.once("close", (code, signal) => {
      if (code && !signal) log(`${prefix} Launcher exited with code ${code}. Waiting for a source change.`);
      else if (code === 0)
        log(`${prefix} Launcher closed. If another launcher owns this profile, close it and save a source file to launch the watched version.`);
      resolve();
    });
  });
  return {
    async close() {
      if (child.exitCode === null && child.signalCode === null && child.pid) {
        log(`${prefix} Closing launcher. Resolve any unsaved-file prompt; active operations will finish first.`);
        child.stdin.write("industrialis-dev-restart\n");
      }
      await done;
    },
  };
}

// One build at a time. Edits during compilation queue one subsequent build;
// never launch an obsolete result or restart repeatedly for a failed build.
export class DevRunner {
  constructor({ build, launch, log = console.log, debounceMs = 250 }) {
    Object.assign(this, { build, launch, log, debounceMs });
    this.pending = false;
    this.stopping = false;
    this.active = null;
    this.app = null;
    this.timer = null;
  }
  changed() {
    if (this.stopping) return;
    this.pending = true;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.start(), this.debounceMs);
  }
  start() {
    if (this.active || this.stopping) return this.active;
    clearTimeout(this.timer);
    this.pending = true;
    this.active = this.rebuild()
      .catch((error) => this.log(`${prefix} ${error.message}`))
      .finally(() => {
        this.active = null;
      });
    return this.active;
  }
  async rebuild() {
    while (this.pending && !this.stopping) {
      if (this.app) {
        await this.app.close();
        this.app = null;
      }
      if (this.stopping) return;
      // Include edits made while waiting for the app's save/close guards.
      this.pending = false;
      clearTimeout(this.timer);
      this.log(`${prefix} Building...`);
      const executable = await this.build();
      if (executable && !this.pending && !this.stopping) {
        this.log(`${prefix} Launching. Watching source and assets for changes.`);
        this.app = this.launch(executable);
      }
    }
  }
  async stop() {
    this.stopping = true;
    clearTimeout(this.timer);
    // Don't force-kill Cargo/compiler descendants or interrupt a file operation.
    await this.active;
    if (this.app) {
      await this.app.close();
      this.app = null;
    }
  }
}

export async function watchLauncher(root = workspace, args = process.argv.slice(2)) {
  if (args.includes("--help")) {
    console.log(
      "GPUI dev: build, launch, then rebuild/restart after source or asset changes.\nCtrl+C closes safely. Use dev:once for a single Cargo run. Arguments are passed to the launcher.",
    );
    return;
  }
  let snapshot = await sourceSnapshot(root);
  const runner = new DevRunner({
    build: () => buildLauncher(root),
    launch: (executable) => launchLauncher(executable, root, args),
  });
  let poll;
  let stopping = false;
  let scanning = false;
  const stop = () => {
    if (stopping) return;
    stopping = true;
    clearInterval(poll);
    console.log(`${prefix} Stopping after the current build/save completes...`);
    runner
      .stop()
      .catch((error) => {
        console.error(error);
        process.exitCode = 1;
      })
      .finally(() => {
        process.off("SIGINT", stop);
        process.off("SIGTERM", stop);
      });
  };
  process.on("SIGINT", stop);
  process.on("SIGTERM", stop);
  poll = setInterval(async () => {
    if (scanning || stopping) return;
    scanning = true;
    try {
      const next = await sourceSnapshot(root);
      if (next !== snapshot) {
        snapshot = next;
        runner.changed();
      }
    } catch (error) {
      console.error(`${prefix} Watch failed: ${error.message}`);
      process.exitCode = 1;
      stop();
    } finally {
      scanning = false;
    }
  }, 500);
  void runner.start();
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  watchLauncher().catch((error) => {
    console.error(`${prefix} ${error.message}`);
    process.exitCode = 1;
  });
}
