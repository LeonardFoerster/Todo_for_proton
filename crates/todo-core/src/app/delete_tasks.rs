//! Mehrere Tasks löschen (Sammelaktion).

use super::{bulk_ids, load_existing, Deps};
use crate::{
    domain::{Task, TaskId},
    ports::{ChangeSet, EntityRef},
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteTasksInput {
    pub ids: Vec<TaskId>,
}

/// Löscht Tasks endgültig – in **einem** Commit. Unbekannte IDs werden übersprungen.
#[derive(Debug, Clone)]
pub struct DeleteTasks {
    deps: Deps,
}

impl DeleteTasks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Liefert die gelöschten Tasks im Zustand **vor** dem Löschen (Grundlage für Rückgängig
    /// per [`super::RestoreTasks`]), Reihenfolge wie `ids`.
    ///
    /// # Errors
    /// `CoreError::Validation` bei mehr als [`super::BULK_LIMIT_MAX`] IDs; Fehler des
    /// Repositories (dann ist nichts gelöscht).
    pub async fn execute(&self, input: DeleteTasksInput) -> CoreResult<Vec<Task>> {
        let ids = bulk_ids(input.ids)?;
        let repo = self.deps.repo.as_ref();
        let tasks = load_existing(repo, &ids).await?;
        if !tasks.is_empty() {
            let mut changes = ChangeSet::new();
            for task in &tasks {
                changes = changes.delete(EntityRef::Task(task.id()));
            }
            repo.commit(changes).await?;
        }
        Ok(tasks)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{test_util::deps, CreateTask, CreateTaskInput, BULK_LIMIT_MAX},
        ports::TaskRepository,
        CoreError,
    };

    #[test]
    fn deletes_and_returns_snapshots_skipping_unknown_and_duplicates() {
        let (deps, repo) = deps();
        let create = CreateTask::new(deps.clone());
        let tasks: Vec<Task> = ["a", "b", "c"]
            .iter()
            .map(|t| {
                block_on(create.execute(CreateTaskInput {
                    title: (*t).into(),
                    list_id: None,
                }))
                .unwrap()
            })
            .collect();
        let uc = DeleteTasks::new(deps);
        let ids = vec![tasks[2].id(), TaskId::new(), tasks[0].id(), tasks[2].id()];
        let deleted = block_on(uc.execute(DeleteTasksInput { ids })).unwrap();
        assert_eq!(deleted, [tasks[2].clone(), tasks[0].clone()]);
        assert_eq!(block_on(repo.get_task(tasks[0].id())).unwrap(), None);
        assert_eq!(block_on(repo.get_task(tasks[2].id())).unwrap(), None);
        assert!(block_on(repo.get_task(tasks[1].id())).unwrap().is_some());

        let again = block_on(uc.execute(DeleteTasksInput {
            ids: vec![tasks[0].id()],
        }))
        .unwrap();
        assert!(
            again.is_empty(),
            "zweites Löschen: übersprungen, kein Fehler"
        );
    }

    #[test]
    fn empty_is_noop_and_limit_is_enforced() {
        let (deps, repo) = deps();
        let uc = DeleteTasks::new(deps);
        assert!(block_on(uc.execute(DeleteTasksInput { ids: Vec::new() }))
            .unwrap()
            .is_empty());
        assert_eq!(block_on(repo.pending_op_count()).unwrap(), 0);
        let ids = (0..=BULK_LIMIT_MAX).map(|_| TaskId::new()).collect();
        assert!(matches!(
            block_on(uc.execute(DeleteTasksInput { ids })),
            Err(CoreError::Validation(_))
        ));
    }
}
