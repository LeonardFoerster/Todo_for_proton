//! Dünne `async`-Commands, 1:1 zu `CoreApi` (Namen wie die Use-Cases).
//!
//! **Sync-Auslöser (ADR-022):** Genau die schreibenden Commands melden nach Erfolg **einmal**
//! `state.sync.local_change()` – der Scheduler lädt dann nach 1,5 s Ruhe hoch, bei einer Serie
//! spätestens 5 s nach der ersten Änderung:
//! - Listen: `create_task_list`, `rename_task_list`, `move_task_list` (Reihenfolge), `delete_task_list`
//! - Tasks: `create_task`, `update_task` (Titel, Notizen, Priorität, Fälligkeit), `complete_task`
//!   (abhaken/öffnen), `move_task` (Liste/Reihenfolge), `delete_task`
//! - Sammelaktionen (ein Aufruf = eine Meldung): `complete_tasks`, `delete_tasks`, `move_tasks`,
//!   `restore_tasks`, `set_dues`
//! - Notizen (ADR-029): `create_note`, `discard_empty_note` (nur wenn verworfen), `rename_note`,
//!   `set_note_kind`, `move_note`, `create_note_folder`, `rename_note_folder`, `move_note_folder`,
//!   `trash_note_items`, `restore_note_items`, `purge_note_items`, `empty_note_trash`
//! - **Inhalt:** `update_note_body` meldet `state.sync.content_change()` statt `local_change()`
//!   (Debounce 3 s, höchstens 30 s, damit Tippen nicht alle paar Sekunden den ganzen Text lädt).
//!   `sync_flush` lädt sofort hoch, wenn etwas wartet (Notiz- oder Moduswechsel in der UI).
//!
//! Lesende Commands (`list_*`, `get_*`, `search_*`, `ensure_inbox`, `auth_*`, `sync_status`,
//! `sync_now`, `app_ready`) melden nichts. Nachweis: `tests/commands.rs`
//! (`only_writing_commands_trigger_a_sync`).

