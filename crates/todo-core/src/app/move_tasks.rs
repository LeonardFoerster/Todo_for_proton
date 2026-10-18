//! Mehrere Tasks als Block verschieben (Sammelaktion).

use std::collections::HashSet;

use super::{bulk_ids, collect_all, load_existing, move_task::insertion_bounds, Deps};
use crate::{
    domain::{SortKey, Task, TaskId, TaskListId},
    ports::{Change, ChangeSet, SmartView, TaskQuery, TaskSummary},
    CoreError, CoreResult,
};

/// Ziel wie [`super::MoveTaskInput`]; Nachbarn liegen in der Zielliste und gehören nicht zum Block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveTasksInput {
    pub ids: Vec<TaskId>,
    /// Zielliste; `None` = die gemeinsame bisherige Liste aller Tasks.
    pub list_id: Option<TaskListId>,
    pub after: Option<TaskId>,
    pub before: Option<TaskId>,
}

/// Verschiebt Tasks als zusammenhängenden Block an eine Position – in **einem** Commit.
///
/// Der Block behält die bisherige Anzeigereihenfolge der Tasks (`sort_key`, dann `TaskId`), auch
/// wenn sie aus verschiedenen Listen kommen (Sammelansichten). Die Zielliste wird nur einmal
/// geladen. Unbekannte IDs werden übersprungen; Tasks, die schon genau so liegen, erzeugen keinen Op.
#[derive(Debug, Clone)]
pub struct MoveTasks {
    deps: Deps,
}

