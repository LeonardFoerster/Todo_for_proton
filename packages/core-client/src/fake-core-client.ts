import type { CoreClient } from "./core-client";
import { CoreClientError } from "./errors";
import { FakeNotes } from "./fake-notes";
import { anchored, dueDay, dueOn, firstOnOrAfter, nextOccurrence } from "./fake-recurrence";
import type {
  AuthStepDto,
  ChangedEventDto,
  ChecklistItemDto,
  CompleteResultDto,
  CreateTaskDto,
  DueDto,
  LocalDayDto,
  MoveListTargetDto,
  MoveTargetDto,
  OccurrenceDto,
  PageDto,
  PageRequestDto,
  RecurrenceDto,
  SessionDto,
  SyncStatusDto,
  LinkDto,
  LinkedNoteDto,
  LinkedTaskDto,
  TagDto,
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
  private tags = new Map<string, TagDto>();
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
      emit: (notes, noteFolders) => this.emitEvent({ tasks: [], lists: [], notes, noteFolders, tags: [], links: [] }),
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

  // --- Verknüpfungen (ADR-036) ---------------------------------------------------------------

  private links = new Map<string, LinkDto>();
  private unlinked = new Set<string>();

  linkTaskNote = (taskId: string, noteId: string) =>
    this.run(() => {
      this.row(taskId);
      if (!this.notes.peek(noteId)) throw new CoreClientError("notFound", `note ${noteId}`);
      const existing = [...this.links.values()].find((l) => l.taskId === taskId && l.noteId === noteId);
      if (existing) return { ...existing };
      let id = `link:${taskId}:${noteId}`;
      for (let n = 1; this.unlinked.has(id); n++) id = `link:${taskId}:${noteId}|${n}`;
      const link = { id, taskId, noteId };
      this.links.set(id, link);
      this.emit([], [], [], [id]);
      return { ...link };
    });

  unlink = (linkId: string) =>
    this.run(() => {
      if (this.links.delete(linkId)) {
        this.unlinked.add(linkId);
        this.emit([], [], [], [linkId]);
      }
    });

  taskLinks = (taskId: string) =>
    this.run((): LinkedNoteDto[] => {
      const out: LinkedNoteDto[] = [];
      for (const l of this.links.values()) {
        if (l.taskId !== taskId) continue;
        const note = this.notes.peek(l.noteId);
        if (note && note.trashedAt == null) out.push({ linkId: l.id, noteId: note.id, name: note.name, kind: note.kind });
      }
      return out;
    });

  noteLinks = (noteId: string) =>
    this.run((): LinkedTaskDto[] =>
      [...this.links.values()].flatMap((l) => {
        const row = l.noteId === noteId ? this.rows.get(l.taskId) : undefined;
        return row && row.task.trashedAt == null ? [{ linkId: l.id, task: summary(this.dto(row), true) }] : [];
      }),
    );

  private hasLinks(taskId: string): boolean {
    for (const l of this.links.values()) if (l.taskId === taskId) return true;
    return false;
  }

  // --- Tags (ADR-035) ------------------------------------------------------------------------

  listTags = () =>
    this.run(() => [...this.tags.values()].sort((a, b) => cmp(a.name.toLowerCase(), b.name.toLowerCase()) || cmp(a.id, b.id)).map((t) => ({ ...t })));

  ensureTag = (name: string) =>
    this.run(() => {
      const clean = tagName(name);
      const found = [...this.tags.values()].find((t) => t.name.toLowerCase() === clean.toLowerCase());
      if (found) return { ...found };
      // Wie der Core: ID aus dem Namen (Fake: lesbar statt UUIDv5).
      let id = `tag:${clean.toLowerCase()}`;
      for (let n = 1; this.tags.has(id) || this.deletedTags.has(id); n++) id = `tag:${clean.toLowerCase()}|${n}`;
      const tag = { id, name: clean };
      this.tags.set(id, tag);
      this.emit([], [], [id]);
      return { ...tag };
    });

  renameTag = (id: string, name: string) =>
    this.run(() => {
      const tag = this.tags.get(id);
      if (!tag) throw new CoreClientError("notFound", `tag ${id}`);
      const clean = tagName(name);
      if ([...this.tags.values()].some((t) => t.id !== id && t.name.toLowerCase() === clean.toLowerCase())) {
        throw new CoreClientError("validation", `tag ${JSON.stringify(clean)} already exists`);
      }
      if (tag.name !== clean) {
        tag.name = clean;
        this.emit([], [], [id]);
      }
      return { ...tag };
    });

  deleteTag = (id: string) =>
    this.run(() => {
      if (!this.tags.delete(id)) return 0;
      this.deletedTags.add(id);
      const rows = [...this.rows.values()].filter((r) => r.task.tags.includes(id));
      for (const row of rows) {
        row.task.tags = row.task.tags.filter((t) => t !== id);
        this.touch(row);
      }
      this.emit(
        rows.map((r) => r.task.id),
        [],
        [id],
      );
      return rows.length;
    });

  private deletedTags = new Set<string>();

  /** Wie `check_task_tags` + „jeder Tag existiert“ im Core. */
  private validTags(tags: string[]): string[] {
    if (tags.length > 20) throw new CoreClientError("validation", "a task can have at most 20 tags");
    if (new Set(tags).size !== tags.length) throw new CoreClientError("validation", "duplicate tag");
    for (const t of tags) if (!this.tags.has(t)) throw new CoreClientError("validation", `unknown tag ${t}`);
    return [...tags];
  }

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
        recurrence: null,
        checklist: [],
        tags: this.validTags(input.tags ?? []),
      };
      if (input.recurrence) this.applyRule(task, input.recurrence);
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
      // Ohne Fälligkeit keine Serie (wie der Core); eine neue Regel verankert sich an der Fälligkeit.
      if (next.due === null) next.recurrence = null;
      if (patch.recurrence !== undefined) {
        if (patch.recurrence === null) next.recurrence = null;
        else this.applyRule(next, patch.recurrence);
      }
      if (patch.checklist != null) next.checklist = this.validChecklist(patch.checklist);
      if (patch.tags != null) next.tags = this.validTags(patch.tags);
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
      this.emitRows(this.complete([row], done, this.iso().slice(0, 10)).rows);
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

  /** Abhaken/Öffnen wie `CompleteTasks` im Core; Serien bekommen einen Verlaufseintrag `${serie}:${tag}`. */
  private complete(rows: Row[], done: boolean, today: string): { rows: Row[]; occurrences: OccurrenceDto[] } {
    const status = done ? "done" : "open";
    const changed: Row[] = [];
    const entries: Row[] = [];
    const occurrences: OccurrenceDto[] = [];
    for (const row of rows) {
      const t = row.task;
      if (t.status === status) continue;
      if (done && t.recurrence && t.due) {
        const previousDue = { ...t.due };
        const next = nextOccurrence(t.recurrence, dueDay(t.due), today);
        let occurrenceId: string | null = null;
        if (next) {
          occurrenceId = `${t.id}:${dueDay(t.due)}`;
          if (!this.rows.has(occurrenceId)) {
            const at = this.iso();
            const task: TaskDto = {
              ...t,
              id: occurrenceId,
              status: "done",
              recurrence: null,
              checklist: t.checklist.map((i) => ({ ...i })),
              createdAt: at,
              updatedAt: at,
              completedAt: at,
              trashedAt: null,
            };
            const entry = { task, order: this.lastOrder(t.listId) + 1 };
            this.rows.set(occurrenceId, entry);
            entries.push(entry);
          }
          t.due = dueOn(next, t.due);
          // Nächstes Vorkommen ohne Haken; der Eintrag behält sie (wie der Core).
          t.checklist = t.checklist.map((i) => ({ ...i, done: false }));
        } else {
          Object.assign(t, { status: "done", completedAt: this.iso() });
        }
        occurrences.push({ seriesId: t.id, occurrenceId, previousDue });
      } else {
        t.status = status;
        t.completedAt = done ? this.iso() : null;
      }
      this.touch(row);
      changed.push(row);
    }
    return { rows: [...changed, ...entries], occurrences };
  }

  /** Wie `ChecklistItem`/`check_checklist` im Core: einzeilig, 1–500 Zeichen, ≤ 100, neue IDs für leere. */
  private validChecklist(items: ChecklistItemDto[]): ChecklistItemDto[] {
    if (items.length > 100) throw new CoreClientError("validation", "checklist must have at most 100 items");
    const out = items.map((i) => ({
      id: i.id || this.newId(),
      text: validName(i.text.replace(/[\r\n\t]/g, " "), 500, "checklist item"),
      done: i.done,
    }));
    if (new Set(out.map((i) => i.id)).size !== out.length) throw new CoreClientError("validation", "duplicate checklist item");
    return out;
  }

  /** Regel setzen: ohne Fälligkeit erst das erste Vorkommen ab heute (ganztägig). */
  private applyRule(task: TaskDto, rule: RecurrenceDto) {
    if (!task.due) {
      const first = firstOnOrAfter(rule, this.iso().slice(0, 10));
      if (!first) throw new CoreClientError("validation", "recurrence ends before today");
      task.due = { kind: "date", date: first };
    }
    if (rule.interval < 1 || rule.interval > 999) throw new CoreClientError("validation", "interval must be 1..=999");
    task.recurrence = anchored(rule, dueDay(task.due));
  }

  // --- Sammelaktionen: ein Event je Aufruf, unbekannte IDs übersprungen (wie ADR-021) ---------

  completeTasks = (ids: string[], done: boolean, today?: LocalDayDto) =>
    this.run((): CompleteResultDto => {
      const { rows, occurrences } = this.complete(this.existing(ids), done, today?.date ?? this.iso().slice(0, 10));
      return { tasks: this.emitRows(rows), occurrences };
    });

  skipOccurrence = (id: string, today?: LocalDayDto) =>
    this.run(() => {
      const row = this.row(id);
      const t = row.task;
      if (!t.recurrence || !t.due || t.status === "done") {
        throw new CoreClientError("validation", `task ${id} is not an open recurring task`);
      }
      const next = nextOccurrence(t.recurrence, dueDay(t.due), today?.date ?? this.iso().slice(0, 10));
      if (next) {
        t.due = dueOn(next, t.due);
        t.checklist = t.checklist.map((i) => ({ ...i, done: false }));
      } else Object.assign(t, { status: "done", completedAt: this.iso() });
      this.touch(row);
      this.emit([id], []);
      return this.dto(row);
    });

  undoOccurrence = (occurrence: OccurrenceDto) =>
    this.run(() => {
      const row = this.row(occurrence.seriesId);
      Object.assign(row.task, { status: "open", completedAt: null, due: { ...occurrence.previousDue } });
      this.touch(row);
      const ids = [row.task.id];
      if (occurrence.occurrenceId && this.rows.delete(occurrence.occurrenceId)) ids.push(occurrence.occurrenceId);
      this.emit(ids, []);
      return this.dto(row);
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
        const tagOk = query.tag == null || t.tags.includes(query.tag);
        const prioOk = query.priority == null || t.priority === query.priority;
        return inView && trashOk && tagOk && prioOk && (query.status == null || t.status === query.status);
      });
      // Sortierung (ADR-037) wie `TaskQuery::order` im Core.
      const sort = query.sort ?? "view";
      const order = byOrder(this.lists);
      rows =
        view.kind === "trash"
          ? rows.sort(byTrashed)
          : view.kind === "completed"
          ? rows.sort(byCompleted)
          : (view.kind === "today" || view.kind === "upcoming") && (sort === "view" || sort === "due")
            ? rows.sort(byDue(view.day, order))
            : sort === "due"
              ? rows.sort((a, b) => roughInstant(a.task.due) - roughInstant(b.task.due) || order(a, b))
              : sort === "priority"
                ? rows.sort((a, b) => PRIORITY_RANK[b.task.priority] - PRIORITY_RANK[a.task.priority] || order(a, b))
                : sort === "title"
                  ? rows.sort((a, b) => cmp(asciiLower(a.task.title), asciiLower(b.task.title)) || cmp(a.task.id, b.task.id))
                  : rows.sort(order);
      const start = page.cursor ? Number(page.cursor) : 0;
      const items = rows.slice(start, start + page.limit).map((r) => summary(this.dto(r), this.hasLinks(r.task.id)));
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
          // Punkte der Checkliste zählen wie die Notiz (ADR-034).
          const notes = [row.task.notes, ...row.task.checklist.map((i) => i.text)].join("\n").toLowerCase();
          if (!terms.every((t) => title.includes(t) || notes.includes(t))) return [];
          return [{ row, done: row.task.status === "done", notesOnly: !terms.some((t) => title.includes(t)) }];
        })
        .sort((a, b) => Number(a.done) - Number(b.done) || Number(a.notesOnly) - Number(b.notesOnly) || order(a.row, b.row))
        .slice(0, limit)
        .map(({ row }) => summary(this.dto(row)));
    });

  // --- Anmeldung & Sync ----------------------------------------------------------------------

  /** Export/Import gibt es nur in der Desktop-Shell; Tests setzen es bei Bedarf (ADR-032). */
  data?: CoreClient["data"];
  /** Erinnerungen/Hintergrund nur in der Desktop-Shell; Tests setzen es bei Bedarf (ADR-033). */
  background?: CoreClient["background"];

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

  private emit(tasks: string[], lists: string[], tags: string[] = [], links: string[] = []) {
    this.emitEvent({ tasks, lists, notes: [], noteFolders: [], tags, links });
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

const PRIORITY_RANK: Record<string, number> = { none: 0, low: 1, medium: 2, high: 3 };
const asciiLower = (s: string) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());

/** Wie `Due::rough_instant` im Core. */
function roughInstant(due: DueDto | null): number {
  if (!due) return Number.MAX_SAFE_INTEGER;
  if (due.kind === "dateTime") return Date.parse(due.at);
  const [y, m, d] = due.date.split("-").map(Number);
  return Date.UTC(y, m - 1, d);
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

function summary(t: TaskDto, hasLinks = false): TaskSummaryDto {
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
    hasLinks,
  };
}

/** Wie `normalize_tag_name`: `#` vorn weg, Leerzeichen zu `-`, 1–50 Zeichen. */
function tagName(input: string): string {
  return validName(input.trim().replace(/^#+/, ""), 50, "tag name").split(/\s+/).join("-");
}
