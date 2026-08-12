import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
import path from "node:path";
import forgeConfig from "../../forge.config";

type ConfiguredMaker = {
  name: string;
  config?: unknown;
  prepareConfig(targetArch: "x64"): Promise<void>;
};

describe("Forge packaging config", () => {
  it("bundles the default GTNH instance icon", () => {
    const resources = forgeConfig.packagerConfig?.extraResource as string[];
    const icons = resources.find((resource) => path.basename(resource) === "icons");

    expect(icons).toBeDefined();
    expect(existsSync(path.join(icons!, "gtnh-logo.png"))).toBe(true);
  });

  it.each([
    ["deb", { name: "industrialis-launcher", bin: "industrialis-launcher" }],
    ["rpm", { name: "industrialis-launcher", bin: "industrialis-launcher", license: "UNLICENSED" }],
  ])("configures package metadata for the %s maker", async (makerName, expectedOptions) => {
    const makers = forgeConfig.makers as ConfiguredMaker[];
    const maker = makers.find(({ name }) => name === makerName);

    expect(maker).toBeDefined();
    await maker!.prepareConfig("x64");
    expect(maker!.config).toMatchObject({
      options: expectedOptions,
    });
  });
});
