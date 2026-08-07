import { defineConfig } from "vite";
import { builtinModules } from "node:module";

const nodeBuiltins = [...builtinModules, ...builtinModules.map((name) => `node:${name}`)];

export default defineConfig({
  build: {
    outDir: ".vite/build",
    emptyOutDir: false,
    lib: {
      entry: "electron/main.ts",
      formats: ["cjs"],
      fileName: () => "main.js",
    },
    rollupOptions: {
      external: ["electron", "electron-squirrel-startup", ...nodeBuiltins],
    },
  },
});
