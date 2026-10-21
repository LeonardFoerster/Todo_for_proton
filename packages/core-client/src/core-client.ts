import type {
  AuthInfoDto,
  AuthStepDto,
  BackgroundSettingsDto,
  ChangedEventDto,
  CompleteResultDto,
  CreateTaskDto,
  ExportResultDto,
  ImportDataReportDto,
  ImportReportDto,
  LocalDayDto,
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
  OccurrenceDto,
  PageDto,
  PageRequestDto,
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
  /** Nur Desktop: Ordner wählen (System-Dialog) und als Kopie importieren; `null` = abgebrochen. */
  importFolder?(): Promise<ImportReportDto | null>;
}

/** Nur Desktop (ADR-032): Export und Import über System-Dialoge; `null` = abgebrochen. */
export interface DataClient {
  /** Zielordner wählen und darin `Tasks-Export-<today>/` anlegen (`tasks.json` + Notizen als Dateien). */
  exportAll(today: string): Promise<ExportResultDto | null>;
  /** Eine `tasks.json` wählen und einspielen (neue IDs, Vorhandenes wird übersprungen). */
  importFile(): Promise<ImportDataReportDto | null>;
}

/** Aktion der Shell außerhalb der Oberfläche: Klick auf eine Erinnerung bzw. „Neue Task“ im Tray. */
export type ShellAction = { kind: "openTask"; taskId: string } | { kind: "newTask" };

/** Nur Desktop (ADR-033): Erinnerungen und Hintergrundbetrieb dieses Geräts (nicht synchronisiert). */
export interface BackgroundClient {
  get(): Promise<BackgroundSettingsDto>;
  /** Speichert und wendet an (Erinnerungen, Tray, Autostart); `validation` bei ungültiger Uhrzeit. */
  set(settings: BackgroundSettingsDto): Promise<BackgroundSettingsDto>;
  onShellAction(cb: (action: ShellAction) => void): () => void;
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
  /** Führt `source` in `target` zusammen (ADR-020): Tasks ans Ende, `source` weg; liefert deren Zahl. */
  mergeTaskLists(source: string, target: string): Promise<number>;

  // Tags (ADR-035): Namen ohne `#`, Groß/Klein egal eindeutig; Tasks tragen IDs.
  /** Alle Tags nach Name. */
  listTags(): Promise<TagDto[]>;
  /** Findet den Tag mit diesem Namen oder legt ihn an. */
  ensureTag(name: string): Promise<TagDto>;
  /** Fehler `validation`, wenn der Name schon vergeben ist. */
  renameTag(id: string, name: string): Promise<TagDto>;
  /** Nimmt ihn von allen Tasks; liefert deren Zahl. */
  deleteTag(id: string): Promise<number>;

  // Verknüpfungen Task ↔ Notiz (ADR-036): ID aus beiden Enden; fehlende/gelöschte Enden ausgeblendet.
  /** Schon verknüpft: dieselbe Verknüpfung. */
  linkTaskNote(taskId: string, noteId: string): Promise<LinkDto>;
  unlink(linkId: string): Promise<void>;
  taskLinks(taskId: string): Promise<LinkedNoteDto[]>;
  noteLinks(noteId: string): Promise<LinkedTaskDto[]>;

  createTask(input: CreateTaskDto): Promise<TaskDto>;
  getTask(id: string): Promise<TaskDto>;
  updateTask(id: string, patch: TaskPatchDto): Promise<TaskDto>;
  /** Eine Serie (ADR-028) bleibt offen und rückt vor; Tag = UTC-Datum – die UI nimmt `completeTasks` mit `today`. */
  completeTask(id: string, done: boolean): Promise<TaskDto>;
  moveTask(id: string, target: MoveTargetDto): Promise<TaskDto>;
  deleteTask(id: string): Promise<void>;
  listTasks(query: TaskQueryDto, page: PageRequestDto): Promise<PageDto<TaskSummaryDto>>;
  /** Anzahl der Tasks einer Abfrage (Zähler ohne Seiten, FIX-04). */
  countTasks(query: TaskQueryDto): Promise<number>;
  /** Suche über Titel und Notizen (ADR-027): jedes Wort als Teilwort; offene zuerst, Titeltreffer vor Notiztreffern. */
  searchTasks(text: string, limit: number): Promise<TaskSummaryDto[]>;

