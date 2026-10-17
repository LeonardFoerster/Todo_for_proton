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
