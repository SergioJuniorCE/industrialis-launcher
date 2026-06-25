import { describe, expect, it } from "vitest";
import { classifyLaunchLogLine } from "./launch-log";

describe("classifyLaunchLogLine", () => {
  it("classifies FML ERROR on stdout as error", () => {
    expect(
      classifyLaunchLogLine({
        stream: "stdout",
        line: "[23:14:55] [Client thread/ERROR] [IC2]: signature mismatch",
      }),
    ).toBe("error");
  });

  it("classifies FML WARN on stdout as warn", () => {
    expect(
      classifyLaunchLogLine({
        stream: "stdout",
        line: "[23:14:54] [Client thread/WARN] [mixin]: Error loading class",
      }),
    ).toBe("warn");
  });

  it("classifies FML INFO on stdout as info", () => {
    expect(
      classifyLaunchLogLine({
        stream: "stdout",
        line: "[23:14:55] [Client thread/INFO] [FML]: Forge Mod Loader",
      }),
    ).toBe("info");
  });

  it("classifies Java exceptions on stdout as error", () => {
    expect(
      classifyLaunchLogLine({
        stream: "stdout",
        line: "java.lang.NullPointerException: Cannot invoke",
      }),
    ).toBe("error");
  });

  it("classifies JDK WARNING on stderr as warn", () => {
    expect(
      classifyLaunchLogLine({
        stream: "stderr",
        line: "WARNING: package sun.lwawt.macosx not in java.desktop",
      }),
    ).toBe("warn");
  });

  it("classifies netty INFO on stderr as info", () => {
    expect(
      classifyLaunchLogLine({
        stream: "stderr",
        line: "INFO: Your platform does not provide complete low-level API",
      }),
    ).toBe("info");
  });

  it("classifies launcher system lines as system", () => {
    expect(
      classifyLaunchLogLine({
        stream: "system",
        line: "──────── Launch ────────",
      }),
    ).toBe("system");
  });

  it("classifies non-zero exit as error", () => {
    expect(
      classifyLaunchLogLine({
        stream: "system",
        line: "Process exited with code -1",
      }),
    ).toBe("error");
  });
});