use tauri::State;
use todo_core::api::{
    dto::{
        CreateTaskDto, MoveListTargetDto, MoveTargetDto, PageDto, PageRequestDto, TaskDto,
        TaskDueDto, TaskListDto, TaskPatchDto, TaskQueryDto, TaskSummaryDto,
    },
    note_dto::{
        NoteDto, NoteFolderDto, NoteHitDto, NoteItemDto, NoteKindDto, NoteTargetDto,
        NoteTrashEntryDto, NoteTreeItemDto,
    },
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
pub async fn set_dues(state: State<'_, AppState>, dues: Vec<TaskDueDto>) -> Result<Vec<TaskDto>> {
    let tasks = state.api.set_dues(dues).await?;
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

#[tauri::command]
pub async fn search_tasks(
    state: State<'_, AppState>,
    text: String,
    limit: u32,
) -> Result<Vec<TaskSummaryDto>> {
    Ok(state.api.search_tasks(text, limit).await?)
}

// --- Notizen (ADR-029) --------------------------------------------------------------------------

#[tauri::command]
pub async fn list_note_folders(state: State<'_, AppState>) -> Result<Vec<NoteFolderDto>> {
    Ok(state.api.list_note_folders().await?)
}

#[tauri::command]
pub async fn list_note_items(
    state: State<'_, AppState>,
    folder_id: Option<String>,
    page: PageRequestDto,
) -> Result<PageDto<NoteTreeItemDto>> {
    Ok(state.api.list_note_items(folder_id, page).await?)
}

#[tauri::command]
pub async fn get_note(state: State<'_, AppState>, id: String) -> Result<NoteDto> {
    Ok(state.api.get_note(id).await?)
}

#[tauri::command]
pub async fn create_note(
    state: State<'_, AppState>,
    folder_id: Option<String>,
    kind: NoteKindDto,
) -> Result<NoteDto> {
    let note = state.api.create_note(folder_id, kind).await?;
    state.sync.local_change();
    Ok(note)
}

#[tauri::command]
pub async fn update_note_body(
    state: State<'_, AppState>,
    id: String,
    text: String,
    base_rev: Option<String>,
) -> Result<NoteDto> {
    let note = state.api.update_note_body(id, text, base_rev).await?;
    state.sync.content_change();
    Ok(note)
}

#[tauri::command]
pub async fn discard_empty_note(state: State<'_, AppState>, id: String) -> Result<bool> {
    let discarded = state.api.discard_empty_note(id).await?;
    if discarded {
        state.sync.local_change();
    }
    Ok(discarded)
}

#[tauri::command]
pub async fn rename_note(state: State<'_, AppState>, id: String, name: String) -> Result<NoteDto> {
    let note = state.api.rename_note(id, name).await?;
    state.sync.local_change();
    Ok(note)
}

#[tauri::command]
pub async fn set_note_kind(
    state: State<'_, AppState>,
    id: String,
    kind: NoteKindDto,
) -> Result<NoteDto> {
    let note = state.api.set_note_kind(id, kind).await?;
    state.sync.local_change();
    Ok(note)
}

#[tauri::command]
pub async fn move_note(
    state: State<'_, AppState>,
    id: String,
    target: NoteTargetDto,
) -> Result<NoteDto> {
    let note = state.api.move_note(id, target).await?;
    state.sync.local_change();
    Ok(note)
}

#[tauri::command]
pub async fn create_note_folder(
    state: State<'_, AppState>,
    parent_id: Option<String>,
    name: String,
) -> Result<NoteFolderDto> {
    let folder = state.api.create_note_folder(parent_id, name).await?;
    state.sync.local_change();
    Ok(folder)
}

#[tauri::command]
pub async fn rename_note_folder(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<NoteFolderDto> {
    let folder = state.api.rename_note_folder(id, name).await?;
    state.sync.local_change();
    Ok(folder)
}

#[tauri::command]
pub async fn move_note_folder(
    state: State<'_, AppState>,
    id: String,
    target: NoteTargetDto,
) -> Result<NoteFolderDto> {
    let folder = state.api.move_note_folder(id, target).await?;
    state.sync.local_change();
    Ok(folder)
}

#[tauri::command]
pub async fn trash_note_items(
    state: State<'_, AppState>,
    items: Vec<NoteItemDto>,
) -> Result<Vec<NoteItemDto>> {
    let changed = state.api.trash_note_items(items).await?;
    state.sync.local_change();
    Ok(changed)
}

#[tauri::command]
pub async fn restore_note_items(
    state: State<'_, AppState>,
    items: Vec<NoteItemDto>,
) -> Result<Vec<NoteItemDto>> {
    let changed = state.api.restore_note_items(items).await?;
    state.sync.local_change();
    Ok(changed)
}

#[tauri::command]
pub async fn purge_note_items(state: State<'_, AppState>, items: Vec<NoteItemDto>) -> Result<u32> {
    let n = state.api.purge_note_items(items).await?;
    state.sync.local_change();
    Ok(n)
}

#[tauri::command]
pub async fn empty_note_trash(state: State<'_, AppState>) -> Result<u32> {
    let n = state.api.empty_note_trash().await?;
    state.sync.local_change();
    Ok(n)
}

#[tauri::command]
pub async fn list_note_trash(state: State<'_, AppState>) -> Result<Vec<NoteTrashEntryDto>> {
    Ok(state.api.list_note_trash().await?)
}

#[tauri::command]
pub async fn search_notes(
    state: State<'_, AppState>,
    text: String,
    limit: u32,
) -> Result<Vec<NoteHitDto>> {
    Ok(state.api.search_notes(text, limit).await?)
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

/// Link aus der Markdown-Vorschau im Standardbrowser öffnen – nie in der WebView. Nur `http:`,
/// `https:` und `mailto:` (`notes.md` §6.5).
#[tauri::command]
pub async fn open_external(url: String) -> Result<()> {
    if !is_external_url(&url) {
        return Err(ErrorDto::from(todo_core::CoreError::Validation(
            "only http, https and mailto links can be opened".into(),
        )));
    }
    match std::process::Command::new("xdg-open").arg(&url).spawn() {
        // Abwarten im Hintergrund, damit kein Zombie-Prozess bleibt.
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(err) => tracing::warn!(error = %err, "xdg-open failed"),
    }
    Ok(())
}

/// Zulässige externe Adresse: Schema `http`/`https`/`mailto`, keine Leer- oder Steuerzeichen.
#[must_use]
pub fn is_external_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:"))
        && url.len() <= 2048
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Die UI hat vor dem Schließen gespeichert (Antwort auf `app://close-requested`).
#[tauri::command]
pub async fn close_ready<R: tauri::Runtime>(window: tauri::Window<R>) -> Result<()> {
    let _ = window.destroy();
    Ok(())
}

/// Jetzt hochladen, falls etwas wartet (Notiz- oder Moduswechsel, `notes.md` §4.6).
#[tauri::command]
pub async fn sync_flush(state: State<'_, AppState>) -> Result<()> {
    state.sync.event(SchedulerEvent::Flush);
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
    state.purge_expired_notes_daily();
    Ok(())
}

// --- Nur Debug-Builds (`dev.rs`) ----------------------------------------------------------------

/// Selbsttest-Suiten aus `TASKS_SELFTEST` (nur im Debug-Build registriert).
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn dev_selftest() -> Result<Option<String>> {
    Ok(crate::dev::selftest_suites())
}

/// Berichtszeilen des Selbsttests auf stdout; `exit` beendet danach die App (nur Debug-Build).
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn dev_selftest_report<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    line: String,
    exit: bool,
) -> Result<()> {
    crate::dev::selftest_line(&line);
    if exit {
        app.exit(0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_external_url;

    #[test]
    fn external_links_are_limited_to_web_and_mail() {
        assert!(is_external_url("https://example.org/a?b=c#d"));
        assert!(is_external_url("HTTP://example.org"));
        assert!(is_external_url("mailto:a@example.org"));
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,x",
            "https://a b",
            "-x",
            "https://\u{0}x",
        ] {
            assert!(!is_external_url(bad), "{bad}");
        }
    }
}
