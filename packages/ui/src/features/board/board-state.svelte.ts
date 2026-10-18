import { CoreClientError, type CoreClient, type TaskListDto } from "@tasks/core-client";
import { localDay } from "../../lib/local-day";
import type { Route } from "../../lib/router.svelte";
import { TaskListState, toSummary } from "../task-list/task-list-state.svelte";

/** Ab so vielen IDs in einem Änderungs-Event alle Spalten neu laden statt je ID `getTask`. */
const REFRESH_BATCH = 20;

export interface BoardColumn {
  list: TaskListDto;
  /** Offene Tasks der Liste, seitenweise (Logik der Listenansicht inkl. Verlassen beim Abhaken). */
  rows: TaskListState;
}

/**
 * Spalten des Kanban-Boards (P2-06): eine je Liste, Eingang zuerst. **Ein** `onChange`-Abo fürs
 * ganze Board: kleine Events laden jede Task einmal und geben sie allen Spalten (jede entscheidet
 * selbst, ob sie dazugehört), große laden alle Spalten neu.
 */
export class BoardState {
  columns = $state.raw<BoardColumn[]>([]);

  constructor(
    private readonly client: CoreClient,
    private readonly inboxId: () => string | undefined,
  ) {}

  /** Spalten an die Listen angleichen (neu/gelöscht/umbenannt/umsortiert); lädt neue Spalten. */
  setLists(lists: readonly TaskListDto[]): Promise<void> {
    // Nur lokale Buchführung für diesen Abgleich, nicht reaktiv.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const old = new Map(this.columns.map((c) => [c.list.id, c]));
    const fresh: BoardColumn[] = [];
    const loads: Promise<void>[] = [];
    for (const list of lists) {
      const existing = old.get(list.id);
      if (existing) {
        fresh.push({ list, rows: existing.rows });
        old.delete(list.id);
        continue;
      }
      const route: Route = list.isInbox ? { view: "inbox" } : { view: "list", listId: list.id };
      const rows = new TaskListState(this.client, () => ({ route, inboxId: this.inboxId(), day: localDay() }));
      fresh.push({ list, rows });
      loads.push(rows.load());
    }
    for (const gone of old.values()) gone.rows.dispose();
    this.columns = fresh;
    return Promise.all(loads).then(() => undefined);
  }

  column(listId: string): BoardColumn | undefined {
    return this.columns.find((c) => c.list.id === listId);
  }

  /** Spalte, die die Task gerade zeigt. */
  columnOf(taskId: string): BoardColumn | undefined {
    return this.columns.find((c) => c.rows.indexOf(taskId) >= 0);
  }

  attach(): () => void {
    return this.client.onChange((e) => {
      if (e.tasks.length > REFRESH_BATCH) {
        for (const c of this.columns) void c.rows.reload();
      } else {
        for (const id of e.tasks) void this.refresh(id);
      }
    });
  }

  /** Eine Task einmal laden und allen Spalten geben (ein- bzw. auspflegen). */
  async refresh(id: string): Promise<void> {
    try {
      const task = toSummary(await this.client.getTask(id));
      for (const c of this.columns) c.rows.apply(task);
    } catch (e) {
      if (CoreClientError.from(e).kind === "notFound") for (const c of this.columns) c.rows.remove(id);
    }
  }

  dispose(): void {
    for (const c of this.columns) c.rows.dispose();
  }
}
