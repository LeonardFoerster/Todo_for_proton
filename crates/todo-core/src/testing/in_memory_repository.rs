//! `InMemoryTaskRepository`: schneller, vollständiger Fake des [`TaskRepository`]-Ports.

use std::{
    cmp::Reverse,
    collections::BTreeMap,
    sync::{Mutex, MutexGuard},
};

use crate::{
    domain::{DeviceId, SortKey, Task, TaskId, TaskList, TaskListId, Timestamp},
    ports::{
        Change, ChangeSet, CommitReceipt, EntityRef, Page, PageRequest, SmartView, TaskQuery,
        TaskRepository, TaskSummary, PAGE_LIMIT_MAX,
    },
    sync::{
        self, EntityState, HlcState, Op, OpClock, Registers, RemoteBatch, RemoteReceipt,
        ReplicaStore,
    },
    CoreError, CoreResult,
};

#[derive(Debug, Clone)]
struct State {
    tasks: BTreeMap<TaskId, Task>,
    lists: BTreeMap<TaskListId, TaskList>,
    // Sync-Buchhaltung
    clock: OpClock,
    entities: BTreeMap<EntityRef, EntityState>,
    outbox: Vec<Op>,
    cursors: BTreeMap<DeviceId, u64>,
    values: BTreeMap<String, String>,
}

/// Hält alles im Speicher; `commit` und `apply_remote` sind atomar (arbeiten auf einer Kopie).
#[derive(Debug)]
pub struct InMemoryTaskRepository {
    state: Mutex<State>,
}

impl Default for InMemoryTaskRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryTaskRepository {
    /// Leerer Speicher mit neuer Geräte-ID.
    #[must_use]
    pub fn new() -> Self {
        Self::with_device(DeviceId::new())
    }

    /// Leerer Speicher mit vorgegebener Geräte-ID (deterministische Tests).
    #[must_use]
    pub fn with_device(device: DeviceId) -> Self {
        Self {
            state: Mutex::new(State {
                tasks: BTreeMap::new(),
                lists: BTreeMap::new(),
                clock: OpClock::new(HlcState::new(device), 0),
                entities: BTreeMap::new(),
                outbox: Vec::new(),
                cursors: BTreeMap::new(),
                values: BTreeMap::new(),
            }),
        }
    }

    /// Führt `f` auf einer Kopie aus und übernimmt sie nur bei Erfolg und intakter Integrität.
    fn transaction<T>(&self, f: impl FnOnce(&mut State) -> CoreResult<T>) -> CoreResult<T> {
        let mut state = self.state();
        let mut next = state.clone();
        let out = f(&mut next)?;
        check_integrity(&next)?;
        *state = next;
        Ok(out)
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
        self.transaction(|state| {
            for change in sync::record_local(state, changes)? {
                apply(state, change);
            }
            Ok(CommitReceipt { changed })
        })
    }

    async fn device_id(&self) -> CoreResult<DeviceId> {
        Ok(self.state().clock.hlc().device())
    }

    async fn pending_ops(&self, limit: u32) -> CoreResult<Vec<Op>> {
        Ok(self
            .state()
            .outbox
            .iter()
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn pending_op_count(&self) -> CoreResult<u64> {
        Ok(self.state().outbox.len() as u64)
    }

    async fn mark_ops_pushed(&self, up_to: u64) -> CoreResult<()> {
        self.state().outbox.retain(|op| op.id.seq > up_to);
        Ok(())
    }

    async fn apply_remote(&self, batch: RemoteBatch) -> CoreResult<RemoteReceipt> {
        self.transaction(|state| sync::apply_remote(state, &batch))
    }

    async fn sync_cursor(&self, device: DeviceId) -> CoreResult<Option<u64>> {
        Ok(self.state().cursors.get(&device).copied())
    }

    async fn set_sync_cursor(&self, device: DeviceId, seq: u64) -> CoreResult<()> {
        self.state().cursors.insert(device, seq);
        Ok(())
    }

    async fn entity_state(&self, entity: EntityRef) -> CoreResult<Option<EntityState>> {
        Ok(self.state().entities.get(&entity).cloned())
    }

    async fn sync_value(&self, key: &str) -> CoreResult<Option<String>> {
        Ok(self.state().values.get(key).cloned())
    }

    async fn set_sync_value(&self, key: &str, value: String) -> CoreResult<()> {
        self.state().values.insert(key.to_owned(), value);
        Ok(())
    }
}

impl ReplicaStore for State {
    fn op_clock(&mut self) -> CoreResult<OpClock> {
        Ok(self.clock)
    }

    fn set_op_clock(&mut self, clock: &OpClock) -> CoreResult<()> {
        self.clock = *clock;
        Ok(())
    }

    fn entity_state(&mut self, entity: EntityRef) -> CoreResult<Option<EntityState>> {
        Ok(self.entities.get(&entity).cloned())
    }

    fn set_entity_state(&mut self, entity: EntityRef, state: &EntityState) -> CoreResult<()> {
        self.entities.insert(entity, state.clone());
        Ok(())
    }

    fn push_outbox(&mut self, op: &Op) -> CoreResult<()> {
        self.outbox.push(op.clone());
        Ok(())
    }

    fn set_cursor(&mut self, device: DeviceId, seq: u64) -> CoreResult<()> {
        self.cursors.insert(device, seq);
        Ok(())
    }

    fn live_tasks(&mut self) -> CoreResult<Vec<(TaskId, Registers)>> {
        Ok(self
            .entities
            .iter()
            .filter_map(|(entity, state)| match (entity, state) {
                (EntityRef::Task(id), EntityState::Live(registers)) => {
                    Some((*id, registers.clone()))
                }
                _ => None,
            })
            .collect())
    }

    fn task(&mut self, id: TaskId) -> CoreResult<Option<Task>> {
        Ok(self.tasks.get(&id).cloned())
    }

    fn list(&mut self, id: TaskListId) -> CoreResult<Option<TaskList>> {
        Ok(self.lists.get(&id).cloned())
    }

    fn put_task(&mut self, task: &Task) -> CoreResult<()> {
        apply(self, Change::UpsertTask(task.clone()));
        Ok(())
    }

    fn put_list(&mut self, list: &TaskList) -> CoreResult<()> {
        apply(self, Change::UpsertList(list.clone()));
        Ok(())
    }

    fn remove(&mut self, entity: EntityRef) -> CoreResult<()> {
        apply(self, Change::Delete(entity));
        Ok(())
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
