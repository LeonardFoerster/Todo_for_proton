// Wiederholungsregeln in der UI (N4, ADR-028): Klartext, Vorlagen und das erste Vorkommen ab heute.
// Rechnet nur, was die UI vor dem Anlegen braucht; das Fortschreiben macht der Core.
import type { DueDto, RecurrenceDto, WeekdayDto } from "@tasks/core-client";
import { addDays, parseIsoDate } from "./local-day";

/** Reihenfolge Mo–So; Index = `(Date#getDay() + 6) % 7`. */
export const WEEKDAYS: readonly WeekdayDto[] = ["mo", "tu", "we", "th", "fr", "sa", "su"];
export const WEEKDAY_SHORT: Record<WeekdayDto, string> = { mo: "Mo", tu: "Di", we: "Mi", th: "Do", fr: "Fr", sa: "Sa", su: "So" };
export const WORKDAYS: readonly WeekdayDto[] = ["mo", "tu", "we", "th", "fr"];

export const weekdayOf = (d: Date): WeekdayDto => WEEKDAYS[(d.getDay() + 6) % 7];

export function rule(freq: RecurrenceDto["freq"], extra: Partial<RecurrenceDto> = {}): RecurrenceDto {
  return { freq, interval: 1, weekdays: [], anchor: "schedule", ...extra };
}

/** Vorlagen im Chip-Menü. */
export type Preset = "daily" | "workdays" | "weekly" | "monthly" | "yearly";
export const PRESETS: { id: Preset; label: string; rule: () => RecurrenceDto }[] = [
  { id: "daily", label: "Täglich", rule: () => rule("daily") },
  { id: "workdays", label: "Werktags", rule: () => rule("weekly", { weekdays: [...WORKDAYS] }) },
  { id: "weekly", label: "Wöchentlich", rule: () => rule("weekly") },
  { id: "monthly", label: "Monatlich", rule: () => rule("monthly") },
  { id: "yearly", label: "Jährlich", rule: () => rule("yearly") },
];

/** Welche Vorlage die Regel ist (Wochentag/Monatstag der Fälligkeit zählen als „ohne“). */
export function presetOf(r: RecurrenceDto, due: DueDto | null): Preset | null {
  if (r.interval !== 1 || r.anchor !== "schedule" || r.until) return null;
  const day = due ? dueDate(due) : null;
  if (r.freq === "daily") return "daily";
  if (r.freq === "weekly") {
    if (sameDays(r.weekdays, WORKDAYS)) return "workdays";
    if (r.weekdays.length === 0 || (day && r.weekdays.length === 1 && r.weekdays[0] === weekdayOf(day))) return "weekly";
    return null;
  }
  if (r.monthDay != null && day && r.monthDay !== day.getDate()) return null;
  return r.freq;
}

const sameDays = (a: readonly WeekdayDto[], b: readonly WeekdayDto[]) =>
  a.length === b.length && b.every((d) => a.includes(d));

const dueDate = (due: DueDto) => (due.kind === "date" ? parseIsoDate(due.date) : new Date(due.at));

const UNITS: Record<RecurrenceDto["freq"], [string, string, string]> = {
  daily: ["täglich", "Tag", "Tage"],
  weekly: ["wöchentlich", "Woche", "Wochen"],
  monthly: ["monatlich", "Monat", "Monate"],
  yearly: ["jährlich", "Jahr", "Jahre"],
};

function joinDays(days: readonly WeekdayDto[]): string {
  const names = WEEKDAYS.filter((d) => days.includes(d)).map((d) => WEEKDAY_SHORT[d]);
  return names.length > 1 ? `${names.slice(0, -1).join(", ")} und ${names.at(-1)}` : names.join("");
}

/** Klartext: „wöchentlich, Mo und Do“, „alle 2 Wochen“, „werktags“, „monatlich am 15.“, „… nach Erledigung, bis 20.12.2026“. */
export function describeRule(r: RecurrenceDto): string {
  const [adverb, , plural] = UNITS[r.freq];
  let text = r.interval === 1 ? adverb : `alle ${r.interval} ${plural}`;
  if (r.freq === "weekly" && sameDays(r.weekdays, WORKDAYS) && r.interval === 1) text = "werktags";
  else if (r.freq === "weekly" && r.weekdays.length > 0) text += `, ${joinDays(r.weekdays)}`;
  if (r.freq === "monthly" && r.monthDay != null) text += ` am ${r.monthDay === 31 ? "Monatsletzten" : `${r.monthDay}.`}`;
  if (r.anchor === "completion") text += " nach Erledigung";
  if (r.until) {
    const until = parseIsoDate(r.until);
    text += `, bis ${until.getDate()}.${until.getMonth() + 1}.${until.getFullYear()}`;
  }
  return text;
}

/** Kurz für Chips: „Täglich“, „Werktags“, „Alle 2 Wochen“ … */
export function ruleChipText(r: RecurrenceDto): string {
  const text = describeRule(r).split(/,| am | nach /)[0];
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Erstes Vorkommen am oder nach `from` (lokal) – für eine Regel ohne Fälligkeit. */
export function firstOnOrAfter(r: RecurrenceDto, from: Date): Date {
  if (r.freq === "weekly" && r.weekdays.length > 0) {
    for (let i = 0; i < 7; i++) {
      const d = addDays(from, i);
      if (r.weekdays.includes(weekdayOf(d))) return d;
    }
  }
  if (r.freq === "monthly" && r.monthDay != null) {
    const inMonth = (offset: number) => {
      const last = new Date(from.getFullYear(), from.getMonth() + offset + 1, 0).getDate();
      return new Date(from.getFullYear(), from.getMonth() + offset, Math.min(r.monthDay!, last));
    };
    const day = new Date(from.getFullYear(), from.getMonth(), from.getDate());
    const here = inMonth(0);
    return here >= day ? here : inMonth(1);
  }
  return new Date(from.getFullYear(), from.getMonth(), from.getDate());
}

