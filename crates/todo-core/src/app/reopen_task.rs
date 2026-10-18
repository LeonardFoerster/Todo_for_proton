//! Erledigte Task wieder öffnen.

use super::{load_task, Deps};
use crate::{
    domain::{Task, TaskId},
    ports::ChangeSet,
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReopenTaskInput {
    pub id: TaskId,
}

/// Öffnet eine erledigte Task wieder. Offene Tasks bleiben unverändert.
#[derive(Debug, Clone)]
pub struct ReopenTask {
    deps: Deps,
}

impl ReopenTask {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für unbekannte Tasks; Fehler des Repositories.
    pub async fn execute(&self, input: ReopenTaskInput) -> CoreResult<Task> {
        let mut task = load_task(self.deps.repo.as_ref(), input.id).await?;
        if task.is_done() {
            task.reopen(self.deps.clock.as_ref());
            self.deps
                .repo
                .commit(ChangeSet::new().upsert_task(task.clone()))
                .await?;
        }
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    //! Gemeinsame Tests für `CompleteTask` und `ReopenTask`.
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{test_util::deps, CompleteTask, CompleteTaskInput, CreateTask, CreateTaskInput},
        ports::TaskRepository,
        CoreError,
    };

    #[test]
    fn complete_then_reopen() {
        let (deps, repo) = deps();
        let task = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "x".into(),
            list_id: None,
            ..CreateTaskInput::default()
        }))
        .unwrap();
        let complete = CompleteTask::new(deps.clone());
        let done = block_on(complete.execute(CompleteTaskInput { id: task.id() })).unwrap();
        assert!(done.is_done());
        assert_eq!(
            block_on(repo.get_task(task.id())).unwrap(),
            Some(done.clone())
        );
        let again = block_on(complete.execute(CompleteTaskInput { id: task.id() })).unwrap();
        assert_eq!(again, done, "zweites Abhaken ändert nichts");

        let reopen = ReopenTask::new(deps);
        let open = block_on(reopen.execute(ReopenTaskInput { id: task.id() })).unwrap();
        assert!(!open.is_done());
        assert_eq!(open.completed_at(), None);
        assert_eq!(
            block_on(reopen.execute(ReopenTaskInput { id: task.id() })).unwrap(),
            open
        );
    }

    #[test]
    fn unknown_task_is_not_found() {
        let (deps, _) = deps();
        let id = TaskId::new();
        let r = block_on(CompleteTask::new(deps.clone()).execute(CompleteTaskInput { id }));
        assert!(matches!(r, Err(CoreError::NotFound(_))));
        let r = block_on(ReopenTask::new(deps).execute(ReopenTaskInput { id }));
        assert!(matches!(r, Err(CoreError::NotFound(_))));
    }
}
