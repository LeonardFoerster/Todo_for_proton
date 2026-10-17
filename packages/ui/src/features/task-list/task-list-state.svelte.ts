/* eslint-disable svelte/prefer-svelte-reactivity -- Sets/Date hier bewusst nicht reaktiv: reine Buchführung, die UI reagiert nur auf `items`. */
import { CoreClientError, type CoreClient, type TaskDto, type TaskSummaryDto } from "@tasks/core-client";
import { routeQuery } from "../../lib/views";
import { compareInView, inView, shownInView, type ViewContext } from "./view-match";

const PAGE_SIZE = 200;

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
  };
}

/**
 * Zeilen einer Ansicht: seitenweise geladen, Änderungen gezielt per ID eingepflegt (kein
 * Neuladen der ganzen Liste). `items` ist `$state.raw` – ersetzt, nie tief verändert.
 */
export class TaskListState {
  items = $state.raw<TaskSummaryDto[]>([]);
  loaded = $state(false);

  private cursor: string | null = null;
  private loadingMore = false;
  /** In dieser Ansicht abgehakt: bleiben gedimmt stehen (Rückgängig per Klick), bis zum Wechsel. */
  private lingering = new Set<string>();
  /** Gerade gelöscht (Rückgängig möglich): ausgeblendet, auch wenn ein Event sie zurückmeldet. */
  private hidden = new Set<string>();

  constructor(
    private readonly client: CoreClient,
    private readonly ctx: () => ViewContext,
  ) {}

  get hasMore(): boolean {
    return this.cursor !== null;
  }

  async load(): Promise<void> {
    const page = await this.client.listTasks(routeQuery(this.ctx().route), { cursor: null, limit: PAGE_SIZE });
    this.items = page.items;
    this.cursor = page.nextCursor;
    this.loaded = true;
  }

  async loadMore(): Promise<void> {
    if (!this.cursor || this.loadingMore) return;
    this.loadingMore = true;
    try {
      const page = await this.client.listTasks(routeQuery(this.ctx().route), { cursor: this.cursor, limit: PAGE_SIZE });
      const known = new Set(this.items.map((t) => t.id));
      this.items = [...this.items, ...page.items.filter((t) => !known.has(t.id))];
      this.cursor = page.nextCursor;
    } finally {
      this.loadingMore = false;
    }
  }

  /** Abo auf Core-Änderungen; gibt die Abmeldung zurück. */
  attach(): () => void {
    return this.client.onChange((e) => {
      for (const id of e.tasks) void this.refresh(id);
    });
  }

  /** Lädt eine Task neu und pflegt sie ein (oder aus). */
  async refresh(id: string): Promise<void> {
    try {
      this.apply(toSummary(await this.client.getTask(id)));
    } catch (e) {
      if (CoreClientError.from(e).kind === "notFound") this.remove(id);
    }
  }

  apply(task: TaskSummaryDto): void {
    const ctx = this.ctx();
    if (this.hidden.has(task.id)) return;
    const visible = shownInView(task, ctx) || (this.lingering.has(task.id) && inView(task, ctx));
    const rest = this.items.filter((t) => t.id !== task.id);
    if (!visible) {
      if (rest.length !== this.items.length) this.items = rest;
      return;
    }
    let at = rest.findIndex((t) => compareInView(task, t, ctx.route) < 0);
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

  /** Optimistisch abhaken/öffnen; erledigte bleiben bis zum Ansichtswechsel stehen. */
  setDone(id: string, done: boolean): TaskSummaryDto | undefined {
    const task = this.items.find((t) => t.id === id);
    if (!task) return undefined;
    if (done) this.lingering.add(id);
    const next: TaskSummaryDto = {
      ...task,
      status: done ? "done" : "open",
      completedAt: done ? new Date().toISOString() : null,
    };
    this.items = this.items.map((t) => (t.id === id ? next : t));
    return task;
  }

  /** Ausblenden (z. B. Löschen mit Rückgängig) bzw. wieder einblenden. */
  hide(id: string): void {
    this.hidden.add(id);
    this.remove(id);
  }

  unhide(id: string): void {
    this.hidden.delete(id);
    void this.refresh(id);
  }

  replace(task: TaskSummaryDto): void {
    this.items = this.items.map((t) => (t.id === task.id ? task : t));
  }
}
