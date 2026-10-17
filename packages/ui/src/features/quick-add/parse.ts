import type { DueDto } from "@tasks/core-client";
import { todayPlus } from "../../lib/format-date";

export interface ParsedQuickAdd {
  title: string;
  due: DueDto | null;
  /** Erkanntes Schlüsselwort (für die Vorschau), z. B. „morgen“. */
  dueWord: string | null;
}

/** Prototyp-Teilmenge (roadmap P7-08): nur einfache Tagesangaben, als ganzes Wort. */
const WORDS: Record<string, number> = { heute: 0, morgen: 1, übermorgen: 2 };
const PATTERN = /(^|\s)(heute|morgen|übermorgen)(?=\s|$)/iu;

/**
 * Zerlegt die Eingabe in Titel und Fälligkeit. Das letzte erkannte Wort zählt; es wird aus dem
 * Titel entfernt. Ohne Rest-Titel bleibt der Text, wie er ist („Morgen“ als Titel).
 */
export function parseQuickAdd(input: string, now: Date = new Date()): ParsedQuickAdd {
  const text = input.trim();
  let match: RegExpExecArray | null = null;
  const re = new RegExp(PATTERN.source, "giu");
  for (let m = re.exec(text); m; m = re.exec(text)) match = m;
  if (!match) return { title: text, due: null, dueWord: null };
  const word = match[2].toLowerCase();
  const start = match.index + match[1].length;
  const title = (text.slice(0, start) + text.slice(start + match[2].length)).replace(/\s{2,}/g, " ").trim();
  if (!title) return { title: text, due: null, dueWord: null };
  return { title, due: todayPlus(WORDS[word], now), dueWord: word };
}
