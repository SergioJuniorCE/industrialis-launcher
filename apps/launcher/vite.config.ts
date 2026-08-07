import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { themeBootInlineScript } from "./src/lib/theme-boot";

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    {
      name: "theme-boot-inline",
      transformIndexHtml(html) {
        return html.replace(
          /<!-- theme-boot -->[\s\S]*?<!-- \/theme-boot -->/,
          `<script>${themeBootInlineScript()}</script>`
        );
      },
    },
  ],

  clearScreen: false,
  resolve: {
    dedupe: ["react", "react-dom"],
  },
  server: {
    port: 5173,
    strictPort: true,
    host: true,
  },
});
