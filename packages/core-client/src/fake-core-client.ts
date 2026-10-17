import type { CoreClient } from "./core-client";
import { CoreClientError } from "./errors";
import type {
  AuthStepDto,
  ChangedEventDto,
  CreateTaskDto,
  DueDto,
  LocalDayDto,
  MoveTargetDto,
  PageDto,
  PageRequestDto,
  SessionDto,
  SyncStatusDto,
  TaskDto,
  TaskListDto,
  TaskPatchDto,
  TaskQueryDto,
  TaskSummaryDto,
} from "./types";

/** Wie im Core (`todo_core::domain`): feste Inbox-ID, Grenzen für Titel/Namen/Seiten. */
const INBOX_ID = "00000000-0000-7000-8000-000000000001";
const TITLE_MAX = 500;
const LIST_NAME_MAX = 100;
const PAGE_LIMIT_MAX = 500;

export interface FakeOptions {
  /** Künstliche Latenz je Aufruf in ms (UI-Entwicklung). */
  latencyMs?: number;
  /** So viele Polls bleibt ein Login `pending`, bevor er gelingt. */
  loginPolls?: number;
  /** Uhr (Tests). */
  now?: () => Date;
}

interface Row {
  task: TaskDto;
  /** Reihenfolge innerhalb der Liste (statt Fractional Index des Cores). */
  order: number;
}

/**
 * In-Memory-`CoreClient` mit der Semantik des Cores (Validierung, Inbox, Reihenfolge, Ansichten,
 * Seiten, Events). Für UI-Entwicklung im Browser und Komponententests – nicht für echte Daten.
 */
export class FakeCoreClient implements CoreClient {
  private lists = new Map<string, TaskListDto>();
  private rows = new Map<string, Row>();
  private changeListeners = new Set<(e: ChangedEventDto) => void>();
  private statusListeners = new Set<(s: SyncStatusDto) => void>();
  private session: SessionDto | null = null;
  private flows = new Map<string, number>();
  private syncStatus: SyncStatusDto = { state: "disabled", lastSuccess: null, pendingOps: 0, message: null };
  private readonly latencyMs: number;
  private readonly loginPolls: number;
  private readonly now: () => Date;

  constructor(options: FakeOptions = {}) {
    this.latencyMs = options.latencyMs ?? 0;
    this.loginPolls = options.loginPolls ?? 1;
    this.now = options.now ?? (() => new Date());
  }

  // --- Listen --------------------------------------------------------------------------------

  ensureInbox = () =>
    this.run(() => {
      let inbox = this.lists.get(INBOX_ID);
      if (!inbox) {
        inbox = { id: INBOX_ID, name: "Inbox", isInbox: true, sortKey: "a0" };
        this.lists.set(INBOX_ID, inbox);
        this.emit([], [INBOX_ID]);
      }
      return { ...inbox };
    });

  listTaskLists = () =>
    this.run(() =>
      [...this.lists.values()]
        .sort((a, b) => cmp(a.sortKey, b.sortKey) || cmp(a.id, b.id))
        .map((l) => ({ ...l })),
    );

  createTaskList = (name: string) =>
    this.run(() => {
      const list: TaskListDto = {
        id: this.newId(),
        name: validName(name, LIST_NAME_MAX, "list name"),
        isInbox: false,
        sortKey: `b${String(this.lists.size).padStart(6, "0")}`,
      };
      this.lists.set(list.id, list);
      this.emit([], [list.id]);
      return { ...list };
    });

  renameTaskList = (id: string, name: string) =>
    this.run(() => {
      const list = this.list(id);
      if (list.isInbox) throw new CoreClientError("validation", "the inbox cannot be renamed");
      list.name = validName(name, LIST_NAME_MAX, "list name");
      this.emit([], [id]);
      return { ...list };
    });

  deleteTaskList = (id: string) =>
    this.run(() => {
      const list = this.list(id);
      if (list.isInbox) throw new CoreClientError("validation", "the inbox cannot be deleted");
      const moved = [...this.rows.values()].filter((r) => r.task.listId === id);
      let last = this.lastOrder(INBOX_ID);
      for (const row of moved) {
        row.task.listId = INBOX_ID;
        row.order = ++last;
        this.touch(row);
      }
      this.lists.delete(id);
      this.emit(
        moved.map((r) => r.task.id),
        [id],
      );
      return moved.length;
    });

  // --- Tasks ---------------------------------------------------------------------------------

  createTask = (input: CreateTaskDto) =>
    this.run(() => {
      const listId = input.listId ?? INBOX_ID;
      this.list(listId);
      const at = this.iso();
      const task: TaskDto = {
        id: this.newId(),
        listId,
        title: validName(input.title, TITLE_MAX, "title"),
        notes: "",
        status: "open",
        priority: "none",
        due: null,
        sortKey: "",
        createdAt: at,
        updatedAt: at,
        completedAt: null,
      };
      const row = { task, order: this.lastOrder(listId) + 1 };
      this.rows.set(task.id, row);
      this.emit([task.id], []);
      return this.dto(row);
    });

