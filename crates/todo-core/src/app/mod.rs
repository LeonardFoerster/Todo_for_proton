//! Use-Cases: orchestrieren Domain-Objekte über Ports.
//!
//! Muster: Ein Use-Case ist ein Struct mit seinen Abhängigkeiten ([`Deps`]) und einer Methode
//! `execute(&self, input) -> CoreResult<Output>`.

mod complete_task;
mod complete_tasks;
mod create_note;
mod create_note_folder;
mod create_task;
mod create_task_list;
mod delete_task;
mod delete_task_list;
mod delete_tasks;
mod edit_note;
mod ensure_inbox;
mod export_data;
mod import_notes;
mod links;
mod move_note_items;
mod move_task;
mod move_task_list;
mod move_tasks;
mod notes;
#[cfg(test)]
mod notes_sync_tests;
mod purge_notes;
mod recurring;
mod rename_task_list;
mod reopen_task;
mod restore_tasks;
mod set_dues;
mod tags;
mod trash_notes;
mod trash_tasks;
mod update_note_body;
mod update_task;

use std::{fmt, sync::Arc};

use futures::lock::{Mutex, MutexGuard};

pub use complete_task::{CompleteTask, CompleteTaskInput};
pub use complete_tasks::{
    CompleteTasks, CompleteTasksInput, CompleteTasksOutput, CompletedOccurrence,
};
pub use create_note::{CreateNote, CreateNoteInput};
pub use create_note_folder::{
    CreateNoteFolder, CreateNoteFolderInput, RenameNoteFolder, RenameNoteFolderInput,
};
pub use create_task::{CreateTask, CreateTaskInput};
pub use create_task_list::{CreateTaskList, CreateTaskListInput};
pub use delete_task::{DeleteTask, DeleteTaskInput};
pub use delete_task_list::{DeleteTaskList, DeleteTaskListInput, DeleteTaskListOutput};
pub use delete_tasks::{DeleteTasks, DeleteTasksInput};
pub use edit_note::{RenameNote, RenameNoteInput, SetNoteKind, SetNoteKindInput};
pub use ensure_inbox::EnsureInbox;
pub use export_data::{
    DataSnapshot, ExportData, ExportNote, ExportNotes, ImportData, ImportDataInput,
    ImportDataReport, ImportList, ImportTag, IMPORT_TASK_CHUNK,
};
pub use import_notes::{
    ImportNoteFile, ImportNotes, ImportNotesInput, ImportReport, ImportSkip, IMPORT_CHUNK,
};
pub use links::{LinkTaskNote, ListLinks, PurgeDanglingLinks, UnlinkTaskNote};
pub use move_note_items::{
    MoveNote, MoveNoteFolder, MoveNoteFolderInput, MoveNoteInput, NoteTarget,
};
pub use move_task::{MoveTask, MoveTaskInput};
pub use move_task_list::{MoveTaskList, MoveTaskListInput};
pub use move_tasks::{MoveTasks, MoveTasksInput};
pub use notes::{NoteItem, NOTE_BULK_LIMIT_MAX};
pub use purge_notes::{
    EmptyNoteTrash, ListNoteTrash, NoteTrashEntry, PurgeExpiredNotes, PurgeNoteItems,
    PurgeNoteItemsInput, NOTE_TRASH_RETENTION_MS,
};
pub use recurring::{SkipOccurrence, SkipOccurrenceInput, UndoOccurrence, UndoOccurrenceInput};
pub use rename_task_list::{RenameTaskList, RenameTaskListInput};
pub use reopen_task::{ReopenTask, ReopenTaskInput};
pub use restore_tasks::{RestoreTasks, RestoreTasksInput};
pub use set_dues::{SetDues, SetDuesInput};
pub use tags::{DeleteTag, EnsureTag, RenameTag, RenameTagInput};
pub use trash_notes::{
    RestoreNoteItems, RestoreNoteItemsInput, TrashNoteItems, TrashNoteItemsInput,
};
pub use trash_tasks::{
    EmptyTaskTrash, PurgeExpiredTasks, PurgeTasks, PurgeTasksInput, TrashTasks, TrashTasksInput,
    UntrashTasks, UntrashTasksInput, TASK_TRASH_RETENTION_MS,
};
pub use update_note_body::{
    DiscardEmptyNote, DiscardEmptyNoteInput, UpdateNoteBody, UpdateNoteBodyInput,
};
pub use update_task::{TaskPatch, UpdateTask, UpdateTaskInput};

