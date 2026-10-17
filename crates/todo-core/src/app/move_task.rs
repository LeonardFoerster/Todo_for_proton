//! Task umsortieren und/oder in eine andere Liste verschieben.

use super::{collect_all, load_task, Deps};
use crate::{
    domain::{SortKey, Task, TaskId, TaskListId},
    ports::{Change, ChangeSet, SmartView, TaskQuery, TaskSummary},
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
///
/// Ausnahme: Haben die beiden Nachbarn denselben Sort-Key (parallele Verschiebungen auf zwei
/// Geräten; Reihenfolge dann nach `TaskId`), bekommt der obere Nachbar samt allen folgenden
/// Tasks mit diesem Key neue, aufsteigende Keys – sonst gäbe es keinen Platz dazwischen.
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

        // Indizes der Nachbarn unter- und oberhalb der Zielposition.
        let (lower, upper) = match (input.after, input.before) {
            (Some(after), Some(before)) => (Some(index_of(after)?), Some(index_of(before)?)),
            (Some(after), None) => {
                let a = index_of(after)?;
                (Some(a), (a + 1 < siblings.len()).then_some(a + 1))
            }
            (None, Some(before)) => {
                let b = index_of(before)?;
                (b.checked_sub(1), Some(b))
            }
            (None, None) => (siblings.len().checked_sub(1), None),
        };

        let clock = self.deps.clock.as_ref();
        let mut changes = ChangeSet::new();
        let mut upper_key = upper.map(|i| siblings[i].sort_key.clone());
        if let (Some(l), Some(u)) = (lower, upper) {
            if l < u && siblings[l].sort_key == siblings[u].sort_key {
                let rekeyed = spread_equal_keys(&siblings, u)?;
                upper_key = rekeyed.first().map(|(_, key)| key.clone());
                for (id, key) in rekeyed {
                    let mut sibling = load_task(repo, id).await?;
                    sibling.move_to(list_id, key, clock);
                    changes.push(Change::UpsertTask(sibling));
                }
            }
        }
        let key = SortKey::between(lower.map(|i| &siblings[i].sort_key), upper_key.as_ref())?;

        let before_move = task.clone();
        task.move_to(list_id, key, clock);
        if task != before_move {
            changes.push(Change::UpsertTask(task.clone()));
        }
        if !changes.is_empty() {
            repo.commit(changes).await?;
        }
        Ok(task)
    }
}

/// Neue, aufsteigende Keys für die Tasks ab `start`, die denselben Key wie `start` haben – alle
/// oberhalb dieses Keys und unterhalb des nächsten größeren. Reihenfolge bleibt erhalten.
fn spread_equal_keys(siblings: &[TaskSummary], start: usize) -> CoreResult<Vec<(TaskId, SortKey)>> {
    let key = &siblings[start].sort_key;
    let end = siblings[start..]
        .iter()
        .position(|s| &s.sort_key != key)
        .map_or(siblings.len(), |offset| start + offset);
    let next = siblings.get(end).map(|s| &s.sort_key);
    let mut previous = key.clone();
    let mut keys = Vec::with_capacity(end - start);
    for sibling in &siblings[start..end] {
        let new_key = SortKey::between(Some(&previous), next)?;
        keys.push((sibling.id, new_key.clone()));
        previous = new_key;
    }
    Ok(keys)
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

    /// FIX-02: Nach parallelen Verschiebungen auf zwei Geräten können Nachbarn denselben Key
    /// haben (Reihenfolge nach `TaskId`). Dazwischen einfügen muss trotzdem gehen.
    #[test]
    fn moves_between_neighbours_with_equal_keys() {
        let s = setup();
        let clock = crate::ports::FixedClock(crate::domain::Timestamp::from_millis(9));
        let mut c = s.tasks[2].clone();
        c.move_to(s.inbox, s.tasks[1].sort_key().clone(), &clock);
        let mut d = s.tasks[3].clone();
        d.move_to(s.inbox, s.tasks[1].sort_key().clone(), &clock);
        block_on(
            s.repo
                .commit(ChangeSet::new().upsert_task(c).upsert_task(d)),
        )
        .unwrap();
        assert_eq!(
            s.order(s.inbox),
            "abcd",
            "b, c, d mit gleichem Key, nach ID"
        );

        s.mv(0, None, Some(1), Some(2)).unwrap(); // a zwischen b und c
        assert_eq!(s.order(s.inbox), "bacd");
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
            "Keys wieder eindeutig: {keys:?}"
        );
        s.mv(3, None, Some(1), None).unwrap(); // d direkt hinter b
        assert_eq!(s.order(s.inbox), "bdac");
    }
}