  // Sammelaktionen (ADR-021): ein Aufruf, ein Commit, ein `onChange`. Unbekannte IDs werden
  // übersprungen; Ergebnis = tatsächlich geänderte Tasks. Höchstens 2 000 je Aufruf.
  /**
   * Abhaken bzw. wieder öffnen; Tasks im Zielzustand fehlen im Ergebnis. Eine Serie (ADR-028) bleibt
   * offen: Sie bekommt einen erledigten Verlaufseintrag (in `tasks` hinter den übrigen) und ihr nächstes
   * Vorkommen ab `today` (lokaler Tag; ohne = UTC-Datum). `occurrences` dient „Rückgängig“.
   */
  completeTasks(ids: string[], done: boolean, today?: LocalDayDto): Promise<CompleteResultDto>;
  /** „Dieses Mal überspringen“: Serie rückt vor, kein Verlaufseintrag (endet sie, ist sie erledigt). */
  skipOccurrence(id: string, today?: LocalDayDto): Promise<TaskDto>;
  /** Rückgängig nach dem Abhaken einer Serie: Eintrag weg, alte Fälligkeit, wieder offen. */
  undoOccurrence(occurrence: OccurrenceDto): Promise<TaskDto>;
  /** Liefert die gelöschten Tasks im Zustand davor (für `restoreTasks`). */
  deleteTasks(ids: string[]): Promise<TaskDto[]>;
  /** Verschiebt die Tasks als Block in bisheriger Reihenfolge; ohne `listId` in ihre gemeinsame Liste. */
  moveTasks(ids: string[], target: MoveTargetDto): Promise<TaskDto[]>;
  /** Kopien gelöschter Tasks mit **neuen** IDs, alle Felder, alte Position (Rückgängig). */
  restoreTasks(tasks: TaskDto[]): Promise<TaskDto[]>;
  /** Je Task eine eigene Fälligkeit (`null` = entfernen); Tasks mit diesem Wert fehlen im Ergebnis. */
  setDues(dues: TaskDueDto[]): Promise<TaskDto[]>;

  // Papierkorb (ADR-031): „Löschen“ in der UI. Gleiche IDs beim Zurückholen; nur Ansicht `trash`
  // zeigt solche Tasks, alle anderen Ansichten und die Suche blenden sie aus. Nach 30 Tagen weg.
  /** In den Papierkorb; Ergebnis = tatsächlich geänderte (schon gelöschte fehlen). */
  trashTasks(ids: string[]): Promise<TaskDto[]>;
  /** Aus dem Papierkorb zurück an den alten Platz (Rückgängig, „Wiederherstellen“). */
  untrashTasks(ids: string[]): Promise<TaskDto[]>;
  /** Endgültig löschen – nur Tasks im Papierkorb; liefert die Zahl gelöschter. */
  purgeTasks(ids: string[]): Promise<number>;
  /** Papierkorb leeren; liefert die Zahl gelöschter. */
  emptyTaskTrash(): Promise<number>;

  notes: NotesClient;
  auth: AuthClient;
  sync: SyncClient;
  /** Nur Desktop: Export/Import (ADR-032). */
  data?: DataClient;
  /** Nur Desktop: Erinnerungen und Hintergrundbetrieb (ADR-033). */
  background?: BackgroundClient;

  /** Erste Ansicht ist gerendert (Startzeitmessung, Sync-Start). */
  ready(): Promise<void>;

  /**
   * Nur Shells mit eigenem Fenster: Vor dem Schließen läuft `cb` (z. B. offene Notiz speichern),
   * danach schließt die Shell – spätestens nach ihrem Timeout.
   */
  onCloseRequested?(cb: () => Promise<void>): () => void;

  /** Nur Shells: Link (http/https/mailto) im Standardbrowser öffnen statt in der Oberfläche. */
  openExternal?(url: string): Promise<void>;

  /** Gibt die Abmeldung zurück. */
  onChange(cb: (e: ChangedEventDto) => void): () => void;
  onSyncStatus(cb: (s: SyncStatusDto) => void): () => void;
}
