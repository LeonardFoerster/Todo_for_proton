/* eslint-disable svelte/prefer-svelte-reactivity -- Sets/Date hier bewusst nicht reaktiv: reine Buchführung, die UI reagiert nur auf `items`. */
import { CoreClientError, type CoreClient, type TaskDto, type TaskSummaryDto } from "@tasks/core-client";
import { routeQuery } from "../../lib/views";
import type { RowOrder } from "./selection.svelte";
import { compareInView, inView, shownInView, sortedByDue, type ViewContext } from "./view-match";

const PAGE_SIZE = 200;
/** Wie im Core (`PAGE_LIMIT_MAX`). */
const PAGE_LIMIT_MAX = 500;
/** Ab so vielen IDs in einem Änderungs-Event die geladenen Seiten neu holen statt je ID `getTask`. */
const REFRESH_BATCH = 20;
/** So lange bleibt eine abgehakte Zeile stehen (Haken-Animation), bevor sie die offene Ansicht verlässt. */
export const LEAVE_MS = 250;

function leaveDelay(): number {
  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  return reduced ? 0 : LEAVE_MS;
}

export function toSummary(t: TaskDto): TaskSummaryDto {
  return {
    id: t.id,
    listId: t.listId,
    title: t.title,
    status: t.status,
    priority: t.priority,
    due: t.due,
    hasNotes: t.notes.length > 0,
    sortKey: t.sortKey,
    completedAt: t.completedAt,
    trashedAt: t.trashedAt,
    recurring: t.recurrence != null,
    checklistDone: t.checklist.filter((i) => i.done).length,
    checklistTotal: t.checklist.length,
    tags: [...t.tags],
    // Kennt der `TaskDto` nicht; `apply` behält den bisherigen Wert.
    hasLinks: false,
  };
}

/**
 * Zeilen einer Ansicht: seitenweise geladen, Änderungen gezielt per ID eingepflegt (kein
 * Neuladen der ganzen Liste). `items` ist `$state.raw` – ersetzt, nie tief verändert.
 */
export class TaskListState {
  items = $state.raw<TaskSummaryDto[]>([]);
  loaded = $state(false);

  /** Reaktiv: `hasMore` steuert, ob die Liste beim Scrollen nachlädt (FIX-12). */
  private cursor = $state<string | null>(null);
  private loadingMore = false;
  /**
   * Gerade abgehakt: bleiben für die Haken-Animation kurz stehen und verlassen dann die offene
   * Ansicht (P1-01). Wert = Timer, der die Zeile entfernt.
   */
  private leaving = new Map<string, ReturnType<typeof setTimeout>>();
  /** Gerade gelöscht (Rückgängig möglich): ausgeblendet, auch wenn ein Event sie zurückmeldet. */
  private hidden = new Set<string>();
  /** Index für `byId`/`indexOf`, gültig für genau dieses `items`-Array. */
  private index = new Map<string, number>();
  private indexed: TaskSummaryDto[] | null = null;
  private reloading: Promise<void> | null = null;
  private reloadQueued = false;

  constructor(
    private readonly client: CoreClient,
    private readonly ctx: () => ViewContext,
  ) {}

  get hasMore(): boolean {
    return this.cursor !== null;
  }

  async load(): Promise<void> {
    const page = await this.client.listTasks(routeQuery(this.ctx().route, undefined, this.ctx().options), { cursor: null, limit: PAGE_SIZE });
    this.items = this.ordered(page.items);
    this.cursor = page.nextCursor;
    this.loaded = true;
  }

  async loadMore(): Promise<void> {
    if (!this.cursor || this.loadingMore) return;
    this.loadingMore = true;
    try {
      const page = await this.client.listTasks(routeQuery(this.ctx().route, undefined, this.ctx().options), { cursor: this.cursor, limit: PAGE_SIZE });
      const known = new Set(this.items.map((t) => t.id));
      this.items = this.ordered([...this.items, ...page.items.filter((t) => !known.has(t.id))]);
      this.cursor = page.nextCursor;
    } finally {
      this.loadingMore = false;
    }
  }

