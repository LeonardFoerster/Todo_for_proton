import type { NotesClient } from "./core-client";
import { CoreClientError } from "./errors";
import type {
  NoteConflictDto,
  NoteDto,
  NoteFolderDto,
  NoteHitDto,
  NoteItemDto,
  NoteKindDto,
  NoteSummaryDto,
  NoteTargetDto,
  NoteTrashEntryDto,
  NoteTreeItemDto,
  PageDto,
  PageRequestDto,
} from "./types";

/** Wie im Core (`todo_core::domain`, ADR-029). */
const NAME_MAX = 200;
const AUTO_NAME_MAX = 60;
const UNTITLED = "Unbenannt";
const BODY_MAX_BYTES = 1024 * 1024;
const MAX_DEPTH = 32;
const PAGE_LIMIT_MAX = 500;
const BULK_LIMIT_MAX = 2000;
const SEARCH_TEXT_MAX = 200;
const MIN_BODY_TERM = 3;
const SNIPPET_CONTEXT = 40;

/** Was der Fake von `FakeCoreClient` braucht. */
export interface FakeNotesEnv {
  run<T>(fn: () => T): Promise<T>;
  emit(notes: string[], noteFolders: string[]): void;
  now(): Date;
  newId(): string;
}

interface FolderRow {
  folder: NoteFolderDto;
  order: number;
}

interface NoteRow {
  note: NoteDto;
  order: number;
}

type Row = { kind: "folder"; row: FolderRow } | { kind: "note"; row: NoteRow };

/** In-Memory-Notizen mit der Semantik des Cores (ohne Sync und Merge). */
export class FakeNotes implements NotesClient {
  private folders = new Map<string, FolderRow>();
  private notes = new Map<string, NoteRow>();
  private conflictListeners = new Set<(c: NoteConflictDto[]) => void>();
  private rev = 0;
  private readonly device = "00000000-0000-7000-8000-00000000fa4e";

  constructor(private readonly env: FakeNotesEnv) {}

  listFolders = () =>
    this.env.run(() =>
      [...this.folders.values()]
        .sort((a, b) => cmp(a.folder.sortKey, b.folder.sortKey) || cmp(a.folder.id, b.folder.id))
        .map((r) => ({ ...r.folder })),
    );

  listItems = (folderId: string | null, page: PageRequestDto) =>
    this.env.run((): PageDto<NoteTreeItemDto> => {
      checkLimit(page.limit);
      if (folderId !== null) this.folder(folderId);
      const items = this.children(folderId).filter((r) => !trashedSelf(r));
      const start = page.cursor ? parseCursor(page.cursor) : 0;
      const slice = items.slice(start, start + page.limit);
      const next = start + page.limit < items.length ? String(start + page.limit) : null;
      return { items: slice.map((r) => this.item(r)), nextCursor: next };
    });

  get = (id: string) => this.env.run(() => ({ ...this.note(id).note }));

  create = (folderId: string | null, kind: NoteKindDto) =>
    this.env.run(() => {
      this.checkPlace(folderId);
      const at = this.iso();
      const row: NoteRow = {
        order: this.lastOrder(folderId) + 1,
        note: {
          id: this.env.newId(),
          folderId,
          name: UNTITLED,
          autoName: true,
          kind,
          body: "",
          bodyRev: null,
          sortKey: "",
          createdAt: at,
          modifiedAt: at,
          trashedAt: null,
        },
      };
      row.note.sortKey = orderKey(row.order);
      this.notes.set(row.note.id, row);
      this.env.emit([row.note.id], []);
      return { ...row.note };
    });

  updateBody = (id: string, text: string, _baseRev: string | null) =>
    this.env.run(() => {
      const row = this.note(id);
      this.checkEditable(row.note);
      const body = text.replace(/\r\n?/g, "\n");
      if (new TextEncoder().encode(body).length > BODY_MAX_BYTES) {
        throw new CoreClientError("validation", "validation: note body must be at most 1048576 bytes");
      }
      this.setBody(row, body);
      this.env.emit([id], []);
      return { ...row.note };
    });

  discardIfEmpty = (id: string) =>
    this.env.run(() => {
      const row = this.notes.get(id);
      if (!row || row.note.body !== "" || !row.note.autoName) return false;
      this.notes.delete(id);
      this.env.emit([id], []);
      return true;
    });

