//! `InMemoryTaskRepository`: schneller, vollständiger Fake des [`TaskRepository`]-Ports.

use std::{
    cmp::Reverse,
    collections::BTreeMap,
    sync::{Mutex, MutexGuard},
};

use crate::{
    domain::{SortKey, Task, TaskId, TaskList, TaskListId, Timestamp},
    ports::{
        Change, ChangeSet, CommitReceipt, EntityRef, Page, PageRequest, SmartView, TaskQuery,
        TaskRepository, TaskSummary, PAGE_LIMIT_MAX,
    },
    CoreError, CoreResult,
};

#[derive(Debug, Clone, Default)]
struct State {
    tasks: BTreeMap<TaskId, Task>,
    lists: BTreeMap<TaskListId, TaskList>,
}

/// Hält alles im Speicher; `commit` ist atomar (arbeitet auf einer Kopie).
#[derive(Debug, Default)]
pub struct InMemoryTaskRepository {
    state: Mutex<State>,
}

impl InMemoryTaskRepository {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // Ein Panic in einem Test vergiftet den Mutex; der Zustand ist trotzdem konsistent,
        // weil Änderungen nur per vollständigem Austausch übernommen werden.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Sortierposition einer Task innerhalb einer Ansicht; zugleich Inhalt des Cursors.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Position {
    BySortKey(SortKey, TaskId),
    ByCompletion(Reverse<Timestamp>, TaskId),
}

impl Position {
    fn of(task: &Task, view: &SmartView) -> Self {
        match view {
            SmartView::Completed => Self::ByCompletion(
                Reverse(task.completed_at().unwrap_or(Timestamp::EPOCH)),
                task.id(),
            ),
            _ => Self::BySortKey(task.sort_key().clone(), task.id()),
        }
    }

    fn encode(&self) -> String {
        match self {
            Self::BySortKey(key, id) => format!("s:{key}:{id}"),
            Self::ByCompletion(Reverse(at), id) => format!("c:{}:{id}", at.as_millis()),
        }
    }

    fn decode(cursor: &str, view: &SmartView) -> CoreResult<Self> {
        let invalid = || CoreError::Validation(format!("invalid cursor: {cursor:?}"));
        let mut parts = cursor.splitn(3, ':');
        let (Some(kind), Some(value), Some(id)) = (parts.next(), parts.next(), parts.next()) else {
            return Err(invalid());
        };
        // Ein Cursor gehört zu genau einer Sortierart.
        if (kind == "c") != matches!(view, SmartView::Completed) {
            return Err(invalid());
        }
        let id = TaskId::parse(id).map_err(|_| invalid())?;
        match kind {
            "s" => Ok(Self::BySortKey(
                SortKey::parse(value).map_err(|_| invalid())?,
                id,
            )),
            "c" => Ok(Self::ByCompletion(
                Reverse(Timestamp::from_millis(
                    value.parse().map_err(|_| invalid())?,
                )),
                id,
            )),
            _ => Err(invalid()),
        }
    }
}

fn apply(state: &mut State, change: Change) {
    match change {
        Change::UpsertTask(task) => {
            state.tasks.insert(task.id(), task);
        }
        Change::UpsertList(list) => {
            state.lists.insert(list.id(), list);
        }
        Change::Delete(EntityRef::Task(id)) => {
            state.tasks.remove(&id);
        }
        Change::Delete(EntityRef::TaskList(id)) => {
            state.lists.remove(&id);
        }
    }
}

