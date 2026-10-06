import { describe, expect, it } from "vitest";
import { buildVersionDetails } from "./version-details.js";

describe("buildVersionDetails", () => {
  it("marks stable releases and beta/rc releases like the launcher", () => {
    const details = buildVersionDetails(["stable-latest", "2.9.0-RC-2", "2.8.4"], {
      "2.9.0-RC-2": { title: "Beta release", releaseDate: "2026/10/04", maxJavaVersion: 26 },
      "2.8.4": { title: "Stable release", releaseDate: "2025/12/23", maxJavaVersion: 25 },
    });

    expect(details[0]?.tag).toBe("stable-latest");
    const rc = details.find((detail) => detail.tag === "2.9.0-RC-2");
    expect(rc?.channel).toBe("beta");
    expect(rc?.releaseDate).toBe("2026/10/04");
    expect(rc?.maxJavaVersion).toBe(26);
    expect(details.find((detail) => detail.tag === "2.8.4")?.channel).toBe("stable");
  });

  it("sorts by release date with the stable alias first", () => {
    const details = buildVersionDetails(["2.8.4", "2.9.0-RC-2", "stable-latest"], {
      "2.9.0-RC-2": { title: "Beta release", releaseDate: "2026/10/04", maxJavaVersion: 26 },
      "2.8.4": { title: "Stable release", releaseDate: "2025/12/23", maxJavaVersion: 25 },
    });

    expect(details.map((detail) => detail.tag)).toEqual(["stable-latest", "2.9.0-RC-2", "2.8.4"]);
  });

  it("falls back to tag heuristics when pack metadata is missing", () => {
    const details = buildVersionDetails(["2.9.0-beta-3", "2.8.4"], null);

    expect(details.find((detail) => detail.tag === "2.9.0-beta-3")?.channel).toBe("beta");
    expect(details.find((detail) => detail.tag === "2.8.4")?.channel).toBe("stable");
    expect(details.find((detail) => detail.tag === "2.8.4")?.releaseDate).toBeNull();
  });

  it("labels the stable-latest alias as a stable release", () => {
    const details = buildVersionDetails(["stable-latest", "2.8.4"], null);

    expect(details.find((detail) => detail.tag === "stable-latest")?.channel).toBe("stable");
  });

  it("collapses stable-prefixed duplicates into the bare release tag", () => {
    const details = buildVersionDetails(["stable-2.9.0-RC-1", "2.9.0-RC-1", "stable-2.8.4", "2.8.4", "stable-latest"], {
      "2.9.0-RC-1": { title: "Beta release", releaseDate: "2026/09/25", maxJavaVersion: 26 },
      "2.8.4": { title: "Stable release", releaseDate: "2025/12/23", maxJavaVersion: 25 },
    });

    expect(details.map((detail) => detail.tag)).toEqual(["stable-latest", "2.9.0-RC-1", "2.8.4"]);
    expect(details.find((detail) => detail.tag === "2.9.0-RC-1")?.channel).toBe("beta");
  });

  it("drops nightly, sha, date, and build-number tags from the registry", () => {
    const details = buildVersionDetails(
      ["stable-latest", "nightly-latest", "nightly-2024-11-26", "941b637fb1154188c1b5ad1420ea61836ab014e2", "2024-11-25", "752", "2.8.4", "stable-2.8.4"],
      null,
    );

    expect(details.map((detail) => detail.tag)).toEqual(["stable-latest", "2.8.4"]);
  });
});
