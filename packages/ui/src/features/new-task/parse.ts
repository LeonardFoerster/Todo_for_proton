import type { DueDto, PriorityDto, TaskListDto } from "@tasks/core-client";
import { addDays, isoDate } from "../../lib/local-day";

export interface ParsedNewTask {
  title: string;
  due: DueDto | null;
  priority: PriorityDto | null;
  /** Über `@name` gewählte Liste. */
  listId: string | null;
}

export interface ParseOptions {
  now?: Date;
  /** Für `@name`; ohne Listen bleibt `@…` Teil des Titels. */
  lists?: readonly TaskListDto[];
  /** Zeitzone für Uhrzeiten (Standard: die des Systems). */
  tz?: string;
}

const DAY_WORDS: Record<string, number> = { heute: 0, morgen: 1, übermorgen: 2 };
/** Wochentag (0 = Sonntag wie `Date#getDay`). „so“ nur mit Punkt – sonst ist es das Füllwort. */
const WEEKDAYS: Record<string, number> = {
  sonntag: 0, "so.": 0, montag: 1, mo: 1, dienstag: 2, di: 2, mittwoch: 3, mi: 3,
  donnerstag: 4, do: 4, freitag: 5, fr: 5, samstag: 6, sa: 6,
};
const PRIORITIES: Record<string, PriorityDto> = { "!1": "high", "!2": "medium", "!3": "low" };
/** Steht direkt vor Datum/Uhrzeit und fällt mit ihnen weg („bis Fr“, „um 14:00“). */
const PREPOSITIONS = new Set(["am", "um", "bis"]);

interface Day {
  date: Date;
}
interface Time {
  hour: number;
  minute: number;
}
/** Erkannte Angabe: Tokenbereich `[start, end)` und Wert. */
interface Hit<T> {
  start: number;
  end: number;
  value: T;
}

/** Erkennt Tag, Uhrzeit, Priorität und Liste in den Wörtern; je Art zählt die letzte Angabe. */
function scan(tokens: readonly string[], now: Date, lists: readonly TaskListDto[] | undefined) {
  let day: Hit<Day> | null = null;
  let time: Hit<Time> | null = null;
  let priority: Hit<PriorityDto> | null = null;
  let list: Hit<string> | null = null;

  for (let i = 0; i < tokens.length; i++) {
    const word = tokens[i].toLowerCase();
    const next = tokens[i + 1]?.toLowerCase();
    let found: Day | null = null;
    if (word in DAY_WORDS) found = { date: addDays(now, DAY_WORDS[word]) };
    else if (word.replace(/\.$/, "") in WEEKDAYS || word in WEEKDAYS) {
      const target = WEEKDAYS[word] ?? WEEKDAYS[word.replace(/\.$/, "")];
      found = { date: addDays(now, ((target - now.getDay() + 6) % 7) + 1) };
    } else {
      const date = parseDate(word, now);
      if (date) found = { date };
    }
    if (found) {
      day = { start: i, end: i + 1, value: found };
      continue;
    }
    const clock = parseTime(word) ?? (next === "uhr" ? parseHour(word) : null);
    if (clock) {
      const end = next === "uhr" && !word.includes(":") ? i + 2 : i + 1;
      time = { start: i, end, value: clock };
      i = end - 1;
      continue;
    }
    if (word in PRIORITIES) {
      priority = { start: i, end: i + 1, value: PRIORITIES[word] };
      continue;
    }
    if (word.startsWith("@") && word.length > 1 && lists) {
      const id = matchList(word.slice(1), lists);
      if (id) list = { start: i, end: i + 1, value: id };
    }
  }

  return { day, time, priority, list };
}

/**
 * Nur eine Fälligkeit aus Text wie in der Titelzeile („fr“, „24.10.“, „morgen 14:00“) – für
 * „Datum wählen…“ (P4-04). Andere Wörter werden ignoriert; ohne Tag und Uhrzeit `null`.
 */
export function parseDue(input: string, options: Omit<ParseOptions, "lists"> = {}): DueDto | null {
  const now = options.now ?? new Date();
  const { day, time } = scan(input.trim().split(/\s+/).filter(Boolean), now, undefined);
  if (!day && !time) return null;
  return toDue(day?.value.date ?? null, time?.value ?? null, now, options.tz);
}