  getTask = (id: string) => this.run(() => this.dto(this.row(id)));

  updateTask = (id: string, patch: TaskPatchDto) =>
    this.run(() => {
      const row = this.row(id);
      const t = row.task;
      const next = { ...t };
      if (patch.title != null) next.title = validName(patch.title, TITLE_MAX, "title");
      if (patch.notes != null) next.notes = patch.notes;
      if (patch.priority != null) next.priority = patch.priority;
      if (patch.due !== undefined) next.due = patch.due === null ? null : validDue(patch.due);
      if (JSON.stringify(next) !== JSON.stringify(t)) {
        row.task = next;
        this.touch(row);
        this.emit([id], []);
      }
      return this.dto(row);
    });

  completeTask = (id: string, done: boolean) =>
    this.run(() => {
      const row = this.row(id);
      const status = done ? "done" : "open";
      if (row.task.status !== status) {
        row.task.status = status;
        row.task.completedAt = done ? this.iso() : null;
        this.touch(row);
        this.emit([id], []);
      }
      return this.dto(row);
    });

  moveTask = (id: string, target: MoveTargetDto) =>
    this.run(() => {
      const row = this.row(id);
      const listId = target.listId ?? row.task.listId;
      this.list(listId);
      if (target.after === id || target.before === id) {
        throw new CoreClientError("validation", "a task cannot be its own neighbour");
      }
      const siblings = this.ordered(listId).filter((r) => r.task.id !== id);
      const index = (nid: string | null | undefined) => {
        if (nid == null) return -1;
        const i = siblings.findIndex((r) => r.task.id === nid);
        if (i < 0) throw new CoreClientError("validation", `neighbour ${nid} is not in the target list`);
        return i;
      };
      const ai = index(target.after);
      const bi = index(target.before);
      let lo: number;
      let hi: number;
      if (target.after != null && target.before != null) {
        if (bi !== ai + 1) throw new CoreClientError("validation", "neighbours are not adjacent");
        lo = siblings[ai].order;
        hi = siblings[bi].order;
      } else if (target.after != null) {
        lo = siblings[ai].order;
        hi = siblings[ai + 1]?.order ?? lo + 2;
      } else if (target.before != null) {
        hi = siblings[bi].order;
        lo = siblings[bi - 1]?.order ?? hi - 2;
      } else {
        lo = siblings.at(-1)?.order ?? 0;
        hi = lo + 2;
      }
      row.task.listId = listId;
      row.order = (lo + hi) / 2;
      this.touch(row);
      this.emit([id], []);
      return this.dto(row);
    });

  deleteTask = (id: string) =>
    this.run(() => {
      // Wie im Core: Löschen unbekannter IDs ist ein No-op.
      if (this.rows.delete(id)) this.emit([id], []);
    });

  listTasks = (query: TaskQueryDto, page: PageRequestDto) =>
    this.run((): PageDto<TaskSummaryDto> => {
      if (page.limit < 1 || page.limit > PAGE_LIMIT_MAX) {
        throw new CoreClientError("validation", `page limit must be 1..=${PAGE_LIMIT_MAX}`);
      }
      const view = query.view;
      if (view.kind === "list") this.list(view.listId);
      let rows = [...this.rows.values()].filter((r) => {
        const t = r.task;
        const inView =
          view.kind === "inbox"
            ? t.listId === INBOX_ID
            : view.kind === "list"
              ? t.listId === view.listId
              : view.kind === "today"
                ? t.due != null && dueBy(t.due, view.day)
                : view.kind === "upcoming"
                  ? t.due != null && !dueBy(t.due, view.day)
                  : view.kind === "all"
                    ? true
                    : t.status === "done";
        return inView && (query.status == null || t.status === query.status);
      });
      rows = view.kind === "completed" ? rows.sort(byCompleted) : rows.sort(byOrder(this.lists));
      const start = page.cursor ? Number(page.cursor) : 0;
      const items = rows.slice(start, start + page.limit).map((r) => summary(this.dto(r)));
      const end = start + items.length;
      return { items, nextCursor: end < rows.length ? String(end) : null };
    });

  // --- Anmeldung & Sync ----------------------------------------------------------------------

  auth: CoreClient["auth"] = {
    info: () =>
      this.run(() => ({ provider: "Demo", notice: "Demo-Anmeldung ohne Server.", available: true })),
    current: () => this.run(() => (this.session ? { ...this.session } : null)),
    beginLogin: () =>
      this.run((): AuthStepDto => {
        const flow = this.newId();
        this.flows.set(flow, this.loginPolls);
        return { kind: "openBrowser", flow, url: "https://login.example.invalid/", pollIntervalS: 1, expiresInS: 600 };
      }),
    openBrowser: (flow) =>
      this.run(() => {
        this.flow(flow);
      }),
    pollLogin: (flow) =>
      this.run((): AuthStepDto => {
        const left = this.flow(flow);
        if (left > 0) {
          this.flows.set(flow, left - 1);
          return { kind: "pending", flow };
        }
        this.flows.delete(flow);
        this.session = { accountLabel: "demo@example.invalid" };
        this.setStatus({ state: "idle", lastSuccess: this.iso(), pendingOps: 0, message: null });
        return { kind: "done", session: { ...this.session } };
      }),
    cancelLogin: (flow) =>
      this.run(() => {
        this.flows.delete(flow);
      }),
    logout: () =>
      this.run(() => {
        this.session = null;
        this.setStatus({ state: "disabled", lastSuccess: null, pendingOps: 0, message: null });
      }),
  };

