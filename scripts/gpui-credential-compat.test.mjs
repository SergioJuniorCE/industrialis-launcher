import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile, readFile, rm } from "node:fs/promises";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import path from "node:path";
import os from "node:os";
const isolatedLinux = process.platform === "linux" && process.env.GPUI_ISOLATED_CREDENTIAL_SESSION === "1";
const linuxStore = process.env.GPUI_ACCEPTANCE_PASSWORD_STORE ?? "gnome-libsecret";
const electron = process.env.GPUI_ACCEPTANCE_ELECTRON;
const rust = path.resolve(process.env.GPUI_ACCEPTANCE_RUST ?? "apps/launcher-gpui/target/debug/examples/credential_compat.exe");
async function run(executable, args, input = "", signal) {
  const env = { ...process.env };
  delete env.ELECTRON_RUN_AS_NODE;
  await new Promise((resolve, reject) => {
    const child = spawn(executable, args, { env, signal, windowsHide: true, stdio: ["pipe", "ignore", "pipe"] });
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill();
    }, 30000);
    let errors = "";
    child.stderr.on("data", (value) => {
      errors += value;
    });
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("close", (code, exitSignal) => {
      clearTimeout(timer);
      if (timedOut) reject(new Error(`Owned credential fixture exceeded 30 seconds: ${errors.slice(-4000)}`));
      else if (code === 0) resolve();
      else reject(new Error(`Compatibility fixture ${path.basename(executable)} exited ${code} (${exitSignal ?? "no signal"}): ${errors}`));
    });
    child.stdin.on("error", () => {});
    child.stdin.end(input);
  });
}
async function stateBytes(directory) {
  try {
    return await readFile(path.join(directory, "Local State"));
  } catch (error) {
    if (isolatedLinux && error.code === "ENOENT") return null;
    throw error;
  }
}
async function nativeKeyHash(signal) {
  return await new Promise((resolve, reject) => {
    const kwallet = linuxStore === "kwallet6";
    const child = spawn(
      kwallet ? "kwallet-query" : "secret-tool",
      kwallet ? ["--read-password", "Chromium Safe Storage", "--folder", "Chromium Keys", "kdewallet"] : ["lookup", "application", "Industrialis Launcher"],
      { signal, stdio: ["ignore", "pipe", "pipe"] },
    );
    const timer = setTimeout(() => child.kill(), 10000);
    const bytes = [];
    child.stdout.on("data", (data) => bytes.push(data));
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("close", (code) => {
      clearTimeout(timer);
      if (code === 0 && bytes.length) resolve(createHash("sha256").update(Buffer.concat(bytes)).digest("hex"));
      else reject(new Error("Owned native credential key was not found"));
    });
  });
}
test(
  "original Electron safeStorage and Rust use the same existing key in both directions",
  { skip: !electron || (process.platform !== "win32" && !isolatedLinux), timeout: 120000 },
  async (t) => {
    if (isolatedLinux) {
      assert.ok(["gnome-libsecret", "kwallet6"].includes(linuxStore), "Use a supported isolated native password store");
      assert.ok(
        process.env.DBUS_SESSION_BUS_ADDRESS && process.env.GPUI_ACCEPTANCE_ELECTRON && process.env.GPUI_ACCEPTANCE_RUST,
        "Use the owned Linux credential-session runner",
      );
      assert.ok(os.tmpdir().startsWith(process.env.HOME + path.sep), "Linux fixture must stay in its fresh owned home");
    }
    const directory = await mkdtemp(path.join(os.tmpdir(), "industrialis-credential-compat-"));
    const fixture = path.join(directory, "fixture.cjs");
    const value = "Synthetic GPUI interoperability value \u{1f600}";
    await writeFile(
      path.join(directory, "package.json"),
      JSON.stringify({ name: "@industrialis/launcher", productName: "Industrialis Launcher", version: "0.3.0", main: "fixture.cjs" }),
    );
    await writeFile(
      fixture,
      `const {app,safeStorage}=require('electron');const fs=require('node:fs');const path=require('node:path');const [dir,mode]=process.argv.slice(-2);if(!path.isAbsolute(dir)||!['encrypt','decrypt'].includes(mode))app.exit(2);app.setPath('userData',dir);app.whenReady().then(()=>{const value=${JSON.stringify(value)};if(!safeStorage.isEncryptionAvailable())throw new Error('Native encryption unavailable');if(mode==='encrypt'){const encrypted=safeStorage.encryptString(value);fs.writeFileSync(path.join(dir,'electron-cipher.txt'),encrypted.toString('base64'));fs.writeFileSync(path.join(dir,'storage.json'),JSON.stringify({electron:process.versions.electron,app:app.getName(),appData:app.getPath('appData'),backend:process.platform==='linux'?safeStorage.getSelectedStorageBackend():'dpapi',prefix:encrypted.subarray(0,3).toString()}));}else{const decrypted=safeStorage.decryptString(Buffer.from(fs.readFileSync(path.join(dir,'rust-cipher.txt'),'utf8'),'base64'));fs.writeFileSync(path.join(dir,'result.json'),JSON.stringify({verified:decrypted===value}));}app.quit();}).catch(error=>{console.error('Owned credential fixture:',error.message);app.exit(1)});`,
    );
    const electronFlags = isolatedLinux ? ["--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage", `--password-store=${linuxStore}`] : [];
    try {
      await run(electron, [...electronFlags, directory, directory, "encrypt"], "", t.signal);
      const before = await stateBytes(directory);
      const nativeBefore = isolatedLinux ? await nativeKeyHash(t.signal) : null;
      const storage = JSON.parse(await readFile(path.join(directory, "storage.json"), "utf8"));
      if (isolatedLinux) {
        assert.equal(storage.backend, linuxStore === "kwallet6" ? "kwallet6" : "gnome_libsecret");
        assert.equal(storage.prefix, "v11");
        assert.equal(storage.appData, process.env.XDG_CONFIG_HOME);
      }
      await run(rust, [directory], value, t.signal);
      assert.deepEqual(await stateBytes(directory), before);
      if (isolatedLinux) assert.equal(await nativeKeyHash(t.signal), nativeBefore, "Rust must reuse the original native key");
      await run(electron, [...electronFlags, directory, directory, "decrypt"], "", t.signal);
      assert.equal(JSON.parse(await readFile(path.join(directory, "result.json"), "utf8")).verified, true);
      if (isolatedLinux) assert.equal(await nativeKeyHash(t.signal), nativeBefore);
      const receipt = {
        ...storage,
        platform: process.platform,
        nativeKeyUnchanged: true,
        bothDirections: true,
        localStateUnchangedByRust: true,
        generatedProfile: true,
      };
      if (process.env.GPUI_CREDENTIAL_RECEIPT) await writeFile(process.env.GPUI_CREDENTIAL_RECEIPT, JSON.stringify(receipt, null, 2) + "\n");
      t.diagnostic(JSON.stringify(receipt));
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  },
);
