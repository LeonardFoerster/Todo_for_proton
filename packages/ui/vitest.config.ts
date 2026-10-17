import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { defineConfig } from "vitest/config";

// Eigene Konfiguration, weil vite.config.ts die Demo als Wurzel setzt.
export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  test: {
    include: ["src/**/*.test.ts"],
    environment: "jsdom",
    setupFiles: ["src/test-setup.ts"],
  },
});
