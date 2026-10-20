import { formatCount } from "./lists.svelte";
import type { Route } from "./router.svelte";

const TODAY_DATE = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long" });

/**
 * Unterzeile im Kopf einer Ansicht (P5-05): „Mittwoch, 21. Oktober · 3 offen · 1 überfällig“ in „Heute“,
 * „4 offen“ im Eingang. Andere Ansichten haben (noch) keine Zahl ohne eigenen Zählaufruf → leer.
 */
export function viewSubtitle(
  route: Route,
  counts: { inbox: number; today: number; overdue: number },
  now: Date = new Date(),
): string {
  const open = (n: number) => (n > 0 ? `${formatCount(n)} offen` : "Nichts offen");
  if (route.view === "today") {
    const parts = [TODAY_DATE.format(now), open(counts.today)];
    if (counts.overdue > 0) parts.push(`${formatCount(counts.overdue)} überfällig`);
    return parts.join(" · ");
  }
  if (route.view === "inbox") return open(counts.inbox);
  return "";
}