fn check_integrity(state: &State) -> CoreResult<()> {
    match state
        .tasks
        .values()
        .find(|t| !state.lists.contains_key(&t.list_id()))
    {
        Some(task) => Err(CoreError::Validation(format!(
            "task {} refers to unknown list {}",
            task.id(),
            task.list_id()
        ))),
        None => Ok(()),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl TaskRepository for InMemoryTaskRepository {
    async fn get_task(&self, id: TaskId) -> CoreResult<Option<Task>> {
        Ok(self.state().tasks.get(&id).cloned())
    }

    async fn get_task_list(&self, id: TaskListId) -> CoreResult<Option<TaskList>> {
        Ok(self.state().lists.get(&id).cloned())
    }

    async fn list_task_lists(&self) -> CoreResult<Vec<TaskList>> {
        let mut lists: Vec<TaskList> = self.state().lists.values().cloned().collect();
        lists.sort_by(|a, b| (a.sort_key(), a.id()).cmp(&(b.sort_key(), b.id())));
        Ok(lists)
    }

    async fn list_tasks(
        &self,
        query: &TaskQuery,
        page: PageRequest,
    ) -> CoreResult<Page<TaskSummary>> {
        if !(1..=PAGE_LIMIT_MAX).contains(&page.limit) {
            return Err(CoreError::Validation(format!(
                "page limit must be 1..={PAGE_LIMIT_MAX}, got {}",
                page.limit
            )));
        }
        let after = page
            .cursor
            .as_deref()
            .map(|c| Position::decode(c, &query.view))
            .transpose()?;

        let state = self.state();
        let mut matching: Vec<(Position, &Task)> = state
            .tasks
            .values()
            .filter(|task| {
                state
                    .lists
                    .get(&task.list_id())
                    .is_some_and(|list| query.matches(task, list))
            })
            .map(|task| (Position::of(task, &query.view), task))
            .filter(|(pos, _)| after.as_ref().is_none_or(|a| pos > a))
            .collect();
        matching.sort_by(|a, b| a.0.cmp(&b.0));

        let limit = page.limit as usize;
        let next_cursor = (matching.len() > limit).then(|| matching[limit - 1].0.encode());
        let items = matching
            .iter()
            .take(limit)
            .map(|(_, task)| TaskSummary::from(*task))
            .collect();
        Ok(Page { items, next_cursor })
    }

    async fn commit(&self, changes: ChangeSet) -> CoreResult<CommitReceipt> {
        let changed = changes.entities();
        let mut state = self.state();
        let mut next = state.clone();
        for change in changes {
            apply(&mut next, change);
        }
        check_integrity(&next)?;
        *state = next;
        Ok(CommitReceipt { changed })
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        domain::{Due, LocalDay, NaiveDate, TaskStatus},
        ports::SteppingClock,
    };

    struct Fixture {
        repo: InMemoryTaskRepository,
        clock: SteppingClock,
        inbox: TaskList,
        work: TaskList,
    }

    fn fixture() -> Fixture {
        let clock = SteppingClock::new(Timestamp::from_millis(1_000), 1);
        let inbox = TaskList::inbox(SortKey::first(), &clock);
        let work = TaskList::new(
            "Arbeit",
            SortKey::between(Some(&SortKey::first()), None).unwrap(),
            &clock,
        )
        .unwrap();
        let repo = InMemoryTaskRepository::new();
        block_on(
            repo.commit(
                ChangeSet::new()
                    .upsert_list(inbox.clone())
                    .upsert_list(work.clone()),
            ),
        )
        .unwrap();
        Fixture {
            repo,
            clock,
            inbox,
            work,
        }
    }

    impl Fixture {
        /// Legt `n` Tasks in `list` mit aufsteigenden Sort-Keys an und gibt sie in dieser Reihenfolge zurück.
        fn add_tasks(&self, list: TaskListId, n: usize) -> Vec<Task> {
            let mut key: Option<SortKey> = None;
            let mut set = ChangeSet::new();
            let mut tasks = Vec::new();
            for i in 0..n {
                let k = SortKey::between(key.as_ref(), None).unwrap();
                let task = Task::new(list, &format!("Task {i}"), k.clone(), &self.clock).unwrap();
                key = Some(k);
                set = set.upsert_task(task.clone());
                tasks.push(task);
            }
            block_on(self.repo.commit(set)).unwrap();
            tasks
        }

        fn page(
            &self,
            query: TaskQuery,
            cursor: Option<String>,
            limit: u32,
        ) -> CoreResult<Page<TaskSummary>> {
            block_on(self.repo.list_tasks(&query, PageRequest { cursor, limit }))
        }

        fn all_ids(&self, query: TaskQuery, limit: u32) -> Vec<TaskId> {
            let mut ids = Vec::new();
            let mut cursor = None;
            loop {
                let page = self.page(query, cursor, limit).unwrap();
                ids.extend(page.items.iter().map(|s| s.id));
                match page.next_cursor {
                    Some(c) => cursor = Some(c),
                    None => return ids,
                }
            }
        }
    }

    fn all() -> TaskQuery {
        TaskQuery::view(SmartView::All)
    }

    #[test]
    fn empty_repository_returns_empty_page() {
        let f = fixture();
        let page = f.page(all(), None, 10).unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn exactly_limit_items_has_no_next_cursor() {
        let f = fixture();
        f.add_tasks(f.inbox.id(), 3);
        let page = f.page(all(), None, 3).unwrap();
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn limit_plus_one_items_yields_second_page_of_one() {
        let f = fixture();
        let tasks = f.add_tasks(f.inbox.id(), 4);
        let first = f.page(all(), None, 3).unwrap();
        assert_eq!(first.items.len(), 3);
        let second = f.page(all(), first.next_cursor.clone(), 3).unwrap();
        assert_eq!(
            second.items.iter().map(|s| s.id).collect::<Vec<_>>(),
            [tasks[3].id()]
        );
        assert_eq!(second.next_cursor, None);
    }

    #[test]
    fn paging_visits_every_task_once_in_sort_order() {
        let f = fixture();
        let tasks = f.add_tasks(f.work.id(), 25);
        let expected: Vec<TaskId> = tasks.iter().map(Task::id).collect();
        for limit in [1, 2, 7, 25, 100] {
            assert_eq!(f.all_ids(all(), limit), expected, "limit {limit}");
        }
    }

    #[test]
    fn equal_sort_keys_are_ordered_by_id() {
        let f = fixture();
        let a = Task::new(f.inbox.id(), "a", SortKey::first(), &f.clock).unwrap();
        let b = Task::new(f.inbox.id(), "b", SortKey::first(), &f.clock).unwrap();
        block_on(
            f.repo.commit(
                ChangeSet::new()
                    .upsert_task(b.clone())
                    .upsert_task(a.clone()),
            ),
        )
        .unwrap();
        let mut expected = [a.id(), b.id()];
        expected.sort();
        assert_eq!(f.all_ids(all(), 1), expected);
    }

    #[test]
    fn rejects_invalid_limits_and_cursors() {
        let f = fixture();
        for limit in [0, PAGE_LIMIT_MAX + 1] {
            assert!(matches!(
                f.page(all(), None, limit),
                Err(CoreError::Validation(_))
            ));
        }
        assert!(f.page(all(), None, PAGE_LIMIT_MAX).is_ok());
        for bad in [
            "",
            "x",
            "s:a0",
            "s:a0:nope",
            "z:a0:0192f0c4-7a3b-7c2d-8e4f-0123456789ab",
        ] {
            let r = f.page(all(), Some(bad.into()), 10);
            assert!(matches!(r, Err(CoreError::Validation(_))), "{bad:?}");
        }
        // Cursor einer anderen Sortierart.
        let completed_cursor = format!("c:5:{}", TaskId::new());
        assert!(f.page(all(), Some(completed_cursor), 10).is_err());
    }

    #[test]
    fn views_filter_and_completed_sorts_newest_first() {
        let f = fixture();
        let date = |d| NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
        let today = LocalDay::new(
            date(17),
            Timestamp::from_millis(0),
            Timestamp::from_millis(10),
        )
        .unwrap();
        let mut tasks = f.add_tasks(f.inbox.id(), 2);
        tasks.extend(f.add_tasks(f.work.id(), 3));
        tasks[0]
            .set_due(Some(Due::Date(date(16))), &f.clock)
            .unwrap();
        tasks[2]
            .set_due(Some(Due::Date(date(20))), &f.clock)
            .unwrap();
        tasks[3].complete(&f.clock);
        tasks[1].complete(&f.clock);
        let mut set = ChangeSet::new();
        for t in &tasks {
            set = set.upsert_task(t.clone());
        }
        block_on(f.repo.commit(set)).unwrap();
        let id = |i: usize| tasks[i].id();

        assert_eq!(
            f.all_ids(TaskQuery::view(SmartView::Inbox), 10),
            [id(0), id(1)]
        );
        assert_eq!(f.all_ids(TaskQuery::open(SmartView::Inbox), 10), [id(0)]);
        assert_eq!(
            f.all_ids(TaskQuery::view(SmartView::List(f.work.id())), 10),
            [id(2), id(3), id(4)]
        );
        assert_eq!(
            f.all_ids(TaskQuery::view(SmartView::Today(today)), 10),
            [id(0)]
        );
        assert_eq!(
            f.all_ids(TaskQuery::view(SmartView::Upcoming(today)), 10),
            [id(2)]
        );
        assert_eq!(
            f.all_ids(TaskQuery::open(SmartView::All), 10),
            [id(0), id(2), id(4)]
        );
        // tasks[1] wurde nach tasks[3] erledigt → steht vorne; auch über Seitengrenzen.
        assert_eq!(
            f.all_ids(TaskQuery::view(SmartView::Completed), 1),
            [id(1), id(3)]
        );
        let done_only = TaskQuery {
            status: Some(TaskStatus::Done),
            ..TaskQuery::view(SmartView::All)
        };
        assert_eq!(f.all_ids(done_only, 10), [id(1), id(3)]);
    }

    #[test]
    fn commit_is_atomic_on_integrity_violation() {
        let f = fixture();
        let ok = Task::new(f.inbox.id(), "ok", SortKey::first(), &f.clock).unwrap();
        let orphan = Task::new(TaskListId::new(), "waise", SortKey::first(), &f.clock).unwrap();
        let err = block_on(
            f.repo
                .commit(ChangeSet::new().upsert_task(ok.clone()).upsert_task(orphan)),
        );
        assert!(matches!(err, Err(CoreError::Validation(_))));
        assert_eq!(
            block_on(f.repo.get_task(ok.id())).unwrap(),
            None,
            "nichts übernommen"
        );

        // Liste löschen, die noch Tasks hat → ebenfalls abgelehnt.
        block_on(f.repo.commit(ChangeSet::new().upsert_task(ok.clone()))).unwrap();
        let err = block_on(
            f.repo
                .commit(ChangeSet::new().delete(EntityRef::TaskList(f.inbox.id()))),
        );
        assert!(err.is_err());
        assert!(block_on(f.repo.get_task_list(f.inbox.id()))
            .unwrap()
            .is_some());
    }

    #[test]
    fn upsert_replaces_and_delete_is_idempotent() {
        let f = fixture();
        let mut task = f.add_tasks(f.inbox.id(), 1).remove(0);
        task.rename("neu", &f.clock).unwrap();
        let receipt = block_on(f.repo.commit(ChangeSet::new().upsert_task(task.clone()))).unwrap();
        assert_eq!(receipt.changed, [EntityRef::Task(task.id())]);
        assert_eq!(
            block_on(f.repo.get_task(task.id()))
                .unwrap()
                .unwrap()
                .title(),
            "neu"
        );

        let delete = || ChangeSet::new().delete(EntityRef::Task(task.id()));
        block_on(f.repo.commit(delete())).unwrap();
        block_on(f.repo.commit(delete())).unwrap();
        assert_eq!(block_on(f.repo.get_task(task.id())).unwrap(), None);
    }

    #[test]
    fn lists_are_sorted_by_sort_key() {
        let f = fixture();
        let first = TaskList::new(
            "Zuerst",
            SortKey::between(None, Some(&SortKey::first())).unwrap(),
            &f.clock,
        )
        .unwrap();
        block_on(f.repo.commit(ChangeSet::new().upsert_list(first.clone()))).unwrap();
        let names: Vec<String> = block_on(f.repo.list_task_lists())
            .unwrap()
            .iter()
            .map(|l| l.name().to_owned())
            .collect();
        assert_eq!(names, ["Zuerst", "Inbox", "Arbeit"]);
    }
}
