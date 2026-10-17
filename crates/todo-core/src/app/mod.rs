//! Use-Cases: orchestrieren Domain-Objekte über Ports.
//!
//! Muster: Ein Use-Case ist ein Struct mit seinen Abhängigkeiten ([`Deps`]) und einer Methode
//! `execute(&self, input) -> CoreResult<Output>`.

mod complete_task;
mod create_task;
mod create_task_list;
mod delete_task;
mod delete_task_list;
mod ensure_inbox;
mod move_task;
mod rename_task_list;
mod reopen_task;
mod update_task;

use std::{fmt, sync::Arc};

pub use complete_task::{CompleteTask, CompleteTaskInput};
pub use create_task::{CreateTask, CreateTaskInput};
pub use create_task_list::{CreateTaskList, CreateTaskListInput};
pub use delete_task::{DeleteTask, DeleteTaskInput};
pub use delete_task_list::{DeleteTaskList, DeleteTaskListInput, DeleteTaskListOutput};
pub use ensure_inbox::EnsureInbox;
pub use move_task::{MoveTask, MoveTaskInput};
pub use rename_task_list::{RenameTaskList, RenameTaskListInput};
pub use reopen_task::{ReopenTask, ReopenTaskInput};
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
