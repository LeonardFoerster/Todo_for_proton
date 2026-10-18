//! Liste umsortieren.

use super::Deps;
use crate::{
    domain::{SortKey, TaskList, TaskListId},
    ports::{Change, ChangeSet},
    CoreError, CoreResult,
};

/// Zielposition unter den Benutzerlisten (die Inbox steht immer davor, ADR-023).
///
/// - nur `after`: direkt hinter diese Liste
/// - nur `before`: direkt vor diese Liste
/// - beide (Drag & Drop): dazwischen
/// - keins: ans Ende
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveTaskListInput {
    pub id: TaskListId,
    pub after: Option<TaskListId>,
    pub before: Option<TaskListId>,
}

/// Verschiebt eine Liste. Ändert nur deren `sort_key` (konfliktarm im Sync).
///
/// Ausnahme wie bei `MoveTask` (FIX-02): Haben beide Nachbarn denselben Sort-Key (paralleles
/// Umsortieren auf zwei Geräten; Reihenfolge dann nach `TaskListId`), bekommen der obere Nachbar und
/// alle folgenden mit diesem Key neue, aufsteigende Keys.
#[derive(Debug, Clone)]
pub struct MoveTaskList {
    deps: Deps,
}

impl MoveTaskList {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für eine unbekannte Liste; `CoreError::Validation` für die Inbox (als
    /// Liste oder Nachbar), die Liste als eigenen Nachbarn, unbekannte oder vertauschte Nachbarn;
    /// Fehler des Repositories.
    pub async fn execute(&self, input: MoveTaskListInput) -> CoreResult<TaskList> {
        let repo = self.deps.repo.as_ref();
        let clock = self.deps.clock.as_ref();
        let lists = repo.list_task_lists().await?;
        let mut list = lists
            .iter()
            .find(|l| l.id() == input.id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound(format!("list {}", input.id)))?;
        if !list.can_move() {
            return Err(CoreError::Validation("the inbox cannot be moved".into()));
        }
        if input.after == Some(list.id()) || input.before == Some(list.id()) {
            return Err(CoreError::Validation(
                "a list cannot be its own neighbour".into(),
            ));
        }

        // Übrige Benutzerlisten in Anzeigereihenfolge (Repository: sort_key, dann id).
        let siblings: Vec<&TaskList> = lists
            .iter()
            .filter(|l| l.can_move() && l.id() != list.id())
            .collect();
        let index_of = |id: TaskListId| {
            siblings
                .iter()
                .position(|l| l.id() == id)
                .ok_or_else(|| CoreError::Validation(format!("{id} is not a movable list")))
        };
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

        let mut changes = ChangeSet::new();
        let mut upper_key = upper.map(|i| siblings[i].sort_key().clone());
        if let (Some(l), Some(u)) = (lower, upper) {
            if l < u && siblings[l].sort_key() == siblings[u].sort_key() {
                let key = siblings[u].sort_key();
                let end = siblings[u..]
                    .iter()
                    .position(|s| s.sort_key() != key)
                    .map_or(siblings.len(), |offset| u + offset);
                let next = siblings.get(end).map(|s| s.sort_key());
                let keys = SortKey::n_between(Some(key), next, end - u)?;
                upper_key = keys.first().cloned();
                for (sibling, new_key) in siblings[u..end].iter().zip(keys) {
                    let mut moved = (*sibling).clone();
                    moved.move_to(new_key, clock);
                    changes.push(Change::UpsertList(moved));
                }
            }
        }
        let lower_key = lower.map(|i| siblings[i].sort_key().clone());
        let key = SortKey::between(lower_key.as_ref(), upper_key.as_ref())?;

        let before_move = list.clone();
        list.move_to(key, clock);
        if list != before_move {
            changes.push(Change::UpsertList(list.clone()));
        }
        if !changes.is_empty() {
            repo.commit(changes).await?;
        }
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{test_util::deps, CreateTaskList, CreateTaskListInput, EnsureInbox},
        ports::{FixedClock, TaskRepository},
        sync::{FieldName, OpKind},
    };

    struct Setup {
        uc: MoveTaskList,
        repo: std::sync::Arc<crate::testing::InMemoryTaskRepository>,
        /// Listen A, B, C, D in dieser Reihenfolge (hinter der Inbox).
        ids: Vec<TaskListId>,
    }

    fn setup() -> Setup {
        let (deps, repo) = deps();
        block_on(EnsureInbox::new(deps.clone()).execute()).unwrap();
        let create = CreateTaskList::new(deps.clone());
        let ids = ["A", "B", "C", "D"]
            .iter()
            .map(|n| {
                block_on(create.execute(CreateTaskListInput { name: (*n).into() }))
                    .unwrap()
                    .id()
            })
            .collect();
        Setup {
            uc: MoveTaskList::new(deps),
            repo,
            ids,
        }
    }

    impl Setup {
        fn mv(
            &self,
            i: usize,
            after: Option<usize>,
            before: Option<usize>,
        ) -> CoreResult<TaskList> {
            block_on(self.uc.execute(MoveTaskListInput {
                id: self.ids[i],
                after: after.map(|j| self.ids[j]),
                before: before.map(|j| self.ids[j]),
            }))
        }

        /// Namen der Benutzerlisten in Anzeigereihenfolge.
        fn order(&self) -> String {
            block_on(self.repo.list_task_lists())
                .unwrap()
                .iter()
                .filter(|l| !l.is_inbox())
                .map(TaskList::name)
                .collect()
        }
    }

    #[test]
    fn moves_to_start_middle_and_end() {
        let s = setup();
        s.mv(3, None, Some(0)).unwrap(); // D vor A
        assert_eq!(s.order(), "DABC");
        s.mv(0, Some(1), Some(2)).unwrap(); // A zwischen B und C
        assert_eq!(s.order(), "DBAC");
        s.mv(3, None, None).unwrap(); // D ans Ende
        assert_eq!(s.order(), "BACD");
        s.mv(2, Some(1), None).unwrap(); // C hinter B
        assert_eq!(s.order(), "BCAD");
    }

    #[test]
    fn only_the_sort_key_of_the_moved_list_changes() {
        let s = setup();
        block_on(s.repo.mark_ops_pushed(u64::MAX)).unwrap();
        s.mv(0, Some(2), None).unwrap();
        let ops = block_on(s.repo.pending_ops(10)).unwrap();
        assert_eq!(ops.len(), 1);
        assert!(matches!(
            ops[0].kind,
            OpKind::Set {
                field: FieldName::SortKey,
                ..
            }
        ));
    }

    #[test]
    fn rejects_inbox_unknown_and_invalid_neighbours() {
        let s = setup();
        let inbox = block_on(s.uc.execute(MoveTaskListInput {
            id: TaskListId::INBOX,
            after: None,
            before: None,
        }));
        assert!(matches!(inbox, Err(CoreError::Validation(_))), "Inbox");
        let before_inbox = block_on(s.uc.execute(MoveTaskListInput {
            id: s.ids[0],
            after: None,
            before: Some(TaskListId::INBOX),
        }));
        assert!(
            matches!(before_inbox, Err(CoreError::Validation(_))),
            "Inbox als Nachbar"
        );
        assert!(matches!(
            s.mv(0, Some(0), None),
            Err(CoreError::Validation(_))
        ));
        assert!(
            matches!(s.mv(0, Some(3), Some(1)), Err(CoreError::Validation(_))),
            "vertauscht"
        );
        let unknown = block_on(s.uc.execute(MoveTaskListInput {
            id: TaskListId::new(),
            after: None,
            before: None,
        }));
        assert!(matches!(unknown, Err(CoreError::NotFound(_))));
        assert_eq!(s.order(), "ABCD");
    }

    /// Paralleles Umsortieren auf zwei Geräten kann gleiche Keys hinterlassen (Ordnung nach ID).
    #[test]
    fn moves_between_neighbours_with_equal_keys() {
        let s = setup();
        let lists = block_on(s.repo.list_task_lists()).unwrap();
        let key = lists
            .iter()
            .find(|l| l.id() == s.ids[1])
            .unwrap()
            .sort_key()
            .clone();
        let clock = FixedClock(crate::domain::Timestamp::from_millis(9));
        let mut changes = ChangeSet::new();
        for id in [s.ids[2], s.ids[3]] {
            let mut l = lists.iter().find(|l| l.id() == id).unwrap().clone();
            l.move_to(key.clone(), &clock);
            changes = changes.upsert_list(l);
        }
        block_on(s.repo.commit(changes)).unwrap();
        let tied = s.order();
        let first = &tied[1..2];
        let second = &tied[2..3];
        let id_of = |name: &str| s.ids["ABCD".find(name).unwrap()];
        let idx = |name: &str| s.ids.iter().position(|i| *i == id_of(name)).unwrap();

        s.mv(0, Some(idx(first)), Some(idx(second))).unwrap();
        assert_eq!(s.order(), format!("{first}A{second}{}", &tied[3..]));
        let keys: Vec<SortKey> = block_on(s.repo.list_task_lists())
            .unwrap()
            .iter()
            .filter(|l| !l.is_inbox())
            .map(|l| l.sort_key().clone())
            .collect();
        assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "Keys eindeutig: {keys:?}"
        );
    }
}