impl MoveTasks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Liefert die tatsächlich geänderten Tasks des Blocks in neuer Reihenfolge.
    ///
    /// # Errors
    /// `CoreError::NotFound` für eine unbekannte Zielliste; `CoreError::Validation` bei zu vielen
    /// IDs, ohne Zielliste bei Tasks aus verschiedenen Listen, für Nachbarn außerhalb der
    /// Zielliste oder aus dem Block und für vertauschte Nachbarn; Fehler des Repositories.
    /// Bei jedem Fehler ist nichts geschrieben.
    pub async fn execute(&self, input: MoveTasksInput) -> CoreResult<Vec<Task>> {
        let ids = bulk_ids(input.ids)?;
        let repo = self.deps.repo.as_ref();
        let mut tasks = load_existing(repo, &ids).await?;
        let Some(first) = tasks.first() else {
            return Ok(Vec::new());
        };
        let list_id = match input.list_id {
            Some(id) => id,
            None if tasks.iter().all(|t| t.list_id() == first.list_id()) => first.list_id(),
            None => {
                return Err(CoreError::Validation(
                    "tasks are in different lists; a target list is required".into(),
                ))
            }
        };
        if repo.get_task_list(list_id).await?.is_none() {
            return Err(CoreError::NotFound(format!("list {list_id}")));
        }
        let moving: HashSet<TaskId> = tasks.iter().map(Task::id).collect();
        if [input.after, input.before]
            .iter()
            .flatten()
            .any(|id| moving.contains(id))
        {
            return Err(CoreError::Validation(
                "a moved task cannot be a neighbour".into(),
            ));
        }
        tasks.sort_by(|a, b| (a.sort_key(), a.id()).cmp(&(b.sort_key(), b.id())));

        // Zielliste ohne den Block, einmal geladen (O(n), FIX-03).
        let siblings: Vec<TaskSummary> =
            collect_all(repo, &TaskQuery::view(SmartView::List(list_id)))
                .await?
                .into_iter()
                .filter(|s| !moving.contains(&s.id))
                .collect();
        let mut changes = ChangeSet::new();
        let (lower, upper) = insertion_bounds(
            &self.deps,
            list_id,
            &siblings,
            (input.after, input.before),
            &mut changes,
        )
        .await?;
        let keys = SortKey::n_between(lower.as_ref(), upper.as_ref(), tasks.len())?;

        let clock = self.deps.clock.as_ref();
        let mut moved = Vec::new();
        for (mut task, key) in tasks.into_iter().zip(keys) {
            let before_move = task.clone();
            task.move_to(list_id, key, clock);
            if task != before_move {
                changes.push(Change::UpsertTask(task.clone()));
                moved.push(task);
            }
        }
        if !changes.is_empty() {
            repo.commit(changes).await?;
        }
        Ok(moved)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{
            test_util::deps, CreateTask, CreateTaskInput, CreateTaskList, CreateTaskListInput,
            EnsureInbox, BULK_LIMIT_MAX,
        },
        ports::TaskRepository,
    };

    struct Setup {
        uc: MoveTasks,
        repo: std::sync::Arc<crate::testing::InMemoryTaskRepository>,
        inbox: TaskListId,
        other: TaskListId,
        /// Inbox: a–f, Liste B: x, y, z – jeweils in dieser Reihenfolge.
        tasks: std::collections::HashMap<char, Task>,
    }

    fn setup() -> Setup {
        let (deps, repo) = deps();
        let inbox = block_on(EnsureInbox::new(deps.clone()).execute())
            .unwrap()
            .id();
        let other = block_on(
            CreateTaskList::new(deps.clone()).execute(CreateTaskListInput { name: "B".into() }),
        )
        .unwrap()
        .id();
        let create = CreateTask::new(deps.clone());
        let mut tasks = std::collections::HashMap::new();
        for (titles, list) in [("abcdef", inbox), ("xyz", other)] {
            for c in titles.chars() {
                let task = block_on(create.execute(CreateTaskInput {
                    title: c.to_string(),
                    list_id: Some(list),
                }))
                .unwrap();
                tasks.insert(c, task);
            }
        }
        Setup {
            uc: MoveTasks::new(deps),
            repo,
            inbox,
            other,
            tasks,
        }
    }

    impl Setup {
        fn id(&self, c: char) -> TaskId {
            self.tasks[&c].id()
        }

        fn mv(
            &self,
            block: &str,
            list: Option<TaskListId>,
            after: Option<char>,
            before: Option<char>,
        ) -> CoreResult<Vec<Task>> {
            block_on(self.uc.execute(MoveTasksInput {
                ids: block.chars().map(|c| self.id(c)).collect(),
                list_id: list,
                after: after.map(|c| self.id(c)),
                before: before.map(|c| self.id(c)),
            }))
        }

        fn order(&self, list: TaskListId) -> String {
            block_on(collect_all(
                self.repo.as_ref(),
                &TaskQuery::view(SmartView::List(list)),
            ))
            .unwrap()
            .into_iter()
            .map(|s| s.title)
            .collect()
        }
    }

    #[test]
    fn moves_block_in_display_order_to_start_middle_and_end() {
        let s = setup();
        // Auswahlreihenfolge egal: der Block bleibt in Anzeigereihenfolge.
        s.mv("eb", None, None, Some('a')).unwrap();
        assert_eq!(s.order(s.inbox), "beacdf", "an den Anfang");
        s.mv("fb", None, Some('c'), Some('d')).unwrap();
        assert_eq!(s.order(s.inbox), "eacbfd", "in die Mitte (verstreut)");
        s.mv("ea", None, None, None).unwrap();
        assert_eq!(s.order(s.inbox), "cbfdea", "ans Ende");
        s.mv("bf", None, Some('c'), None).unwrap();
        assert_eq!(s.order(s.inbox), "cbfdea", "liegt schon so");
    }

    #[test]
    fn unchanged_tasks_produce_no_ops_moved_ones_only_sort_key_ops() {
        let s = setup();
        let ops = block_on(s.repo.pending_op_count()).unwrap();
        let moved = s.mv("de", None, Some('c'), Some('f')).unwrap();
        assert_eq!(s.order(s.inbox), "abcdef");
        assert!(moved.is_empty(), "Keys zufällig gleich: nichts geändert");
        assert_eq!(block_on(s.repo.pending_op_count()).unwrap(), ops);

        let moved = s.mv("de", None, Some('f'), None).unwrap();
        assert_eq!(moved.len(), 2);
        let new_ops = block_on(s.repo.pending_ops(u32::MAX)).unwrap();
        let new_ops = &new_ops[usize::try_from(ops).unwrap()..];
        assert_eq!(new_ops.len(), 2, "ein Op je verschobener Task");
        assert!(new_ops.iter().all(|op| matches!(
            op.kind,
            crate::sync::OpKind::Set {
                field: crate::sync::FieldName::SortKey,
                ..
            }
        )));
    }

    #[test]
    fn moves_to_other_list_and_from_mixed_lists() {
        let s = setup();
        let moved = s.mv("bd", Some(s.other), Some('x'), Some('y')).unwrap();
        assert!(moved.iter().all(|t| t.list_id() == s.other));
        assert_eq!(s.order(s.inbox), "acef");
        assert_eq!(s.order(s.other), "xbdyz");

        s.mv("zfa", Some(s.inbox), None, Some('c')).unwrap();
        // Block in Anzeigereihenfolge (Sort-Key, dann ID): a (a0) < z (a2) < f (a5).
        assert_eq!(s.order(s.inbox), "azfce", "gemischte Quellen");
        assert_eq!(s.order(s.other), "xbdy");

        assert!(
            matches!(s.mv("cx", None, None, None), Err(CoreError::Validation(_))),
            "verschiedene Listen ohne Ziel"
        );
    }

    #[test]
    fn skips_unknown_dedups_and_handles_empty() {
        let s = setup();
        let ids = vec![TaskId::new(), s.id('a'), s.id('a')];
        let moved = block_on(s.uc.execute(MoveTasksInput {
            ids,
            list_id: None,
            after: None,
            before: None,
        }))
        .unwrap();
        assert_eq!(moved.len(), 1);
        assert_eq!(s.order(s.inbox), "bcdefa");

        let ops = block_on(s.repo.pending_op_count()).unwrap();
        let none = block_on(s.uc.execute(MoveTasksInput {
            ids: vec![TaskId::new()],
            list_id: Some(TaskListId::new()),
            after: None,
            before: None,
        }))
        .unwrap();
        assert!(none.is_empty(), "nur unbekannte IDs: nichts zu tun");
        assert_eq!(block_on(s.repo.pending_op_count()).unwrap(), ops);
    }

    #[test]
    fn rejects_invalid_targets_without_writing() {
        let s = setup();
        let err = |r: CoreResult<Vec<Task>>| r.unwrap_err();
        assert!(matches!(
            err(s.mv("ab", None, Some('b'), None)),
            CoreError::Validation(_)
        ));
        assert!(matches!(
            err(s.mv("ab", None, Some('x'), None)),
            CoreError::Validation(_)
        ));
        assert!(matches!(
            err(s.mv("ab", None, Some('e'), Some('c'))),
            CoreError::Validation(_)
        ));
        assert!(matches!(
            err(s.mv("ab", Some(TaskListId::new()), None, None)),
            CoreError::NotFound(_)
        ));
        let ids = (0..=BULK_LIMIT_MAX).map(|_| TaskId::new()).collect();
        assert!(matches!(
            block_on(s.uc.execute(MoveTasksInput {
                ids,
                list_id: None,
                after: None,
                before: None
            })),
            Err(CoreError::Validation(_))
        ));
        assert_eq!(s.order(s.inbox), "abcdef");
    }

    /// FIX-02 für Blöcke: Nachbarn mit gleichem Key.
    #[test]
    fn moves_block_between_neighbours_with_equal_keys() {
        let s = setup();
        let clock = crate::ports::FixedClock(crate::domain::Timestamp::from_millis(9));
        let key = s.tasks[&'b'].sort_key().clone();
        let mut changes = ChangeSet::new();
        for c in ['c', 'd'] {
            let mut t = s.tasks[&c].clone();
            t.move_to(s.inbox, key.clone(), &clock);
            changes = changes.upsert_task(t);
        }
        block_on(s.repo.commit(changes)).unwrap();
        assert_eq!(
            s.order(s.inbox),
            "abcdef",
            "b, c, d mit gleichem Key, nach ID"
        );

        s.mv("fa", None, Some('b'), Some('c')).unwrap();
        assert_eq!(s.order(s.inbox), "bafcde");
        let keys: Vec<SortKey> = block_on(collect_all(
            s.repo.as_ref(),
            &TaskQuery::view(SmartView::List(s.inbox)),
        ))
        .unwrap()
        .into_iter()
        .map(|t| t.sort_key)
        .collect();
        assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "Keys eindeutig: {keys:?}"
        );
    }

    #[test]
    fn large_block_keeps_keys_short() {
        let s = setup();
        let create = CreateTask::new(s.uc.deps.clone());
        let ids: Vec<TaskId> = (0..1_000)
            .map(|i| {
                block_on(create.execute(CreateTaskInput {
                    title: format!("n{i}"),
                    list_id: Some(s.other),
                }))
                .unwrap()
                .id()
            })
            .collect();
        let moved = block_on(s.uc.execute(MoveTasksInput {
            ids,
            list_id: Some(s.inbox),
            after: Some(s.id('a')),
            before: Some(s.id('b')),
        }))
        .unwrap();
        assert_eq!(moved.len(), 1_000);
        let max = moved
            .iter()
            .map(|t| t.sort_key().as_str().len())
            .max()
            .unwrap();
        assert!(max <= 6, "max key length {max}");
        let order = s.order(s.inbox);
        assert!(order.starts_with("an0n1n2"), "{}", &order[..10]);
        assert!(order.ends_with("n999bcdef"));
    }
}
