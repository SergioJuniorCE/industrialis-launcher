// Uses the actual app entrypoint, with synthetic games and no launcher profile.
import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { mkdtemp, readFile, writeFile, rm, access } from "node:fs/promises";
import { promisify } from "node:util";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
const root = fileURLToPath(new URL("../", import.meta.url));
const target = process.env.CARGO_TARGET_DIR ?? path.join(root, "apps/launcher-gpui/target");
const app = path.join(target, "debug/examples", `profile_ownership_smoke${process.platform === "win32" ? ".exe" : ""}`);
const fixture = path.join(target, "debug/examples", `game_lifecycle${process.platform === "win32" ? ".exe" : ""}`);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function jsonUntil(file) {
  const end = Date.now() + 15000;
  while (Date.now() < end) {
    try {
      return JSON.parse(await readFile(file, "utf8"));
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    await delay(25);
  }
  throw new Error(`Timed out waiting for ${path.basename(file)}`);
}
async function capture(t, request) {
  const directory = await mkdtemp(path.join(os.tmpdir(), "industrialis-game-broker-"));
  await writeFile(path.join(directory, "request.json"), JSON.stringify({ ...request, cwd: directory, environment: {} }));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}
test("native broker survives its parent, captures both streams and runs the hook once", async (t) => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "industrialis-native-parent-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const script =
    "process.stdout.write('before\\n');setTimeout(()=>{process.stdout.write('after \\u00f1\\n');process.stderr.write('error\\n');process.exit(7)},2500);";
  const parent = spawn(fixture, [directory, process.execPath, script], { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
  const parentExit = new Promise((resolve) => parent.once("exit", resolve));
  let output = "",
    error = "";
  parent.stdout.on("data", (chunk) => (output += chunk));
  parent.stderr.on("data", (chunk) => (error += chunk));
  t.after(() => {
    if (parent.exitCode === null && parent.signalCode === null) parent.kill();
  });
  const deadline = Date.now() + 30000;
  while (!output.includes("READY ")) {
    assert.ok(Date.now() < deadline, "Native parent startup timed out: " + error);
    assert.equal(parent.exitCode, null, error);
    await delay(25);
  }
  const captureDirectory = path.join(directory, "instances/game/.industrialis-capture", output.trim().split(" ")[1]);
  const hook = path.join(directory, "hook.txt");
  const ready = await jsonUntil(path.join(captureDirectory, "ready.json"));
  assert.ok(ready.pid > 0);
  assert.ok(ready.creation_id);
  assert.ok(ready.broker_pid > 0);
  await assert.rejects(access(path.join(captureDirectory, "request.json")), { code: "ENOENT" });
  if (process.platform === "win32") {
    const { stdout } = await promisify(execFile)(
      "powershell.exe",
      ["-NoProfile", "-Command", `[math]::Floor((Get-Process -Id ${ready.pid}).StartTime.ToUniversalTime().Ticks / 10000)`],
      { windowsHide: true },
    );
    assert.equal(stdout.trim(), ready.creation_id);
  }
  parent.kill();
  await parentExit;
  const receipt = await jsonUntil(path.join(captureDirectory, "exit.json"));
  assert.equal(receipt.pid, ready.pid);
  assert.equal(receipt.creation_id, ready.creation_id);
  assert.equal(receipt.exit_code, 7);
  assert.equal(receipt.post_exit_error, null);
  assert.equal(await readFile(path.join(captureDirectory, "stdout.log"), "utf8"), "before\nafter ñ\n");
  assert.equal(await readFile(path.join(captureDirectory, "stderr.log"), "utf8"), "error\n");
  assert.equal((await readFile(hook, "utf8")).trim(), "hook");
  assert.ok(receipt.ended_at_ms <= Date.now());
});
test("startup failure is durable and deletes the launch request", async (t) => {
  const directory = await capture(t, { executable: path.join(os.tmpdir(), "nonexistent-game-" + Date.now()), args: ["secret-marker"], post_exit: null });
  const broker = spawn(app, ["--industrialis-game-broker", path.join(directory, "request.json")], { windowsHide: true, stdio: "ignore" });
  const exited = new Promise((resolve) => broker.once("exit", resolve));
  const error = await jsonUntil(path.join(directory, "startup-error.json"));
  await exited;
  assert.match(error, /Game broker failed/);
  assert.ok(!error.includes("secret-marker"));
  await assert.rejects(access(path.join(directory, "request.json")), { code: "ENOENT" });
});
test("graceful close drains logs, preserves the running game, and reopening recovers its actual exit", async (t) => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "industrialis-native-close-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const parent = spawn(
    fixture,
    [directory, process.execPath, "process.stdout.write('first\\n');setTimeout(()=>{process.stdout.write('last\\n');process.exit(0)},2200)"],
    { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] },
  );
  let output = "",
    stderr = "";
  parent.stdout.on("data", (chunk) => (output += chunk));
  parent.stderr.on("data", (chunk) => (stderr += chunk));
  const exited = new Promise((resolve) => parent.once("exit", resolve));
  t.after(() => {
    if (parent.exitCode === null && parent.signalCode === null) parent.kill();
  });
  const deadline = Date.now() + 30000;
  while (!output.includes("READY ")) {
    assert.ok(Date.now() < deadline, stderr);
    assert.equal(parent.exitCode, null, stderr);
    await delay(25);
  }
  const captureDirectory = path.join(directory, "instances/game/.industrialis-capture", output.trim().split(" ")[1]);
  await delay(100);
  parent.stdin.end("close\n");
  await exited;
  assert.equal(parent.exitCode, 0, stderr);
  assert.match(output, /CLOSED/);
  const pids = JSON.parse(await readFile(path.join(directory, "running-game-pids.json"), "utf8"));
  assert.ok(pids.game.pid > 0);
  assert.equal(pids.game.gpui_session.stdout_offset, 6);
  const receipt = await jsonUntil(path.join(captureDirectory, "exit.json"));
  assert.equal(receipt.exit_code, 0);
  await delay(1200); // Time with an already dead game must not be credited.
  const exec = promisify(execFile);
  await exec(fixture, [directory, "recover"], { windowsHide: true });
  const log = await readFile(path.join(directory, "instances/game/console.log"), "utf8");
  const lines = log
    .trim()
    .split(/\r?\n/)
    .map((line) => JSON.parse(line).line);
  assert.equal(lines.filter((line) => line === "first").length, 1);
  assert.equal(lines.filter((line) => line === "last").length, 1);
  assert.equal(lines.filter((line) => line === "Process exited with code 0").length, 1);
  await assert.rejects(access(path.join(directory, "running-game-pids.json")), { code: "ENOENT" });
  const saved = JSON.parse(await readFile(path.join(directory, "instances/game/instance.json"), "utf8"));
  const expected = Math.floor((receipt.ended_at_ms - pids.game.gpui_session.started_at_ms) / 1000);
  assert.equal(saved.total_play_seconds, expected);
  await exec(fixture, [directory, "recover"], { windowsHide: true });
  assert.equal(JSON.parse(await readFile(path.join(directory, "instances/game/instance.json"), "utf8")).total_play_seconds, expected);
  assert.equal((await readFile(path.join(directory, "hook.txt"), "utf8")).trim(), "hook");
});
