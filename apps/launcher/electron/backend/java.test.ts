// @vitest-environment node

import { describe, expect, it } from "vitest";
import { parseJavaRuntimeDetails } from "./java";

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