use crate::{
    domain::{SortKey, Task, TaskId},
    ports::{
        Clock, PageRequest, TaskQuery, TaskRepository, TaskSummary, TimeZones, PAGE_LIMIT_MAX,
    },
    CoreError, CoreResult,
};

/// Gemeinsame Abhängigkeiten aller Use-Cases. Günstig zu klonen; Klone teilen sich die
/// Reihenfolge-Sperre (FIX-10).
#[derive(Clone)]
pub struct Deps {
    pub repo: Arc<dyn TaskRepository>,
    pub clock: Arc<dyn Clock>,
    /// Zeitzonen für Serien mit Uhrzeit (N4); ohne Port gehen nur ganztägige Serien.
    pub zones: Option<Arc<dyn TimeZones>>,
    order: Arc<Mutex<()>>,
}

impl Deps {
    #[must_use]
    pub fn new(repo: Arc<dyn TaskRepository>, clock: Arc<dyn Clock>) -> Self {
        Self {
            repo,
            clock,
            zones: None,
            order: Arc::default(),
        }
    }

    /// Mit Zeitzonen (Serien mit Uhrzeit, N4).
    #[must_use]
    pub fn with_zones(mut self, zones: Arc<dyn TimeZones>) -> Self {
        self.zones = Some(zones);
        self
    }

    /// Serialisiert Use-Cases, die eine Reihenfolge lesen und darauf neue Sort-Keys committen
    /// (anlegen, verschieben, Liste löschen). Ohne die Sperre lesen parallele Aufrufe denselben
    /// Stand und vergeben dieselben Keys (FIX-10). Die Sperre hält bis zum Ende des Commits;
    /// Use-Cases darunter (z. B. `EnsureInbox`) nehmen sie nicht erneut.
    pub(crate) async fn lock_order(&self) -> MutexGuard<'_, ()> {
        self.order.lock().await
    }
}

impl fmt::Debug for Deps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Deps").finish_non_exhaustive()
    }
}

/// Alle Treffer einer Abfrage, seitenweise geladen.
pub(crate) async fn collect_all(
    repo: &dyn TaskRepository,
    query: &TaskQuery,
) -> CoreResult<Vec<TaskSummary>> {
    let mut items = Vec::new();
    let mut page = PageRequest::first(PAGE_LIMIT_MAX);
    loop {
        let result = repo.list_tasks(query, page).await?;
        items.extend(result.items);
        match result.next_cursor {
            Some(cursor) => page = PageRequest::after(cursor, PAGE_LIMIT_MAX),
            None => return Ok(items),
        }
    }
}

/// Lädt eine Task oder liefert `CoreError::NotFound`.
pub(crate) async fn load_task(repo: &dyn TaskRepository, id: TaskId) -> CoreResult<Task> {
    repo.get_task(id)
        .await?
        .ok_or_else(|| CoreError::NotFound(format!("task {id}")))
}

/// Höchstzahl an Tasks pro Sammel-Aufruf (`CompleteTasks`, `DeleteTasks`, `MoveTasks`,
/// `RestoreTasks`, `SetDues`, Papierkorb).
pub const BULK_LIMIT_MAX: usize = 2_000;

/// Prüft die Obergrenze und entfernt doppelte IDs (erstes Vorkommen bleibt).
pub(crate) fn bulk_ids(ids: Vec<TaskId>) -> CoreResult<Vec<TaskId>> {
    check_bulk_len(ids.len())?;
    let mut seen = std::collections::HashSet::with_capacity(ids.len());
    Ok(ids.into_iter().filter(|id| seen.insert(*id)).collect())
}

