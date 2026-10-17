// Einzige Datei mit `@tauri-apps/*` (ESLint erzwingt das).
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CoreClient } from "./core-client";
import { CoreClientError } from "./errors";
import type { ChangedEventDto, SyncStatusDto } from "./types";

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
  deleteTaskList: CoreClient["deleteTaskList"] = (id) => invoke("delete_task_list", { id });

  createTask: CoreClient["createTask"] = (input) => invoke("create_task", { input });
  getTask: CoreClient["getTask"] = (id) => invoke("get_task", { id });
  updateTask: CoreClient["updateTask"] = (id, patch) => invoke("update_task", { id, patch });
  completeTask: CoreClient["completeTask"] = (id, done) => invoke("complete_task", { id, done });
  moveTask: CoreClient["moveTask"] = (id, target) => invoke("move_task", { id, target });
  deleteTask: CoreClient["deleteTask"] = (id) => invoke("delete_task", { id });
  listTasks: CoreClient["listTasks"] = (query, page) => invoke("list_tasks", { query, page });

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
  };

  ready: CoreClient["ready"] = () => invoke("app_ready");

  onChange = (cb: (e: ChangedEventDto) => void) => subscribe("core://changed", cb);
  onSyncStatus = (cb: (s: SyncStatusDto) => void) => subscribe("core://sync-status", cb);
}
