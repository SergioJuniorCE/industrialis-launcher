import { describe, expect, it } from "vitest";
import forgeConfig from "../../forge.config";

type ConfiguredMaker = {
  name: string;
  config?: unknown;
  prepareConfig(targetArch: "x64"): Promise<void>;
};

describe("Forge packaging config", () => {
  it.each(["deb", "rpm"])("uses the packaged executable for the %s maker", async (makerName) => {
    const makers = forgeConfig.makers as ConfiguredMaker[];
    const maker = makers.find(({ name }) => name === makerName);

    expect(maker).toBeDefined();
    await maker!.prepareConfig("x64");
    expect(maker!.config).toMatchObject({
      options: {
        name: "industrialis-launcher",
        bin: "industrialis-launcher",
      },
    });
  });
});
