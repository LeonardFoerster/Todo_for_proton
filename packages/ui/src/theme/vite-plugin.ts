import type { Plugin } from "vite";
// Endung nötig: Die Vite-Konfiguration lädt diese Datei direkt in Node (Type-Stripping).
import { THEME_INIT_SCRIPT } from "./theme.ts";

/** Fügt das Theme-Snippet als erstes Element in den `<head>` ein (kein Weiß-Blitz). */
export function themeInit(): Plugin {
  return {
    name: "tasks-theme-init",
    transformIndexHtml: () => [{ tag: "script", children: THEME_INIT_SCRIPT, injectTo: "head-prepend" }],
  };
}