/**
 * Zerlegt die Eingabe im Fenster „Neue Task“ in Titel und Eigenschaften (ganze Wörter, groß/klein egal):
 * - Tag: `heute`, `morgen`, `übermorgen`, Wochentag (`Fr`, `Freitag` – nächster, nie heute), `24.10.`, `24.10.2026`
 * - Uhrzeit: `14:00`, `14 Uhr` (ohne Tag: heute, falls schon vorbei morgen)
 * - Priorität: `!1` hoch, `!2` mittel, `!3` niedrig
 * - Liste: `@name` (Anfang des Namens reicht, wenn eindeutig)
 *
 * Je Art zählt die letzte Angabe; nur sie verschwindet aus dem Titel (samt „am/um/bis“ davor). Bleibt
 * kein Titel übrig, gilt der Text unverändert als Titel („Morgen“).
 */
export function parseNewTask(input: string, options: ParseOptions = {}): ParsedNewTask {
  const now = options.now ?? new Date();
  const text = input.trim();
  const tokens = text.split(/\s+/).filter(Boolean);
  const { day, time, priority, list } = scan(tokens, now, options.lists);

  const drop = new Set<number>();
  for (const hit of [day, time, priority, list]) {
    if (!hit) continue;
    for (let i = hit.start; i < hit.end; i++) drop.add(i);
  }
  for (const hit of [day, time]) {
    const before = hit ? hit.start - 1 : -1;
    if (before >= 0 && !drop.has(before) && PREPOSITIONS.has(tokens[before].toLowerCase())) drop.add(before);
  }
  const title = tokens.filter((_, i) => !drop.has(i)).join(" ");
  if (!title) return { title: text, due: null, priority: null, listId: null };

  return {
    title,
    due: toDue(day?.value.date ?? null, time?.value ?? null, now, options.tz),
    priority: priority?.value ?? null,
    listId: list?.value ?? null,
  };
}

/** `24.10.`, `24.10.26`, `24.10.2026`; ohne Jahr der nächste solche Tag ab heute. */
function parseDate(word: string, now: Date): Date | null {
  const m = /^(\d{1,2})\.(\d{1,2})\.(\d{2}|\d{4})?$/.exec(word);
  if (!m) return null;
  const [d, mo] = [Number(m[1]), Number(m[2])];
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const year = m[3] ? Number(m[3]) + (m[3].length === 2 ? 2000 : 0) : now.getFullYear();
  let date = new Date(year, mo - 1, d);
  if (!m[3] && date < today) date = new Date(year + 1, mo - 1, d);
  return date.getMonth() === mo - 1 && date.getDate() === d ? date : null;
}

function parseTime(word: string): Time | null {
  const m = /^(\d{1,2}):(\d{2})(?:uhr)?$/.exec(word) ?? /^(\d{1,2})uhr$/.exec(word);
  return m ? validTime(Number(m[1]), Number(m[2] ?? 0)) : null;
}

function parseHour(word: string): Time | null {
  return /^\d{1,2}$/.test(word) ? validTime(Number(word), 0) : null;
}

function validTime(hour: number, minute: number): Time | null {
  return hour < 24 && minute < 60 ? { hour, minute } : null;
}

/** Zum Vergleich: klein, ohne Leer- und Satzzeichen („Uni – Analysis“ → „unianalysis“). */
const normalize = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");

function matchList(query: string, lists: readonly TaskListDto[]): string | null {
  const q = normalize(query);
  if (!q) return null;
  const named = lists.map((l) => ({ id: l.id, name: normalize(l.isInbox ? "Eingang" : l.name) }));
  const exact = named.find((l) => l.name === q);
  if (exact) return exact.id;
  const prefix = named.filter((l) => l.name.startsWith(q));
  return prefix.length === 1 ? prefix[0].id : null;
}

function toDue(day: Date | null, time: Time | null, now: Date, tz?: string): DueDto | null {
  if (!time) return day ? { kind: "date", date: isoDate(day) } : null;
  let base = day ?? now;
  let at = new Date(base.getFullYear(), base.getMonth(), base.getDate(), time.hour, time.minute);
  if (!day && at <= now) {
    base = addDays(now, 1);
    at = new Date(base.getFullYear(), base.getMonth(), base.getDate(), time.hour, time.minute);
  }
  return { kind: "dateTime", at: at.toISOString(), tz: tz ?? Intl.DateTimeFormat().resolvedOptions().timeZone };
}