  /**
   * Heute/Geplant: exakt nach lokaler Fälligkeit nachsortieren. Der Core nähert den Tagesbeginn
   * ohne Zonendaten an (ADR-026); an Tagen der Zeitumstellung kann seine Reihenfolge daher um
   * eine Stunde abweichen. In anderen Ansichten bleibt die Core-Reihenfolge unangetastet.
   */
  private ordered(items: TaskSummaryDto[]): TaskSummaryDto[] {
    const route = this.ctx().route;
    const sort = this.ctx().options?.sort;
    return sortedByDue(route, sort) ? items.sort((a, b) => compareInView(a, b, route, sort)) : items;
  }

  /**
   * Lädt alle restlichen Seiten (für „Alle auswählen", Seiten zu je `PAGE_LIMIT_MAX`) und liefert
   * die IDs der ganzen Ansicht in Anzeigereihenfolge.
   */
  async loadAll(): Promise<string[]> {
    while (this.loadingMore) await new Promise((r) => setTimeout(r, 10));
    this.loadingMore = true;
    try {
      while (this.cursor) {
        const query = routeQuery(this.ctx().route, undefined, this.ctx().options);
        const page = await this.client.listTasks(query, { cursor: this.cursor, limit: PAGE_LIMIT_MAX });
        const known = new Set(this.items.map((t) => t.id));
        this.items = this.ordered([...this.items, ...page.items.filter((t) => !known.has(t.id))]);
        this.cursor = page.nextCursor;
      }
    } finally {
      this.loadingMore = false;
    }
    return this.items.map((t) => t.id);
  }

  /** Zeile per ID (O(1) über einen bei Änderung von `items` neu gebauten Index). */
  byId(id: string): TaskSummaryDto | undefined {
    const i = this.indexOf(id);
    return i < 0 ? undefined : this.items[i];
  }

  /** Position einer Zeile (O(1), Index je `items`-Array); -1 = nicht geladen. */
  indexOf(id: string): number {
    if (this.indexed !== this.items) {
      this.index = new Map(this.items.map((t, i) => [t.id, i]));
      this.indexed = this.items;
    }
    return this.index.get(id) ?? -1;
  }

  /** Reihenfolge der geladenen Zeilen für Bereiche der Auswahl (ohne ID-Array je Taste). */
  order(): RowOrder {
    const items = this.items;
    return { length: items.length, at: (i) => items[i]?.id, indexOf: (id) => this.indexOf(id) };
  }

  /** Abo auf Core-Änderungen; gibt die Abmeldung zurück. */
  attach(): () => void {
    return this.client.onChange((e) => {
      // Verknüpfungen (ADR-036) ändern nur `hasLinks` – das weiß nur die Zusammenfassung des Cores.
      if (e.tasks.length > REFRESH_BATCH || e.links.length > 0) void this.reload();
      else for (const id of e.tasks) void this.refresh(id);
    });
  }

  /**
   * Holt die bisher geladenen Zeilen neu (Sammelaktion, großer Sync): wenige `listTasks` statt n
   * `getTask`. Gedimmt stehende Erledigte bleiben an ihrer Stelle, Ausgeblendete draußen. Aufrufe
   * während eines Neuladens werden zu einem weiteren Durchgang zusammengefasst.
   */
  reload(): Promise<void> {
    if (this.reloading) {
      this.reloadQueued = true;
      return this.reloading;
    }
    this.reloading = (async () => {
      try {
        do {
          this.reloadQueued = false;
          await this.fetchLoaded();
        } while (this.reloadQueued);
      } finally {
        this.reloading = null;
      }
    })();
    return this.reloading;
  }

  private async fetchLoaded(): Promise<void> {
    const want = Math.max(this.items.length, PAGE_SIZE);
    const query = routeQuery(this.ctx().route, undefined, this.ctx().options);
    const fetched: TaskSummaryDto[] = [];
    let cursor: string | null = null;
    do {
      const page = await this.client.listTasks(query, { cursor, limit: PAGE_LIMIT_MAX });
      fetched.push(...page.items);
      cursor = page.nextCursor;
    } while (cursor && fetched.length < want);
    const ctx = this.ctx();
    const fresh = new Set(fetched.map((t) => t.id));
    const linger = this.items.filter((t) => this.leaving.has(t.id) && !fresh.has(t.id) && inView(t, ctx));
    let items = fetched.filter((t) => !this.hidden.has(t.id));
    if (linger.length > 0) items = [...items, ...linger].sort((a, b) => compareInView(a, b, ctx.route, ctx.options?.sort));
    this.items = this.ordered(items);
    this.cursor = cursor;
    this.loaded = true;
  }