  rename = (id: string, name: string) =>
    this.env.run(() => {
      const row = this.note(id);
      this.checkEditable(row.note);
      const valid = validName(name);
      if (valid !== row.note.name || row.note.autoName) {
        row.note.name = valid;
        row.note.autoName = false;
        row.note.modifiedAt = this.iso();
        this.env.emit([id], []);
      }
      return { ...row.note };
    });

  setKind = (id: string, kind: NoteKindDto) =>
    this.env.run(() => {
      const row = this.note(id);
      this.checkEditable(row.note);
      if (row.note.kind !== kind) {
        row.note.kind = kind;
        this.env.emit([id], []);
      }
      return { ...row.note };
    });

  move = (id: string, target: NoteTargetDto) =>
    this.env.run(() => {
      const row = this.note(id);
      this.checkEditable(row.note);
      const folderId = target.folderId ?? null;
      this.checkPlace(folderId);
      row.order = this.position(folderId, { type: "note", id }, target);
      row.note.folderId = folderId;
      row.note.sortKey = orderKey(row.order);
      this.env.emit([id], []);
      return { ...row.note };
    });

  createFolder = (parentId: string | null, name: string) =>
    this.env.run(() => {
      this.checkPlace(parentId);
      if (this.depth(parentId) + 1 > MAX_DEPTH) {
        throw new CoreClientError("validation", `validation: folders can be nested at most ${MAX_DEPTH} levels deep`);
      }
      const row: FolderRow = {
        order: this.lastOrder(parentId) + 1,
        folder: {
          id: this.env.newId(),
          parentId,
          name: validName(name),
          sortKey: "",
          createdAt: this.iso(),
          trashedAt: null,
        },
      };
      row.folder.sortKey = orderKey(row.order);
      this.folders.set(row.folder.id, row);
      this.env.emit([], [row.folder.id]);
      return { ...row.folder };
    });

  renameFolder = (id: string, name: string) =>
    this.env.run(() => {
      const row = this.folder(id);
      this.checkPlace(id);
      const valid = validName(name);
      if (valid !== row.folder.name) {
        row.folder.name = valid;
        this.env.emit([], [id]);
      }
      return { ...row.folder };
    });

  moveFolder = (id: string, target: NoteTargetDto) =>
    this.env.run(() => {
      const row = this.folder(id);
      this.checkPlace(id);
      const parentId = target.folderId ?? null;
      this.checkPlace(parentId);
      if (parentId !== null && (parentId === id || this.ancestors(parentId).includes(id))) {
        throw new CoreClientError(
          "validation",
          "validation: a folder cannot be moved into itself or one of its subfolders",
        );
      }
      if (this.depth(parentId) + this.height(id) > MAX_DEPTH) {
        throw new CoreClientError("validation", `validation: folders can be nested at most ${MAX_DEPTH} levels deep`);
      }
      row.order = this.position(parentId, { type: "folder", id }, target);
      row.folder.parentId = parentId;
      row.folder.sortKey = orderKey(row.order);
      this.env.emit([], [id]);
      return { ...row.folder };
    });

  trash = (items: NoteItemDto[]) =>
    this.env.run(() => {
      checkBulk(items.length);
      const at = this.iso();
      const changed = unique(items).filter((item) => {
        const target = item.type === "note" ? this.notes.get(item.id)?.note : this.folders.get(item.id)?.folder;
        if (!target || target.trashedAt) return false;
        target.trashedAt = at;
        return true;
      });
      this.emitItems(changed);
      return changed;
    });

  restore = (items: NoteItemDto[]) =>
    this.env.run(() => {
      checkBulk(items.length);
      const list = unique(items);
      // Erst die Ordner, dann prüfen, ob die Elemente ein Zuhause haben (wie im Core).
      const restoring = new Set(list.filter((i) => i.type === "folder").map((i) => i.id));
      const homeless = (folderId: string | null) =>
        folderId !== null &&
        [folderId, ...this.ancestors(folderId)].some(
          (f) => !restoring.has(f) && this.folders.get(f)?.folder.trashedAt != null,
        );
      const changed: NoteItemDto[] = [];
      for (const item of list) {
        const row = item.type === "note" ? this.notes.get(item.id) : this.folders.get(item.id);
        if (!row) continue;
        const dto = "note" in row ? row.note : row.folder;
        const parent = "note" in row ? row.note.folderId : row.folder.parentId;
        const lost = homeless(parent);
        if (!dto.trashedAt && !lost) continue;
        dto.trashedAt = null;
        if (lost) {
          row.order = this.lastOrder(null) + 1;
          dto.sortKey = orderKey(row.order);
          if ("note" in row) row.note.folderId = null;
          else row.folder.parentId = null;
        }
        changed.push(item);
      }
      this.emitItems(changed);
      return changed;
    });

