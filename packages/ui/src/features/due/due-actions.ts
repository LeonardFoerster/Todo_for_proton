import type { CoreClient, DueDto, TaskSummaryDto } from "@tasks/core-client";
import { keepTime, todayPlus } from "../../lib/format-date";
import { weekdayDate } from "../../lib/groups";
import { addDays, isoDate, parseIsoDate } from "../../lib/local-day";
import { setDuesWithUndo } from "../../lib/task-actions";

/** Was „Fälligkeit setzen“ braucht (Zeile, Karte, Auswahl). */
export type DueTarget = Pick<TaskSummaryDto, "id" | "title" | "due">;

/** Schnellwahl im Menü „Fälligkeit“ (P4-04); `due: null` = entfernen. */
export interface DueChoice {
  id: "today" | "tomorrow" | "monday" | "week" | "none";
  label: string;
  due: DueDto | null;
}

/** Nächster Montag, nie heute. */
function nextMonday(now: Date): DueDto {
  return { kind: "date", date: isoDate(addDays(now, ((8 - now.getDay()) % 7) || 7)) };
}

export function dueChoices(now: Date = new Date()): DueChoice[] {
  return [
    { id: "today", label: "Heute", due: todayPlus(0, now) },
    { id: "tomorrow", label: "Morgen", due: todayPlus(1, now) },
    { id: "monday", label: "Nächster Montag", due: nextMonday(now) },
    { id: "week", label: "In einer Woche", due: todayPlus(7, now) },
    { id: "none", label: "Entfernen", due: null },
  ];
}

/** Ergebnis für den Toast: „auf heute“, „auf Fr, 23. Okt.“, „auf 3. Feb. 2027“. */
export function dueResult(due: DueDto | null, now: Date = new Date()): string {
  if (!due) return "Fälligkeit entfernt";
  const day = due.kind === "date" ? parseIsoDate(due.date) : new Date(due.at);
  const diff = Math.round(
    (new Date(day.getFullYear(), day.getMonth(), day.getDate()).getTime() -
      new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()) /
      86_400_000,
  );
  const time =
    due.kind === "dateTime" ? `, ${new Intl.DateTimeFormat("de-DE", { hour: "2-digit", minute: "2-digit" }).format(day)} Uhr` : "";
  const what =
    diff === 0
      ? "heute"
      : diff === 1
        ? "morgen"
        : day.getFullYear() === now.getFullYear()
          ? weekdayDate(day)
          : new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "short", year: "numeric" }).format(day);
  return `auf ${what}${time} verschoben`;
}

/**
 * Setzt die Fälligkeit aller `targets` in einem Aufruf (Rückgängig im Toast). Ein reiner Tag behält
 * je Task ihre Uhrzeit; eine Angabe mit Uhrzeit gilt so, wie sie ist.
 */
export function applyDue(client: CoreClient, targets: readonly DueTarget[], due: DueDto | null) {
  return setDuesWithUndo(client, targets, (t) => keepTime(due, t.due), dueResult(due));
}
