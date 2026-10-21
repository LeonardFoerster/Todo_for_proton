// Wiederholung im Fake-Client (ADR-028), vereinfachtes Abbild von `todo_core::domain::Recurrence`:
// gleiche Regeln für Tage/Wochen/Monate/Jahre, Ende, „nach Erledigung“; Uhrzeiten ohne Zeitzonen.
import type { DueDto, RecurrenceDto, WeekdayDto } from "./types";

const DAY_MS = 86_400_000;
const WEEKDAYS: WeekdayDto[] = ["mo", "tu", "we", "th", "fr", "sa", "su"];

const ms = (date: string) => Date.parse(`${date}T00:00:00Z`);
const iso = (t: number) => new Date(t).toISOString().slice(0, 10);
const addDays = (date: string, n: number) => iso(ms(date) + n * DAY_MS);
const weekday = (date: string): WeekdayDto => WEEKDAYS[(new Date(ms(date)).getUTCDay() + 6) % 7];
const monday = (date: string) => addDays(date, -WEEKDAYS.indexOf(weekday(date)));

/** Tag `day` (31 = letzter) im Monat `date` + `months`. */
function inMonth(date: string, months: number, day: number): string {
  const d = new Date(ms(date));
  const first = Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + months, 1);
  const last = new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + months + 1, 0)).getUTCDate();
  return iso(first + (Math.min(day, last) - 1) * DAY_MS);
}

const within = (rule: RecurrenceDto, day: string | null) => (day && rule.until && day > rule.until ? null : day);

/** Regel an der Fälligkeit verankern (Wochentag bzw. Monatstag). */
export function anchored(rule: RecurrenceDto, due: string): RecurrenceDto {
  if (rule.freq === "weekly" && rule.weekdays.length === 0) return { ...rule, weekdays: [weekday(due)] };
  if ((rule.freq === "monthly" || rule.freq === "yearly") && rule.monthDay == null) {
    return { ...rule, monthDay: Number(due.slice(8)) };
  }
  return { ...rule };
}

/** Erstes Vorkommen am oder nach `start`. */
export function firstOnOrAfter(rule: RecurrenceDto, start: string): string | null {
  let day = start;
  if (rule.freq === "weekly" && rule.weekdays.length > 0) {
    for (let i = 0; i < 7 && !rule.weekdays.includes(weekday(day)); i++) day = addDays(start, i + 1);
  } else if (rule.freq === "monthly" && rule.monthDay != null) {
    day = inMonth(start, 0, rule.monthDay);
    if (day < start) day = inMonth(start, 1, rule.monthDay);
  }
  return within(rule, day);
}

/** Nächstes Vorkommen beim Abhaken am `today` (`null` = Serie endet). */
export function nextOccurrence(rule: RecurrenceDto, due: string, today: string): string | null {
  const n = rule.interval;
  if (rule.anchor === "completion") {
    const step = { daily: () => addDays(today, n), weekly: () => addDays(today, 7 * n) };
    const months = rule.freq === "monthly" ? n : 12 * n;
    const next =
      rule.freq === "daily" || rule.freq === "weekly"
        ? step[rule.freq]()
        : inMonth(today, months, Number(today.slice(8)));
    return within(rule, next);
  }
  if (rule.freq === "daily") {
    let d = addDays(due, n);
    if (d < today) d = addDays(d, Math.ceil((ms(today) - ms(d)) / DAY_MS / n) * n);
    return within(rule, d);
  }
  if (rule.freq === "weekly") {
    const days = rule.weekdays.length > 0 ? rule.weekdays : [weekday(due)];
    const base = monday(due);
    let week = 0;
    const target = monday(today);
    if (target > base) week = Math.floor((ms(target) - ms(base)) / DAY_MS / (7 * n)) * 7 * n;
    for (let p = 0; p < 3; p++, week += 7 * n) {
      for (const wd of WEEKDAYS.filter((w) => days.includes(w))) {
        const d = addDays(base, week + WEEKDAYS.indexOf(wd));
        if (d > due && d >= today) return within(rule, d);
      }
    }
    return null;
  }
  const step = rule.freq === "monthly" ? n : 12 * n;
  const day = rule.monthDay ?? Number(due.slice(8));
  for (let k = step; k < 12_000; k += step) {
    const d = inMonth(due, k, day);
    if (d >= today) return within(rule, d);
  }
  return null;
}

/** Lokaler Tag einer Fälligkeit (Fake: UTC). */
export const dueDay = (due: DueDto) => (due.kind === "date" ? due.date : due.at.slice(0, 10));

/** Fälligkeit am Tag `day` mit der Uhrzeit von `like`. */
export function dueOn(day: string, like: DueDto): DueDto {
  if (like.kind === "date") return { kind: "date", date: day };
  const shift = ms(day) - ms(dueDay(like));
  return { ...like, at: new Date(Date.parse(like.at) + shift).toISOString() };
}
