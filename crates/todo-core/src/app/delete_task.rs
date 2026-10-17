//! Task löschen.

use super::{load_task, Deps};
use crate::{
    domain::TaskId,
    ports::{ChangeSet, EntityRef},
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteTaskInput {
    pub id: TaskId,
}

/// Löscht eine Task endgültig.
#[derive(Debug, Clone)]
pub struct DeleteTask {
    deps: Deps,
}

impl DeleteTask {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für unbekannte Tasks; Fehler des Repositories.
    pub async fn execute(&self, input: DeleteTaskInput) -> CoreResult<()> {
        let task = load_task(self.deps.repo.as_ref(), input.id).await?;
        self.deps
            .repo
            .commit(ChangeSet::new().delete(EntityRef::Task(task.id())))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{test_util::deps, CreateTask, CreateTaskInput},
        ports::TaskRepository,
        CoreError,
    };

    #[test]
    fn deletes_task_and_second_delete_is_not_found() {
        let (deps, repo) = deps();
        let task = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "x".into(),
            list_id: None,
        }))
        .unwrap();
        let uc = DeleteTask::new(deps);
        block_on(uc.execute(DeleteTaskInput { id: task.id() })).unwrap();
        assert_eq!(block_on(repo.get_task(task.id())).unwrap(), None);
        let again = block_on(uc.execute(DeleteTaskInput { id: task.id() }));
        assert!(matches!(again, Err(CoreError::NotFound(_))));
    }
}