  purge = (items: NoteItemDto[]) =>
    this.env.run(() => {
      checkBulk(items.length);
      return this.purgeItems(unique(items));
    });

  emptyTrash = () =>
    this.env.run(() => {
      const items: NoteItemDto[] = [
        ...[...this.folders.values()].filter((r) => r.folder.trashedAt).map((r) => ({ type: "folder" as const, id: r.folder.id })),
        ...[...this.notes.values()].filter((r) => r.note.trashedAt).map((r) => ({ type: "note" as const, id: r.note.id })),
      ];
      return this.purgeItems(items);
    });

  listTrash = () =>
    this.env.run((): NoteTrashEntryDto[] => {
      const entries: NoteTrashEntryDto[] = [];
      for (const r of this.folders.values()) {
        if (r.folder.trashedAt && !this.inTrash(r.folder.parentId)) {
          entries.push({ item: { type: "folder", ...r.folder }, path: this.path(r.folder.parentId), trashedAt: r.folder.trashedAt });
        }
      }
      for (const r of this.notes.values()) {
        if (r.note.trashedAt && !this.inTrash(r.note.folderId)) {
          entries.push({ item: { type: "note", ...summary(r) }, path: this.path(r.note.folderId), trashedAt: r.note.trashedAt });
        }
      }
      return entries.sort((a, b) => cmp(b.trashedAt, a.trashedAt));
    });

  search = (text: string, limit: number) =>
    this.env.run((): NoteHitDto[] => {
      checkLimit(limit);
      if ([...text].length > SEARCH_TEXT_MAX) {
        throw new CoreClientError("validation", `validation: search text longer than ${SEARCH_TEXT_MAX} characters`);
      }
      const terms = text.toLowerCase().split(/\s+/).filter(Boolean);
      if (terms.length === 0) return [];
      const namesOnly = terms.every((t) => [...t].length < MIN_BODY_TERM);
      const hits: NoteHitDto[] = [];
      for (const r of this.notes.values()) {
        if (r.note.trashedAt || this.inTrash(r.note.folderId)) continue;
        const name = r.note.name.toLowerCase();
        const body = r.note.body.toLowerCase();
        const all = terms.every((t) => name.includes(t) || (!namesOnly && body.includes(t)));
        if (!all) continue;
        const nameHit = terms.some((t) => name.includes(t));
        const first = namesOnly
          ? null
          : terms
              .map((t) => ({ at: body.indexOf(t), length: t.length }))
              .filter((m) => m.at >= 0)
              .sort((a, b) => a.at - b.at)[0];
        hits.push({
          note: summary(r),
          path: this.path(r.note.folderId),
          nameHit,
          snippet: first ? snippet(r.note.body, first.at, first.length) : null,
        });
      }
      return hits
        .sort(
          (a, b) =>
            Number(b.nameHit) - Number(a.nameHit) ||
            cmp(b.note.modifiedAt, a.note.modifiedAt) ||
            cmp(a.note.id, b.note.id),
        )
        .slice(0, limit);
    });

  onConflict = (cb: (c: NoteConflictDto[]) => void) => {
    this.conflictListeners.add(cb);
    return () => void this.conflictListeners.delete(cb);
  };

  /** Test-Hilfe: eine Revision „von einem anderen Gerät" kommt an (neuer Text, neues `bodyRev`). */
  simulateRemoteBody(id: string, text: string) {
    const row = this.note(id);
    this.setBody(row, text);
    this.env.emit([id], []);
  }

  /** Test-Hilfe: Konfliktkopien melden wie der Sync. */
  simulateConflict(conflicts: NoteConflictDto[]) {
    queueMicrotask(() => {
      for (const cb of this.conflictListeners) cb(conflicts.map((c) => ({ ...c })));
    });
  }

  // --- intern ----------------------------------------------------------------------------------

  private setBody(row: NoteRow, body: string) {
    if (body !== row.note.body) {
      row.note.body = body;
      row.note.modifiedAt = this.iso();
    }
    row.note.bodyRev = `${this.device}.${++this.rev}`;
    if (row.note.autoName) row.note.name = autoName(body);
  }

