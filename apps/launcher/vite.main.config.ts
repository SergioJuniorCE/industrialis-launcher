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
      // Keep runtime packages inside the ASAR; pnpm workspace links are not portable.
      external: ["electron", ...nodeBuiltins],
    },
  },
});
