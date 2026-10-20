//! `InMemoryTaskRepository`: schneller, vollständiger Fake des [`TaskRepository`]-Ports.

use std::{
    cmp::Reverse,
    collections::BTreeMap,
    sync::{Mutex, MutexGuard},
};

use crate::{
    domain::{
        DeviceId, Note, NoteFolder, NoteFolderId, NoteId, RevisionId, SortKey, Task, TaskId,
        TaskList, TaskListId, Timestamp,
    },
    ports::{
        check_page_limit, decode_tree_cursor, encode_tree_cursor, Change, ChangeSet, CommitReceipt,
        EntityRef, FolderItemsQuery, NoteRepository, NoteSummary, NoteTreeItem, Page, PageRequest,
        SmartView, TaskQuery, TaskRepository, TaskSummary, PAGE_LIMIT_MAX,
    },
    sync::{
        self, EntityState, FieldName, HlcState, Op, OpClock, OpKind, Registers, RemoteBatch,
        RemoteReceipt, ReplicaStore, Revision, RevisionSet,
    },
    CoreError, CoreResult,
};

#[derive(Debug, Clone)]
struct State {
    tasks: BTreeMap<TaskId, Task>,
    lists: BTreeMap<TaskListId, TaskList>,
    notes: BTreeMap<NoteId, Note>,
    folders: BTreeMap<NoteFolderId, NoteFolder>,
    // Sync-Buchhaltung
    clock: OpClock,
    entities: BTreeMap<EntityRef, EntityState>,
    revisions: BTreeMap<NoteId, RevisionSet>,
    outbox: Vec<Op>,
    /// Bis hier ist die Outbox zum Upload herausgegeben (`freeze_outbox`).
    frozen_up_to: u64,
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
                notes: BTreeMap::new(),
                folders: BTreeMap::new(),
                clock: OpClock::new(HlcState::new(device), 0),
                entities: BTreeMap::new(),
                revisions: BTreeMap::new(),
                outbox: Vec::new(),
                frozen_up_to: 0,
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
#[allow(clippy::enum_variant_names)] // „sortiert nach …“; `SortKey` hieße wie der Typ
enum Position {
    BySortKey(SortKey, TaskId),
    ByCompletion(Reverse<Timestamp>, TaskId),
    /// „Heute"/„Geplant": Fälligkeit (ADR-026), dann wie gewohnt.
    ByDue(Timestamp, SortKey, TaskId),
}

/// Cursor-Kennung der Sortierart einer Ansicht (ein Cursor gehört zu genau einer).
fn cursor_kind(view: &SmartView) -> &'static str {
    match view {
        SmartView::Completed => "c",
        SmartView::Today(_) | SmartView::Upcoming(_) => "d",
        _ => "s",
    }
}

impl Position {
    fn of(task: &Task, view: &SmartView) -> Self {
        match view {
            SmartView::Completed => Self::ByCompletion(
                Reverse(task.completed_at().unwrap_or(Timestamp::EPOCH)),
                task.id(),
            ),
            SmartView::Today(day) | SmartView::Upcoming(day) => Self::ByDue(
                task.due()
                    .map_or(Timestamp::from_millis(i64::MAX), |d| d.sort_instant(day)),
                task.sort_key().clone(),
                task.id(),
            ),
            _ => Self::BySortKey(task.sort_key().clone(), task.id()),
        }
    }

    fn encode(&self) -> String {
        match self {
            Self::BySortKey(key, id) => format!("s:{key}:{id}"),
            Self::ByCompletion(Reverse(at), id) => format!("c:{}:{id}", at.as_millis()),
            Self::ByDue(at, key, id) => format!("d:{}:{key}:{id}", at.as_millis()),
        }
    }

