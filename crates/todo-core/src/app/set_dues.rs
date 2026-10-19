//! Fälligkeit mehrerer Tasks setzen (Sammelaktion, je Task ein eigener Wert).

use std::collections::HashMap;

use super::{bulk_ids, load_existing, Deps};
use crate::{
    domain::{Due, Task, TaskId},
    ports::{Change, ChangeSet},
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetDuesInput {
    /// Je Task die neue Fälligkeit (`None` = entfernen). Bei doppelten IDs gilt die erste.
    pub dues: Vec<(TaskId, Option<Due>)>,
}

/// Setzt die Fälligkeit mehrerer Tasks – in **einem** Commit. Jede Task bekommt ihren eigenen
/// Wert („Alle auf heute" behält je Task die Uhrzeit, Rückgängig schreibt die alten Werte zurück).
/// Unbekannte IDs und Tasks, die den Wert schon haben, werden übersprungen (kein Op).
#[derive(Debug, Clone)]
pub struct SetDues {
    deps: Deps,
}

impl SetDues {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Liefert die tatsächlich geänderten Tasks (Reihenfolge wie `dues`).
    ///
    /// # Errors
    /// `CoreError::Validation` bei mehr als [`super::BULK_LIMIT_MAX`] Einträgen oder einer
    /// ungültigen Fälligkeit (dann ist nichts geschrieben); Fehler des Repositories.
    pub async fn execute(&self, input: SetDuesInput) -> CoreResult<Vec<Task>> {
        let mut wanted: HashMap<TaskId, Option<Due>> = HashMap::with_capacity(input.dues.len());
        let mut ids = Vec::with_capacity(input.dues.len());
        for (id, due) in input.dues {
            if let std::collections::hash_map::Entry::Vacant(slot) = wanted.entry(id) {
                ids.push(id);
                slot.insert(due);
            }
        }
        let ids = bulk_ids(ids)?;
        let repo = self.deps.repo.as_ref();
        let clock = self.deps.clock.as_ref();
        let mut changed = Vec::new();
        for mut task in load_existing(repo, &ids).await? {
            let due = wanted.remove(&task.id()).flatten();
            let before = task.due().cloned();
            task.set_due(due, clock)?;
            if task.due() != before.as_ref() {
                changed.push(task);
            }
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
        domain::{NaiveDate, Timestamp},
        ports::TaskRepository,
        CoreError,
    };

    fn date(d: u32) -> Due {
        Due::Date(NaiveDate::from_ymd_opt(2026, 10, d).unwrap())
    }

    fn setup(
        n: usize,
    ) -> (
        SetDues,
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
        (SetDues::new(deps), repo, tasks)
    }

    fn run(uc: &SetDues, dues: Vec<(TaskId, Option<Due>)>) -> CoreResult<Vec<Task>> {
        block_on(uc.execute(SetDuesInput { dues }))
    }

    fn outbox_len(repo: &crate::testing::InMemoryTaskRepository) -> usize {
        block_on(repo.pending_ops(10_000)).unwrap().len()
    }

    #[test]
    fn sets_each_value_in_one_commit_and_skips_unchanged() {
        let (uc, repo, tasks) = setup(3);
        let ids: Vec<TaskId> = tasks.iter().map(Task::id).collect();
        let at = Due::DateTime {
            at: Timestamp::from_millis(1_000),
            tz: " Europe/Berlin ".into(),
        };
        let ops = outbox_len(&repo);
        let changed = run(
            &uc,
            vec![
                (ids[0], Some(date(20))),
                (ids[1], Some(at)),
                (ids[2], None), // hat schon keine
                (TaskId::new(), Some(date(1))),
            ],
        )
        .unwrap();
        assert_eq!(
            changed.iter().map(Task::id).collect::<Vec<_>>(),
            [ids[0], ids[1]]
        );
        assert_eq!(outbox_len(&repo) - ops, 2, "je geändertem Feld ein Op");
        let second = block_on(repo.get_task(ids[1])).unwrap().unwrap();
        assert!(matches!(second.due(), Some(Due::DateTime { tz, .. }) if tz == "Europe/Berlin"));

        // Gleicher Wert erneut: nichts zu tun.
        assert!(run(&uc, vec![(ids[0], Some(date(20)))]).unwrap().is_empty());
        // Doppelte ID: der erste Wert gilt.
        let dup = run(&uc, vec![(ids[0], None), (ids[0], Some(date(1)))]).unwrap();
        assert_eq!(dup[0].due(), None);
    }

    #[test]
    fn invalid_due_writes_nothing_and_limit_is_enforced() {
        let (uc, repo, tasks) = setup(2);
        let bad = Due::DateTime {
            at: Timestamp::from_millis(1),
            tz: "   ".into(),
        };
        let ops = outbox_len(&repo);
        let err = run(
            &uc,
            vec![(tasks[0].id(), Some(date(3))), (tasks[1].id(), Some(bad))],
        );
        assert!(matches!(err, Err(CoreError::Validation(_))));
        assert_eq!(outbox_len(&repo), ops);
        assert_eq!(
            block_on(repo.get_task(tasks[0].id()))
                .unwrap()
                .unwrap()
                .due(),
            None
        );

        let many = (0..=BULK_LIMIT_MAX)
            .map(|_| (TaskId::new(), None))
            .collect();
        assert!(matches!(run(&uc, many), Err(CoreError::Validation(_))));
    }
}
