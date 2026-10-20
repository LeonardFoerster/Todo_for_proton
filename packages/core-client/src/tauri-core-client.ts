// Einzige Datei mit `@tauri-apps/*` (ESLint erzwingt das).
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CoreClient } from "./core-client";
import { CoreClientError } from "./errors";
import type { ChangedEventDto, NoteConflictDto, SyncStatusDto } from "./types";

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (e) {
    throw CoreClientError.from(e);
  }
}

/** Abo auf ein Tauri-Event mit synchronem Abmelden (auch vor Abschluss von `listen`). */
function subscribe<T>(event: string, cb: (payload: T) => void): () => void {
  let active = true;
  const unlisten = listen<T>(event, (e) => {
    if (active) cb(e.payload);
  });
  return () => {
    active = false;
    void unlisten.then((stop) => stop());
  };
}

/** `CoreClient` über Tauri-IPC (Commands in `apps/desktop/src-tauri/src/commands.rs`). */
export class TauriCoreClient implements CoreClient {
  ensureInbox: CoreClient["ensureInbox"] = () => invoke("ensure_inbox");
  listTaskLists: CoreClient["listTaskLists"] = () => invoke("list_task_lists");
  createTaskList: CoreClient["createTaskList"] = (name) => invoke("create_task_list", { name });
  renameTaskList: CoreClient["renameTaskList"] = (id, name) =>
    invoke("rename_task_list", { id, name });
  moveTaskList: CoreClient["moveTaskList"] = (id, target) => invoke("move_task_list", { id, target });
  deleteTaskList: CoreClient["deleteTaskList"] = (id) => invoke("delete_task_list", { id });

  createTask: CoreClient["createTask"] = (input) => invoke("create_task", { input });
  getTask: CoreClient["getTask"] = (id) => invoke("get_task", { id });
  updateTask: CoreClient["updateTask"] = (id, patch) => invoke("update_task", { id, patch });
  completeTask: CoreClient["completeTask"] = (id, done) => invoke("complete_task", { id, done });
  moveTask: CoreClient["moveTask"] = (id, target) => invoke("move_task", { id, target });
  deleteTask: CoreClient["deleteTask"] = (id) => invoke("delete_task", { id });
  listTasks: CoreClient["listTasks"] = (query, page) => invoke("list_tasks", { query, page });
  searchTasks: CoreClient["searchTasks"] = (text, limit) => invoke("search_tasks", { text, limit });
  completeTasks: CoreClient["completeTasks"] = (ids, done) => invoke("complete_tasks", { ids, done });
  setDues: CoreClient["setDues"] = (dues) => invoke("set_dues", { dues });
  deleteTasks: CoreClient["deleteTasks"] = (ids) => invoke("delete_tasks", { ids });
  moveTasks: CoreClient["moveTasks"] = (ids, target) => invoke("move_tasks", { ids, target });
  restoreTasks: CoreClient["restoreTasks"] = (tasks) => invoke("restore_tasks", { tasks });

  notes: CoreClient["notes"] = {
    listFolders: () => invoke("list_note_folders"),
    listItems: (folderId, page) => invoke("list_note_items", { folderId, page }),
    get: (id) => invoke("get_note", { id }),
    create: (folderId, kind) => invoke("create_note", { folderId, kind }),
    updateBody: (id, text, baseRev) => invoke("update_note_body", { id, text, baseRev }),
    discardIfEmpty: (id) => invoke("discard_empty_note", { id }),
    rename: (id, name) => invoke("rename_note", { id, name }),
    setKind: (id, kind) => invoke("set_note_kind", { id, kind }),
    move: (id, target) => invoke("move_note", { id, target }),
    createFolder: (parentId, name) => invoke("create_note_folder", { parentId, name }),
    renameFolder: (id, name) => invoke("rename_note_folder", { id, name }),
    moveFolder: (id, target) => invoke("move_note_folder", { id, target }),
    trash: (items) => invoke("trash_note_items", { items }),
    restore: (items) => invoke("restore_note_items", { items }),
    purge: (items) => invoke("purge_note_items", { items }),
    emptyTrash: () => invoke("empty_note_trash"),
    listTrash: () => invoke("list_note_trash"),
    search: (text, limit) => invoke("search_notes", { text, limit }),
    onConflict: (cb) => subscribe<NoteConflictDto[]>("core://note-conflicts", cb),
  };

  auth: CoreClient["auth"] = {
    info: () => invoke("auth_info"),
    current: () => invoke("auth_current"),
    beginLogin: () => invoke("auth_begin_login"),
    openBrowser: (flow) => invoke("auth_open_browser", { flow }),
    pollLogin: (flow) => invoke("auth_poll_login", { flow }),
    cancelLogin: (flow) => invoke("auth_cancel_login", { flow }),
    logout: () => invoke("auth_logout"),
  };

  sync: CoreClient["sync"] = {
    status: () => invoke("sync_status"),
    now: () => invoke("sync_now"),
    flush: () => invoke("sync_flush"),
  };

  ready: CoreClient["ready"] = () => invoke("app_ready");

  /** Nur Desktop-Shell: Netzwerkwechsel der WebView an den Sync-Scheduler. */
  setOnline = (online: boolean): Promise<void> => invoke("set_online", { online });

  onChange = (cb: (e: ChangedEventDto) => void) => subscribe("core://changed", cb);
  onSyncStatus = (cb: (s: SyncStatusDto) => void) => subscribe("core://sync-status", cb);
}

/** Nur Debug-Builds der Desktop-Shell: Selbsttest von außen steuern (`src-tauri/src/dev.rs`). */
export const devSelftest = {
  /** Suiten aus `TASKS_SELFTEST` (Release: `null`). */
  suites: (): Promise<string | null> => invoke("dev_selftest"),
  /** Zeile auf stdout der App; `exit` beendet sie danach. */
  report: (line: string, exit = false): Promise<void> => invoke("dev_selftest_report", { line, exit }),
};
