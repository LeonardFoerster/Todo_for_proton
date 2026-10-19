/**
 * Befehlspalette (P4-05): Suchwörter, Filter für Listen/Befehle, Hervorhebung der Treffer. Die
 * Task-Suche selbst läuft im Core (`searchTasks`, ADR-027) – hier nur, was die Anzeige braucht.
 */

/** Suchwörter wie im Core: an Leerraum getrennt, klein. */
export function terms(text: string): string[] {
  return text.toLowerCase().split(/\s+/).filter(Boolean);
}

/** Enthält `label` alle Wörter (Teilwort, ohne Groß/Klein)? Ohne Wörter: ja. */
export function matchesAll(label: string, words: readonly string[]): boolean {
  const lower = label.toLowerCase();
  return words.every((w) => lower.includes(w));
}

export interface Segment {
  text: string;
  hit: boolean;
}

/**
 * Zerlegt `text` in Abschnitte mit und ohne Treffer (alle Vorkommen aller Wörter). Ändert
 * `toLowerCase` die Länge (seltene Sonderfälle), bleibt der Text unmarkiert.
 */
export function highlight(text: string, words: readonly string[]): Segment[] {
  const lower = text.toLowerCase();
  if (words.length === 0 || lower.length !== text.length) return [{ text, hit: false }];
  const marked = new Array<boolean>(text.length).fill(false);
  for (const w of words) {
    for (let at = lower.indexOf(w); at >= 0; at = lower.indexOf(w, at + 1)) marked.fill(true, at, at + w.length);
  }
  const out: Segment[] = [];
  for (let i = 0; i < text.length; i++) {
    const last = out.at(-1);
    if (last && last.hit === marked[i]) last.text += text[i];
    else out.push({ text: text[i], hit: marked[i] });
  }
  return out;
}
