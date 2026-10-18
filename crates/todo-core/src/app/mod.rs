//! Use-Cases: orchestrieren Domain-Objekte über Ports.
//!
//! Muster: Ein Use-Case ist ein Struct mit seinen Abhängigkeiten ([`Deps`]) und einer Methode
//! `execute(&self, input) -> CoreResult<Output>`.

mod complete_task;
mod complete_tasks;
mod create_task;
mod create_task_list;
mod delete_task;
mod delete_task_list;
mod delete_tasks;
mod ensure_inbox;
mod move_task;
mod move_tasks;
mod rename_task_list;
mod reopen_task;
mod restore_tasks;
mod update_task;

use std::{fmt, sync::Arc};

pub use complete_task::{CompleteTask, CompleteTaskInput};
pub use complete_tasks::{CompleteTasks, CompleteTasksInput};
pub use create_task::{CreateTask, CreateTaskInput};
pub use create_task_list::{CreateTaskList, CreateTaskListInput};
pub use delete_task::{DeleteTask, DeleteTaskInput};
pub use delete_task_list::{DeleteTaskList, DeleteTaskListInput, DeleteTaskListOutput};
pub use delete_tasks::{DeleteTasks, DeleteTasksInput};
pub use ensure_inbox::EnsureInbox;
pub use move_task::{MoveTask, MoveTaskInput};
pub use move_tasks::{MoveTasks, MoveTasksInput};
pub use rename_task_list::{RenameTaskList, RenameTaskListInput};
pub use reopen_task::{ReopenTask, ReopenTaskInput};
pub use restore_tasks::{RestoreTasks, RestoreTasksInput};
pub use update_task::{TaskPatch, UpdateTask, UpdateTaskInput};

use crate::{
    domain::{SortKey, Task, TaskId},
    ports::{Clock, PageRequest, TaskQuery, TaskRepository, TaskSummary, PAGE_LIMIT_MAX},
    CoreError, CoreResult,
};

/// Gemeinsame Abhängigkeiten aller Use-Cases. Günstig zu klonen.
#[derive(Clone)]
pub struct Deps {
    pub repo: Arc<dyn TaskRepository>,
    pub clock: Arc<dyn Clock>,
}

impl Deps {
    #[must_use]
    pub fn new(repo: Arc<dyn TaskRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
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
/// `RestoreTasks`).
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
        assert_send(DeleteTaskList::new(deps).execute(DeleteTaskListInput {
            id: TaskListId::new(),
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
