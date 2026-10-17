import type { LocalDayDto } from "@tasks/core-client";

/** Kalendertag in der lokalen Zeitzone als `LocalDayDto` (Grenzen in UTC, wie der Core sie erwartet). */
export function localDay(now: Date = new Date()): LocalDayDto {
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const end = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  return { date: isoDate(start), start: start.toISOString(), end: end.toISOString() };
}

/** `YYYY-MM-DD` des lokalen Datums. */
export function isoDate(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Lokales Datum aus `YYYY-MM-DD` (Mitternacht lokal). */
export function parseIsoDate(s: string): Date {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function addDays(d: Date, days: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + days);
}
