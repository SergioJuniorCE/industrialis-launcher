import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { promisify } from "node:util";
import { DevRunner, sourceSnapshot } from "./dev-launcher-gpui.mjs";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
};
async function until(check, description, timeout = 20000) {
  const end = Date.now() + timeout;
  while (!(await check())) {
    assert.ok(Date.now() < end, `Timed out: ${description}`);
    await sleep(30);
  }
}
async function fixture(t, cleanup = true) {
  const root = await mkdtemp(path.join(os.tmpdir(), "gpui dev watch "));
  if (cleanup) t.after(() => rm(root, { recursive: true, force: true }));
  return root;
}

test("snapshot catches atomic saves, new folders, assets and manifests, ignoring generated outputs", async (t) => {
  const root = await fixture(t);
  await mkdir(path.join(root, "crates/app/src"), { recursive: true });
  const source = path.join(root, "crates/app/src/main.rs");
  await writeFile(source, "before");
  const original = await sourceSnapshot(root);
  await mkdir(path.join(root, "target/debug"), { recursive: true });
  await writeFile(path.join(root, "target/debug/output"), "compiler output");
  assert.equal(await sourceSnapshot(root), original);
  await writeFile(`${source}.tmp`, "after atomic save");
  assert.equal(await sourceSnapshot(root), original);
  await rename(`${source}.tmp`, source);
  let current = await sourceSnapshot(root);
  assert.notEqual(current, original);
  for (const relative of ["crates/app/src/new/module.rs", "crates/app/assets/icon.svg", "Cargo.toml", "Cargo.lock", ".cargo/config.toml"]) {
    await mkdir(path.dirname(path.join(root, relative)), { recursive: true });
    await writeFile(path.join(root, relative), "new");
    const next = await sourceSnapshot(root);
    assert.notEqual(next, current, relative);
    current = next;
  }
});

test("save bursts coalesce; edits during a build discard its old result; errors recover on the next save", async () => {
  const first = deferred();
  let builds = 0;
  const launches = [];
  let closes = 0;
  const runner = new DevRunner({
    debounceMs: 10,
    log: () => {},
    build: async () => (++builds === 1 ? first.promise : builds === 3 ? null : `binary-${builds}`),
    launch: (binary) => {
      launches.push(binary);
      return {
        close: async () => {
          closes++;
        },
      };
    },
  });
  const initial = runner.start();
  runner.changed();
  runner.changed();
  runner.changed();
  first.resolve("obsolete");
  await initial;
  assert.equal(builds, 2);
  assert.deepEqual(launches, ["binary-2"]);
  await sleep(30);
  assert.equal(builds, 2);
  runner.changed();
  runner.changed();
  await until(() => builds === 3 && runner.active === null, "failed build");
  assert.equal(closes, 1);
  assert.deepEqual(launches, ["binary-2"]);
  await sleep(30);
  assert.equal(builds, 3);
  runner.changed();
  await until(() => launches.length === 2, "recovery");
  assert.deepEqual(launches, ["binary-2", "binary-4"]);
  await runner.stop();
  assert.equal(closes, 2);
});

test("restart waits for save/drain approval; stopping during a build never launches it", async () => {
  const close = deferred();
  let builds = 0;
  const runner = new DevRunner({
    log: () => {},
    build: async () => {
      builds++;
      return "binary";
    },
    launch: () => ({ close: () => close.promise }),
  });
  await runner.start();
  const rebuilding = runner.start();
  await sleep(20);
  assert.equal(builds, 1, "cannot overwrite the binary while its app is saving");
  const stopping = runner.stop();
  close.resolve();
  await Promise.all([rebuilding, stopping]);
  assert.equal(builds, 1);
  const build = deferred();
  let launches = 0;
  const building = new DevRunner({
    log: () => {},
    build: () => build.promise,
    launch: () => {
      launches++;
    },
  });
  const active = building.start();
  const stopped = building.stop();
  build.resolve("binary");
  await Promise.all([active, stopped]);
  assert.equal(launches, 0);
});

