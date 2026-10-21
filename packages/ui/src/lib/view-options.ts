// Sortieren und Filtern je Ansicht (N10-01, ADR-037): gemerkt pro Gerät (localStorage), angewandt im
// Core (`TaskQueryDto.sort/priority/tag`). Hier nur Typen, Speicher und Klartext.
import type { PriorityDto, TaskSortDto } from "@tasks/core-client";
import { routeHash, type Route } from "./router.svelte";

export interface ViewOptions {
  sort: TaskSortDto;
  /** Nur diese Priorität; `null` = alle. */
  priority: PriorityDto | null;
  /** Nur dieser Tag (nicht in der Tag-Ansicht selbst); `null` = alle. */
  tag: string | null;
}

export const DEFAULT_OPTIONS: ViewOptions = { sort: "view", priority: null, tag: null };
const KEY = "tasks.view-options";

export const SORT_LABELS: Record<TaskSortDto, string> = {
  view: "Eigene Reihenfolge",
  due: "Fälligkeit",
  priority: "Priorität",
  title: "Titel",
};
export const PRIORITY_LABELS: Record<PriorityDto, string> = { high: "Hoch", medium: "Mittel", low: "Niedrig", none: "Keine" };

/** In Heute/Geplant heißt „wie die Ansicht“ Fälligkeit. */
export function sortLabel(sort: TaskSortDto, route: Route): string {
  if (sort === "view" && (route.view === "today" || route.view === "upcoming")) return "Fälligkeit";
  return SORT_LABELS[sort];
}

/** Gilt eine Sortierung bzw. ein Filter in dieser Ansicht? (Erledigt/Papierkorb: nur Filter.) */
export function sortable(route: Route): boolean {
  return route.view !== "completed" && route.view !== "trash";
}

function readAll(): Record<string, ViewOptions> {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}") as Record<string, ViewOptions>;
  } catch {
    return {};
  }
}

export function readOptions(route: Route): ViewOptions {
  const stored = readAll()[routeHash(route)];
  return stored ? { ...DEFAULT_OPTIONS, ...stored } : { ...DEFAULT_OPTIONS };
}

export function writeOptions(route: Route, options: ViewOptions): void {
  try {
    const all = readAll();
    const hash = routeHash(route);
    if (isDefault(options)) delete all[hash];
    else all[hash] = options;
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    // Ohne Speicher gilt es nur bis zum Neustart.
  }
}

export function isDefault(o: ViewOptions): boolean {
  return o.sort === "view" && o.priority === null && o.tag === null;
}

/** Ein stabiler Schlüssel (für `{#key}`): andere Optionen = neue Liste. */
export function optionsKey(o: ViewOptions): string {
  return `${o.sort}|${o.priority ?? ""}|${o.tag ?? ""}`;
}
