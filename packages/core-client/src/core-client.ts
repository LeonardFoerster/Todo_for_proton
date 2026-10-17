import type {
  AuthInfoDto,
  AuthStepDto,
  ChangedEventDto,
  CreateTaskDto,
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
  /** Liefert die Anzahl der in die Inbox verschobenen Tasks. */
  deleteTaskList(id: string): Promise<number>;

  createTask(input: CreateTaskDto): Promise<TaskDto>;
  getTask(id: string): Promise<TaskDto>;
  updateTask(id: string, patch: TaskPatchDto): Promise<TaskDto>;
  completeTask(id: string, done: boolean): Promise<TaskDto>;
  moveTask(id: string, target: MoveTargetDto): Promise<TaskDto>;
  deleteTask(id: string): Promise<void>;
  listTasks(query: TaskQueryDto, page: PageRequestDto): Promise<PageDto<TaskSummaryDto>>;

  auth: AuthClient;
  sync: SyncClient;

  /** Erste Ansicht ist gerendert (Startzeitmessung, Sync-Start). */
  ready(): Promise<void>;

  /** Gibt die Abmeldung zurück. */
  onChange(cb: (e: ChangedEventDto) => void): () => void;
  onSyncStatus(cb: (s: SyncStatusDto) => void): () => void;
}
