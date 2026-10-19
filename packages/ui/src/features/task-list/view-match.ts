import type { DueDto, LocalDayDto, TaskStatusDto } from "@tasks/core-client";
import type { Route } from "../../lib/router.svelte";

/** Was die Liste von einer Task braucht (TaskDto und TaskSummaryDto passen beide). */
export interface TaskLike {
  id: string;
  listId: string;
  status: TaskStatusDto;
  due: DueDto | null;
  sortKey: string;
  completedAt: string | null;
}

export interface ViewContext {
  route: Route;
  inboxId: string | undefined;
  day: LocalDayDto;
}

/** Heute fällig oder überfällig (wie `Due::is_due_by` im Core). */
function dueBy(due: DueDto, day: LocalDayDto): boolean {
  return due.kind === "date" ? due.date <= day.date : new Date(due.at) < new Date(day.end);
}

/** Gehört die Task zur Ansicht – ohne Statusfilter (wie `TaskQuery::matches` im Core). */
export function inView(task: TaskLike, ctx: ViewContext): boolean {
  const { route } = ctx;
  switch (route.view) {
    case "inbox":
      return task.listId === ctx.inboxId;
    case "list":
      return task.listId === route.listId;
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

/** Erfüllt die Task auch den Statusfilter der Ansicht (offen, außer in „Erledigt“)? */
export function shownInView(task: TaskLike, ctx: ViewContext): boolean {
  return inView(task, ctx) && (ctx.route.view === "completed" || task.status === "open");
}

/** Heute/Geplant sind nach Fälligkeit sortiert (ADR-026). */
export function sortedByDue(route: Route): boolean {
  return route.view === "today" || route.view === "upcoming";
}

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
export function compareInView(a: TaskLike, b: TaskLike, route: Route): number {
  const cmp = (x: string, y: string) => (x < y ? -1 : x > y ? 1 : 0);
  if (route.view === "completed") return cmp(b.completedAt ?? "", a.completedAt ?? "") || cmp(a.id, b.id);
  if (sortedByDue(route)) {
    return dueInstant(a.due) - dueInstant(b.due) || cmp(a.sortKey, b.sortKey) || cmp(a.id, b.id);
  }
  return cmp(a.sortKey, b.sortKey) || cmp(a.id, b.id);
}
