import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { readCargoVersion, setVersion, validateVersion } from "./launcher-version.mjs";

const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../apps/launcher");
test("Launcher version stamping keeps package, Cargo, About and Windows metadata on one version", async () => {
  await mkdir(path.join(app, "target"), { recursive: true });
  const fixture = await mkdtemp(path.join(app, "target/version-test-"));
  await mkdir(path.join(fixture, "crates/app"), { recursive: true });
  await writeFile(path.join(fixture, "package.json"), '{"name":"fixture","version":"0.1.0"}\n');
  await writeFile(path.join(fixture, "crates/app/Cargo.toml"), '[package]\nname = "fixture"\nversion = "0.1.0"\n\n[dependencies]\nunchanged = "1"\n');
  await setVersion(fixture, "1.2.3");
  assert.equal(JSON.parse(await readFile(path.join(fixture, "package.json"), "utf8")).version, "1.2.3");
  assert.equal(
    await readFile(path.join(fixture, "crates/app/Cargo.toml"), "utf8"),
    '[package]\nname = "fixture"\nversion = "1.2.3"\n\n[dependencies]\nunchanged = "1"\n',
  );
  for (const value of ["", "v1.0.0", "1.0.0-beta", "1.0.0+build", "01.0.0", "65536.0.0", "1.0"]) assert.throws(() => validateVersion(value));
  assert.equal(readCargoVersion('[package]\nversion = "1.0.0"\n[dependencies]\nversion = "2.0.0"\n'), "1.0.0");
});

test("Windows packaging exposes the public product name without the migration codename", async () => {
  const installer = await readFile(path.join(app, "packaging/windows.nsi"), "utf8");
  const build = await readFile(path.join(app, "crates/app/build.rs"), "utf8");
  assert.match(installer, /^Name "Industrialis Launcher"$/m);
  assert.match(installer, /^InstallDir "\$LOCALAPPDATA\\Programs\\Industrialis Launcher"$/m);
  assert.match(installer, /^InstallDirRegKey HKCU "\$\{APP_REGKEY\}" "InstallDir"$/m);
  assert.match(installer, /!define APP_REGKEY "Software\\Industrialis\\Launcher"/);
  assert.match(installer, /!define UNINSTALL_KEY "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\IndustrialisLauncher"/);
  assert.match(installer, /!define INSTALL_MARKER "install\.json"/);
  assert.match(installer, /"ProductName" "Industrialis Launcher"/);
  assert.match(installer, /"FileDescription" "Industrialis Launcher Installer"/);
  assert.match(installer, /"DisplayName" "Industrialis Launcher"/);
  assert.match(installer, /CreateShortcut "\$SMPROGRAMS\\Industrialis Launcher\.lnk"/);
  assert.match(build, /"FileDescription", "Industrialis Launcher\\0"/);
  assert.ok(!installer.includes('InstallDirRegKey HKCU "${LEGACY_REGKEY}"'));
});

test("existing installations still upgrade while the retired identity is removed", async () => {
  const installer = await readFile(path.join(app, "packaging/windows.nsi"), "utf8");
  const updater = await readFile(path.join(app, "crates/backend/src/release_updater.rs"), "utf8");
  assert.match(updater, /pub const INSTALL_MARKER: &str = "install\.json"/);
  assert.match(updater, /const LEGACY_INSTALL_MARKER: &str = "gpui-install\.json"/);
  assert.match(installer, /DeleteRegKey HKCU "\$\{LEGACY_REGKEY\}"/);
  assert.match(installer, /DeleteRegKey HKCU "\$\{LEGACY_UNINSTALL_KEY\}"/);
  assert.match(installer, /Delete "\$LegacyDir\\\$\{LEGACY_INSTALL_MARKER\}"/);
});

test("stable native release and optional previews use the correct update channels", async () => {
  const workflow = await readFile(path.join(app, "../../.github/workflows/launcher-release.yml"), "utf8");
  const updater = await readFile(path.join(app, "crates/backend/src/release_updater.rs"), "utf8");
  const packager = await readFile(path.join(app, "../../scripts/build-launcher.mjs"), "utf8");
  assert.match(workflow, /launcher-preview-v\*/);
  assert.match(workflow, /launcher-v\*/);
  assert.ok(workflow.includes("prerelease: ${{ startsWith(github.ref_name, 'launcher-preview-v') }}"));
  assert.ok(workflow.includes("make_latest: ${{ startsWith(github.ref_name, 'launcher-v') }}"));
  assert.match(workflow, /pull_request:/);
  assert.ok(!workflow.includes("electron-forge"));
  assert.ok(!workflow.toLowerCase().includes("gpui"));
  const notes = await readFile(path.join(app, "RELEASE_NOTES.md"), "utf8");
  assert.ok(notes.includes("<!-- industrialis-launcher-release-v1 -->"));
  const manifest = JSON.parse(await readFile(path.join(app, "../../package.json"), "utf8"));
  const launcher = JSON.parse(await readFile(path.join(app, "package.json"), "utf8"));
  const lockfile = await readFile(path.join(app, "../../pnpm-lock.yaml"), "utf8");
  assert.equal(launcher.name, "@industrialis/launcher");
  assert.match(manifest.scripts["dev:launcher"], /--filter @industrialis\/launcher dev$/);
  assert.match(lockfile, /^  apps\/launcher: \{\}$/m);
  assert.match(manifest.scripts["build:launcher:release"], /--channel stable/);
  for (const asset of ["IndustrialisLauncherPreviewSetup-x64.exe", "IndustrialisLauncher-macos-arm64.dmg", "IndustrialisLauncher-linux-x64.tar.gz"]) {
    assert.ok(updater.includes(asset));
    assert.ok(packager.includes(asset));
  }
});

test("stable packaging cannot relabel a preview binary without compiling its update channel", () => {
  const script = path.join(app, "../../scripts/build-launcher.mjs");
  for (const args of [
    ["--channel", "stable", "--skip-build"],
    ["--channel", "invalid"],
  ]) {
    const result = spawnSync(process.execPath, [script, ...args], { encoding: "utf8", timeout: 10_000 });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Stable packaging requires compiling|Channel must be preview or stable/);
  }
});
