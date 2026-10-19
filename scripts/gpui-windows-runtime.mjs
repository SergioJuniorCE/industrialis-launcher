import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

export function windowsPackageEnvironment(environment) {
  const flags = environment.CARGO_ENCODED_RUSTFLAGS?.split("\x1f").filter(Boolean) ?? environment.RUSTFLAGS?.trim().split(/\s+/).filter(Boolean) ?? [];
  return { ...environment, CARGO_ENCODED_RUSTFLAGS: [...flags, "-C", "target-feature=+crt-static"].join("\x1f") };
}

// Read import names from the actual x64 PE, without executing it or requiring VS.
export function windowsImports(bytes) {
  function range(offset, length) {
    if (!Number.isSafeInteger(offset) || offset < 0 || offset + length > bytes.length) throw new Error("Truncated Windows PE");
    return offset;
  }
  const u16 = (offset) => bytes.readUInt16LE(range(offset, 2));
  const u32 = (offset) => bytes.readUInt32LE(range(offset, 4));
  if (u16(0) !== 0x5a4d) throw new Error("Expected Windows PE executable");
  const pe = u32(0x3c);
  if (u32(pe) !== 0x4550 || u16(pe + 4) !== 0x8664) throw new Error("Expected x64 Windows PE");
  const optional = pe + 24;
  const optionalSize = u16(pe + 20);
  range(optional, optionalSize);
  if (optionalSize < 112 || u16(optional) !== 0x20b) throw new Error("Expected PE32+ optional header");
  const directories = u32(optional + 108);
  if (directories > Math.floor((optionalSize - 112) / 8)) throw new Error("Invalid PE data directories");
  const sections = [];
  for (let i = 0; i < u16(pe + 6); i++) {
    const section = range(optional + optionalSize + i * 40, 40);
    sections.push({ rva: u32(section + 12), size: u32(section + 16), offset: u32(section + 20) });
  }
  function address(rva, length) {
    const section = sections.find((entry) => rva >= entry.rva && rva - entry.rva + length <= entry.size);
    if (!section) throw new Error("PE import address is outside file-backed sections");
    return range(section.offset + rva - section.rva, length);
  }
  function name(rva) {
    let value = "";
    for (let i = 0; i < 260; i++) {
      const byte = bytes[address(rva + i, 1)];
      if (byte === 0 && value) return value;
      if (byte < 32 || byte > 126) throw new Error("Invalid PE import name");
      value += String.fromCharCode(byte);
    }
    throw new Error("Unterminated PE import name");
  }
  const imports = [];
  for (const [index, stride, nameOffset] of [
    [1, 20, 12],
    [13, 32, 4],
  ]) {
    if (index >= directories) continue;
    const rva = u32(optional + 112 + index * 8);
    const size = u32(optional + 116 + index * 8);
    if (!rva && !size) continue;
    let terminated = false;
    for (let offset = 0; offset + stride <= size; offset += stride) {
      const descriptor = address(rva + offset, stride);
      if (bytes.subarray(descriptor, descriptor + stride).every((byte) => byte === 0)) {
        terminated = true;
        break;
      }
      // PE32+ delay imports must use RVAs; old 32-bit VA descriptors are rejected.
      if (index === 13 && u32(descriptor) !== 1) throw new Error("Unsupported PE delay import addressing");
      imports.push(name(u32(descriptor + nameOffset)));
    }
    if (!terminated) throw new Error("Unterminated PE import directory");
  }
  return [...new Set(imports)].sort();
}

export function verifyWindowsRuntime(bytes) {
  const imports = windowsImports(bytes);
  const external = imports.filter((name) => /^(?:vcruntime|msvcp|msvcr|concrt)\d.*\.dll$/i.test(name));
  if (external.length) throw new Error(`Windows package requires external MSVC runtime: ${external.join(", ")}; rebuild with static CRT`);
  return imports;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify({ imports: verifyWindowsRuntime(await readFile(process.argv[2])), externalMsvcRuntime: false }));
}