// Actual Cargo + child processes, but no desktop, launcher profile or GPUI build.
test("real watcher rebuilds/restarts after edits, survives compile errors and stops its owned app", async (t) => {
  const root = await fixture(t, false);
  await mkdir(path.join(root, "crates/app/src"), { recursive: true });
  await mkdir(path.join(root, ".cargo"));
  await writeFile(path.join(root, "Cargo.toml"), '[workspace]\nmembers = ["crates/app"]\nresolver = "2"\n');
  await writeFile(path.join(root, "Cargo.lock"), 'version = 4\n[[package]]\nname = "industrialis-app"\nversion = "0.1.0"\n');
  await writeFile(
    path.join(root, "crates/app/Cargo.toml"),
    '[package]\nname = "industrialis-app"\nversion = "0.1.0"\nedition = "2021"\n[features]\ndev-watch = []\n[[bin]]\nname = "industrialis"\npath = "src/main.rs"\n[[bin]]\nname = "industrialis-dev"\npath = "src/main.rs"\nrequired-features = ["dev-watch"]\n',
  );
  // Cargo's reported executable must be honored, including a custom target dir.
  await writeFile(path.join(root, ".cargo/config.toml"), '[build]\ntarget-dir = "target/custom output"\n');
  const source = path.join(root, "crates/app/src/main.rs");
  const rust = (version) => `#![cfg_attr(all(windows, feature="dev-watch"), windows_subsystem="windows")]
use std::{io::BufRead, io::Write};
#[cfg(windows)]
fn assert_visible_startup() {
    #[repr(C)]
    struct StartupInfo {
        cb: u32, reserved: *mut u16, desktop: *mut u16, title: *mut u16,
        x: u32, y: u32, width: u32, height: u32, x_chars: u32, y_chars: u32,
        fill: u32, flags: u32, show: u16, reserved_size: u16,
        reserved_data: *mut u8, input: *mut std::ffi::c_void,
        output: *mut std::ffi::c_void, error: *mut std::ffi::c_void,
    }
    #[link(name="kernel32")]
    extern "system" { fn GetStartupInfoW(info: *mut StartupInfo); }
    let mut info: StartupInfo = unsafe { std::mem::zeroed() };
    unsafe { GetStartupInfoW(&mut info) };
    assert!(!(info.flags & 1 != 0 && info.show == 0), "GUI startup must not force SW_HIDE");
}
fn record(s: &str) { writeln!(std::fs::OpenOptions::new().append(true).create(true).open("events.log").unwrap(), "{}", s).unwrap(); }
fn main() {
assert_eq!(std::env::var("INDUSTRIALIS_DEV_WATCH").unwrap(), "1");
let legacy = std::env::var("GPUI_TEST_LEGACY").is_ok();
#[cfg(windows)]
if !legacy { assert_visible_startup(); }
record(if legacy { "legacy-start" } else { "start-${version}" });
for line in std::io::stdin().lock().lines() { if line.unwrap() == "industrialis-dev-restart" { break; } }
record(if legacy { "legacy-stop" } else { "stop-${version}" });
}`;
  await writeFile(source, rust("one"));
  const environment = Object.fromEntries(Object.entries(process.env).filter(([key]) => key !== "CARGO_TARGET_DIR"));
  await promisify(execFile)("cargo", ["build", "--bin", "industrialis", "--locked"], { cwd: root, env: environment, windowsHide: true });
  // Keep the original executable running. Windows must refuse replacing it;
  // watch mode must build its own executable without killing this process.
  const legacy = spawn(path.join(root, "target/custom output/debug", `industrialis${process.platform === "win32" ? ".exe" : ""}`), [], {
    cwd: root,
    env: { ...environment, INDUSTRIALIS_DEV_WATCH: "1", GPUI_TEST_LEGACY: "1" },
    windowsHide: true,
    stdio: ["pipe", "ignore", "ignore"],
  });
  const legacyExit = new Promise((resolve) => legacy.once("close", resolve));
  t.after(async () => {
    legacy.stdin.end("industrialis-dev-restart\n");
    await legacyExit;
  });
  await writeFile(source, rust("one") + "\n// Require recompilation while the original binary is running.\n");
  const module = new URL("./dev-launcher-gpui.mjs", import.meta.url).href;
  const child = spawn(
    process.execPath,
    ["--input-type=module", "-e", `import { watchLauncher } from ${JSON.stringify(module)}; await watchLauncher(process.argv[1], []);`, root],
    {
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
      // Avoid using the real app's configured target dir when tests are run in CI.
      env: environment,
    },
  );
  let output = "";
  child.stdout.on("data", (data) => {
    output += data;
  });
  child.stderr.on("data", (data) => {
    output += data;
  });
  const exited = new Promise((resolve) => child.once("close", resolve));
  t.after(async () => {
    if (child.exitCode === null) {
      child.kill();
      await exited;
    }
  });
  t.after(() => rm(root, { recursive: true, force: true }));
  const events = async () => {
    try {
      return (await readFile(path.join(root, "events.log"), "utf8"))
        .split("\n")
        .filter((line) => !line.startsWith("legacy-"))
        .join("\n");
    } catch (error) {
      if (error.code === "ENOENT") return "";
      throw error;
    }
  };
  await until(
    async () => (await events()).includes("start-one") || output.includes("Build failed.") || output.includes("Launcher exited with code"),
    "initial Cargo/app startup: " + output,
    60000,
  );
  assert.ok((await events()).includes("start-one"), "watch mode failed with an existing executable running:\n" + output);
  assert.equal(legacy.exitCode, null, "existing launcher must remain untouched");
  await writeFile(`${source}.tmp`, rust("two"));
  await rename(`${source}.tmp`, source);
  await until(async () => (await events()).includes("start-two"), "restart after atomic save");
  assert.equal(await events(), "start-one\nstop-one\nstart-two\n");
  await writeFile(source, "invalid Rust syntax");
  await until(() => output.includes("Build failed. Fix the error"), "compiler error shown");
  assert.equal(await events(), "start-one\nstop-one\nstart-two\nstop-two\n");
  await writeFile(source, rust("three"));
  await until(async () => (await events()).includes("start-three"), "restart after fixing compile error");
  child.kill("SIGTERM");
  // Windows Node kill is forceful; EOF still requests safe child shutdown.
  await exited;
  await until(async () => (await events()).includes("stop-three"), "owned child closes on runner shutdown/disconnection");
  assert.equal((await events()).match(/start-/g).length, 3);
  assert.equal(legacy.exitCode, null, "existing launcher survives rebuild, compile failure and watcher shutdown");
});
