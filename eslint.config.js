// Gemeinsame ESLint-Konfiguration (Flat Config) für apps/* und packages/*.
import js from "@eslint/js";
import svelte from "eslint-plugin-svelte";
import globals from "globals";
import ts from "typescript-eslint";

export default ts.config(
  {
    ignores: ["**/dist/**", "**/node_modules/**", "target/**", "**/src-tauri/**", "**/generated/**", "spikes/**", "development/**"],
  },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs["flat/recommended"],
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
  },
  {
    files: ["**/*.svelte", "**/*.svelte.ts"],
    languageOptions: { parserOptions: { parser: ts.parser } },
  },
  {
    // Architekturregel (AGENTS.md §3.5): Nur der Tauri-Client und die Desktop-Shell kennen Tauri.
    files: ["**/*.ts", "**/*.svelte"],
    ignores: ["packages/core-client/src/tauri-core-client.ts", "apps/desktop/src/**"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            { group: ["@tauri-apps/*"], message: "Tauri nur in packages/core-client/src/tauri-core-client.ts – die UI kennt nur CoreClient." },
          ],
        },
      ],
    },
  },
  {
    rules: {
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
    },
  },
);
