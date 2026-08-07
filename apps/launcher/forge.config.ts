import path from "node:path";
import type { ForgeConfig } from "@electron-forge/shared-types";
import { FusesPlugin } from "@electron-forge/plugin-fuses";
import { VitePlugin } from "@electron-forge/plugin-vite";
import { FuseVersion, FuseV1Options } from "@electron/fuses";
import MakerDeb from "@electron-forge/maker-deb";
import MakerDmg from "@electron-forge/maker-dmg";
import MakerRpm from "@electron-forge/maker-rpm";
import MakerSquirrel from "@electron-forge/maker-squirrel";
import MakerZip from "@electron-forge/maker-zip";

const config: ForgeConfig = {
  packagerConfig: {
    asar: true,
    name: "industrialis-launcher",
    executableName: "industrialis-launcher",
    icon: path.resolve(__dirname, "electron/assets/icon"),
    extraResource: [path.resolve(__dirname, "electron/config")],
    protocols: [
      {
        name: "Industrialis Launcher",
        schemes: ["industrialislauncher"],
      },
    ],
  },
  rebuildConfig: {},
  makers: [
    new MakerSquirrel({
      name: "IndustrialisLauncher",
      setupExe: "IndustrialisLauncherSetup.exe",
      authors: "Industrialis",
      description: "Industrialis Minecraft launcher",
    }),
    new MakerZip({}, ["win32", "darwin", "linux"]),
    new MakerDmg({ format: "ULFO" }),
    new MakerDeb({}),
    new MakerRpm({}),
  ],
  plugins: [
    new VitePlugin({
      build: [
        { entry: "electron/main.ts", config: "vite.main.config.ts" },
        { entry: "electron/preload.ts", config: "vite.preload.config.ts" },
      ],
      renderer: [{ name: "main_window", config: "vite.renderer.config.ts" }],
    }),
    new FusesPlugin({
      version: FuseVersion.V1,
      [FuseV1Options.RunAsNode]: false,
      [FuseV1Options.EnableCookieEncryption]: true,
      [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
      [FuseV1Options.EnableNodeCliInspectArguments]: false,
      [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
      [FuseV1Options.OnlyLoadAppFromAsar]: true,
    }),
  ],
};

export default config;