  /** Lädt eine Task neu und pflegt sie ein (oder aus). */
  async refresh(id: string): Promise<void> {
    try {
      this.apply(toSummary(await this.client.getTask(id)));
    } catch (e) {
      if (CoreClientError.from(e).kind === "notFound") this.remove(id);
    }
  }

  apply(next: TaskSummaryDto): void {
    const ctx = this.ctx();
    if (this.hidden.has(next.id)) return;
    // Aus einem `TaskDto` (ohne Verknüpfungen): `hasLinks` der bisherigen Zeile behalten.
    const prev = this.items.find((t) => t.id === next.id);
    const task = prev && !next.hasLinks ? { ...next, hasLinks: prev.hasLinks } : next;
    const visible = shownInView(task, ctx) || (this.leaving.has(task.id) && inView(task, ctx));
    const rest = this.items.filter((t) => t.id !== task.id);
    if (!visible) {
      if (rest.length !== this.items.length) this.items = rest;
      return;
    }
    let at = rest.findIndex((t) => compareInView(task, t, ctx.route, ctx.options?.sort) < 0);
    if (at < 0) {
      // Hinter dem letzten geladenen Eintrag: nur einfügen, wenn alles geladen ist.
      if (this.hasMore && !this.items.some((t) => t.id === task.id)) return;
      at = rest.length;
    }
    this.items = [...rest.slice(0, at), task, ...rest.slice(at)];
  }

  remove(id: string): void {
    if (this.items.some((t) => t.id === id)) this.items = this.items.filter((t) => t.id !== id);
  }

  /**
   * Optimistisch abhaken/öffnen; abgehakte verlassen offene Ansichten nach `LEAVE_MS`. Eine Serie
   * (ADR-028) bleibt offen und stehen – ihre neue Fälligkeit kommt mit dem Änderungs-Event.
   */
  setDone(id: string, done: boolean): TaskSummaryDto | undefined {
    const task = this.items.find((t) => t.id === id);
    if (!task || (done && task.recurring)) return task;
    if (done) this.leave([id]);
    const next: TaskSummaryDto = {
      ...task,
      status: done ? "done" : "open",
      completedAt: done ? new Date().toISOString() : null,
    };
    this.items = this.items.map((t) => (t.id === id ? next : t));
    return task;
  }

  /** Optimistisch abhaken/öffnen (Sammelaktion, ein neues Array); liefert die Zeilen davor. */
  setDoneMany(ids: readonly string[], done: boolean): TaskSummaryDto[] {
    const set = new Set(ids);
    const before: TaskSummaryDto[] = [];
    const at = done ? new Date().toISOString() : null;
    this.items = this.items.map((t) => {
      if (!set.has(t.id) || t.status === (done ? "done" : "open") || (done && t.recurring)) return t;
      before.push(t);
      return { ...t, status: done ? "done" : "open", completedAt: at };
    });
    if (done) this.leave(before.map((t) => t.id));
    return before;
  }

  /**
   * Abgehakte Zeilen nach der Animation aus der offenen Ansicht nehmen – nur, wenn sie dann noch
   * erledigt sind (Rückgängig in der Zwischenzeit lässt sie stehen). „Erledigt“ zeigt sie weiter.
   */
  private leave(ids: readonly string[]): void {
    if (this.ctx().route.view === "completed" || ids.length === 0) return;
    this.cancelLeave(ids);
    const timer = setTimeout(() => {
      const gone = new Set(ids.filter((id) => this.leaving.get(id) === timer));
      for (const id of gone) this.leaving.delete(id);
      const ctx = this.ctx();
      const next = this.items.filter((t) => !gone.has(t.id) || shownInView(t, ctx));
      if (next.length !== this.items.length) this.items = next;
    }, leaveDelay());
    for (const id of ids) this.leaving.set(id, timer);
  }

