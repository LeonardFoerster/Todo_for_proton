/* eslint-disable svelte/prefer-svelte-reactivity -- Maps/Sets werden nie verändert, sondern ersetzt (`$state.raw`); lokale Hilfsmengen sind nicht reaktiv. */
/**
 * Zustand des Modus „Notes" (`notes.md` §5): geladener Baum, aufgeklappte Ordner, offene Notiz,
 * Papierkorb. Ordnerinhalte werden erst beim Aufklappen geladen und über `onChange` aktuell
 * gehalten. Pro Gerät gemerkt: `notes.open`, `notes.expanded` (`localStorage`, Fehler egal).
 */
import type {
  ChangedEventDto,
  CoreClient,
  NoteDto,
  NoteFolderDto,
  NoteTrashEntryDto,
  NoteTreeItemDto,
} from "@tasks/core-client";
import { toasts } from "../../components/toasts.svelte";
import { defaultKind, readZen, writeZen } from "../../lib/note-preferences";
import { flatten, ROOT, type TreeRow } from "./tree-model";

export const OPEN_KEY = "notes.open";
export const EXPANDED_KEY = "notes.expanded";
const PAGE = 500;

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

function writeJson(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // egal – gilt dann nur bis zum Neustart
  }
}

export class NotesState {
  /** Alle Ordner (auch im Papierkorb) – für Pfade und „Verschieben nach…". */
  folders = $state.raw<NoteFolderDto[]>([]);
  /** Geladene Inhalte je Ordner (`ROOT` = Wurzel), ohne Papierkorb. */
  children = $state.raw<Map<string, NoteTreeItemDto[]>>(new Map());
  expanded = $state.raw<Set<string>>(new Set(readJson<string[]>(EXPANDED_KEY, [])));
  openId = $state<string | null>(readJson<string | null>(OPEN_KEY, null));
  /** Hauptbereich zeigt den Papierkorb statt des Editors. */
  showTrash = $state(false);
  trash = $state.raw<NoteTrashEntryDto[]>([]);
  /** Fokussierte Zeile im Baum (Schlüssel aus `itemKey`). */
  cursor = $state<string | null>(null);
  loaded = $state(false);
  /** Gerade angelegt und noch nicht beschrieben: beim Verlassen verwerfen (§5.3). */
  fresh = $state<string | null>(null);
  /** Bitte, den Editor zu fokussieren (Enter im Baum, neue Notiz); der Editor löst sie einmal ein. */
  editorFocus = $state(false);
  /** Auswahl, die der Editor beim Öffnen dieser Notiz setzt (Sprung zum Suchtreffer). */
  select = $state<{ id: string; start: number; end: number } | null>(null);
  /** Zen-Modus: Seitenleiste aus, Statuszeile nur bei Bewegung (`notes.md` §6.1). */
  zen = $state(readZen());
  readonly rows: TreeRow[] = $derived(flatten(this.children, this.expanded));

  private pending: Promise<void> | null = null;
  private dirty = new Set<string>();
  private full = false;

  constructor(readonly client: CoreClient) {}

  /** Erste Ladung und Abo auf Änderungen; gibt die Abmeldung zurück. */
  start(): () => void {
    void this.reload();
    const offChange = this.client.onChange((e) => this.changed(e));
    const offConflict = this.client.notes.onConflict((conflicts) => void this.conflicted(conflicts));
    return () => {
      offChange();
      offConflict();
    };
  }

  /** Konfliktkopie angelegt (Sync oder eigenes Speichern): Hinweis mit „Anzeigen" (§6.3). */
  private async conflicted(conflicts: { noteId: string; copyId: string }[]) {
    await this.reload();
    for (const { copyId } of conflicts) {
      const copy = await this.client.notes.get(copyId).catch(() => null);
      if (!copy) continue;
      toasts.show({
        message: `Konflikt – Kopie „${copy.name}“ angelegt`,
        actionLabel: "Anzeigen",
        timeoutMs: 8000,
        onAction: () => {
          void this.reveal(copy.folderId);
          this.open(copy.id);
        },
      });
    }
  }

  folder(id: string | null): NoteFolderDto | undefined {
    return id === null ? undefined : this.folders.find((f) => f.id === id);
  }

  /** Liegt die Notiz selbst oder über einen Ordner im Papierkorb (dann nur lesbar)? */
  isTrashed(note: { trashedAt: string | null; folderId: string | null }): boolean {
    if (note.trashedAt) return true;
    const seen = new Set<string>();
    let current = this.folder(note.folderId);
    while (current && !seen.has(current.id)) {
      if (current.trashedAt) return true;
      seen.add(current.id);
      current = this.folder(current.parentId);
    }
    return false;
  }

  /** Ordnernamen von der Wurzel bis `folderId` (einschließlich). */
  path(folderId: string | null): string[] {
    const out: string[] = [];
    let current = this.folder(folderId);
    const seen = new Set<string>();
    while (current && !seen.has(current.id)) {
      seen.add(current.id);
      out.unshift(current.name);
      current = this.folder(current.parentId);
    }
    return out;
  }

  /** Ordner und alle Vorfahren aufklappen (z. B. um eine geöffnete Notiz zu zeigen). */
  async reveal(folderId: string | null) {
    const chain: string[] = [];
    let current = this.folder(folderId);
    while (current && !chain.includes(current.id)) {
      chain.unshift(current.id);
      current = this.folder(current.parentId);
    }
    for (const id of chain) await this.expand(id);
  }

