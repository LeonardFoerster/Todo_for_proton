import type { DueDto, LocalDayDto, PriorityDto, TaskSortDto, TaskStatusDto } from "@tasks/core-client";
import type { Route } from "../../lib/router.svelte";
import type { ViewOptions } from "../../lib/view-options";

/** Was die Liste von einer Task braucht (TaskDto und TaskSummaryDto passen beide). */
export interface TaskLike {
  id: string;
  listId: string;
  status: TaskStatusDto;
  due: DueDto | null;
  sortKey: string;
  completedAt: string | null;
  trashedAt: string | null;
  tags: string[];
  title: string;
  priority: PriorityDto;
}

export interface ViewContext {
  route: Route;
  inboxId: string | undefined;
  day: LocalDayDto;
  /** Sortierung/Filter der Ansicht (N10-01); fehlt = wie die Ansicht. */
  options?: ViewOptions;
}

/** Heute fällig oder überfällig (wie `Due::is_due_by` im Core). */
function dueBy(due: DueDto, day: LocalDayDto): boolean {
  return due.kind === "date" ? due.date <= day.date : new Date(due.at) < new Date(day.end);
}

/** Gehört die Task zur Ansicht – ohne Statusfilter (wie `TaskQuery::matches` im Core). */
export function inView(task: TaskLike, ctx: ViewContext): boolean {
  const { route, options } = ctx;
  if (options?.priority && task.priority !== options.priority) return false;
  if (options?.tag && route.view !== "tag" && !task.tags.includes(options.tag)) return false;
  // Papierkorb nur in seiner Ansicht (ADR-031).
  if ((task.trashedAt != null) !== (route.view === "trash")) return false;
  switch (route.view) {
    case "inbox":
      return task.listId === ctx.inboxId;
    case "list":
      return task.listId === route.listId;
    case "tag":
      return task.tags.includes(route.tagId);
    case "today":
      return task.due != null && dueBy(task.due, ctx.day);
    case "upcoming":
      return task.due != null && !dueBy(task.due, ctx.day);
    case "completed":
      return task.status === "done";
    default:
      return true;
  }
}

/** Erfüllt die Task auch den Statusfilter der Ansicht (offen, außer in „Erledigt“ und im Papierkorb)? */
export function shownInView(task: TaskLike, ctx: ViewContext): boolean {
  const anyStatus = ctx.route.view === "completed" || ctx.route.view === "trash";
  return inView(task, ctx) && (anyStatus || task.status === "open");
}

/** Heute/Geplant sind nach Fälligkeit sortiert (ADR-026) – außer bei Sortierung nach Priorität/Titel. */
export function sortedByDue(route: Route, sort: TaskSortDto = "view"): boolean {
  return (route.view === "today" || route.view === "upcoming") && (sort === "view" || sort === "due");
}

const PRIORITY_RANK: Record<PriorityDto, number> = { none: 0, low: 1, medium: 2, high: 3 };

/** Wie `Due::rough_instant` im Core: Zeitpunkt bzw. Mitternacht UTC; ohne Fälligkeit zuletzt. */
function roughInstant(due: DueDto | null): number {
  if (!due) return Number.MAX_SAFE_INTEGER;
  if (due.kind === "dateTime") return Date.parse(due.at);
  const [y, m, d] = due.date.split("-").map(Number);
  return Date.UTC(y, m - 1, d);
}

/** Klein nur bei ASCII – wie SQLite `NOCASE` bzw. `to_ascii_lowercase` im Core. */
const asciiLower = (s: string) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());

/**
 * Zeitpunkt einer Fälligkeit für die Sortierung: Uhrzeit bzw. lokaler Tagesbeginn (ganztägig vor
 * Uhrzeiten desselben Tages). Exakt in der lokalen Zone – der Core nähert den Tagesbeginn an.
 */
export function dueInstant(due: DueDto | null): number {
  if (!due) return Number.MAX_SAFE_INTEGER;
  if (due.kind === "dateTime") return Date.parse(due.at);
  const [y, m, d] = due.date.split("-").map(Number);
  return new Date(y, m - 1, d).getTime();
}

/**
 * Reihenfolge wie im Core: Erledigt nach `completedAt` absteigend, Heute/Geplant nach Fälligkeit,
 * sonst `sortKey`; Tiebreak ID.
 */
export function compareInView(a: TaskLike, b: TaskLike, route: Route, sort: TaskSortDto = "view"): number {
  const cmp = (x: string, y: string) => (x < y ? -1 : x > y ? 1 : 0);
  if (route.view === "completed") return cmp(b.completedAt ?? "", a.completedAt ?? "") || cmp(a.id, b.id);
  if (route.view === "trash") return cmp(b.trashedAt ?? "", a.trashedAt ?? "") || cmp(a.id, b.id);
  if (sortedByDue(route, sort)) {
    return dueInstant(a.due) - dueInstant(b.due) || cmp(a.sortKey, b.sortKey) || cmp(a.id, b.id);
  }
  if (sort === "due") return roughInstant(a.due) - roughInstant(b.due) || cmp(a.sortKey, b.sortKey) || cmp(a.id, b.id);
  if (sort === "priority") {
    return PRIORITY_RANK[b.priority] - PRIORITY_RANK[a.priority] || cmp(a.sortKey, b.sortKey) || cmp(a.id, b.id);
  }
  if (sort === "title") return cmp(asciiLower(a.title), asciiLower(b.title)) || cmp(a.id, b.id);
  return cmp(a.sortKey, b.sortKey) || cmp(a.id, b.id);
}