  /** Verlassen abbrechen (Abhaken fehlgeschlagen). */
  cancelLeave(ids: readonly string[]): void {
    for (const id of ids) {
      const timer = this.leaving.get(id);
      if (timer === undefined) continue;
      this.leaving.delete(id);
      if (![...this.leaving.values()].includes(timer)) clearTimeout(timer);
    }
  }

  /** Offene Timer beenden (Ansicht wird verlassen). */
  dispose(): void {
    for (const timer of new Set(this.leaving.values())) clearTimeout(timer);
    this.leaving.clear();
  }

  /** Zeilen auf einen früheren Stand zurücksetzen (fehlgeschlagene Aktion). */
  replaceMany(tasks: readonly TaskSummaryDto[]): void {
    this.cancelLeave(tasks.map((t) => t.id));
    const byId = new Map(tasks.map((t) => [t.id, t]));
    this.items = this.items.map((t) => byId.get(t.id) ?? t);
  }

  /** Aus der Ansicht nehmen (ohne Merken – ein späteres Event kann sie zurückbringen). */
  removeMany(ids: readonly string[]): void {
    const set = new Set(ids);
    this.items = this.items.filter((t) => !set.has(t.id));
  }

  /** Ausblenden (Löschen mit Rückgängig): bleiben draußen, auch wenn ein Event sie meldet. */
  hide(ids: readonly string[]): void {
    for (const id of ids) this.hidden.add(id);
    this.removeMany(ids);
  }

  /**
   * Ausblenden beenden, ohne neu zu laden: Die Änderung ist gespeichert und hält die Zeilen selbst
   * draußen (Papierkorb, ADR-031) – „Rückgängig“ (gleiche IDs) bringt sie dann per Event zurück.
   */
  release(ids: readonly string[]): void {
    for (const id of ids) this.hidden.delete(id);
  }

  /** Wieder einblenden (Löschen fehlgeschlagen). */
  async unhide(ids: readonly string[]): Promise<void> {
    for (const id of ids) this.hidden.delete(id);
    await this.reload();
  }

  /** Optimistisch an Einfügeposition `to` (Index vor dem Entfernen) schieben; liefert die Nachbarn. */
  reorder(id: string, to: number): { after?: string; before?: string } | null {
    const from = this.items.findIndex((t) => t.id === id);
    if (from < 0) return null;
    const rest = this.items.filter((t) => t.id !== id);
    const at = Math.max(0, Math.min(rest.length, to > from ? to - 1 : to));
    if (at === from) return null;
    this.items = [...rest.slice(0, at), this.items[from], ...rest.slice(at)];
    return { after: rest[at - 1]?.id, before: rest[at]?.id };
  }

  /**
   * Optimistisch den Block `ids` (in Anzeigereihenfolge, auch verstreut) an Einfügeposition `to`
   * (Index vor dem Entfernen) schieben – verstreute Zeilen werden dabei zusammengezogen. Liefert
   * die Nachbarn für `moveTasks` bzw. `null`, wenn sich nichts ändert.
   */
  reorderMany(ids: ReadonlySet<string>, to: number): { after?: string; before?: string } | null {
    const block: TaskSummaryDto[] = [];
    const rest: TaskSummaryDto[] = [];
    let at = 0;
    this.items.forEach((t, i) => {
      if (ids.has(t.id)) block.push(t);
      else {
        if (i < to) at++;
        rest.push(t);
      }
    });
    if (block.length === 0) return null;
    const next = [...rest.slice(0, at), ...block, ...rest.slice(at)];
    if (next.every((t, i) => t === this.items[i])) return null;
    this.items = next;
    return { after: rest[at - 1]?.id, before: rest[at]?.id };
  }

  /**
   * Optimistisch eine Task (aus einer anderen Liste) an Position `index` einfügen; liefert die
   * Nachbarn für `moveTask`.
   */
  insertAt(task: TaskSummaryDto, index: number): { after?: string; before?: string } {
    const rest = this.items.filter((t) => t.id !== task.id);
    const at = Math.max(0, Math.min(rest.length, index));
    this.items = [...rest.slice(0, at), task, ...rest.slice(at)];
    return { after: rest[at - 1]?.id, before: rest[at]?.id };
  }

  replace(task: TaskSummaryDto): void {
    this.cancelLeave([task.id]);
    this.items = this.items.map((t) => (t.id === task.id ? task : t));
  }
}
