import type { DueDto } from "@tasks/core-client";
import { addDays, isoDate, parseIsoDate } from "./local-day";

export type DueTone = "overdue" | "today" | "soon" | "later";

export interface DueLabel {
  text: string;
  tone: DueTone;
  /** Ausführlich für Tooltip/Screenreader. */
  long: string;
}

const WEEKDAY = new Intl.DateTimeFormat("de-DE", { weekday: "short" });
const SHORT = new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "short" });
const SHORT_YEAR = new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "short", year: "numeric" });
const LONG = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
const TIME = new Intl.DateTimeFormat("de-DE", { hour: "2-digit", minute: "2-digit" });

/** Kalendertag + optionale Uhrzeit (lokal) einer Fälligkeit. */
function parts(due: DueDto): { day: Date; time: string | null } {
  if (due.kind === "date") return { day: parseIsoDate(due.date), time: null };
  const at = new Date(due.at);
  return { day: new Date(at.getFullYear(), at.getMonth(), at.getDate()), time: TIME.format(at) };
}

/** Kurzform für Zeilen: „Heute“, „Morgen“, „Gestern“, Wochentag (≤ 6 Tage), sonst „20. Okt.“. */
export function dueLabel(due: DueDto, now: Date = new Date()): DueLabel {
  const { day, time } = parts(due);
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const diff = Math.round((day.getTime() - today.getTime()) / 86_400_000);
  let text: string;
  if (diff === 0) text = "Heute";
  else if (diff === 1) text = "Morgen";
  else if (diff === -1) text = "Gestern";
  else if (diff > 1 && diff < 7) text = WEEKDAY.format(day).replace(".", "");
  else text = (day.getFullYear() === today.getFullYear() ? SHORT : SHORT_YEAR).format(day);
  const overdue = due.kind === "dateTime" ? new Date(due.at) < now : diff < 0;
  const tone: DueTone = overdue ? "overdue" : diff === 0 ? "today" : diff < 7 ? "soon" : "later";
  return {
    text: time ? `${text} ${time}` : text,
    tone,
    long: `Fällig ${LONG.format(day)}${time ? `, ${time} Uhr` : ""}${overdue ? " (überfällig)" : ""}`,
  };
}

/** Datum für Eingabefelder (`<input type="date">`). */
export function dueToInputDate(due: DueDto | null): string {
  if (!due) return "";
  return due.kind === "date" ? due.date : isoDate(parts(due).day);
}

export function todayPlus(days: number, now: Date = new Date()): DueDto {
  return { kind: "date", date: isoDate(addDays(now, days)) };
}
