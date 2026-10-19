import { spawnSync, execFileSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { readCargoVersion, validateVersion } from "./set-gpui-launcher-version.mjs";
import { collectDistributionNotices } from "./gpui-notices.mjs";
import { verifyWindowsRuntime, windowsPackageEnvironment } from "./gpui-windows-runtime.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const app = path.join(repo, "apps/launcher");
const args = process.argv.slice(2);
const target = args[0] && !args[0].startsWith("--") ? args.shift() : "all";
if (!["all", "installer", "portable"].includes(target)) throw new Error("Target must be all, installer, or portable");
function option(name) {
  const i = args.indexOf(name);
  if (i < 0) return undefined;
  const value = args[i + 1];
  if (!value || value.startsWith("--")) throw new Error(`Missing ${name} value`);
  return value;
}
const skipBuild = args.includes("--skip-build");
const channel = option("--channel") ?? "preview";
if (!["preview", "stable"].includes(channel)) throw new Error("Channel must be preview or stable");
if (channel === "stable" && skipBuild) throw new Error("Stable packaging requires compiling the stable-updates feature; --skip-build is not allowed");
const replacement = args.includes("--replacement") || channel === "stable";
const windowsInstaller = channel === "stable" ? "IndustrialisLauncherSetup.exe" : "IndustrialisGPUISetup-x64.exe";
const known = new Set(["--skip-build", "--replacement", "--binary", "--out", "--makensis", "--channel"]);
for (let i = 0; i < args.length; i++) {
  if (!known.has(args[i])) throw new Error(`Unknown option ${args[i]}`);
  if (!["--skip-build", "--replacement"].includes(args[i])) i++;
}
const out = path.resolve(option("--out") ?? path.join(app, "dist"));
const version = validateVersion(JSON.parse(await readFile(path.join(app, "package.json"), "utf8")).version);
const cargo = await readFile(path.join(app, "crates/app/Cargo.toml"), "utf8");
if (readCargoVersion(cargo) !== version) throw new Error("Cargo/package versions differ; run set-gpui-launcher-version.mjs");
if (
  ![
    ["win32", "x64"],
    ["linux", "x64"],
    ["darwin", "arm64"],
  ].some(([os, arch]) => process.platform === os && process.arch === arch)
)
  throw new Error("Supported builds: Windows x64, Linux x64, macOS arm64");
function run(command, parameters, cwd = app, env = process.env) {
  const result = spawnSync(command, parameters, { cwd, env, stdio: "inherit", shell: false });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed with exit ${result.status}`);
}
if (!skipBuild)
  run(
    "cargo",
    ["build", "--release", "--locked", "--jobs", "1", "-p", "industrialis-app", "--features", channel === "stable" ? "stable-updates" : "packaged-updates"],
    app,
    process.platform === "win32" ? windowsPackageEnvironment(process.env) : process.env,
  );
if (option("--binary") && !skipBuild) throw new Error("--binary requires --skip-build (packaging smoke tests only)");
const binary = path.resolve(option("--binary") ?? path.join(app, "target/release", process.platform === "win32" ? "industrialis.exe" : "industrialis"));
if (!(await stat(binary)).isFile()) throw new Error("Launcher executable not found");
if (process.platform === "win32") verifyWindowsRuntime(await readFile(binary));
await mkdir(out, { recursive: true });
await mkdir(path.join(app, "target/package-staging"), { recursive: true });
const staging = await mkdtemp(path.join(app, "target/package-staging/build-"));
const triple = { win32: "x86_64-pc-windows-msvc", darwin: "aarch64-apple-darwin", linux: "x86_64-unknown-linux-gnu" }[process.platform];
const metadata = JSON.parse(
  execFileSync(
    "cargo",
    ["metadata", "--locked", "--offline", "--format-version", "1", "--filter-platform", triple, "--manifest-path", path.join(app, "Cargo.toml")],
    { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  ),
);
const notices = path.join(staging, "THIRD_PARTY_NOTICES.txt");
await writeFile(notices, await collectDistributionNotices(metadata, app));
const license = path.join(app, "LICENSE");
const ps = (s) => `'${s.replaceAll("'", "''")}'`;
if (process.platform === "win32") {
  if (target !== "installer") {
    const portable = path.join(staging, "portable");
    await mkdir(portable);
    await copyFile(binary, path.join(portable, "industrialis.exe"));
    await copyFile(notices, path.join(portable, "THIRD_PARTY_NOTICES.txt"));
    await copyFile(license, path.join(portable, "LICENSE"));
    await writeFile(
      path.join(portable, "README.txt"),
      `Industrialis Launcher ${version}\r\nPortable builds use manual launcher updates. Instances/settings use the normal app-data directory.\r\n`,
    );
    run("powershell.exe", [
      "-NoProfile",
      "-Command",
      `Compress-Archive -Path ${ps(path.join(portable, "*"))} -DestinationPath ${ps(path.join(out, "IndustrialisGPUI-windows-x64.zip"))} -Force`,
    ]);
  }
  if (target !== "portable") {
    const compiler = path.resolve(
      option("--makensis") ?? process.env.MAKENSIS ?? path.join(process.env["ProgramFiles(x86)"] ?? "C:/Program Files (x86)", "NSIS/makensis.exe"),
    );
    run(compiler, [
      "/V3",
      `/DAPPVERSION=${version}`,
      `/DBINARY=${binary}`,
      `/DOUTFILE=${path.join(out, windowsInstaller)}`,
      `/DICONFILE=${path.join(app, "crates/app/assets/icon.ico")}`,
      `/DNOTICEFILE=${notices}`,
      `/DLICENSEFILE=${license}`,
      ...(replacement ? ["/DREGISTER_PROTOCOL"] : []),
      ...(channel === "stable" ? [`/DRETIRE_SCRIPT_FILE=${path.join(app, "packaging/retire-electron.ps1")}`] : []),
      path.join(app, "packaging/windows.nsi"),
    ]);
  }
} else if (process.platform === "darwin") {
  const bundle = path.join(staging, "Industrialis Launcher.app/Contents");
  await mkdir(path.join(bundle, "MacOS"), { recursive: true });
  await mkdir(path.join(bundle, "Resources"));
  await copyFile(binary, path.join(bundle, "MacOS/industrialis"));
  run("chmod", ["+x", path.join(bundle, "MacOS/industrialis")]);
  await copyFile(path.join(app, "crates/app/assets/icon.icns"), path.join(bundle, "Resources/icon.icns"));
  await copyFile(notices, path.join(bundle, "Resources/THIRD_PARTY_NOTICES.txt"));
  await copyFile(license, path.join(bundle, "Resources/LICENSE"));
  await writeFile(
    path.join(bundle, "Info.plist"),
    `<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.industrialis.launcher.gpui</string><key>CFBundleName</key><string>Industrialis Launcher</string><key>CFBundleExecutable</key><string>industrialis</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleIconFile</key><string>icon.icns</string><key>CFBundleShortVersionString</key><string>${version}</string><key>CFBundleVersion</key><string>${version}</string><key>NSHighResolutionCapable</key><true/>${replacement ? "<key>CFBundleURLTypes</key><array><dict><key>CFBundleURLName</key><string>Industrialis Launcher</string><key>CFBundleURLSchemes</key><array><string>industrialislauncher</string></array></dict></array>" : ""}</dict></plist>\n`,
  );
  run("codesign", ["--force", "--deep", "--sign", "-", path.dirname(bundle)]);
  run("hdiutil", [
    "create",
    "-volname",
    "Industrialis Launcher",
    "-srcfolder",
    staging,
    "-ov",
    "-format",
    "UDZO",
    path.join(out, "IndustrialisGPUI-macos-arm64.dmg"),
  ]);
} else {
  const folder = path.join(staging, "industrialis-gpui");
  await mkdir(folder);
  await copyFile(binary, path.join(folder, "industrialis"));
  await copyFile(notices, path.join(folder, "THIRD_PARTY_NOTICES.txt"));
  await copyFile(license, path.join(folder, "LICENSE"));
  run("chmod", ["+x", path.join(folder, "industrialis")]);
  await copyFile(path.join(app, "crates/app/assets/brand.png"), path.join(folder, "icon.png"));
  await writeFile(
    path.join(folder, "industrialis-gpui.desktop"),
    `[Desktop Entry]\nType=Application\nName=Industrialis Launcher\nExec=industrialis %u\nIcon=industrialis-gpui\nCategories=Game;\nTerminal=false\n${replacement ? "MimeType=x-scheme-handler/industrialislauncher;\n" : ""}`,
  );
  await writeFile(
    path.join(folder, "README.txt"),
    `Industrialis Launcher ${version}\nRun ./industrialis. Requires a graphical desktop, Vulkan or OpenGL drivers, Fontconfig, X11 or Wayland libraries. Launcher updates are manual.\nIf an older Mesa software driver shows a black window, try: VK_DRIVER_FILES=/dev/null VK_ICD_FILENAMES=/dev/null LIBGL_ALWAYS_SOFTWARE=1 ./industrialis\nDesktop installation: put this directory on PATH, install icon.png as industrialis-gpui, and copy industrialis-gpui.desktop to ~/.local/share/applications/. ${replacement ? "Register it with: xdg-mime default industrialis-gpui.desktop x-scheme-handler/industrialislauncher" : "Preview desktop files do not register the original launcher protocol."}\n`,
  );
  run("tar", ["-czf", path.join(out, "IndustrialisGPUI-linux-x64.tar.gz"), "-C", staging, "industrialis-gpui"]);
}
console.log(`GPUI ${version} ${channel} artifacts written to ${out}`);
