import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { readCargoVersion, setVersion, validateVersion } from "./set-gpui-launcher-version.mjs";

const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../apps/launcher");
test("GPUI version stamping keeps package, Cargo, About and Windows metadata on one version", async () => {
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

test("stable native release and optional previews use the correct update channels", async () => {
  const workflow = await readFile(path.join(app, "../../.github/workflows/launcher-release.yml"), "utf8");
  const updater = await readFile(path.join(app, "crates/backend/src/release_updater.rs"), "utf8");
  const packager = await readFile(path.join(app, "../../scripts/build-launcher-gpui.mjs"), "utf8");
  assert.match(workflow, /launcher-gpui-v\*/);
  assert.match(workflow, /launcher-v\*/);
  assert.ok(workflow.includes("prerelease: ${{ startsWith(github.ref_name, 'launcher-gpui-v') }}"));
  assert.ok(workflow.includes("make_latest: ${{ startsWith(github.ref_name, 'launcher-v') }}"));
  assert.match(workflow, /pull_request:/);
  assert.ok(!workflow.includes("electron-forge"));
  const notes = await readFile(path.join(app, "RELEASE_NOTES.md"), "utf8");
  assert.ok(notes.includes("<!-- industrialis-gpui-release-v1 -->"));
  const manifest = JSON.parse(await readFile(path.join(app, "../../package.json"), "utf8"));
  const launcher = JSON.parse(await readFile(path.join(app, "package.json"), "utf8"));
  const lockfile = await readFile(path.join(app, "../../pnpm-lock.yaml"), "utf8");
  assert.equal(launcher.name, "@industrialis/launcher");
  assert.match(manifest.scripts["dev:launcher"], /--filter @industrialis\/launcher dev$/);
  assert.match(lockfile, /^  apps\/launcher: \{\}$/m);
  assert.ok(!workflow.includes("apps/launcher-gpui"));
  assert.match(manifest.scripts["build:launcher:release"], /--channel stable/);
  for (const asset of ["IndustrialisGPUISetup-x64.exe", "IndustrialisGPUI-macos-arm64.dmg", "IndustrialisGPUI-linux-x64.tar.gz"]) {
    assert.ok(updater.includes(asset));
    assert.ok(packager.includes(asset));
  }
});

test("stable packaging cannot relabel a preview binary without compiling its update channel", () => {
  const script = path.join(app, "../../scripts/build-launcher-gpui.mjs");
  for (const args of [
    ["--channel", "stable", "--skip-build"],
    ["--channel", "invalid"],
  ]) {
    const result = spawnSync(process.execPath, [script, ...args], { encoding: "utf8", timeout: 10_000 });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Stable packaging requires compiling|Channel must be preview or stable/);
  }
});