  async expand(folderId: string) {
    if (this.expanded.has(folderId)) return;
    this.expanded = new Set(this.expanded).add(folderId);
    writeJson(EXPANDED_KEY, [...this.expanded]);
    if (!this.children.has(folderId)) await this.loadFolder(folderId);
  }

  collapse(folderId: string) {
    if (!this.expanded.has(folderId)) return;
    const next = new Set(this.expanded);
    next.delete(folderId);
    this.expanded = next;
    writeJson(EXPANDED_KEY, [...next]);
  }

  async toggle(folderId: string) {
    if (this.expanded.has(folderId)) this.collapse(folderId);
    else await this.expand(folderId);
  }

  /** Notiz öffnen; eine leer verlassene neue Notiz wird verworfen. */
  open(id: string | null) {
    const previous = this.openId;
    if (previous !== id && this.fresh === previous && previous) {
      this.fresh = null;
      void this.client.notes.discardIfEmpty(previous).catch(() => false);
    }
    this.openId = id;
    this.showTrash = false;
    writeJson(OPEN_KEY, id);
    if (previous !== id) void this.client.sync.flush().catch(() => undefined);
  }

  focusEditor() {
    this.editorFocus = true;
  }

  /** Ergebnis des letzten Imports (Dialog); `null` = keiner offen. */
  importReport = $state.raw<import("@tasks/core-client").ImportReportDto | null>(null);

  /** „Ordner importieren…" (nur Desktop): wählen, importieren, Ergebnis zeigen, Ordner zeigen. */
  async importFolder() {
    if (!this.client.notes.importFolder) return;
    try {
      const report = await this.client.notes.importFolder();
      if (!report) return;
      await this.reload();
      await this.expand(report.folderId);
      this.cursor = `f:${report.folderId}`;
      this.importReport = report;
    } catch (e) {
      toasts.show({ message: `Import fehlgeschlagen: ${(e as Error).message}`, tone: "danger" });
    }
  }

  setZen(on: boolean) {
    this.zen = on;
    writeZen(on);
  }

  /** Neue Notiz im Ordner der offenen Notiz (sonst der Wurzel); öffnet sie. */
  async createNote(folderId?: string | null): Promise<NoteDto> {
    const target = folderId !== undefined ? folderId : this.openFolder();
    const note = await this.client.notes.create(target, defaultKind());
    if (target) await this.reveal(target);
    this.open(note.id);
    this.fresh = note.id;
    this.cursor = `n:${note.id}`;
    this.focusEditor();
    await this.reload();
    return note;
  }

  /** Ordner der offenen Notiz, sofern sie im geladenen Baum steht. */
  openFolder(): string | null {
    const row = this.rows.find((r) => r.key === `n:${this.openId}`);
    return row?.parentId ?? null;
  }

  /** Lädt Ordner, Papierkorb und alle aufgeklappten Ordner neu (gebündelt). */
  reload(): Promise<void> {
    this.full = true;
    return this.schedule();
  }

  /** Nach `onChange`: betroffene Ordner neu laden; bei Ordneränderungen oder Unbekanntem alles. */
  private changed(e: ChangedEventDto) {
    if (e.notes.length === 0 && e.noteFolders.length === 0) return;
    if (e.noteFolders.length > 0) this.full = true;
    for (const id of e.notes) {
      const row = this.rows.find((r) => r.key === `n:${id}`);
      if (!row) this.full = true;
      else this.dirty.add(row.parentId ?? ROOT);
    }
    void this.schedule();
  }

  private schedule(): Promise<void> {
    this.pending ??= Promise.resolve().then(async () => {
      this.pending = null;
      const full = this.full;
      const dirty = [...this.dirty];
      this.full = false;
      this.dirty.clear();
      try {
        await (full ? this.loadAll() : this.loadFolders(dirty));
      } catch {
        // Nächste Änderung lädt erneut; die UI zeigt den letzten Stand.
      }
    });
    return this.pending;
  }

  private async loadAll() {
    const [folders, trash] = await Promise.all([this.client.notes.listFolders(), this.client.notes.listTrash()]);
    this.folders = folders;
    this.trash = trash;
    const visible = new Set(folders.filter((f) => !f.trashedAt).map((f) => f.id));
    const expanded = [...this.expanded].filter((id) => visible.has(id));
    if (expanded.length !== this.expanded.size) {
      this.expanded = new Set(expanded);
      writeJson(EXPANDED_KEY, expanded);
    }
    const next = new Map<string, NoteTreeItemDto[]>();
    const lists = await Promise.all([null, ...expanded].map((id) => this.fetch(id)));
    [ROOT, ...expanded].forEach((key, i) => next.set(key, lists[i]));
    this.children = next;
    this.loaded = true;
  }

  private async loadFolders(keys: string[]) {
    const [lists, trash] = await Promise.all([
      Promise.all(keys.map((k) => this.fetch(k === ROOT ? null : k))),
      this.client.notes.listTrash(),
    ]);
    const next = new Map(this.children);
    keys.forEach((k, i) => next.set(k, lists[i]));
    this.children = next;
    this.trash = trash;
  }

  private async loadFolder(folderId: string) {
    const items = await this.fetch(folderId);
    this.children = new Map(this.children).set(folderId, items);
  }

  private async fetch(folderId: string | null): Promise<NoteTreeItemDto[]> {
    const items: NoteTreeItemDto[] = [];
    let cursor: string | null = null;
    do {
      const page = await this.client.notes.listItems(folderId, { cursor, limit: PAGE });
      items.push(...page.items);
      cursor = page.nextCursor;
    } while (cursor);
    return items;
  }
}
