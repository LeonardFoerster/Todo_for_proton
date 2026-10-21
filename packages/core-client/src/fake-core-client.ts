import type { CoreClient } from "./core-client";
import { CoreClientError } from "./errors";
import { FakeNotes } from "./fake-notes";
import type {
  AuthStepDto,
  ChangedEventDto,
  CreateTaskDto,
  DueDto,
  LocalDayDto,
  MoveListTargetDto,
  MoveTargetDto,
  PageDto,
  PageRequestDto,
  SessionDto,
  SyncStatusDto,
  TaskDto,
  TaskDueDto,
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
const BULK_LIMIT_MAX = 2000;

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

  /** Notizen (ADR-029); `simulateRemoteBody`/`simulateConflict` für UI-Tests. */
  readonly notes: FakeNotes;

  constructor(options: FakeOptions = {}) {
    this.latencyMs = options.latencyMs ?? 0;
    this.loginPolls = options.loginPolls ?? 1;
    this.now = options.now ?? (() => new Date());
    this.notes = new FakeNotes({
      run: (fn) => this.run(fn),
      emit: (notes, noteFolders) => this.emitEvent({ tasks: [], lists: [], notes, noteFolders }),
      now: () => this.now(),
      newId: () => this.newId(),
    });
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

  listTaskLists = () => this.run(() => this.orderedLists().map((l) => ({ ...l })));

  createTaskList = (name: string) =>
    this.run(() => {
      const list: TaskListDto = {
        id: this.newId(),
        name: validName(name, LIST_NAME_MAX, "list name"),
        isInbox: false,
        sortKey: listKey(this.userLists().length + 1),
      };
      this.lists.set(list.id, list);
      this.emit([], [list.id]);
      return { ...list };
    });

  moveTaskList = (id: string, target: MoveListTargetDto) =>
    this.run(() => {
      const list = this.list(id);
      if (list.isInbox) throw new CoreClientError("validation", "the inbox cannot be moved");
      if (target.after === id || target.before === id) {
        throw new CoreClientError("validation", "a list cannot be its own neighbour");
      }
      const rest = this.userLists().filter((l) => l.id !== id);
      const index = (nid: string | null | undefined) => {
        if (nid == null) return -1;
        const i = rest.findIndex((l) => l.id === nid);
        if (i < 0) throw new CoreClientError("validation", `${nid} is not a movable list`);
        return i;
      };
      const ai = index(target.after);
      const bi = index(target.before);
      if (target.after != null && target.before != null && bi !== ai + 1) {
        throw new CoreClientError("validation", "neighbours are not adjacent");
      }
      const at = target.after != null ? ai + 1 : target.before != null ? bi : rest.length;
      rest.splice(at, 0, list);
      // Fake: Schlüssel neu durchnummerieren (der Core ändert nur den der verschobenen Liste).
      rest.forEach((l, i) => (l.sortKey = listKey(i + 1)));
      this.emit([], [id]);
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
      // Wie im Core: gezählt werden nur Tasks außerhalb des Papierkorbs (mitgenommen werden alle).
      return moved.filter((r) => r.task.trashedAt == null).length;
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
        notes: input.notes ?? "",
        status: "open",
        priority: input.priority ?? "none",
        due: input.due ? validDue(input.due) : null,
        sortKey: "",
        createdAt: at,
        updatedAt: at,
        completedAt: null,
        trashedAt: null,
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
      const [lo, hi] = this.bounds(listId, new Set([id]), target);
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

  // --- Sammelaktionen: ein Event je Aufruf, unbekannte IDs übersprungen (wie ADR-021) ---------

  completeTasks = (ids: string[], done: boolean) =>
    this.run(() => {
      const status = done ? "done" : "open";
      const changed = this.existing(ids).filter((row) => row.task.status !== status);
      for (const row of changed) {
        row.task.status = status;
        row.task.completedAt = done ? this.iso() : null;
        this.touch(row);
      }
      return this.emitRows(changed);
    });

  setDues = (dues: TaskDueDto[]) =>
    this.run(() => {
      // Erst alles prüfen: eine ungültige Fälligkeit schreibt nichts (wie der Core).
      const wanted = new Map<string, DueDto | null>();
      for (const d of dues) if (!wanted.has(d.id)) wanted.set(d.id, d.due === null ? null : validDue(d.due));
      const changed = this.existing([...wanted.keys()]).filter(
        (row) => JSON.stringify(row.task.due) !== JSON.stringify(wanted.get(row.task.id)),
      );
      for (const row of changed) {
        row.task = { ...row.task, due: wanted.get(row.task.id) ?? null };
        this.touch(row);
      }
      return this.emitRows(changed);
    });

  deleteTasks = (ids: string[]) =>
    this.run(() => {
      const rows = this.existing(ids);
      const before = rows.map((row) => this.dto(row));
      for (const row of rows) this.rows.delete(row.task.id);
      this.emitRows(rows);
      return before;
    });

  trashTasks = (ids: string[]) =>
    this.run(() => {
      const at = this.iso();
      const changed = this.existing(ids).filter((row) => row.task.trashedAt == null);
      for (const row of changed) {
        row.task.trashedAt = at;
        this.touch(row);
      }
      return this.emitRows(changed);
    });

  untrashTasks = (ids: string[]) =>
    this.run(() => {
      const changed = this.existing(ids).filter((row) => row.task.trashedAt != null);
      for (const row of changed) {
        row.task.trashedAt = null;
        this.touch(row);
      }
      return this.emitRows(changed);
    });

  purgeTasks = (ids: string[]) =>
    this.run(() => {
      const rows = this.existing(ids).filter((row) => row.task.trashedAt != null);
      for (const row of rows) this.rows.delete(row.task.id);
      this.emitRows(rows);
      return rows.length;
    });

  emptyTaskTrash = () =>
    this.run(() => {
      const rows = [...this.rows.values()].filter((row) => row.task.trashedAt != null);
      for (const row of rows) this.rows.delete(row.task.id);
      if (rows.length > 0) this.emitRows(rows);
      return rows.length;
    });

  moveTasks = (ids: string[], target: MoveTargetDto) =>
    this.run(() => {
      const rows = this.existing(ids).sort(byOrder(this.lists));
      if (rows.length === 0) return [];
      const listId = target.listId ?? rows[0].task.listId;
      if (target.listId == null && rows.some((r) => r.task.listId !== listId)) {
        throw new CoreClientError("validation", "tasks are in different lists; a target list is required");
      }
      this.list(listId);
      const moving = new Set(rows.map((r) => r.task.id));
      if ([target.after, target.before].some((n) => n != null && moving.has(n))) {
        throw new CoreClientError("validation", "a moved task cannot be a neighbour");
      }
      const [lo, hi] = this.bounds(listId, moving, target);
      const changed: Row[] = [];
      rows.forEach((row, i) => {
        const order = lo + ((hi - lo) * (i + 1)) / (rows.length + 1);
        if (row.task.listId === listId && row.order === order) return;
        row.task.listId = listId;
        row.order = order;
        this.touch(row);
        changed.push(row);
      });
      return this.emitRows(changed);
    });

  restoreTasks = (tasks: TaskDto[]) =>
    this.run(() => {
      checkBulk(tasks.length);
      const at = this.iso();
      const rows = tasks.map((t): Row => {
        const order = Number(t.sortKey);
        if (!t.sortKey || Number.isNaN(order)) {
          throw new CoreClientError("validation", `invalid sort key: ${JSON.stringify(t.sortKey)}`);
        }
        if ((t.status === "done") !== (t.completedAt != null)) {
          throw new CoreClientError("validation", "completed_at must be set exactly when the task is done");
        }
        const task: TaskDto = {
          ...t,
          id: this.newId(),
          listId: this.lists.has(t.listId) ? t.listId : INBOX_ID,
          title: validName(t.title, TITLE_MAX, "title"),
          due: t.due ? validDue(t.due) : null,
          updatedAt: at,
        };
        return { task, order };
      });
      for (const row of rows) this.rows.set(row.task.id, row);
      return this.emitRows(rows);
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
                  : view.kind === "all" || view.kind === "trash"
                    ? true
                    : t.status === "done";
        // Papierkorb nur in seiner Ansicht (ADR-031).
        const trashOk = (t.trashedAt != null) === (view.kind === "trash");
        return inView && trashOk && (query.status == null || t.status === query.status);
      });
      rows =
        view.kind === "trash"
          ? rows.sort(byTrashed)
          : view.kind === "completed"
          ? rows.sort(byCompleted)
          : view.kind === "today" || view.kind === "upcoming"
            ? rows.sort(byDue(view.day, byOrder(this.lists)))
            : rows.sort(byOrder(this.lists));
      const start = page.cursor ? Number(page.cursor) : 0;
      const items = rows.slice(start, start + page.limit).map((r) => summary(this.dto(r)));
      const end = start + items.length;
      return { items, nextCursor: end < rows.length ? String(end) : null };
    });

  searchTasks = (text: string, limit: number) =>
    this.run((): TaskSummaryDto[] => {
      if (limit < 1 || limit > PAGE_LIMIT_MAX) throw new CoreClientError("validation", `search limit must be 1..=${PAGE_LIMIT_MAX}`);
      const terms = text.toLowerCase().split(/\s+/).filter(Boolean);
      if (terms.length === 0) return [];
      // Wie im Core (`SearchQuery::rank`): alle Wörter in Titel oder Notiz; offen, Titeltreffer, Reihenfolge.
      const order = byOrder(this.lists);
      return [...this.rows.values()]
        .flatMap((row) => {
          if (row.task.trashedAt != null) return [];
          const title = row.task.title.toLowerCase();
          const notes = row.task.notes.toLowerCase();
          if (!terms.every((t) => title.includes(t) || notes.includes(t))) return [];
          return [{ row, done: row.task.status === "done", notesOnly: !terms.some((t) => title.includes(t)) }];
        })
        .sort((a, b) => Number(a.done) - Number(b.done) || Number(a.notesOnly) - Number(b.notesOnly) || order(a.row, b.row))
        .slice(0, limit)
        .map(({ row }) => summary(this.dto(row)));
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
    flush: () => this.run(() => undefined),
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
    this.emitEvent({ tasks, lists, notes: [], noteFolders: [] });
  }

  private emitEvent(event: ChangedEventDto) {
    // Wie Tauri-Events: nach der Antwort des Aufrufs.
    queueMicrotask(() => {
      for (const cb of this.changeListeners) cb(event);
    });
  }

  /** Ein Event für alle Zeilen (keins, wenn leer); liefert sie als DTOs. */
  private emitRows(rows: Row[]): TaskDto[] {
    if (rows.length > 0) {
      this.emit(
        rows.map((r) => r.task.id),
        [],
      );
    }
    return rows.map((r) => this.dto(r));
  }

  /** Existierende Zeilen zu `ids` ohne Duplikate (Reihenfolge wie `ids`). */
  private existing(ids: string[]): Row[] {
    checkBulk(ids.length);
    const rows: Row[] = [];
    for (const id of new Set(ids)) {
      const row = this.rows.get(id);
      if (row) rows.push(row);
    }
    return rows;
  }

  /** Reihenfolgewerte unter- und oberhalb der Zielposition (Nachbarn wie `MoveTargetDto`). */
  private bounds(listId: string, moving: Set<string>, target: MoveTargetDto): [number, number] {
    const siblings = this.ordered(listId).filter((r) => !moving.has(r.task.id));
    const index = (nid: string | null | undefined) => {
      if (nid == null) return -1;
      const i = siblings.findIndex((r) => r.task.id === nid);
      if (i < 0) throw new CoreClientError("validation", `neighbour ${nid} is not in the target list`);
      return i;
    };
    const ai = index(target.after);
    const bi = index(target.before);
    if (target.after != null && target.before != null) {
      if (bi !== ai + 1) throw new CoreClientError("validation", "neighbours are not adjacent");
      return [siblings[ai].order, siblings[bi].order];
    }
    if (target.after != null) {
      const lo = siblings[ai].order;
      return [lo, siblings[ai + 1]?.order ?? lo + 2];
    }
    if (target.before != null) {
      const hi = siblings[bi].order;
      return [siblings[bi - 1]?.order ?? hi - 2, hi];
    }
    const lo = siblings.at(-1)?.order ?? 0;
    return [lo, lo + 2];
  }

  /** Eingang zuerst, dann Benutzerlisten nach Sort-Key (wie `CoreApi::list_task_lists`). */
  private orderedLists(): TaskListDto[] {
    return [...this.lists.values()].sort(
      (a, b) => Number(b.isInbox) - Number(a.isInbox) || cmp(a.sortKey, b.sortKey) || cmp(a.id, b.id),
    );
  }

  private userLists(): TaskListDto[] {
    return this.orderedLists().filter((l) => !l.isInbox);
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

function checkBulk(n: number) {
  if (n > BULK_LIMIT_MAX) {
    throw new CoreClientError("validation", `at most ${BULK_LIMIT_MAX} tasks per call, got ${n}`);
  }
}

/** Sortierbarer Listen-Schlüssel des Fakes (n ≥ 1). */
function listKey(n: number): string {
  return `b${String(n).padStart(6, "0")}`;
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

/** Heute/Geplant wie im Core (ADR-026): Sortierzeitpunkt der Fälligkeit, dann `then`. */
function byDue(day: LocalDayDto, then: (a: Row, b: Row) => number) {
  const instant = (due: DueDto | null): number => {
    if (!due) return Number.MAX_SAFE_INTEGER;
    if (due.kind === "dateTime") return Date.parse(due.at);
    const days = (Date.parse(`${due.date}T00:00:00Z`) - Date.parse(`${day.date}T00:00:00Z`)) / 86_400_000;
    return Date.parse(day.start) + days * 86_400_000;
  };
  return (a: Row, b: Row) => instant(a.task.due) - instant(b.task.due) || then(a, b);
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

/** Papierkorb: zuletzt gelöschte zuerst, dann ID (wie der Core). */
function byTrashed(a: Row, b: Row): number {
  return (b.task.trashedAt ?? "").localeCompare(a.task.trashedAt ?? "") || a.task.id.localeCompare(b.task.id);
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
    trashedAt: t.trashedAt,
  };
}
