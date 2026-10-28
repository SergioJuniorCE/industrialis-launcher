import { test } from "node:test";
import assert from "node:assert/strict";
import { verifyWindowsRuntime, windowsPackageEnvironment } from "./windows-runtime.mjs";

function executable(dependency, delay = false) {
  const bytes = Buffer.alloc(1024);
  bytes.writeUInt16LE(0x5a4d, 0);
  bytes.writeUInt32LE(0x80, 0x3c);
  bytes.writeUInt32LE(0x4550, 0x80);
  bytes.writeUInt16LE(0x8664, 0x84);
  bytes.writeUInt16LE(1, 0x86);
  bytes.writeUInt16LE(240, 0x94);
  bytes.writeUInt16LE(0x20b, 0x98);
  bytes.writeUInt32LE(16, 0x98 + 108);
  bytes.writeUInt32LE(0x1000, 0x188 + 12);
  bytes.writeUInt32LE(512, 0x188 + 16);
  bytes.writeUInt32LE(512, 0x188 + 20);
  const directory = 0x98 + 112 + (delay ? 13 : 1) * 8;
  bytes.writeUInt32LE(0x1000, directory);
  bytes.writeUInt32LE(delay ? 64 : 40, directory + 4);
  if (delay) bytes.writeUInt32LE(1, 512);
  bytes.writeUInt32LE(0x1080, 512 + (delay ? 4 : 12));
  bytes.write(dependency + "\0", 640, "ascii");
  return bytes;
}

test("Windows package rejects ordinary and delay-loaded external MSVC runtimes", () => {
  assert.deepEqual(verifyWindowsRuntime(executable("KERNEL32.dll")), ["KERNEL32.dll"]);
  for (const delay of [false, true]) {
    for (const name of ["VCRUNTIME140.dll", "vcruntime140_1.dll", "MSVCP140.dll", "concrt140.dll"]) {
      assert.throws(() => verifyWindowsRuntime(executable(name, delay)), /external MSVC runtime/);
    }
  }
  assert.throws(() => verifyWindowsRuntime(executable("KERNEL32.dll").subarray(0, 550)), /Truncated|outside/);
  const invalid = executable("KERNEL32.dll");
  invalid.writeUInt32LE(0x9000, 512 + 12);
  assert.throws(() => verifyWindowsRuntime(invalid), /outside file-backed/);
});

test("Windows package flags retain existing flags and enable static CRT last", () => {
  assert.equal(windowsPackageEnvironment({ CARGO_ENCODED_RUSTFLAGS: "" }).CARGO_ENCODED_RUSTFLAGS, "-C\x1ftarget-feature=+crt-static");
  assert.equal(windowsPackageEnvironment({ RUSTFLAGS: "-C debuginfo=1" }).CARGO_ENCODED_RUSTFLAGS, "-C\x1fdebuginfo=1\x1f-C\x1ftarget-feature=+crt-static");
  const original = { RUSTFLAGS: "ignored", CARGO_ENCODED_RUSTFLAGS: "-C\x1ftarget-feature=-crt-static" };
  const result = windowsPackageEnvironment(original);
  assert.equal(result.CARGO_ENCODED_RUSTFLAGS, "-C\x1ftarget-feature=-crt-static\x1f-C\x1ftarget-feature=+crt-static");
  assert.equal(original.CARGO_ENCODED_RUSTFLAGS, "-C\x1ftarget-feature=-crt-static");
});
