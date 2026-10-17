//! Task umsortieren und/oder in eine andere Liste verschieben.

use super::{collect_all, key_after, load_task, Deps};
use crate::{
    domain::{SortKey, Task, TaskId, TaskListId},
    ports::{ChangeSet, SmartView, TaskQuery, TaskSummary},
    CoreError, CoreResult,
};

/// Zielposition. Nachbarn müssen in der Zielliste liegen.
///
/// - nur `after`: direkt hinter diese Task
/// - nur `before`: direkt vor diese Task
/// - beide (z. B. Drag & Drop): dazwischen
/// - keins: ans Ende der Liste
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveTaskInput {
    pub id: TaskId,
    /// Zielliste; `None` = bisherige Liste.
    pub list_id: Option<TaskListId>,
    pub after: Option<TaskId>,
    pub before: Option<TaskId>,
}

/// Verschiebt eine Task. Ändert nur deren `list_id`/`sort_key` (konfliktarm im Sync).
#[derive(Debug, Clone)]
pub struct MoveTask {
    deps: Deps,
}

impl MoveTask {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für unbekannte Task oder Liste, `CoreError::Validation` für
    /// Nachbarn außerhalb der Zielliste, die Task selbst als Nachbarn oder vertauschte
    /// Nachbarn; Fehler des Repositories.
    pub async fn execute(&self, input: MoveTaskInput) -> CoreResult<Task> {
        let repo = self.deps.repo.as_ref();
        let mut task = load_task(repo, input.id).await?;
        let list_id = input.list_id.unwrap_or(task.list_id());
        if repo.get_task_list(list_id).await?.is_none() {
            return Err(CoreError::NotFound(format!("list {list_id}")));
        }
        if input.after == Some(task.id()) || input.before == Some(task.id()) {
            return Err(CoreError::Validation(
                "a task cannot be its own neighbour".into(),
            ));
        }

        // Reihenfolge der Zielliste ohne die verschobene Task. O(n) – Listen sind klein.
        let siblings: Vec<TaskSummary> =
            collect_all(repo, &TaskQuery::view(SmartView::List(list_id)))
                .await?
                .into_iter()
                .filter(|s| s.id != task.id())
                .collect();
        let index_of = |id: TaskId| {
            siblings.iter().position(|s| s.id == id).ok_or_else(|| {
                CoreError::Validation(format!("neighbour {id} is not in list {list_id}"))
            })
        };
        let key_at = |i: usize| siblings.get(i).map(|s| &s.sort_key);

        let key = match (input.after, input.before) {
            (Some(after), Some(before)) => {
                let (a, b) = (index_of(after)?, index_of(before)?);
                SortKey::between(key_at(a), key_at(b))?
            }
            (Some(after), None) => {
                let a = index_of(after)?;
                SortKey::between(key_at(a), key_at(a + 1))?
            }
            (None, Some(before)) => {
                let b = index_of(before)?;
                SortKey::between(b.checked_sub(1).and_then(key_at), key_at(b))?
            }
            (None, None) => key_after(siblings.last().map(|s| &s.sort_key))?,
        };

        let before_move = task.clone();
        task.move_to(list_id, key, self.deps.clock.as_ref());
        if task != before_move {
            repo.commit(ChangeSet::new().upsert_task(task.clone()))
                .await?;
        }
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{
            test_util::deps, CreateTask, CreateTaskInput, CreateTaskList, CreateTaskListInput,
            EnsureInbox,
        },
        ports::TaskRepository,
    };

    struct Setup {
        uc: MoveTask,
        repo: std::sync::Arc<crate::testing::InMemoryTaskRepository>,
        inbox: TaskListId,
        other: TaskListId,
        /// Inbox-Tasks a, b, c, d in dieser Reihenfolge.
        tasks: Vec<Task>,
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
        let tasks = ["a", "b", "c", "d"]
            .iter()
            .map(|t| {
                block_on(create.execute(CreateTaskInput {
                    title: (*t).into(),
                    list_id: None,
                }))
                .unwrap()
            })
            .collect();
        Setup {
            uc: MoveTask::new(deps),
            repo,
            inbox,
            other,
            tasks,
        }
    }

    impl Setup {
        fn mv(
            &self,
            i: usize,
            list: Option<TaskListId>,
            after: Option<usize>,
            before: Option<usize>,
        ) -> CoreResult<Task> {
            let id = |j: usize| self.tasks[j].id();
            block_on(self.uc.execute(MoveTaskInput {
                id: id(i),
                list_id: list,
                after: after.map(id),
                before: before.map(id),
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
    fn reorders_within_list() {
        let s = setup();
        s.mv(0, None, Some(2), None).unwrap(); // a hinter c
        assert_eq!(s.order(s.inbox), "bcad");
        s.mv(3, None, None, Some(1)).unwrap(); // d vor b
        assert_eq!(s.order(s.inbox), "dbca");
        s.mv(1, None, Some(2), Some(0)).unwrap(); // b zwischen c und a
        assert_eq!(s.order(s.inbox), "dcba");
        s.mv(3, None, None, None).unwrap(); // d ans Ende
        assert_eq!(s.order(s.inbox), "cbad");
    }

    #[test]
    fn only_moved_task_changes() {
        let s = setup();
        s.mv(0, None, Some(3), None).unwrap();
        for t in &s.tasks[1..] {
            assert_eq!(block_on(s.repo.get_task(t.id())).unwrap().as_ref(), Some(t));
        }
    }

    #[test]
    fn moves_to_other_list() {
        let s = setup();
        let moved = s.mv(1, Some(s.other), None, None).unwrap();
        assert_eq!(moved.list_id(), s.other);
        assert_eq!(s.order(s.inbox), "acd");
        assert_eq!(s.order(s.other), "b");
        s.mv(2, Some(s.other), None, Some(1)).unwrap(); // c vor b in Liste B
        assert_eq!(s.order(s.other), "cb");
    }

    #[test]
    fn rejects_invalid_targets() {
        let s = setup();
        assert!(
            matches!(s.mv(0, None, Some(0), None), Err(CoreError::Validation(_))),
            "selbst als Nachbar"
        );
        assert!(
            matches!(
                s.mv(0, Some(s.other), Some(1), None),
                Err(CoreError::Validation(_))
            ),
            "Nachbar fremd"
        );
        assert!(
            matches!(
                s.mv(0, None, Some(3), Some(1)),
                Err(CoreError::Validation(_))
            ),
            "vertauscht"
        );
        assert!(matches!(
            s.mv(0, Some(TaskListId::new()), None, None),
            Err(CoreError::NotFound(_))
        ));
        let unknown = block_on(s.uc.execute(MoveTaskInput {
            id: TaskId::new(),
            list_id: None,
            after: None,
            before: None,
        }));
        assert!(matches!(unknown, Err(CoreError::NotFound(_))));
        assert_eq!(s.order(s.inbox), "abcd", "nichts verändert");
    }
}
