import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";
import { themeInit } from "./src/theme/vite-plugin";

// Nur für die Demo-Seite (`pnpm --filter @tasks/ui dev`); Apps binden die Quellen direkt ein.
export default defineConfig({
  root: "demo",
  plugins: [svelte(), themeInit()],
  server: { port: 5180, strictPort: true },
});
