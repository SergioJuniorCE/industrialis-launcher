// @vitest-environment node

import type { ChildProcess } from "node:child_process";
import { execFile } from "node:child_process";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { parseJavaRuntimeDetails, testJava } from "./java";

vi.mock("node:child_process", () => ({ execFile: vi.fn() }));

const mockedExecFile = vi.mocked(execFile);
type ExecFileCallback = (error: Error | null, stdout: string, stderr: string) => void;

function mockJavaCommand(output: string, error: Error | null = null): void {
  mockedExecFile.mockImplementation((...args: unknown[]) => {
    const callback = args.at(-1) as ExecFileCallback;
    callback(error, "", output);
    return {} as ChildProcess;
  });
}

beforeEach(() => {
  mockedExecFile.mockReset();
});

describe("parseJavaRuntimeDetails", () => {
  it("reads the full version, major version, vendor, and architecture from modern Java properties", () => {
    expect(
      parseJavaRuntimeDetails(`
Property settings:
    java.vendor = Oracle Corporation
    java.version = 21.0.5
    os.arch = amd64
java version "21.0.5" 2024-10-15 LTS
`),
    ).toEqual({
      version: "21.0.5",
      majorVersion: 21,
      architecture: "amd64",
      vendor: "Oracle Corporation",
    });
  });

  it("normalizes legacy Java 8 versions to major version 8", () => {
    expect(
      parseJavaRuntimeDetails(`
    java.vendor = Oracle Corporation
    java.version = 1.8.0_51
    os.arch = x86
java version "1.8.0_51"
`),
    ).toEqual({
      version: "1.8.0_51",
      majorVersion: 8,
      architecture: "x86",
      vendor: "Oracle Corporation",
    });
  });
});

describe("testJava", () => {
  it("accepts an executable that reports a Java runtime", async () => {
    mockJavaCommand(`
Property settings:
    java.vendor = Eclipse Adoptium
    java.version = 21.0.5
    os.arch = amd64
openjdk version "21.0.5" 2024-10-15 LTS
`);

    await expect(testJava("C:\\Java\\bin\\java.exe")).resolves.toContain("OK - C:\\Java\\bin\\java.exe");
  });

  it("rejects a successful executable that does not report Java", async () => {
    mockJavaCommand("");

    await expect(testJava("/bin/true")).rejects.toThrow("executable did not report a valid Java runtime");
  });
});
