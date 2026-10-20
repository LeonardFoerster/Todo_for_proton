import type {
  AuthInfoDto,
  AuthStepDto,
  ChangedEventDto,
  CreateTaskDto,
  MoveListTargetDto,
  MoveTargetDto,
  NoteConflictDto,
  NoteDto,
  NoteFolderDto,
  NoteHitDto,
  NoteItemDto,
  NoteKindDto,
  NoteTargetDto,
  NoteTrashEntryDto,
  NoteTreeItemDto,
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

/** Anmeldung beim Sync-Backend (anbieterneutral, Login im Browser). */
export interface AuthClient {
  /** Anbietername + Pflichthinweis; `available: false` → Login hier nicht möglich. */
  info(): Promise<AuthInfoDto>;
  current(): Promise<SessionDto | null>;
  /** Startet den Login; die Shell öffnet den Browser selbst. */
  beginLogin(): Promise<AuthStepDto>;
  /** Öffnet die Login-Seite eines laufenden Vorgangs erneut. */
  openBrowser(flow: string): Promise<void>;
  /** Alle `pollIntervalS` Sekunden aufrufen, bis `done` oder Fehler. */
  pollLogin(flow: string): Promise<AuthStepDto>;
  cancelLogin(flow: string): Promise<void>;
  logout(): Promise<void>;
}

export interface SyncClient {
  status(): Promise<SyncStatusDto>;
  /** Sync sofort anstoßen (Ergebnis kommt über `onSyncStatus`). */
  now(): Promise<void>;
  /** Hochladen, falls etwas wartet – beim Wechsel der Notiz oder des Modus (ADR-029). */
  flush(): Promise<void>;
}

/**
 * Notizen und Ordner (Modus „Notes", ADR-029). Schreibende Methoden lösen `onChange` mit den
 * geschriebenen `notes`/`noteFolders` aus. Elemente im Papierkorb sind nur lesbar.
 */
export interface NotesClient {
  /** Alle Ordner (auch im Papierkorb), nach `sortKey` – für Pfade und „Verschieben nach…". */
  listFolders(): Promise<NoteFolderDto[]>;
  /** Inhalt eines Ordners (`null` = Wurzel): Ordner und Notizen gemischt, ohne Papierkorb. */
  listItems(folderId: string | null, page: PageRequestDto): Promise<PageDto<NoteTreeItemDto>>;
  get(id: string): Promise<NoteDto>;
  /** Leere Notiz „Unbenannt" am Ende des Ordners. */
  create(folderId: string | null, kind: NoteKindDto): Promise<NoteDto>;
  /**
   * Autosave: `baseRev` = zuletzt geliefertes `bodyRev` (je Notiz nacheinander speichern).
   * Die Antwort trägt den aufgelösten Inhalt – nach einem Remote-Merge weicht er vom Text ab.
   */
  updateBody(id: string, text: string, baseRev: string | null): Promise<NoteDto>;
  /** Leer verlassene neue Notiz verwerfen; `true` = verworfen. */
  discardIfEmpty(id: string): Promise<boolean>;
  rename(id: string, name: string): Promise<NoteDto>;
  setKind(id: string, kind: NoteKindDto): Promise<NoteDto>;
  move(id: string, target: NoteTargetDto): Promise<NoteDto>;
  createFolder(parentId: string | null, name: string): Promise<NoteFolderDto>;
  renameFolder(id: string, name: string): Promise<NoteFolderDto>;
  /** `target.folderId` = neuer Elternordner. */
  moveFolder(id: string, target: NoteTargetDto): Promise<NoteFolderDto>;
  /** In den Papierkorb; Ergebnis = tatsächlich geänderte (für „Rückgängig" per `restore`). */
  trash(items: NoteItemDto[]): Promise<NoteItemDto[]>;
  restore(items: NoteItemDto[]): Promise<NoteItemDto[]>;
  /** Endgültig löschen (samt Inhalt); Zahl gelöschter Elemente. */
  purge(items: NoteItemDto[]): Promise<number>;
  emptyTrash(): Promise<number>;
  /** Oberste Elemente im Papierkorb, neueste zuerst. */
  listTrash(): Promise<NoteTrashEntryDto[]>;
  /** Suche über Name und Inhalt (jedes Wort; nur Wörter < 3 Zeichen: nur Namen). */
  search(text: string, limit: number): Promise<NoteHitDto[]>;
  /** Beim Zusammenlegen nebenläufiger Bearbeitungen entstandene Konfliktkopien. */
  onConflict(cb: (conflicts: NoteConflictDto[]) => void): () => void;
}

/**
 * Einzige Schnittstelle der UI zum Core. Alle Methoden lehnen mit `CoreClientError` ab.
 * Schreibende Methoden lösen zusätzlich `onChange` mit genau den geschriebenen IDs aus.
 */
export interface CoreClient {
  ensureInbox(): Promise<TaskListDto>;
  listTaskLists(): Promise<TaskListDto[]>;
  createTaskList(name: string): Promise<TaskListDto>;
  renameTaskList(id: string, name: string): Promise<TaskListDto>;
  /** Liste unter den Benutzerlisten umsortieren (Eingang bleibt vorn, ADR-023). */
  moveTaskList(id: string, target: MoveListTargetDto): Promise<TaskListDto>;
  /** Liefert die Anzahl der in die Inbox verschobenen Tasks. */
  deleteTaskList(id: string): Promise<number>;

  createTask(input: CreateTaskDto): Promise<TaskDto>;
  getTask(id: string): Promise<TaskDto>;
  updateTask(id: string, patch: TaskPatchDto): Promise<TaskDto>;
  completeTask(id: string, done: boolean): Promise<TaskDto>;
  moveTask(id: string, target: MoveTargetDto): Promise<TaskDto>;
  deleteTask(id: string): Promise<void>;
  listTasks(query: TaskQueryDto, page: PageRequestDto): Promise<PageDto<TaskSummaryDto>>;
  /** Suche über Titel und Notizen (ADR-027): jedes Wort als Teilwort; offene zuerst, Titeltreffer vor Notiztreffern. */
  searchTasks(text: string, limit: number): Promise<TaskSummaryDto[]>;

  // Sammelaktionen (ADR-021): ein Aufruf, ein Commit, ein `onChange`. Unbekannte IDs werden
  // übersprungen; Ergebnis = tatsächlich geänderte Tasks. Höchstens 2 000 je Aufruf.
  /** Abhaken bzw. wieder öffnen; Tasks im Zielzustand fehlen im Ergebnis. */
  completeTasks(ids: string[], done: boolean): Promise<TaskDto[]>;
  /** Liefert die gelöschten Tasks im Zustand davor (für `restoreTasks`). */
  deleteTasks(ids: string[]): Promise<TaskDto[]>;
  /** Verschiebt die Tasks als Block in bisheriger Reihenfolge; ohne `listId` in ihre gemeinsame Liste. */
  moveTasks(ids: string[], target: MoveTargetDto): Promise<TaskDto[]>;
  /** Kopien gelöschter Tasks mit **neuen** IDs, alle Felder, alte Position (Rückgängig). */
  restoreTasks(tasks: TaskDto[]): Promise<TaskDto[]>;
  /** Je Task eine eigene Fälligkeit (`null` = entfernen); Tasks mit diesem Wert fehlen im Ergebnis. */
  setDues(dues: TaskDueDto[]): Promise<TaskDto[]>;

  notes: NotesClient;
  auth: AuthClient;
  sync: SyncClient;

  /** Erste Ansicht ist gerendert (Startzeitmessung, Sync-Start). */
  ready(): Promise<void>;

  /** Gibt die Abmeldung zurück. */
  onChange(cb: (e: ChangedEventDto) => void): () => void;
  onSyncStatus(cb: (s: SyncStatusDto) => void): () => void;
}
