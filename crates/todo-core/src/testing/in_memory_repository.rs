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
    use crate::ports::FixedClock;

    mod contract {
        crate::repository_contract_tests!(super::InMemoryTaskRepository::new());
    }

    #[test]
    fn run_all_covers_the_fake() {
        crate::testing::repository_contract::run_all(InMemoryTaskRepository::new);
    }

    /// Fake-spezifisch: Cursor-Format und Sortierart des Cursors.
    #[test]
    fn rejects_malformed_or_foreign_cursors() {
        let repo = InMemoryTaskRepository::new();
        let clock = FixedClock(Timestamp::EPOCH);
        let inbox = TaskList::inbox(SortKey::first(), &clock);
        block_on(repo.commit(ChangeSet::new().upsert_list(inbox))).unwrap();
        let page = |view, cursor: &str| {
            block_on(repo.list_tasks(
                &TaskQuery::view(view),
                PageRequest::after(cursor.into(), 10),
            ))
        };
        let id = TaskId::new();
        assert!(page(SmartView::All, &format!("s:a0:{id}")).is_ok());
        assert!(page(SmartView::Completed, &format!("c:5:{id}")).is_ok());
        for bad in [
            "",
            "x",
            "s:a0",
            "s:a0:nope",
            "s:a00:{id}",
            "c:x:{id}",
            "z:a0:{id}",
        ] {
            let bad = bad.replace("{id}", &id.to_string());
            assert!(
                matches!(page(SmartView::All, &bad), Err(CoreError::Validation(_))),
                "{bad:?}"
            );
        }
        assert!(
            page(SmartView::All, &format!("c:5:{id}")).is_err(),
            "Completed-Cursor in All"
        );
        assert!(
            page(SmartView::Completed, &format!("s:a0:{id}")).is_err(),
            "All-Cursor in Completed"
        );
    }
}
