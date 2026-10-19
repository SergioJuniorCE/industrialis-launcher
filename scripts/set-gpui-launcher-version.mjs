import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function validateVersion(version) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version) || version.split(".").some((n) => Number(n) > 65535)) {
    throw new Error("GPUI version must be major.minor.patch with each component between 0 and 65535");
  }
  return version;
}

export function readCargoVersion(cargo) {
  const section = cargo.split(/(?=^\[)/m).find((s) => s.startsWith("[package]"));
  const version = section?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!version) throw new Error("App Cargo package version was not found");
  return version;
}

export async function setVersion(directory, version) {
  validateVersion(version);
  const packagePath = path.join(directory, "package.json");
  const cargoPath = path.join(directory, "crates/app/Cargo.toml");
  const packageJson = JSON.parse(await readFile(packagePath, "utf8"));
  const cargo = await readFile(cargoPath, "utf8");
  readCargoVersion(cargo);
  const updated = cargo.replace(/^(\[package\][\s\S]*?\nversion\s*=\s*)"[^"]+"/m, `$1"${version}"`);
  if (readCargoVersion(updated) !== version) throw new Error("App Cargo version was not updated");
  packageJson.version = version;
  await writeFile(cargoPath, updated);
  await writeFile(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../apps/launcher-gpui");
  await setVersion(root, process.argv[2] ?? "");
  console.log(`GPUI launcher version set to ${process.argv[2]}. Run cargo update --workspace --offline to sync Cargo.lock.`);
}
