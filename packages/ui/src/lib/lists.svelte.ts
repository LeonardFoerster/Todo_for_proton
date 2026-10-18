import type { CoreClient, MoveListTargetDto, TaskListDto, TaskQueryDto } from "@tasks/core-client";
import { routeQuery } from "./views";

/** So viele offene Tasks zählt die Sidebar höchstens (darüber „999+“). */
const COUNT_CAP = 999;
const PAGE = 500;

/**
 * Listen + Zähler der Sidebar. Lädt Listen neu, wenn `onChange` Listen meldet, und zählt
 * (entprellt) Eingang/Heute neu, wenn Tasks sich ändern.
 */
export class ListsState {
  lists = $state.raw<TaskListDto[]>([]);
  counts = $state<{ inbox: number; today: number }>({ inbox: 0, today: 0 });
  loaded = $state(false);

  private recount: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly client: CoreClient) {}

  /** Startet Laden + Abo; gibt die Abmeldung zurück. */
  start(): () => void {
    void this.reloadLists();
    void this.refreshCounts();
    const off = this.client.onChange((e) => {
      if (e.lists.length) void this.reloadLists();
      if (e.tasks.length || e.lists.length) this.scheduleCounts();
    });
    return () => {
      off();
      if (this.recount) clearTimeout(this.recount);
    };
  }

  get userLists(): TaskListDto[] {
    return this.lists.filter((l) => !l.isInbox);
  }

  get inbox(): TaskListDto | undefined {
    return this.lists.find((l) => l.isInbox);
  }

  async reloadLists() {
    this.lists = await this.client.listTaskLists();
    this.loaded = true;
  }

  /** Umbenennen (optimistisch; bei Fehler neu laden und weiterwerfen). */
  async rename(id: string, name: string): Promise<void> {
    const trimmed = name.trim();
    this.lists = this.lists.map((l) => (l.id === id ? { ...l, name: trimmed } : l));
    try {
      const saved = await this.client.renameTaskList(id, trimmed);
      this.lists = this.lists.map((l) => (l.id === id ? saved : l));
    } catch (e) {
      await this.reloadLists();
      throw e;
    }
  }

  /** Löschen (Tasks wandern im Core in den Eingang); liefert deren Anzahl. */
  async remove(id: string): Promise<number> {
    const moved = await this.client.deleteTaskList(id);
    this.lists = this.lists.filter((l) => l.id !== id);
    return moved;
  }

  /**
   * Benutzerliste optimistisch an Einfügeposition `to` (Index unter den Benutzerlisten vor dem
   * Entfernen) schieben und speichern. Eingang bleibt vorn. Bei Fehler neu laden und weiterwerfen.
   */
  async move(id: string, to: number): Promise<void> {
    const user = this.userLists;
    const from = user.findIndex((l) => l.id === id);
    if (from < 0) return;
    const rest = user.filter((l) => l.id !== id);
    const at = Math.max(0, Math.min(rest.length, to > from ? to - 1 : to));
    if (at === from) return;
    const target: MoveListTargetDto = { after: rest[at - 1]?.id, before: rest[at]?.id };
    const inbox = this.inbox;
    this.lists = [...(inbox ? [inbox] : []), ...rest.slice(0, at), user[from], ...rest.slice(at)];
    try {
      await this.client.moveTaskList(id, target);
    } catch (e) {
      await this.reloadLists();
      throw e;
    }
  }

  /** Offene bzw. erledigte Tasks einer Liste zählen (für den Lösch-Dialog, höchstens 999+). */
  async countTasks(listId: string): Promise<{ open: number; done: number }> {
    const view = { kind: "list" as const, listId };
    const [open, all] = await Promise.all([
      this.count({ view, status: "open" }),
      this.count({ view }),
    ]);
    return { open, done: Math.max(0, all - open) };
  }

  private scheduleCounts() {
    if (this.recount) clearTimeout(this.recount);
    this.recount = setTimeout(() => void this.refreshCounts(), 250);
  }

  async refreshCounts() {
    const [inbox, today] = await Promise.all([
      this.count(routeQuery({ view: "inbox" })),
      this.count(routeQuery({ view: "today" })),
    ]);
    this.counts = { inbox, today };
  }

  private async count(query: TaskQueryDto): Promise<number> {
    let cursor: string | null = null;
    let n = 0;
    do {
      const page = await this.client.listTasks(query, { cursor, limit: PAGE });
      n += page.items.length;
      cursor = page.nextCursor;
    } while (cursor && n <= COUNT_CAP);
    return n;
  }
}

export function formatCount(n: number): string {
  return n > COUNT_CAP ? `${COUNT_CAP}+` : String(n);
}
