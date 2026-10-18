//! Dünne `async`-Commands, 1:1 zu `CoreApi` (Namen wie die Use-Cases).
//!
//! **Sync-Auslöser (ADR-022):** Genau die schreibenden Commands melden nach Erfolg **einmal**
//! `state.sync.local_change()` – der Scheduler lädt dann nach 1,5 s Ruhe hoch, bei einer Serie
//! spätestens 5 s nach der ersten Änderung:
//! - Listen: `create_task_list`, `rename_task_list`, `move_task_list` (Reihenfolge), `delete_task_list`
//! - Tasks: `create_task`, `update_task` (Titel, Notizen, Priorität, Fälligkeit), `complete_task`
//!   (abhaken/öffnen), `move_task` (Liste/Reihenfolge), `delete_task`
//! - Sammelaktionen (ein Aufruf = eine Meldung): `complete_tasks`, `delete_tasks`, `move_tasks`,
//!   `restore_tasks`
//!
//! Lesende Commands (`list_*`, `get_task`, `ensure_inbox`, `auth_*`, `sync_*`, `app_ready`) melden
//! nichts. Nachweis: `tests/commands.rs` (`only_writing_commands_trigger_a_sync`).

use tauri::State;
use todo_core::api::dto::{
    CreateTaskDto, MoveListTargetDto, MoveTargetDto, PageDto, PageRequestDto, TaskDto, TaskListDto,
    TaskPatchDto, TaskQueryDto, TaskSummaryDto,
};
use todo_sync::SchedulerEvent;

use crate::{
    dto::{AuthInfoDto, AuthStepDto, ErrorDto, SessionDto, SyncStatusDto},
    state::{AppState, PROVIDER_NAME, PROVIDER_NOTICE},
    STARTED,
};

type Result<T> = std::result::Result<T, ErrorDto>;

// --- Listen ------------------------------------------------------------------------------------

#[tauri::command]
pub async fn ensure_inbox(state: State<'_, AppState>) -> Result<TaskListDto> {
    Ok(state.api.ensure_inbox().await?)
}

#[tauri::command]
pub async fn list_task_lists(state: State<'_, AppState>) -> Result<Vec<TaskListDto>> {
    Ok(state.api.list_task_lists().await?)
}

#[tauri::command]
pub async fn create_task_list(state: State<'_, AppState>, name: String) -> Result<TaskListDto> {
    let list = state.api.create_task_list(name).await?;
    state.sync.local_change();
    Ok(list)
}

