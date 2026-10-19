import type { DueDto } from "@tasks/core-client";
import type { Route } from "./router.svelte";
import { addDays, isoDate, parseIsoDate } from "./local-day";

/**
 * Abschnitte in Sammelansichten (P4): „Heute“ teilt in Überfällig/Heute, „Geplant“ nach Tagen,
 * „Erledigt“ nach dem Tag des Abhakens.
 * Die Eingabe ist bereits sortiert (Core + `compareInView`); hier wird nur aufgeteilt – O(n) je
 * Änderung der Zeilen, nie je Tastendruck.
 */

/** Wie viel Datum die Zeile noch zeigt: alles, nur die Uhrzeit (Tag steht in der Überschrift). */
export type DueMode = "full" | "time";

export type GroupKind = "overdue" | "today" | "day" | "later";

export interface GroupHeader {
  kind: "header";
  /** Eindeutig in der Ansicht (Schlüssel der virtuellen Liste). */
  id: string;
  group: GroupKind;
  label: string;
  count: number;
}

export interface TaskEntry<T> {
  kind: "task";
  task: T;
  dueMode: DueMode;
}

export type Entry<T> = GroupHeader | TaskEntry<T>;

interface Dated {
  due: DueDto | null;
  /** Nur „Erledigt“ (ISO-8601). */
  completedAt?: string | null;
}

const WEEKDAY_DATE = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short" });
const MONTH = new Intl.DateTimeFormat("de-DE", { month: "long" });
const MONTH_YEAR = new Intl.DateTimeFormat("de-DE", { month: "long", year: "numeric" });
const WEEKDAY_LONG = new Intl.DateTimeFormat("de-DE", { weekday: "long" });

/** Ist die Fälligkeit verstrichen? Wie `dueLabel`: Datum ab dem Folgetag, Uhrzeit sobald vorbei. */
export function isOverdue(due: DueDto, now: Date = new Date()): boolean {
  return due.kind === "date" ? due.date < isoDate(now) : new Date(due.at) < now;
}

/** Lokaler Kalendertag einer Fälligkeit (Mitternacht). */
function dueDay(due: DueDto): Date {
  if (due.kind === "date") return parseIsoDate(due.date);
  const at = new Date(due.at);
  return new Date(at.getFullYear(), at.getMonth(), at.getDate());
}

/** „Mi, 21. Okt.“ (ohne Punkt nach dem Wochentag). */
export function weekdayDate(day: Date): string {
  return WEEKDAY_DATE.format(day).replace(/^(\w+)\.,/, "$1,");
}

/**
 * Abschnitt in „Geplant“: Morgen, die nächsten Tage einzeln, Rest des Monats, dann Monate. Liefert
 * nur Schlüssel und Art – der Text (`Intl`) entsteht erst je Überschrift, nicht je Task.
 */
function upcomingGroup(due: DueDto, today: Date): { id: string; group: GroupKind; day: Date } {
  const day = dueDay(due);
  const diff = Math.round((day.getTime() - today.getTime()) / 86_400_000);
  if (diff <= 1) return { id: "tomorrow", group: "day", day: addDays(today, 1) };
  if (diff < 7) return { id: isoDate(day), group: "day", day };
  if (day.getFullYear() === today.getFullYear() && day.getMonth() === today.getMonth()) {
    return { id: "later-this-month", group: "later", day };
  }
  return { id: `${day.getFullYear()}-${day.getMonth() + 1}`, group: "later", day };
}

function upcomingLabel(g: { id: string; day: Date }, today: Date): string {
  if (g.id === "tomorrow") return "Morgen";
  if (g.id === "later-this-month") return "Später in diesem Monat";
  if (/^\d{4}-\d{2}-\d{2}$/.test(g.id)) return weekdayDate(g.day);
  return (g.day.getFullYear() === today.getFullYear() ? MONTH : MONTH_YEAR).format(g.day);
}

