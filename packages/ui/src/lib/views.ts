import type { TaskListDto, TaskQueryDto } from "@tasks/core-client";
import { CalendarDays, CircleCheck, Hash, Inbox, List, Sun, type IconComponent } from "../icons";
import { localDay } from "./local-day";
import type { Route, SmartViewKind } from "./router.svelte";

export interface SmartViewMeta {
  kind: SmartViewKind;
  label: string;
  icon: IconComponent;
  /** Leere Ansicht: was hier hingehört. */
  empty: string;
}

export const SMART_VIEWS: readonly SmartViewMeta[] = [
  { kind: "inbox", label: "Eingang", icon: Inbox, empty: "Alles, was keiner Liste zugeordnet ist, landet hier." },
  { kind: "today", label: "Heute", icon: Sun, empty: "Nichts fällig heute. Neue Tasks mit „heute“ landen hier." },
  { kind: "upcoming", label: "Geplant", icon: CalendarDays, empty: "Keine Tasks mit Fälligkeit in der Zukunft." },
  { kind: "all", label: "Alle Tasks", icon: List, empty: "Noch keine Tasks. Leg mit N die erste an." },
  { kind: "completed", label: "Erledigt", icon: CircleCheck, empty: "Erledigte Tasks erscheinen hier." },
];

/** Anzeigename einer Liste (die Inbox heißt im Core „Inbox“, in der UI „Eingang“). */
export function listName(list: TaskListDto): string {
  return list.isInbox ? "Eingang" : list.name;
}

export function routeTitle(route: Route, lists: readonly TaskListDto[]): string {
  if (route.view === "list") {
    const list = lists.find((l) => l.id === route.listId);
    return list ? listName(list) : "Liste";
  }
  return SMART_VIEWS.find((v) => v.kind === route.view)!.label;
}

export function routeIcon(route: Route): IconComponent {
  return route.view === "list" ? Hash : SMART_VIEWS.find((v) => v.kind === route.view)!.icon;
}

/** Abfrage der offenen Tasks einer Ansicht (Erledigt: die erledigten). */
export function routeQuery(route: Route, now: Date = new Date()): TaskQueryDto {
  switch (route.view) {
    case "list":
      return { view: { kind: "list", listId: route.listId }, status: "open" };
    case "today":
      return { view: { kind: "today", day: localDay(now) }, status: "open" };
    case "upcoming":
      return { view: { kind: "upcoming", day: localDay(now) }, status: "open" };
    case "completed":
      return { view: { kind: "completed" } };
    default:
      return { view: { kind: route.view }, status: "open" };
  }
}

/** Zielliste für neue Tasks in dieser Ansicht (`undefined` = Inbox). */
export function routeListId(route: Route): string | undefined {
  return route.view === "list" ? route.listId : undefined;
}
