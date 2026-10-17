import { svelte } from "@sveltejs/vite-plugin-svelte";
import { themeInit } from "@tasks/ui/vite";
import { defineConfig } from "vite";

// Tauri lädt im Dev-Modus von diesem festen Port; im Release die gebauten Dateien aus `dist/`.
export default defineConfig({
  plugins: [svelte(), themeInit()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // WebKitGTK 2.4x (ADR-001) – modernes JS ohne Down-Level-Transpilierung.
    target: "safari16",
    sourcemap: false,
  },
});
