/** Kleine UI-Vorlieben in `localStorage` (nie Daten oder Geheimnisse). */
export function readFlag(key: string, fallback = false): boolean {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === "1";
  } catch {
    return fallback;
  }
}

export function writeFlag(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, value ? "1" : "0");
  } catch {
    // egal – gilt dann nur bis zum Neustart
  }
}

/** Eine von mehreren Möglichkeiten (z. B. Darstellung `list`/`board`); unbekannt → `fallback`. */
export function readChoice<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return allowed.includes(v as T) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
}

export function writeChoice(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // egal – gilt dann nur bis zum Neustart
  }
}
