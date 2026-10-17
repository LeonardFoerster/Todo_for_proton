import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

// Eigene Konfiguration, weil vite.config.ts die Demo als Wurzel setzt.
export default defineConfig({
  plugins: [svelte()],
  test: { include: ["src/**/*.test.ts"] },
});