/** Zeilen samt Überschriften; `null` = Ansicht wird nicht gruppiert (flache Liste). */
export function groupEntries<T extends Dated>(items: readonly T[], route: Route, now: Date = new Date()): Entry<T>[] | null {
  if (route.view === "today") return groupToday(items, now);
  if (route.view === "upcoming") return groupUpcoming(items, now);
  if (route.view === "completed") return groupCompleted(items, now);
  return null;
}

/**
 * „Heute“: überfällige oben (älteste zuerst – Eingabe ist nach Fälligkeit sortiert), dann heute.
 * Ohne Überfällige keine Überschriften – der Seitentitel sagt schon „Heute“.
 */
function groupToday<T extends Dated>(items: readonly T[], now: Date): Entry<T>[] {
  const overdue: T[] = [];
  const today: T[] = [];
  for (const t of items) (t.due && isOverdue(t.due, now) ? overdue : today).push(t);
  const rows = (list: T[], dueMode: DueMode) => list.map((task): TaskEntry<T> => ({ kind: "task", task, dueMode }));
  if (overdue.length === 0) return rows(today, "time");
  const out: Entry<T>[] = [
    { kind: "header", id: "overdue", group: "overdue", label: "Überfällig", count: overdue.length },
    ...rows(overdue, "full"),
  ];
  if (today.length > 0) {
    out.push({ kind: "header", id: "today", group: "today", label: "Heute", count: today.length }, ...rows(today, "time"));
  }
  return out;
}

function groupUpcoming<T extends Dated>(items: readonly T[], now: Date): Entry<T>[] {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const out: Entry<T>[] = [];
  let header = null as GroupHeader | null;
  for (const task of items) {
    if (!task.due) continue;
    const g = upcomingGroup(task.due, today);
    if (header?.id !== g.id) {
      header = { kind: "header", id: g.id, group: g.group, label: upcomingLabel(g, today), count: 0 };
      out.push(header);
    }
    header.count++;
    out.push({ kind: "task", task, dueMode: g.group === "day" ? "time" : "full" });
  }
  return out;
}

/** Montag der Woche von `day` (lokal, Mitternacht). */
function mondayOf(day: Date): Date {
  return addDays(day, -((day.getDay() + 6) % 7));
}

/**
 * Abschnitt in „Erledigt“: Heute, Gestern, übrige Tage dieser Woche („Montag“), Letzte Woche, dann
 * Monate (fremdes Jahr mit Jahreszahl). Nur Schlüssel und Art; Text je Überschrift.
 */
function completedGroup(at: Date, today: Date): { id: string; day: Date } {
  const day = new Date(at.getFullYear(), at.getMonth(), at.getDate());
  const diff = Math.round((today.getTime() - day.getTime()) / 86_400_000);
  if (diff <= 0) return { id: "today", day };
  if (diff === 1) return { id: "yesterday", day };
  const monday = mondayOf(today);
  if (day >= monday) return { id: `weekday-${isoDate(day)}`, day };
  if (day >= addDays(monday, -7)) return { id: "last-week", day };
  return { id: `${day.getFullYear()}-${day.getMonth() + 1}`, day };
}

function completedLabel(g: { id: string; day: Date }, today: Date): string {
  if (g.id === "today") return "Heute";
  if (g.id === "yesterday") return "Gestern";
  if (g.id === "last-week") return "Letzte Woche";
  if (g.id.startsWith("weekday-")) return WEEKDAY_LONG.format(g.day);
  return (g.day.getFullYear() === today.getFullYear() ? MONTH : MONTH_YEAR).format(g.day);
}

/** Eingabe ist nach `completedAt` absteigend sortiert (Core), Abschnitte liegen also am Stück. */
function groupCompleted<T extends Dated>(items: readonly T[], now: Date): Entry<T>[] {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const out: Entry<T>[] = [];
  let header = null as GroupHeader | null;
  for (const task of items) {
    const g = completedGroup(task.completedAt ? new Date(task.completedAt) : now, today);
    if (header?.id !== g.id) {
      header = { kind: "header", id: g.id, group: "later", label: completedLabel(g, today), count: 0 };
      out.push(header);
    }
    header.count++;
    out.push({ kind: "task", task, dueMode: "full" });
  }
  return out;
}
