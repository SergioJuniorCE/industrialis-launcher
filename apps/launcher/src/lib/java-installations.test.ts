import { describe, expect, it } from "vitest";
import { filterJavaInstallations, sameJavaPath, sortJavaInstallations, type JavaInfo } from "./java-installations";

const installations: JavaInfo[] = [
  { path: "C:\\Java\\jdk-21\\bin\\java.exe", version: "21.0.5", majorVersion: 21, architecture: "amd64", vendor: "Oracle Corporation" },
  { path: "C:\\Prism\\jre-legacy\\bin\\java.exe", version: "1.8.0_51", majorVersion: 8, architecture: "x86", vendor: "Oracle Corporation" },
  { path: "C:\\Java\\jdk-17\\bin\\java.exe", version: "17.0.12", majorVersion: 17, architecture: "amd64", vendor: "Oracle Corporation" },
];

describe("Java installation filtering", () => {
  it("matches full version, architecture, vendor, and path", () => {
    expect(filterJavaInstallations(installations, "21.0.5")).toEqual([installations[0]]);
    expect(filterJavaInstallations(installations, "x86")).toEqual([installations[1]]);
    expect(filterJavaInstallations(installations, "prism")).toEqual([installations[1]]);
    expect(filterJavaInstallations(installations, "oracle")).toEqual(installations);
  });

  it("compares Windows paths without case sensitivity", () => {
    expect(sameJavaPath("C:\\JAVA\\BIN\\JAVA.EXE", "c:\\java\\bin\\java.exe")).toBe(true);
  });
});

describe("Java installation sorting", () => {
  it("sorts Java versions numerically in either direction", () => {
    expect(sortJavaInstallations(installations, "version", "desc").map((java) => java.majorVersion)).toEqual([21, 17, 8]);
    expect(sortJavaInstallations(installations, "version", "asc").map((java) => java.majorVersion)).toEqual([8, 17, 21]);
  });

  it("applies the sort direction to equal-version path tie-breakers", () => {
    const sameVersion = [
      { ...installations[0], path: "C:\\Java\\jdk-21-a\\bin\\java.exe" },
      { ...installations[0], path: "C:\\Java\\jdk-21-b\\bin\\java.exe" },
    ];

    expect(sortJavaInstallations(sameVersion, "version", "asc").map((java) => java.path)).toEqual([
      "C:\\Java\\jdk-21-a\\bin\\java.exe",
      "C:\\Java\\jdk-21-b\\bin\\java.exe",
    ]);
    expect(sortJavaInstallations(sameVersion, "version", "desc").map((java) => java.path)).toEqual([
      "C:\\Java\\jdk-21-b\\bin\\java.exe",
      "C:\\Java\\jdk-21-a\\bin\\java.exe",
    ]);
  });

  it("does not mutate the detected installation order", () => {
    const original = [...installations];
    sortJavaInstallations(installations, "path", "asc");
    expect(installations).toEqual(original);
  });
});
