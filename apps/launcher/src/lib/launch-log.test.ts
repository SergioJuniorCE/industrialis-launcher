import { describe, expect, it } from "vitest";
import { classifyLaunchLogLine, extractLatestCrashLines, isLaunchStartLine, sliceLatestLaunch, type LaunchLogLine } from "./launch-log";

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

function line(text: string, stream: LaunchLogLine["stream"] = "stdout"): LaunchLogLine {
  return { stream, line: text };
}

describe("sliceLatestLaunch", () => {
  it("returns the full log when there is no launch marker", () => {
    const log = [line("one"), line("two")];
    expect(sliceLatestLaunch(log)).toEqual(log);
  });

  it("isolates lines from the last launch marker", () => {
    const log = [
      line("──────── Launch ────────", "system"),
      line("old crash: java.lang.NullPointerException"),
      line("Process exited with code 1", "system"),
      line("──────── Launch ────────", "system"),
      line("Java: C:/java.exe", "system"),
      line("new crash: java.lang.OutOfMemoryError"),
    ];
    const latest = sliceLatestLaunch(log);
    expect(latest.map((entry) => entry.line)).toEqual(["──────── Launch ────────", "Java: C:/java.exe", "new crash: java.lang.OutOfMemoryError"]);
  });

  it("detects the backend launch separator", () => {
    expect(isLaunchStartLine("──────── Launch ────────")).toBe(true);
    expect(isLaunchStartLine("Launch args saved to C:/instance/launch.arg")).toBe(false);
  });
});

describe("extractLatestCrashLines", () => {
  it("returns an empty excerpt for an empty log", () => {
    expect(extractLatestCrashLines([])).toEqual([]);
  });

  it("returns the whole latest launch when it fits in the limit", () => {
    const log = [
      line("──────── Launch ────────", "system"),
      line("---- Minecraft Crash Report ----"),
      line("java.lang.RuntimeException: crash"),
      line("Process exited with code 1", "system"),
    ];
    expect(extractLatestCrashLines(log)).toEqual(log.slice(0));
  });

  it("drops earlier launches and keeps header plus crash tail for large launches", () => {
    const oldLaunch = [line("──────── Launch ────────", "system"), line("old launch output")];
    const latestLaunch: LaunchLogLine[] = [line("──────── Launch ────────", "system")];
    for (let index = 0; index < 20; index += 1) latestLaunch.push(line(`filler ${index}`));
    latestLaunch.push(line("java.lang.IllegalStateException: latest crash"));
    latestLaunch.push(line("Process exited with code 1", "system"));
    const log = [...oldLaunch, ...latestLaunch];

    const excerpt = extractLatestCrashLines(log, 10);
    expect(excerpt).toHaveLength(10);
    expect(excerpt[0].line).toBe("──────── Launch ────────");
    expect(excerpt.some((entry) => entry.line.includes("truncated"))).toBe(true);
    expect(excerpt.some((entry) => entry.line.includes("latest crash"))).toBe(true);
    expect(excerpt.some((entry) => entry.line.includes("old launch output"))).toBe(false);
  });
});
