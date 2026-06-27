import { describe, expect, it } from "vitest";
import { getLatestPackVersion, getPackVersionInfo } from "./pack-version-status";

const versions = {
  "2.8.0": { title: "Stable release", releaseDate: "2025/09/27", maxJavaVersion: 25 },
  "2.8.4": { title: "Stable release", releaseDate: "2025/06/08", maxJavaVersion: 25 },
  "2.9.0-beta-1": { title: "Beta release", releaseDate: "2026/01/15", maxJavaVersion: 25 },
};

describe("pack version status", () => {
  it("picks the newest pack as latest", () => {
    expect(getLatestPackVersion(versions)).toBe("2.9.0-beta-1");
  });

  it("marks older instances as update available", () => {
    expect(getPackVersionInfo("2.8.0", versions)).toEqual({
      status: "update-available",
      currentVersion: "2.8.0",
      latestVersion: "2.9.0-beta-1",
    });
  });

  it("marks instances on the latest pack as up to date", () => {
    expect(getPackVersionInfo("2.9.0-beta-1", versions)).toEqual({
      status: "up-to-date",
      currentVersion: "2.9.0-beta-1",
      latestVersion: "2.9.0-beta-1",
    });
  });

  it("treats unknown catalogs as unknown status", () => {
    expect(getPackVersionInfo("2.8.0", null).status).toBe("unknown");
  });
});