import { defineConfig } from "astro/config";
import node from "@astrojs/node";
import react from "@astrojs/react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  output: "server",
  adapter: node({ mode: "standalone" }),
  integrations: [react()],
  server: {
    host: "127.0.0.1",
    port: 3001,
  },
  vite: {
    plugins: [tailwindcss()],
    ssr: {
      noExternal: ["@industrialis/server-contracts"],
    },
  },
});
