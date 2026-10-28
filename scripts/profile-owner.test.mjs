// Build the CLI, backend profile_owner example and app profile_ownership_smoke
// example first. Real Rust/Node processes; synthetic profile directories only.
import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { test } from "node:test";
// Frozen cross-runtime compatibility contract; Electron is no longer a workspace app.
import { acquireProfileOwnership, profileEndpoint } from "../apps/launcher/compatibility/electron-profile-owner.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const extension = process.platform === "win32" ? ".exe" : "";
const target = process.env.CARGO_TARGET_DIR ?? path.join(root, "apps/launcher/target");
const cli = path.join(target, "debug", `industrialis-cli${extension}`);
const native = path.join(target, "debug/examples", `profile_ownership_smoke${extension}`);
const fixture = path.join(target, "debug/examples", `profile_owner${extension}`);
const exec = promisify(execFile);

async function profile(t) {
  const directory = await mkdtemp(path.join(os.tmpdir(), "industrialis-owner-process-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

function processOwner(t, directory, kind = "launcher") {
  const child = spawn(fixture, [directory, kind], { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
  let output = "",
    stderr = "";
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    output += chunk;
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  const exited = new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => resolve({ code, signal }));
  });
  t.after(async () => {
    if (child.exitCode === null && child.signalCode === null) child.kill();
    await exited;
  });
  async function until(fragment) {
    const deadline = Date.now() + 5000;
    while (!output.includes(fragment)) {
      if (child.exitCode !== null || child.signalCode !== null) throw new Error(`Owner exited: ${stderr}`);
      if (Date.now() > deadline) throw new Error(`Owner did not report ${fragment}: ${output} ${stderr}`);
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  return { child, exited, until, output: () => output, diagnostic: () => ({ output, stderr, exitCode: child.exitCode, signal: child.signalCode }) };
}

test("Node owner prevents Rust writes, allows read-only inspection, and receives native second-launch activation", async (t) => {
  const directory = await profile(t);
  let activated = 0;
  const owner = await acquireProfileOwnership(directory, () => activated++);
  assert.equal(owner.primary, true);
  t.after(() => owner.close());
  const original = '{ "theme_mode":"dark", "future":7 }\n';
  await writeFile(path.join(directory, "launcher-settings.json"), original);
  const result = await exec(cli, ["--data-dir", directory, "verify-profile"], { windowsHide: true, timeout: 5000 });
  assert.equal(JSON.parse(result.stdout).settings_readable, true);
  await assert.rejects(exec(cli, ["--data-dir", directory, "create-offline-account", "Alex"], { windowsHide: true, timeout: 5000 }), /profile is in use/i);
  await assert.rejects(readFile(path.join(directory, "accounts.json")), { code: "ENOENT" });
  await exec(native, [], { windowsHide: true, timeout: 5000, env: { ...process.env, INDUSTRIALIS_DATA_DIR: directory } });
  assert.equal(activated, 1);
  assert.equal(await readFile(path.join(directory, "launcher-settings.json"), "utf8"), original);
});

test("Rust owner activates from Node, blocks writers, and releases ownership after a forced process exit", async (t) => {
  const directory = await profile(t);
  const owner = processOwner(t, directory);
  await owner.until("PRIMARY");
  const second = await acquireProfileOwnership(directory, () => assert.fail("secondary callback must not run"));
  assert.deepEqual(second, { primary: false, owner: "launcher" });
  await owner.until("FOCUS");
  await assert.rejects(exec(cli, ["--data-dir", directory, "create-offline-account", "Steve"], { windowsHide: true, timeout: 5000 }), /profile is in use/i);
  owner.child.kill();
  await owner.exited;
  await exec(cli, ["--data-dir", directory, "create-offline-account", "Steve"], { windowsHide: true, timeout: 5000 });
  const accounts = JSON.parse(await readFile(path.join(directory, "accounts.json"), "utf8"));
  assert.equal(accounts.length, 1);
  const next = await acquireProfileOwnership(directory, () => {});
  assert.equal(next.primary, true);
  await next.close();
});

test("Rust CLI ownership blocks Node launch and cannot receive a focus event", async (t) => {
  const directory = await profile(t);
  const owner = processOwner(t, directory, "operation");
  await owner.until("PRIMARY");
  assert.deepEqual(await acquireProfileOwnership(directory, () => {}), { primary: false, owner: "operation" });
  assert.equal(owner.output().includes("FOCUS"), false);
  owner.child.stdin.end("quit\n");
  assert.equal((await owner.exited).code, 0);
  const next = await acquireProfileOwnership(directory, () => {});
  assert.equal(next.primary, true);
  await next.close();
});

test("Rust owner receives a fragmented Node focus request before its deadline", async (t) => {
  const directory = await profile(t);
  const owner = processOwner(t, directory);
  await owner.until("PRIMARY");
  const { port, key } = await profileEndpoint(directory);
  const reply = await new Promise((resolve, reject) => {
    const socket = net.createConnection({ host: "127.0.0.1", port });
    t.after(() => socket.destroy());
    let text = "";
    socket.setEncoding("utf8");
    socket.setTimeout(2000, () => socket.destroy(new Error("Fragmented ownership request timed out")));
    socket.on("error", reject);
    socket.on("data", (chunk) => (text += chunk));
    socket.on("end", () => resolve(text));
    socket.once("connect", () => {
      socket.write(`industrialis-profile-v1 ${key} `);
      const remaining = setTimeout(() => socket.end("focus\n"), 100);
      t.after(() => clearTimeout(remaining));
    });
  });
  assert.match(reply, new RegExp(`^industrialis-profile-v1 ${key} launcher [0-9]+\\n$`, "u"));
  await owner.until("FOCUS");
  await assert.rejects(readFile(path.join(directory, "accounts.json")), { code: "ENOENT" });
});

test("simultaneous Rust processes select only one primary", async (t) => {
  const directory = await profile(t);
  const processes = Array.from({ length: 5 }, () => processOwner(t, directory));
  const deadline = Date.now() + 5000;
  while (processes.some((owner) => !/PRIMARY|EXISTING/u.test(owner.output()))) {
    const failed = processes.some((owner) => owner.child.exitCode !== null && !/EXISTING/u.test(owner.output()));
    if (failed || Date.now() > deadline)
      throw new Error(`Contending processes did not settle: ${JSON.stringify(processes.map((owner) => owner.diagnostic()))}`);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  assert.equal(processes.filter((owner) => owner.output().includes("PRIMARY")).length, 1);
  assert.equal(processes.filter((owner) => owner.output().includes("EXISTING")).length, 4);
});

test("Node fails closed on an unrelated occupied endpoint", async (t) => {
  const directory = await profile(t);
  const { port } = await profileEndpoint(directory);
  const server = net.createServer((socket) => {
    socket.on("error", () => {});
    socket.once("data", () => socket.end("unrelated-service\n"));
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  t.after(() => new Promise((resolve) => server.close(resolve)));
  await assert.rejects(
    acquireProfileOwnership(directory, () => assert.fail("must not activate")),
    /did not identify/,
  );
  await assert.rejects(exec(cli, ["--data-dir", directory, "create-offline-account", "Alex"], { windowsHide: true, timeout: 5000 }), /did not identify/);
  await assert.rejects(readFile(path.join(directory, "accounts.json")), { code: "ENOENT" });
});