    fn decode(cursor: &str, view: &SmartView) -> CoreResult<Self> {
        let invalid = || CoreError::Validation(format!("invalid cursor: {cursor:?}"));
        let parts: Vec<&str> = cursor.split(':').collect();
        if parts.first() != Some(&cursor_kind(view)) {
            return Err(invalid());
        }
        let millis = |v: &str| v.parse().map(Timestamp::from_millis).map_err(|_| invalid());
        let key = |v: &str| SortKey::parse(v).map_err(|_| invalid());
        let id = |v: &str| TaskId::parse(v).map_err(|_| invalid());
        match parts.as_slice() {
            ["s", k, i] => Ok(Self::BySortKey(key(k)?, id(i)?)),
            ["c", at, i] => Ok(Self::ByCompletion(Reverse(millis(at)?), id(i)?)),
            ["d", at, k, i] => Ok(Self::ByDue(millis(at)?, key(k)?, id(i)?)),
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
        Change::UpsertNote(note) => {
            state.notes.insert(note.id(), note);
        }
        Change::UpsertNoteFolder(folder) => {
            state.folders.insert(folder.id(), folder);
        }
        Change::Delete(EntityRef::Note(id)) => {
            state.notes.remove(&id);
        }
        Change::Delete(EntityRef::NoteFolder(id)) => {
            state.folders.remove(&id);
        }
    }
}

fn check_integrity(state: &State) -> CoreResult<()> {
    let known =
        |folder: Option<NoteFolderId>| folder.is_none_or(|f| state.folders.contains_key(&f));
    if let Some(note) = state.notes.values().find(|n| !known(n.folder())) {
        return Err(CoreError::Validation(format!(
            "note {} refers to an unknown folder",
            note.id()
        )));
    }
    if let Some(folder) = state.folders.values().find(|f| !known(f.parent())) {
        return Err(CoreError::Validation(format!(
            "folder {} refers to an unknown parent",
            folder.id()
        )));
    }
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
impl NoteRepository for InMemoryTaskRepository {
    async fn get_note(&self, id: NoteId) -> CoreResult<Option<Note>> {
        Ok(self.state().notes.get(&id).cloned())
    }

    async fn get_note_folder(&self, id: NoteFolderId) -> CoreResult<Option<NoteFolder>> {
        Ok(self.state().folders.get(&id).cloned())
    }

    async fn list_note_folders(&self) -> CoreResult<Vec<NoteFolder>> {
        let mut folders: Vec<NoteFolder> = self.state().folders.values().cloned().collect();
        folders.sort_by(|a, b| (a.sort_key(), a.id()).cmp(&(b.sort_key(), b.id())));
        Ok(folders)
    }

    async fn list_folder_items(
        &self,
        query: FolderItemsQuery,
        page: PageRequest,
    ) -> CoreResult<Page<NoteTreeItem>> {
        check_page_limit(page.limit)?;
        let after = page.cursor.as_deref().map(decode_tree_cursor).transpose()?;
        let state = self.state();
        let folders = state
            .folders
            .values()
            .filter(|f| f.parent() == query.folder)
            .cloned()
            .map(NoteTreeItem::Folder);
        let notes = state
            .notes
            .values()
            .filter(|n| n.folder() == query.folder)
            .map(|n| NoteTreeItem::Note(NoteSummary::from(n)));
        let mut items: Vec<(_, NoteTreeItem)> = folders
            .chain(notes)
            .filter(|item| query.include_trashed || !item.is_trashed())
            .map(|item| (item.position(), item))
            .filter(|(pos, _)| after.as_ref().is_none_or(|a| pos > a))
            .collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        let limit = page.limit as usize;
        let next_cursor = (items.len() > limit).then(|| encode_tree_cursor(&items[limit - 1].0));
        Ok(Page {
            items: items
                .into_iter()
                .take(limit)
                .map(|(_, item)| item)
                .collect(),
            next_cursor,
        })
    }

    async fn list_trashed_notes(&self) -> CoreResult<Vec<NoteSummary>> {
        Ok(self
            .state()
            .notes
            .values()
            .filter(|n| n.is_trashed())
            .map(NoteSummary::from)
            .collect())
    }

    async fn last_note_sort_key(
        &self,
        folder: Option<NoteFolderId>,
    ) -> CoreResult<Option<SortKey>> {
        let state = self.state();
        let notes = state
            .notes
            .values()
            .filter(|n| n.folder() == folder)
            .map(Note::sort_key);
        let folders = state
            .folders
            .values()
            .filter(|f| f.parent() == folder)
            .map(NoteFolder::sort_key);
        Ok(notes.chain(folders).max().cloned())
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
        let requested = changes.entities();
        self.transaction(|state| {
            let recorded = sync::record_local(state, changes)?;
            let changed = recorded.changed(requested);
            for change in recorded.changes {
                apply(state, change);
            }
            Ok(CommitReceipt {
                changed,
                conflicts: recorded.conflicts,
            })
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

    async fn freeze_outbox(&self, up_to: u64) -> CoreResult<()> {
        let mut state = self.state();
        state.frozen_up_to = state.frozen_up_to.max(up_to);
        Ok(())
    }

    async fn revision_ops(&self) -> CoreResult<Vec<Op>> {
        let state = self.state();
        Ok(state
            .revisions
            .iter()
            .filter(|(note, _)| {
                !state
                    .entities
                    .get(&EntityRef::Note(**note))
                    .is_some_and(EntityState::is_deleted)
            })
            .flat_map(|(note, revs)| revs.iter().map(|rev| sync::revise_op(*note, rev)))
            .collect())
    }

    async fn prune_revisions(&self, acked: Vec<(DeviceId, u64)>) -> CoreResult<u32> {
        let acked = acked.into_iter().collect();
        self.transaction(|state| sync::prune_revisions(state, &acked))
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

    async fn sync_cursors(&self) -> CoreResult<Vec<(DeviceId, u64)>> {
        Ok(self
            .state()
            .cursors
            .iter()
            .map(|(device, seq)| (*device, *seq))
            .collect())
    }

    async fn entity_state(&self, entity: EntityRef) -> CoreResult<Option<EntityState>> {
        Ok(self.state().entities.get(&entity).cloned())
    }

    async fn entity_states(&self) -> CoreResult<Vec<(EntityRef, EntityState)>> {
        Ok(self
            .state()
            .entities
            .iter()
            .map(|(entity, state)| (*entity, state.clone()))
            .collect())
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

    fn set_entity_state(
        &mut self,
        entity: EntityRef,
        _before: Option<&EntityState>,
        state: &EntityState,
    ) -> CoreResult<()> {
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

    fn revisions(&mut self, note: NoteId) -> CoreResult<RevisionSet> {
        Ok(self.revisions.get(&note).cloned().unwrap_or_default())
    }

    fn revision_heads(&mut self, note: NoteId) -> CoreResult<Vec<Revision>> {
        Ok(self
            .revisions
            .get(&note)
            .map(|revs| revs.heads().into_iter().cloned().collect())
            .unwrap_or_default())
    }

    fn revision(&mut self, note: NoteId, id: RevisionId) -> CoreResult<Option<Revision>> {
        Ok(self
            .revisions
            .get(&note)
            .and_then(|revs| revs.get(id))
            .cloned())
    }

    fn put_revision(&mut self, note: NoteId, rev: &Revision) -> CoreResult<()> {
        let revs = self.revisions.entry(note).or_default();
        revs.remove(rev.id);
        revs.insert(rev.clone());
        Ok(())
    }

    fn remove_revisions(&mut self, note: NoteId, ids: Option<&[RevisionId]>) -> CoreResult<()> {
        match ids {
            None => {
                self.revisions.remove(&note);
            }
            Some(ids) => {
                if let Some(revs) = self.revisions.get_mut(&note) {
                    for id in ids {
                        revs.remove(*id);
                    }
                }
            }
        }
        Ok(())
    }

    fn notes_with_revisions(&mut self) -> CoreResult<Vec<NoteId>> {
        Ok(self
            .revisions
            .iter()
            .filter(|(_, revs)| !revs.is_empty())
            .map(|(id, _)| *id)
            .collect())
    }

    fn replaceable_op(
        &mut self,
        entity: EntityRef,
        field: Option<FieldName>,
    ) -> CoreResult<Option<Op>> {
        let latest = self.outbox.iter().rev().find(|op| {
            op.entity == entity
                && match (&op.kind, field) {
                    (OpKind::Revise { .. }, None) => true,
                    (OpKind::Set { field: f, .. }, Some(wanted)) => *f == wanted,
                    (OpKind::Create { fields }, Some(wanted)) => fields.contains_key(&wanted),
                    _ => false,
                }
        });
        Ok(latest
            .filter(|op| op.id.seq > self.frozen_up_to && !matches!(op.kind, OpKind::Create { .. }))
            .cloned())
    }

    fn replace_outbox(&mut self, op: &Op) -> CoreResult<()> {
        if let Some(slot) = self.outbox.iter_mut().find(|o| o.id == op.id) {
            *slot = op.clone();
        }
        Ok(())
    }

    fn live_note_folders(&mut self) -> CoreResult<Vec<(NoteFolderId, Registers)>> {
        Ok(self
            .entities
            .iter()
            .filter_map(|(entity, state)| match (entity, state) {
                (EntityRef::NoteFolder(id), EntityState::Live(registers)) => {
                    Some((*id, registers.clone()))
                }
                _ => None,
            })
            .collect())
    }

    fn live_notes(&mut self) -> CoreResult<Vec<(NoteId, Registers)>> {
        Ok(self
            .entities
            .iter()
            .filter_map(|(entity, state)| match (entity, state) {
                (EntityRef::Note(id), EntityState::Live(registers)) => {
                    Some((*id, registers.clone()))
                }
                _ => None,
            })
            .collect())
    }

    fn next_sort_key(
        &mut self,
        folder: Option<NoteFolderId>,
        after: &SortKey,
    ) -> CoreResult<Option<SortKey>> {
        let notes = self
            .notes
            .values()
            .filter(|n| n.folder() == folder)
            .map(Note::sort_key);
        let folders = self
            .folders
            .values()
            .filter(|f| f.parent() == folder)
            .map(NoteFolder::sort_key);
        Ok(notes.chain(folders).filter(|k| *k > after).min().cloned())
    }

    fn note(&mut self, id: NoteId) -> CoreResult<Option<Note>> {
        Ok(self.notes.get(&id).cloned())
    }

    fn note_folder(&mut self, id: NoteFolderId) -> CoreResult<Option<NoteFolder>> {
        Ok(self.folders.get(&id).cloned())
    }

    fn put_note(&mut self, note: &Note) -> CoreResult<()> {
        apply(self, Change::UpsertNote(note.clone()));
        Ok(())
    }

    fn put_note_folder(&mut self, folder: &NoteFolder) -> CoreResult<()> {
        apply(self, Change::UpsertNoteFolder(folder.clone()));
        Ok(())
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
