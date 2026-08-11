import { describe, expect, it } from "vitest";
import forgeConfig from "../../forge.config";

type ConfiguredMaker = {
  name: string;
  config?: unknown;
  prepareConfig(targetArch: "x64"): Promise<void>;
};

describe("Forge packaging config", () => {
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
