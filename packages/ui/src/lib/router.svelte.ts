/** Mini-Router über den URL-Hash (`#/today`, `#/list/<id>`): Zurück/Vor funktioniert, kein SvelteKit. */

export type SmartViewKind = "inbox" | "today" | "upcoming" | "all" | "completed" | "trash";
export type Route = { view: SmartViewKind } | { view: "list"; listId: string } | { view: "tag"; tagId: string };

const SMART: readonly SmartViewKind[] = ["inbox", "today", "upcoming", "all", "completed", "trash"];
export const DEFAULT_ROUTE: Route = { view: "inbox" };

export function parseHash(hash: string): Route {
  const parts = hash.replace(/^#\/?/, "").split("/").filter(Boolean);
  if ((parts[0] === "list" || parts[0] === "tag") && parts[1]) {
    // Kaputte %-Sequenzen (von Hand eingetippt) führen zur Startansicht statt zu einem Fehler.
    try {
      const id = decodeURIComponent(parts[1]);
      return parts[0] === "list" ? { view: "list", listId: id } : { view: "tag", tagId: id };
    } catch {
      return DEFAULT_ROUTE;
    }
  }
  const view = parts[0] as SmartViewKind;
  return SMART.includes(view) ? { view } : DEFAULT_ROUTE;
}

export function routeHash(route: Route): string {
  if (route.view === "list") return `#/list/${encodeURIComponent(route.listId)}`;
  if (route.view === "tag") return `#/tag/${encodeURIComponent(route.tagId)}`;
  return `#/${route.view}`;
}

export function sameRoute(a: Route, b: Route): boolean {
  return routeHash(a) === routeHash(b);
}

export class Router {
  route = $state<Route>(DEFAULT_ROUTE);

  constructor() {
    if (typeof location !== "undefined") this.route = parseHash(location.hash);
  }

  /** Startet das Lauschen auf Hash-Änderungen; gibt die Abmeldung zurück. */
  listen(): () => void {
    const update = () => (this.route = parseHash(location.hash));
    addEventListener("hashchange", update);
    return () => removeEventListener("hashchange", update);
  }

  navigate(route: Route, { replace = false } = {}): void {
    this.route = route;
    const hash = routeHash(route);
    if (location.hash === hash) return;
    if (replace) history.replaceState(null, "", hash);
    else location.hash = hash;
  }
}
