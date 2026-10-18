//! Mehrere Tasks abhaken oder wieder öffnen (Sammelaktion).

use super::{bulk_ids, load_existing, Deps};
use crate::{
    domain::{Task, TaskId},
    ports::{Change, ChangeSet},
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteTasksInput {
    pub ids: Vec<TaskId>,
    /// `true` = abhaken, `false` = wieder öffnen.
    pub done: bool,
}

/// Hakt Tasks ab bzw. öffnet sie wieder – in **einem** Commit. Unbekannte IDs und Tasks, die
/// schon im Zielzustand sind, werden übersprungen (kein Op).
#[derive(Debug, Clone)]
pub struct CompleteTasks {
    deps: Deps,
}

impl CompleteTasks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Liefert die tatsächlich geänderten Tasks (Reihenfolge wie `ids`).
    ///
    /// # Errors
    /// `CoreError::Validation` bei mehr als [`super::BULK_LIMIT_MAX`] IDs; Fehler des
    /// Repositories (dann ist nichts geschrieben).
    pub async fn execute(&self, input: CompleteTasksInput) -> CoreResult<Vec<Task>> {
        let ids = bulk_ids(input.ids)?;
        let repo = self.deps.repo.as_ref();
        let clock = self.deps.clock.as_ref();
        let mut changed = Vec::new();
        for mut task in load_existing(repo, &ids).await? {
            if task.is_done() == input.done {
                continue;
            }
            if input.done {
                task.complete(clock);
            } else {
                task.reopen(clock);
            }
            changed.push(task);
        }
        if !changed.is_empty() {
            let mut changes = ChangeSet::new();
            for task in &changed {
                changes.push(Change::UpsertTask(task.clone()));
            }
            repo.commit(changes).await?;
        }
        Ok(changed)
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

    fn setup(
        n: usize,
    ) -> (
        CompleteTasks,
        std::sync::Arc<crate::testing::InMemoryTaskRepository>,
        Vec<Task>,
    ) {
        let (deps, repo) = deps();
        let create = CreateTask::new(deps.clone());
        let tasks = (0..n)
            .map(|i| {
                block_on(create.execute(CreateTaskInput {
                    title: format!("t{i}"),
                    list_id: None,
                    ..CreateTaskInput::default()
                }))
                .unwrap()
            })
            .collect();
        (CompleteTasks::new(deps), repo, tasks)
    }

    fn run(uc: &CompleteTasks, ids: Vec<TaskId>, done: bool) -> CoreResult<Vec<Task>> {
        block_on(uc.execute(CompleteTasksInput { ids, done }))
    }

    #[test]
    fn completes_and_reopens_and_skips_tasks_in_target_state() {
        let (uc, repo, tasks) = setup(3);
        let ids: Vec<TaskId> = tasks.iter().map(Task::id).collect();
        let first = run(&uc, vec![ids[0]], true).unwrap();
        assert_eq!(first.len(), 1);

        let done = run(&uc, ids.clone(), true).unwrap();
        assert_eq!(
            done.iter().map(Task::id).collect::<Vec<_>>(),
            [ids[1], ids[2]],
            "bereits erledigte übersprungen"
        );
        for id in &ids {
            assert!(block_on(repo.get_task(*id)).unwrap().unwrap().is_done());
        }
        let reopened = run(&uc, ids.clone(), false).unwrap();
        assert_eq!(reopened.len(), 3);
        assert!(reopened
            .iter()
            .all(|t| !t.is_done() && t.completed_at().is_none()));
    }

    #[test]
    fn unknown_duplicate_and_empty_ids() {
        let (uc, repo, tasks) = setup(2);
        let a = tasks[0].id();
        let changed = run(&uc, vec![TaskId::new(), a, a], true).unwrap();
        assert_eq!(changed.len(), 1, "unbekannt übersprungen, Duplikat einmal");
        assert_eq!(changed[0].id(), a);

        let before = block_on(repo.pending_op_count()).unwrap();
        assert!(run(&uc, Vec::new(), true).unwrap().is_empty());
        assert!(
            run(&uc, vec![a], true).unwrap().is_empty(),
            "schon erledigt"
        );
        assert_eq!(
            block_on(repo.pending_op_count()).unwrap(),
            before,
            "kein Commit"
        );
    }

    #[test]
    fn too_many_ids_is_validation() {
        let (uc, _, _) = setup(0);
        let ids = (0..=BULK_LIMIT_MAX).map(|_| TaskId::new()).collect();
        assert!(matches!(run(&uc, ids, true), Err(CoreError::Validation(_))));
    }
}