  private purgeItems(items: NoteItemDto[]): number {
    const folders = new Set<string>();
    const notes = new Set<string>();
    for (const item of items) {
      if (item.type === "note") {
        if (this.notes.has(item.id)) notes.add(item.id);
      } else if (this.folders.has(item.id)) {
        folders.add(item.id);
        for (const f of this.descendants(item.id)) folders.add(f);
      }
    }
    for (const r of this.notes.values()) {
      if (r.note.folderId !== null && folders.has(r.note.folderId)) notes.add(r.note.id);
    }
    for (const id of notes) this.notes.delete(id);
    for (const id of folders) this.folders.delete(id);
    if (notes.size + folders.size > 0) this.env.emit([...notes], [...folders]);
    return notes.size + folders.size;
  }

  private emitItems(items: NoteItemDto[]) {
    if (items.length === 0) return;
    this.env.emit(
      items.filter((i) => i.type === "note").map((i) => i.id),
      items.filter((i) => i.type === "folder").map((i) => i.id),
    );
  }

  private children(folderId: string | null): Row[] {
    const rows: Row[] = [
      ...[...this.folders.values()].filter((r) => r.folder.parentId === folderId).map((row) => ({ kind: "folder" as const, row })),
      ...[...this.notes.values()].filter((r) => r.note.folderId === folderId).map((row) => ({ kind: "note" as const, row })),
    ];
    return rows.sort((a, b) => a.row.order - b.row.order || cmp(rowId(a), rowId(b)));
  }

  private item(r: Row): NoteTreeItemDto {
    return r.kind === "folder" ? { type: "folder", ...r.row.folder } : { type: "note", ...summary(r.row) };
  }

  /** Reihenfolgewert für eine Position (Semantik wie `NoteTargetDto`). */
  private position(folderId: string | null, moving: NoteItemDto, target: NoteTargetDto): number {
    const same = (a: NoteItemDto | null | undefined, b: NoteItemDto) => a != null && a.type === b.type && a.id === b.id;
    if (same(target.after, moving) || same(target.before, moving)) {
      throw new CoreClientError("validation", "validation: an item cannot be its own neighbour");
    }
    const siblings = this.children(folderId).filter((r) => !(r.kind === moving.type && rowId(r) === moving.id));
    const index = (n: NoteItemDto | null | undefined) => {
      if (n == null) return -1;
      const i = siblings.findIndex((r) => r.kind === n.type && rowId(r) === n.id);
      if (i < 0) throw new CoreClientError("validation", "validation: neighbour is not in the folder");
      return i;
    };
    const ai = index(target.after);
    const bi = index(target.before);
    const order = (i: number) => siblings[i].row.order;
    if (target.after != null && target.before != null) return (order(ai) + order(bi)) / 2;
    if (target.after != null) return ai + 1 < siblings.length ? (order(ai) + order(ai + 1)) / 2 : order(ai) + 1;
    if (target.before != null) return bi > 0 ? (order(bi - 1) + order(bi)) / 2 : order(bi) - 1;
    return (siblings.at(-1)?.row.order ?? 0) + 1;
  }

  private lastOrder(folderId: string | null): number {
    return this.children(folderId).at(-1)?.row.order ?? 0;
  }

  private ancestors(folderId: string): string[] {
    const out: string[] = [];
    let current = this.folders.get(folderId)?.folder.parentId ?? null;
    while (current !== null && !out.includes(current)) {
      out.push(current);
      current = this.folders.get(current)?.folder.parentId ?? null;
    }
    return out;
  }

  private descendants(folderId: string): string[] {
    const out: string[] = [];
    const queue = [folderId];
    while (queue.length > 0) {
      const id = queue.pop()!;
      for (const r of this.folders.values()) {
        if (r.folder.parentId === id && !out.includes(r.folder.id)) {
          out.push(r.folder.id);
          queue.push(r.folder.id);
        }
      }
    }
    return out;
  }

  private depth(folderId: string | null): number {
    return folderId === null ? 0 : this.ancestors(folderId).length + 1;
  }

  private height(folderId: string): number {
    const below = this.descendants(folderId).map((d) => this.depth(d) - this.depth(folderId));
    return Math.max(0, ...below) + 1;
  }

  private inTrash(folderId: string | null): boolean {
    if (folderId === null) return false;
    return [folderId, ...this.ancestors(folderId)].some((f) => this.folders.get(f)?.folder.trashedAt != null);
  }

  private checkPlace(folderId: string | null) {
    if (folderId !== null) this.folder(folderId);
    if (this.inTrash(folderId)) {
      throw new CoreClientError("validation", "validation: items in the trash cannot be changed; restore them first");
    }
  }

