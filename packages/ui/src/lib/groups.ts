import type { DueDto } from "@tasks/core-client";
import type { Route } from "./router.svelte";
import { addDays, isoDate, parseIsoDate } from "./local-day";

/**
 * Abschnitte in Sammelansichten (P4): „Heute“ teilt in Überfällig/Heute, „Geplant“ nach Tagen.
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
}

const WEEKDAY_DATE = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short" });
const MONTH = new Intl.DateTimeFormat("de-DE", { month: "long" });
const MONTH_YEAR = new Intl.DateTimeFormat("de-DE", { month: "long", year: "numeric" });

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

/** Abschnitt in „Geplant“: Morgen, die nächsten Tage einzeln, Rest des Monats, dann Monate. */
function upcomingGroup(due: DueDto, now: Date): { id: string; group: GroupKind; label: string } {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const day = dueDay(due);
  const diff = Math.round((day.getTime() - today.getTime()) / 86_400_000);
  if (diff <= 1) return { id: isoDate(addDays(today, 1)), group: "day", label: "Morgen" };
  if (diff < 7) return { id: isoDate(day), group: "day", label: weekdayDate(day) };
  if (day.getFullYear() === today.getFullYear() && day.getMonth() === today.getMonth()) {
    return { id: "later-this-month", group: "later", label: "Später in diesem Monat" };
  }
  const id = `${day.getFullYear()}-${String(day.getMonth() + 1).padStart(2, "0")}`;
  const label = (day.getFullYear() === today.getFullYear() ? MONTH : MONTH_YEAR).format(day);
  return { id, group: "later", label };
}

/** Zeilen samt Überschriften; `null` = Ansicht wird nicht gruppiert (flache Liste). */
export function groupEntries<T extends Dated>(items: readonly T[], route: Route, now: Date = new Date()): Entry<T>[] | null {
  if (route.view === "today") return groupToday(items, now);
  if (route.view === "upcoming") return groupUpcoming(items, now);
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
  const out: Entry<T>[] = [];
  let header: GroupHeader | null = null;
  for (const task of items) {
    if (!task.due) continue;
    const g = upcomingGroup(task.due, now);
    if (header?.id !== g.id) {
      header = { kind: "header", ...g, count: 0 };
      out.push(header);
    }
    header.count++;
    out.push({ kind: "task", task, dueMode: g.group === "day" ? "time" : "full" });
  }
  return out;
}