#[tauri::command]
pub async fn rename_task_list(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<TaskListDto> {
    let list = state.api.rename_task_list(id, name).await?;
    state.sync.local_change();
    Ok(list)
}

#[tauri::command]
pub async fn move_task_list(
    state: State<'_, AppState>,
    id: String,
    target: MoveListTargetDto,
) -> Result<TaskListDto> {
    let list = state.api.move_task_list(id, target).await?;
    state.sync.local_change();
    Ok(list)
}

#[tauri::command]
pub async fn delete_task_list(state: State<'_, AppState>, id: String) -> Result<u32> {
    let moved = state.api.delete_task_list(id).await?;
    state.sync.local_change();
    Ok(moved)
}

// --- Tasks -------------------------------------------------------------------------------------

#[tauri::command]
pub async fn create_task(state: State<'_, AppState>, input: CreateTaskDto) -> Result<TaskDto> {
    let task = state.api.create_task(input).await?;
    state.sync.local_change();
    Ok(task)
}

#[tauri::command]
pub async fn get_task(state: State<'_, AppState>, id: String) -> Result<TaskDto> {
    Ok(state.api.get_task(id).await?)
}

#[tauri::command]
pub async fn update_task(
    state: State<'_, AppState>,
    id: String,
    patch: TaskPatchDto,
) -> Result<TaskDto> {
    let task = state.api.update_task(id, patch).await?;
    state.sync.local_change();
    Ok(task)
}

#[tauri::command]
pub async fn complete_task(state: State<'_, AppState>, id: String, done: bool) -> Result<TaskDto> {
    let task = state.api.complete_task(id, done).await?;
    state.sync.local_change();
    Ok(task)
}

#[tauri::command]
pub async fn move_task(
    state: State<'_, AppState>,
    id: String,
    target: MoveTargetDto,
) -> Result<TaskDto> {
    let task = state.api.move_task(id, target).await?;
    state.sync.local_change();
    Ok(task)
}

#[tauri::command]
pub async fn delete_task(state: State<'_, AppState>, id: String) -> Result<()> {
    state.api.delete_task(id).await?;
    state.sync.local_change();
    Ok(())
}

// --- Sammelaktionen (ein Core-Aufruf, ein Commit, ein `local_change`) ---------------------------

#[tauri::command]
pub async fn complete_tasks(
    state: State<'_, AppState>,
    ids: Vec<String>,
    done: bool,
) -> Result<Vec<TaskDto>> {
    let tasks = state.api.complete_tasks(ids, done).await?;
    state.sync.local_change();
    Ok(tasks)
}

#[tauri::command]
pub async fn delete_tasks(state: State<'_, AppState>, ids: Vec<String>) -> Result<Vec<TaskDto>> {
    let tasks = state.api.delete_tasks(ids).await?;
    state.sync.local_change();
    Ok(tasks)
}

#[tauri::command]
pub async fn move_tasks(
    state: State<'_, AppState>,
    ids: Vec<String>,
    target: MoveTargetDto,
) -> Result<Vec<TaskDto>> {
    let tasks = state.api.move_tasks(ids, target).await?;
    state.sync.local_change();
    Ok(tasks)
}

#[tauri::command]
pub async fn restore_tasks(
    state: State<'_, AppState>,
    tasks: Vec<TaskDto>,
) -> Result<Vec<TaskDto>> {
    let tasks = state.api.restore_tasks(tasks).await?;
    state.sync.local_change();
    Ok(tasks)
}

#[tauri::command]
pub async fn list_tasks(
    state: State<'_, AppState>,
    query: TaskQueryDto,
    page: PageRequestDto,
) -> Result<PageDto<TaskSummaryDto>> {
    Ok(state.api.list_tasks(query, page).await?)
}

// --- Anmeldung ---------------------------------------------------------------------------------

#[tauri::command]
pub async fn auth_info(state: State<'_, AppState>) -> Result<AuthInfoDto> {
    Ok(AuthInfoDto {
        provider: PROVIDER_NAME.into(),
        notice: PROVIDER_NOTICE.into(),
        available: state.account.available().await,
    })
}

#[tauri::command]
pub async fn auth_current(state: State<'_, AppState>) -> Result<Option<SessionDto>> {
    state.account.current().await
}

#[tauri::command]
pub async fn auth_begin_login(state: State<'_, AppState>) -> Result<AuthStepDto> {
    state.account.begin_login().await
}

#[tauri::command]
pub async fn auth_open_browser(state: State<'_, AppState>, flow: String) -> Result<()> {
    state.account.reopen_browser(&flow)
}

#[tauri::command]
pub async fn auth_poll_login(state: State<'_, AppState>, flow: String) -> Result<AuthStepDto> {
    state
        .account
        .poll_login(flow, &state.api, &state.sync)
        .await
}

#[tauri::command]
pub async fn auth_cancel_login(state: State<'_, AppState>, flow: String) -> Result<()> {
    state.account.cancel_login(flow).await
}

#[tauri::command]
pub async fn auth_logout(state: State<'_, AppState>) -> Result<()> {
    state.account.logout(&state.sync).await
}

// --- Sync & Lebenszyklus -----------------------------------------------------------------------

#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> Result<SyncStatusDto> {
    Ok(state.sync.status())
}

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> Result<()> {
    state.sync.event(SchedulerEvent::ManualSync);
    Ok(())
}

/// `online`/`offline`-Ereignisse der WebView (die Shell hat keinen eigenen Netzwerk-Monitor).
#[tauri::command]
pub async fn set_online(state: State<'_, AppState>, online: bool) -> Result<()> {
    state.sync.event(if online {
        SchedulerEvent::NetworkUp
    } else {
        SchedulerEvent::NetworkDown
    });
    Ok(())
}

/// Die UI hat die erste Liste gerendert: Startzeit melden, Sync anstoßen.
#[tauri::command]
pub async fn app_ready(state: State<'_, AppState>) -> Result<()> {
    crate::report_startup(STARTED.get().copied());
    state.sync.event(SchedulerEvent::AppStarted);
    Ok(())
}

// --- Nur Debug-Builds (`dev.rs`) ----------------------------------------------------------------

/// Selbsttest-Suiten aus `TASKS_SELFTEST` (Release: immer `None`).
#[tauri::command]
pub async fn dev_selftest() -> Result<Option<String>> {
    Ok(crate::dev::selftest_suites())
}

/// Berichtszeilen des Selbsttests auf stdout; `exit` beendet danach die App (Release: no-op).
#[tauri::command]
pub async fn dev_selftest_report<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    line: String,
    exit: bool,
) -> Result<()> {
    crate::dev::selftest_line(&line);
    if exit && cfg!(debug_assertions) {
        app.exit(0);
    }
    Ok(())
}
