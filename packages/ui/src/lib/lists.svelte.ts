import type { CoreClient, TaskListDto, TaskQueryDto } from "@tasks/core-client";
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
