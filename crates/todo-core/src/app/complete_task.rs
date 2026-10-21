//! Task als erledigt markieren.

use super::{load_task, Deps};
use crate::{
    domain::{Task, TaskId},
    ports::ChangeSet,
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteTaskInput {
    pub id: TaskId,
}

/// Hakt eine Task ab. Bereits erledigte Tasks bleiben unverändert.
#[derive(Debug, Clone)]
pub struct CompleteTask {
    deps: Deps,
}

impl CompleteTask {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für unbekannte Tasks; Fehler des Repositories.
    pub async fn execute(&self, input: CompleteTaskInput) -> CoreResult<Task> {
        let mut task = load_task(self.deps.repo.as_ref(), input.id).await?;
        if task.is_recurring() && !task.is_done() {
            // Serie (ADR-028): Verlaufseintrag + nächstes Vorkommen wie beim Sammel-Abhaken.
            super::CompleteTasks::new(self.deps.clone())
                .execute(super::CompleteTasksInput {
                    ids: vec![input.id],
                    done: true,
                    today: None,
                })
                .await?;
            return load_task(self.deps.repo.as_ref(), input.id).await;
        }
        if !task.is_done() {
            task.complete(self.deps.clock.as_ref());
            self.deps
                .repo
                .commit(ChangeSet::new().upsert_task(task.clone()))
                .await?;
        }
        Ok(task)
    }
}