pub(crate) fn check_bulk_len(len: usize) -> CoreResult<()> {
    if len > BULK_LIMIT_MAX {
        return Err(CoreError::Validation(format!(
            "at most {BULK_LIMIT_MAX} tasks per call, got {len}"
        )));
    }
    Ok(())
}

/// Lädt die existierenden Tasks zu `ids` (Reihenfolge wie `ids`); unbekannte IDs entfallen.
pub(crate) async fn load_existing(
    repo: &dyn TaskRepository,
    ids: &[TaskId],
) -> CoreResult<Vec<Task>> {
    repo.get_tasks(ids).await
}

/// Schlüssel für eine neue Position direkt hinter `last` (bzw. den ersten, wenn leer).
pub(crate) fn key_after(last: Option<&SortKey>) -> CoreResult<SortKey> {
    SortKey::between(last, None)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod send_tests {
    //! Tauri-Commands brauchen `Send`-Futures: Hält ein Use-Case z. B. einen Guard über ein
    //! `await`, schlägt das hier beim Kompilieren fehl.
    use super::{test_util::deps, *};
    use crate::domain::TaskListId;

    fn assert_send<T: Send>(_: T) {}

    #[test]
    fn use_case_futures_are_send() {
        let (deps, _) = deps();
        assert_send(EnsureInbox::new(deps.clone()).execute());
        assert_send(
            CreateTaskList::new(deps.clone()).execute(CreateTaskListInput { name: "x".into() }),
        );
        assert_send(
            RenameTaskList::new(deps.clone()).execute(RenameTaskListInput {
                id: TaskListId::new(),
                name: "x".into(),
            }),
        );
        assert_send(
            DeleteTaskList::new(deps.clone()).execute(DeleteTaskListInput {
                id: TaskListId::new(),
            }),
        );
        let note = crate::domain::NoteId::new();
        assert_send(CreateNote::new(deps.clone()).execute(CreateNoteInput {
            folder: None,
            kind: crate::domain::NoteKind::Text,
        }));
        assert_send(
            UpdateNoteBody::new(deps.clone()).execute(UpdateNoteBodyInput {
                id: note,
                text: String::new(),
                base_rev: None,
            }),
        );
        assert_send(MoveNote::new(deps.clone()).execute(MoveNoteInput {
            id: note,
            target: NoteTarget {
                folder: None,
                after: None,
                before: None,
            },
        }));
        assert_send(EmptyNoteTrash::new(deps.clone()).execute());
        assert_send(TrashTasks::new(deps.clone()).execute(TrashTasksInput { ids: Vec::new() }));
        assert_send(EmptyTaskTrash::new(deps.clone()).execute());
        assert_send(ExportData::new(deps.clone()).execute());
        assert_send(ExportNotes::new(deps.clone()).execute());
        assert_send(ImportData::new(deps.clone()).execute(ImportDataInput {
            lists: Vec::new(),
            tasks: Vec::new(),
            tags: Vec::new(),
        }));
        assert_send(PurgeExpiredTasks::new(deps.clone()).execute());
        assert_send(PurgeExpiredNotes::new(deps.clone()).execute());
        assert_send(ListNoteTrash::new(deps.clone()).execute());
        assert_send(ImportNotes::new(deps).execute(ImportNotesInput {
            root: "x".into(),
            files: Vec::new(),
        }));
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use std::sync::Arc;

    use super::Deps;
    use crate::{domain::Timestamp, ports::SteppingClock, testing::InMemoryTaskRepository};

    pub(crate) fn deps() -> (Deps, Arc<InMemoryTaskRepository>) {
        let repo = Arc::new(InMemoryTaskRepository::new());
        let clock = Arc::new(SteppingClock::new(Timestamp::from_millis(1_000), 1));
        (Deps::new(repo.clone(), clock), repo)
    }
}
