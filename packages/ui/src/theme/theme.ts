/**
 * Theme: System (Standard) / Hell / Dunkel. Ergebnis ist `data-theme` auf `<html>`.
 * Die Vorliebe liegt in `localStorage` (kein Geheimnis); `THEME_INIT_SCRIPT` setzt sie vor dem
 * ersten Paint, damit ein dunkler Start nicht weiß aufblitzt.
 */

export type ThemePreference = "system" | "light" | "dark";
export type Theme = "light" | "dark";

export const THEME_STORAGE_KEY = "tasks.theme";
const DARK_QUERY = "(prefers-color-scheme: dark)";

/** Läuft inline im `<head>` (Vite-Plugin) – gleiche Logik wie `resolveTheme`. */
export const THEME_INIT_SCRIPT = `(function(){try{var p=localStorage.getItem(${JSON.stringify(
  THEME_STORAGE_KEY,
)});var d=p==="dark"||(p!=="light"&&matchMedia(${JSON.stringify(
  DARK_QUERY,
)}).matches);document.documentElement.dataset.theme=d?"dark":"light"}catch(e){}})()`;

export function readPreference(): ThemePreference {
  try {
    const value = localStorage.getItem(THEME_STORAGE_KEY);
    return value === "light" || value === "dark" ? value : "system";
  } catch {
    return "system";
  }
}

export function resolveTheme(pref: ThemePreference, systemDark: boolean): Theme {
  return pref === "system" ? (systemDark ? "dark" : "light") : pref;
}

function systemDark(): boolean {
  return typeof matchMedia === "function" && matchMedia(DARK_QUERY).matches;
}

export function applyTheme(pref: ThemePreference = readPreference()): Theme {
  const theme = resolveTheme(pref, systemDark());
  document.documentElement.dataset.theme = theme;
  return theme;
}

export function setThemePreference(pref: ThemePreference): Theme {
  try {
    if (pref === "system") localStorage.removeItem(THEME_STORAGE_KEY);
    else localStorage.setItem(THEME_STORAGE_KEY, pref);
  } catch {
    // Ohne Speicher gilt die Wahl nur bis zum Neustart.
  }
  return applyTheme(pref);
}

/** Folgt dem System, solange die Vorliebe „System" ist. Gibt die Abmeldung zurück. */
export function followSystemTheme(onChange?: (theme: Theme) => void): () => void {
  if (typeof matchMedia !== "function") return () => {};
  const query = matchMedia(DARK_QUERY);
  const listener = () => {
    if (readPreference() === "system") onChange?.(applyTheme("system"));
  };
  query.addEventListener("change", listener);
  return () => query.removeEventListener("change", listener);
}