  private checkEditable(note: NoteDto) {
    if (note.trashedAt || this.inTrash(note.folderId)) {
      throw new CoreClientError("validation", "validation: notes in the trash are read-only; restore them first");
    }
  }

  private path(folderId: string | null): string[] {
    if (folderId === null) return [];
    return [...this.ancestors(folderId).reverse(), folderId].map((f) => this.folders.get(f)?.folder.name ?? "");
  }

  private note(id: string): NoteRow {
    const row = this.notes.get(id);
    if (!row) throw new CoreClientError("notFound", `not found: note ${id}`);
    return row;
  }

  private folder(id: string): FolderRow {
    const row = this.folders.get(id);
    if (!row) throw new CoreClientError("notFound", `not found: folder ${id}`);
    return row;
  }

  private iso(): string {
    return this.env.now().toISOString();
  }
}

function rowId(r: Row): string {
  return r.kind === "folder" ? r.row.folder.id : r.row.note.id;
}

function trashedSelf(r: Row): boolean {
  return (r.kind === "folder" ? r.row.folder.trashedAt : r.row.note.trashedAt) != null;
}

function summary(r: NoteRow): NoteSummaryDto {
  const n = r.note;
  return {
    id: n.id,
    folderId: n.folderId,
    name: n.name,
    kind: n.kind,
    sortKey: n.sortKey,
    createdAt: n.createdAt,
    modifiedAt: n.modifiedAt,
    trashedAt: n.trashedAt,
    size: new TextEncoder().encode(n.body).length,
  };
}

function unique(items: NoteItemDto[]): NoteItemDto[] {
  const seen = new Set<string>();
  return items.filter((i) => {
    const key = `${i.type}:${i.id}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function checkBulk(n: number) {
  if (n > BULK_LIMIT_MAX) {
    throw new CoreClientError("validation", `validation: at most ${BULK_LIMIT_MAX} items per call, got ${n}`);
  }
}

function checkLimit(limit: number) {
  if (!(limit >= 1 && limit <= PAGE_LIMIT_MAX)) {
    throw new CoreClientError("validation", `validation: page limit must be 1..=${PAGE_LIMIT_MAX}, got ${limit}`);
  }
}

function parseCursor(cursor: string): number {
  const n = Number(cursor);
  if (!Number.isInteger(n) || n < 0) throw new CoreClientError("validation", `validation: invalid cursor: ${cursor}`);
  return n;
}

function cmp(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

function orderKey(order: number): string {
  return (order + 1e9).toFixed(9).padStart(24, "0");
}

/** Wie `NoteName::parse`: Steuerzeichen raus, 1–200 Zeichen, ohne `/`, `\`, führenden Punkt. */
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/g;

export function validName(input: string): string {
  const name = input.replace(/[\t\n\r\v\f]/g, " ").replace(CONTROL, "").trim();
  if (!name) throw new CoreClientError("validation", "validation: note name must not be empty");
  if ([...name].length > NAME_MAX) {
    throw new CoreClientError("validation", `validation: note name must be at most ${NAME_MAX} characters`);
  }
  if (/[/\\]/.test(name)) throw new CoreClientError("validation", "validation: note name must not contain / or \\");
  if (name.startsWith(".")) throw new CoreClientError("validation", "validation: note name must not start with a dot");
  return name;
}

/** Wie `NoteName::from_first_line`. */
export function autoName(body: string): string {
  for (const raw of body.split("\n")) {
    let line = raw.replace(/^[#\-*>+ \t]+/, "");
    for (const box of ["[ ]", "[x]", "[X]"]) {
      if (line.startsWith(box)) line = line.slice(box.length).trimStart();
    }
    if (!line.trim()) continue;
    const cleaned = [...line.replace(/[/\\]/g, "-")].slice(0, AUTO_NAME_MAX).join("");
    try {
      return validName(cleaned.replace(/^[. \t]+/, ""));
    } catch {
      return UNTITLED;
    }
  }
  return UNTITLED;
}

function snippet(body: string, at: number, length: number) {
  const from = Math.max(0, at - SNIPPET_CONTEXT);
  const to = Math.min(body.length, at + length + SNIPPET_CONTEXT);
  let text = body.slice(from, to).replace(/\s/g, " ").trim();
  if (from > 0) text = `…${text}`;
  if (to < body.length) text = `${text}…`;
  return { text, offset: at, length };
}
