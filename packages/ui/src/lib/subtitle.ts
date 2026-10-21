import { formatCount } from "./lists.svelte";
import type { Route } from "./router.svelte";

const TODAY_DATE = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long" });

/**
 * Unterzeile im Kopf einer Ansicht (P5-05): „Mittwoch, 21. Oktober · 3 offen · 1 überfällig“ in „Heute“,
 * „4 offen“ im Eingang. Andere Ansichten haben (noch) keine Zahl ohne eigenen Zählaufruf → leer.
 */
export function viewSubtitle(
  route: Route,
  counts: {
    inbox: number;
    today: number;
    overdue: number;
    trash?: number;
    all?: number;
    upcoming?: number;
    lists?: Record<string, number>;
    tags?: Record<string, number>;
  },
  now: Date = new Date(),
): string {
  const open = (n: number) => (n > 0 ? `${formatCount(n)} offen` : "Nichts offen");
  if (route.view === "today") {
    const parts = [TODAY_DATE.format(now), open(counts.today)];
    if (counts.overdue > 0) parts.push(`${formatCount(counts.overdue)} überfällig`);
    return parts.join(" · ");
  }
  if (route.view === "inbox") return open(counts.inbox);
  // Seit FIX-04 (`countTasks`) auch Listen, Tags, Alle und Geplant.
  if (route.view === "list" && counts.lists?.[route.listId] != null) return open(counts.lists[route.listId]);
  if (route.view === "tag" && counts.tags?.[route.tagId] != null) return open(counts.tags[route.tagId]);
  if (route.view === "all" && counts.all != null) return open(counts.all);
  if (route.view === "upcoming" && counts.upcoming != null) {
    return counts.upcoming > 0 ? `${formatCount(counts.upcoming)} geplant` : "Nichts geplant";
  }
  if (route.view === "trash") {
    const n = counts.trash ?? 0;
    return n > 0 ? `${formatCount(n)} ${n === 1 ? "Task" : "Tasks"} · nach 30 Tagen endgültig gelöscht` : "";
  }
  return "";
}