  sync: CoreClient["sync"] = {
    status: () => this.run(() => ({ ...this.syncStatus })),
    now: () =>
      this.run(() => {
        if (this.session) this.setStatus({ ...this.syncStatus, state: "idle", lastSuccess: this.iso() });
      }),
  };

  ready = () => this.run(() => undefined);

  onChange = (cb: (e: ChangedEventDto) => void) => {
    this.changeListeners.add(cb);
    return () => void this.changeListeners.delete(cb);
  };

  onSyncStatus = (cb: (s: SyncStatusDto) => void) => {
    this.statusListeners.add(cb);
    return () => void this.statusListeners.delete(cb);
  };

  /** Simuliert einen Statuswechsel des Sync (z. B. „offline") für UI-Tests. */
  setStatus(status: SyncStatusDto) {
    this.syncStatus = { ...status };
    for (const cb of this.statusListeners) cb({ ...status });
  }

  // --- intern --------------------------------------------------------------------------------

  /** Asynchron wie IPC: Ergebnis in einer Microtask (bzw. nach der Latenz). */
  private async run<T>(fn: () => T): Promise<T> {
    if (this.latencyMs > 0) await new Promise((r) => setTimeout(r, this.latencyMs));
    else await Promise.resolve();
    return fn();
  }

  private emit(tasks: string[], lists: string[]) {
    const event = { tasks, lists };
    // Wie Tauri-Events: nach der Antwort des Aufrufs.
    queueMicrotask(() => {
      for (const cb of this.changeListeners) cb(event);
    });
  }

  private list(id: string): TaskListDto {
    const list = this.lists.get(id);
    if (!list) throw new CoreClientError("notFound", `not found: list ${id}`);
    return list;
  }

  private row(id: string): Row {
    const row = this.rows.get(id);
    if (!row) throw new CoreClientError("notFound", `not found: task ${id}`);
    return row;
  }

  private flow(id: string): number {
    const left = this.flows.get(id);
    if (left === undefined) throw new CoreClientError("auth", "auth: login cancelled");
    return left;
  }

  private ordered(listId: string): Row[] {
    return [...this.rows.values()].filter((r) => r.task.listId === listId).sort(byOrder(this.lists));
  }

  private lastOrder(listId: string): number {
    return this.ordered(listId).at(-1)?.order ?? 0;
  }

  private touch(row: Row) {
    row.task.updatedAt = this.iso();
  }

  private dto(row: Row): TaskDto {
    return { ...row.task, due: row.task.due ? { ...row.task.due } : null, sortKey: orderKey(row.order) };
  }

  private iso(): string {
    return this.now().toISOString();
  }

  private newId(): string {
    return crypto.randomUUID();
  }
}

function cmp(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

function orderKey(order: number): string {
  // Lexikografisch sortierbar wie der Fractional Index des Cores.
  return order.toFixed(9).padStart(24, "0");
}

function byOrder(lists: Map<string, TaskListDto>) {
  return (a: Row, b: Row) =>
    cmp(lists.get(a.task.listId)?.sortKey ?? "", lists.get(b.task.listId)?.sortKey ?? "") ||
    a.order - b.order ||
    cmp(a.task.id, b.task.id);
}

function byCompleted(a: Row, b: Row): number {
  return cmp(b.task.completedAt ?? "", a.task.completedAt ?? "") || cmp(a.task.id, b.task.id);
}

function validName(value: string, max: number, what: string): string {
  const trimmed = value.trim();
  if (!trimmed) throw new CoreClientError("validation", `validation: ${what} must not be empty`);
  if ([...trimmed].length > max) {
    throw new CoreClientError("validation", `validation: ${what} longer than ${max} characters`);
  }
  return trimmed;
}

function validDue(due: DueDto): DueDto {
  if (due.kind === "date" && !/^\d{4}-\d{2}-\d{2}$/.test(due.date)) {
    throw new CoreClientError("validation", `validation: invalid date ${JSON.stringify(due.date)}`);
  }
  return { ...due };
}

/** Heute fällig oder überfällig (wie `Due::is_due_by`). */
function dueBy(due: DueDto, day: LocalDayDto): boolean {
  return due.kind === "date" ? due.date <= day.date : due.at < day.end;
}

function summary(t: TaskDto): TaskSummaryDto {
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
