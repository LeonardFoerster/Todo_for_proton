//! FFI-freundliche Fassade für die UI-Shells (Tauri-Commands, wasm-bindgen, später UniFFI).
//!
//! Nimmt und liefert nur DTOs ([`dto`]); IDs als `String`. Jede erfolgreiche Schreiboperation
//! meldet die tatsächlich geschriebenen Entities als [`CoreEvent::Changed`] an alle [`EventSink`]s.

pub mod dto;

use std::{
    fmt,
    sync::{Arc, RwLock},
};

use self::dto::{
    list_id, task_id, CreateTaskDto, MoveListTargetDto, MoveTargetDto, PageDto, PageRequestDto,
    TaskDto, TaskListDto, TaskPatchDto, TaskQueryDto, TaskSummaryDto,
};
use crate::{
    app::{
        check_bulk_len, CompleteTask, CompleteTaskInput, CompleteTasks, CompleteTasksInput,
        CreateTask, CreateTaskInput, CreateTaskList, CreateTaskListInput, DeleteTask,
        DeleteTaskInput, DeleteTaskList, DeleteTaskListInput, DeleteTasks, DeleteTasksInput, Deps,
        EnsureInbox, MoveTask, MoveTaskInput, MoveTaskList, MoveTaskListInput, MoveTasks,
        MoveTasksInput, RenameTaskList, RenameTaskListInput, ReopenTask, ReopenTaskInput,
        RestoreTasks, RestoreTasksInput, TaskPatch, UpdateTask, UpdateTaskInput,
    },
    domain::{DeviceId, Task, TaskId, TaskList, TaskListId, TaskParts},
    ports::{
        ChangeSet, Clock, CommitReceipt, EntityRef, MaybeSend, MaybeSync, Page, PageRequest,
        TaskQuery, TaskRepository, TaskSummary,
    },
    sync::{EntityState, Op, RemoteBatch, RemoteReceipt},
    CoreError, CoreResult,
};

/// Benachrichtigung an die UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    /// Diese Entities wurden geschrieben (angelegt, geändert oder gelöscht).
    Changed {
        tasks: Vec<String>,
        lists: Vec<String>,
    },
}

/// Empfänger für [`CoreEvent`]s (z. B. Tauri-Event-Emitter, `postMessage` im Worker).
pub trait EventSink: MaybeSend + MaybeSync {
    fn emit(&self, event: CoreEvent);
}

/// Ein geteilter Empfänger (z. B. `Arc<MyEmitter>`) ist selbst ein Empfänger.
impl<T: EventSink + ?Sized> EventSink for Arc<T> {
    fn emit(&self, event: CoreEvent) {
        (**self).emit(event);
    }
}

type Sinks = Arc<RwLock<Vec<Box<dyn EventSink>>>>;

/// Die Fassade. Günstig über `Arc` teilbar; alle Methoden nehmen `&self`.
pub struct CoreApi {
    deps: Deps,
    sinks: Sinks,
}

impl fmt::Debug for CoreApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CoreApi").finish_non_exhaustive()
    }
}

impl CoreApi {
    #[must_use]
    pub fn new(repo: Arc<dyn TaskRepository>, clock: Arc<dyn Clock>) -> Self {
        let sinks: Sinks = Arc::default();
        let repo = Arc::new(NotifyingRepository {
            inner: repo,
            clock: clock.clone(),
            sinks: sinks.clone(),
        });
        Self {
            deps: Deps::new(repo, clock),
            sinks,
        }
    }

    /// Das Repository, wie die Use-Cases es sehen: Jeder Commit bekommt die aktuelle Zeit
    /// (HLC der Ops), jede Änderung – auch durch `apply_remote` – wird als Event gemeldet.
    /// Die Sync-Engine muss **dieses** Repository verwenden, damit die UI Remote-Änderungen sieht.
    #[must_use]
    pub fn repository(&self) -> Arc<dyn TaskRepository> {
        self.deps.repo.clone()
    }

    /// Registriert einen Empfänger für alle folgenden Events.
    pub fn subscribe(&self, sink: Box<dyn EventSink>) {
        write_sinks(&self.sinks).push(sink);
    }

    // --- Listen ---------------------------------------------------------------------------------

    /// Stellt die Inbox sicher (beim Start aufrufen) und liefert sie.
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn ensure_inbox(&self) -> CoreResult<TaskListDto> {
        Ok((&EnsureInbox::new(self.deps.clone()).execute().await?).into())
    }

    /// Alle Listen: die Inbox zuerst, dann die Benutzerlisten nach `sort_key` (ADR-023).
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn list_task_lists(&self) -> CoreResult<Vec<TaskListDto>> {
        let mut lists = self.deps.repo.list_task_lists().await?;
        // Stabil: Reihenfolge der übrigen bleibt die des Repositories.
        lists.sort_by_key(|l| !l.is_inbox());
        Ok(lists.iter().map(TaskListDto::from).collect())
    }

    /// # Errors
    /// `Validation` bei ungültigem Namen.
    pub async fn create_task_list(&self, name: String) -> CoreResult<TaskListDto> {
        let list = CreateTaskList::new(self.deps.clone())
            .execute(CreateTaskListInput { name })
            .await?;
        Ok((&list).into())
    }

    /// # Errors
    /// `NotFound`, `Validation` (Inbox, ungültiger Name/ID).
    pub async fn rename_task_list(&self, id: String, name: String) -> CoreResult<TaskListDto> {
        let input = RenameTaskListInput {
            id: list_id(&id)?,
            name,
        };
        Ok((&RenameTaskList::new(self.deps.clone())
            .execute(input)
            .await?)
            .into())
    }

    /// Sortiert eine Liste unter den Benutzerlisten um (die Inbox bleibt vorn).
    ///
    /// # Errors
    /// `NotFound`, `Validation` (Inbox, Nachbarn, ID).
    pub async fn move_task_list(
        &self,
        id: String,
        target: MoveListTargetDto,
    ) -> CoreResult<TaskListDto> {
        let input = MoveTaskListInput {
            id: list_id(&id)?,
            after: target.after.as_deref().map(list_id).transpose()?,
            before: target.before.as_deref().map(list_id).transpose()?,
        };
        Ok((&MoveTaskList::new(self.deps.clone()).execute(input).await?).into())
    }

    /// Löscht eine Liste; ihre Tasks wandern in die Inbox. Liefert die Zahl verschobener Tasks.
    ///
    /// # Errors
    /// `NotFound`, `Validation` (Inbox, ungültige ID).
    pub async fn delete_task_list(&self, id: String) -> CoreResult<u32> {
        let out = DeleteTaskList::new(self.deps.clone())
            .execute(DeleteTaskListInput { id: list_id(&id)? })
            .await?;
        Ok(u32::try_from(out.moved_tasks).unwrap_or(u32::MAX))
    }

    // --- Tasks ----------------------------------------------------------------------------------

    /// # Errors
    /// `NotFound` (Liste), `Validation`.
    pub async fn create_task(&self, input: CreateTaskDto) -> CoreResult<TaskDto> {
        let input = CreateTaskInput {
            title: input.title,
            list_id: input.list_id.as_deref().map(list_id).transpose()?,
        };
        Ok((&CreateTask::new(self.deps.clone()).execute(input).await?).into())
    }

    /// Vollständige Task (inkl. Notizen) für die Detailansicht.
    ///
    /// # Errors
    /// `NotFound`, `Validation` (ID).
    pub async fn get_task(&self, id: String) -> CoreResult<TaskDto> {
        let id = task_id(&id)?;
        let task = self
            .deps
            .repo
            .get_task(id)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("task {id}")))?;
        Ok((&task).into())
    }

    /// # Errors
    /// `NotFound`, `Validation`.
    pub async fn update_task(&self, id: String, patch: TaskPatchDto) -> CoreResult<TaskDto> {
        let patch = TaskPatch {
            title: patch.title,
            notes: patch.notes,
            priority: patch.priority.map(Into::into),
            due: patch
                .due
                .map(|due| due.map(TryInto::try_into).transpose())
                .transpose()?,
        };
        let input = UpdateTaskInput {
            id: task_id(&id)?,
            patch,
        };
        Ok((&UpdateTask::new(self.deps.clone()).execute(input).await?).into())
    }

    /// Hakt ab (`done = true`) oder öffnet wieder (`done = false`).
    ///
    /// # Errors
    /// `NotFound`, `Validation` (ID).
    pub async fn complete_task(&self, id: String, done: bool) -> CoreResult<TaskDto> {
        let id = task_id(&id)?;
        let task = if done {
            CompleteTask::new(self.deps.clone())
                .execute(CompleteTaskInput { id })
                .await?
        } else {
            ReopenTask::new(self.deps.clone())
                .execute(ReopenTaskInput { id })
                .await?
        };
        Ok((&task).into())
    }

    /// # Errors
    /// `NotFound`, `Validation`.
    pub async fn move_task(&self, id: String, target: MoveTargetDto) -> CoreResult<TaskDto> {
        let input = MoveTaskInput {
            id: task_id(&id)?,
            list_id: target.list_id.as_deref().map(list_id).transpose()?,
            after: target.after.as_deref().map(task_id).transpose()?,
            before: target.before.as_deref().map(task_id).transpose()?,
        };
        Ok((&MoveTask::new(self.deps.clone()).execute(input).await?).into())
    }

    /// # Errors
    /// `NotFound`, `Validation` (ID).
    pub async fn delete_task(&self, id: String) -> CoreResult<()> {
        DeleteTask::new(self.deps.clone())
            .execute(DeleteTaskInput { id: task_id(&id)? })
            .await
    }

    // --- Sammelaktionen (je ein Commit, ein `Changed`-Event; unbekannte IDs übersprungen) ------

    /// Hakt alle ab (`done = true`) bzw. öffnet alle wieder; liefert die tatsächlich geänderten.
    ///
    /// # Errors
    /// `Validation` (ID, mehr als [`crate::app::BULK_LIMIT_MAX`]).
    pub async fn complete_tasks(&self, ids: Vec<String>, done: bool) -> CoreResult<Vec<TaskDto>> {
        let input = CompleteTasksInput {
            ids: task_ids(&ids)?,
            done,
        };
        Ok(dtos(
            &CompleteTasks::new(self.deps.clone()).execute(input).await?,
        ))
    }

    /// Löscht alle; liefert die gelöschten Tasks im Zustand davor (für `restore_tasks`).
    ///
    /// # Errors
    /// `Validation` (ID, Obergrenze).
    pub async fn delete_tasks(&self, ids: Vec<String>) -> CoreResult<Vec<TaskDto>> {
        let input = DeleteTasksInput {
            ids: task_ids(&ids)?,
        };
        Ok(dtos(
            &DeleteTasks::new(self.deps.clone()).execute(input).await?,
        ))
    }

    /// Verschiebt alle als Block (bisherige Anzeigereihenfolge) an `target`; ohne `listId` in die
    /// gemeinsame bisherige Liste. Liefert die tatsächlich geänderten Tasks in neuer Reihenfolge.
    ///
    /// # Errors
    /// `NotFound` (Liste), `Validation` (ID, Obergrenze, Nachbarn, gemischte Listen ohne Ziel).
    pub async fn move_tasks(
        &self,
        ids: Vec<String>,
        target: MoveTargetDto,
    ) -> CoreResult<Vec<TaskDto>> {
        let input = MoveTasksInput {
            ids: task_ids(&ids)?,
            list_id: target.list_id.as_deref().map(list_id).transpose()?,
            after: target.after.as_deref().map(task_id).transpose()?,
            before: target.before.as_deref().map(task_id).transpose()?,
        };
        Ok(dtos(
            &MoveTasks::new(self.deps.clone()).execute(input).await?,
        ))
    }

    /// Legt Kopien gelöschter Tasks (aus `delete_tasks`) mit **neuen** IDs an, an alter Stelle.
    ///
    /// # Errors
    /// `Validation` (Felder, Obergrenze).
    pub async fn restore_tasks(&self, tasks: Vec<TaskDto>) -> CoreResult<Vec<TaskDto>> {
        check_bulk_len(tasks.len())?;
        let tasks = tasks
            .iter()
            .map(TaskParts::try_from)
            .collect::<CoreResult<_>>()?;
        Ok(dtos(
            &RestoreTasks::new(self.deps.clone())
                .execute(RestoreTasksInput { tasks })
                .await?,
        ))
    }

    /// Eine Seite einer Ansicht.
    ///
    /// # Errors
    /// `Validation` (Abfrage, Limit, Cursor).
    pub async fn list_tasks(
        &self,
        query: TaskQueryDto,
        page: PageRequestDto,
    ) -> CoreResult<PageDto<TaskSummaryDto>> {
        let query = TaskQuery::try_from(&query)?;
        let page = PageRequest {
            cursor: page.cursor,
            limit: page.limit,
        };
        Ok(self.deps.repo.list_tasks(&query, page).await?.into())
    }
}

fn task_ids(ids: &[String]) -> CoreResult<Vec<TaskId>> {
    check_bulk_len(ids.len())?;
    ids.iter().map(|id| task_id(id)).collect()
}

fn dtos(tasks: &[Task]) -> Vec<TaskDto> {
    tasks.iter().map(TaskDto::from).collect()
}

fn write_sinks(sinks: &Sinks) -> std::sync::RwLockWriteGuard<'_, Vec<Box<dyn EventSink>>> {
    sinks
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn emit(sinks: &Sinks, changed: &[EntityRef]) {
    if changed.is_empty() {
        return;
    }
    let (mut tasks, mut lists) = (Vec::new(), Vec::new());
    for entity in changed {
        match entity {
            EntityRef::Task(id) => tasks.push(id.to_string()),
            EntityRef::TaskList(id) => lists.push(id.to_string()),
        }
    }
    let event = CoreEvent::Changed { tasks, lists };
    let sinks = sinks
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for sink in sinks.iter() {
        sink.emit(event.clone());
    }
}

/// Decorator: stempelt Commits mit der aktuellen Zeit und meldet jede erfolgreiche Änderung an
/// die Sinks. So muss kein Use-Case Events oder Sync kennen.
struct NotifyingRepository {
    inner: Arc<dyn TaskRepository>,
    clock: Arc<dyn Clock>,
    sinks: Sinks,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl TaskRepository for NotifyingRepository {
    async fn get_task(&self, id: TaskId) -> CoreResult<Option<Task>> {
        self.inner.get_task(id).await
    }

    async fn get_tasks(&self, ids: &[TaskId]) -> CoreResult<Vec<Task>> {
        self.inner.get_tasks(ids).await
    }

    async fn get_task_list(&self, id: TaskListId) -> CoreResult<Option<TaskList>> {
        self.inner.get_task_list(id).await
    }

    async fn list_task_lists(&self) -> CoreResult<Vec<TaskList>> {
        self.inner.list_task_lists().await
    }

    async fn list_tasks(
        &self,
        query: &TaskQuery,
        page: PageRequest,
    ) -> CoreResult<Page<TaskSummary>> {
        self.inner.list_tasks(query, page).await
    }

    async fn commit(&self, changes: ChangeSet) -> CoreResult<CommitReceipt> {
        let changes = changes.at(self.clock.now());
        let receipt = self.inner.commit(changes).await?;
        emit(&self.sinks, &receipt.changed);
        Ok(receipt)
    }

    async fn device_id(&self) -> CoreResult<DeviceId> {
        self.inner.device_id().await
    }

    async fn pending_ops(&self, limit: u32) -> CoreResult<Vec<Op>> {
        self.inner.pending_ops(limit).await
    }

    async fn pending_op_count(&self) -> CoreResult<u64> {
        self.inner.pending_op_count().await
    }

    async fn mark_ops_pushed(&self, up_to: u64) -> CoreResult<()> {
        self.inner.mark_ops_pushed(up_to).await
    }

    async fn apply_remote(&self, batch: RemoteBatch) -> CoreResult<RemoteReceipt> {
        let receipt = self.inner.apply_remote(batch).await?;
        emit(&self.sinks, &receipt.changed);
        Ok(receipt)
    }

    async fn sync_cursor(&self, device: DeviceId) -> CoreResult<Option<u64>> {
        self.inner.sync_cursor(device).await
    }

    async fn set_sync_cursor(&self, device: DeviceId, seq: u64) -> CoreResult<()> {
        self.inner.set_sync_cursor(device, seq).await
    }

    async fn sync_cursors(&self) -> CoreResult<Vec<(DeviceId, u64)>> {
        self.inner.sync_cursors().await
    }

    async fn entity_state(&self, entity: EntityRef) -> CoreResult<Option<EntityState>> {
        self.inner.entity_state(entity).await
    }

    async fn entity_states(&self) -> CoreResult<Vec<(EntityRef, EntityState)>> {
        self.inner.entity_states().await
    }

    async fn sync_value(&self, key: &str) -> CoreResult<Option<String>> {
        self.inner.sync_value(key).await
    }

    async fn set_sync_value(&self, key: &str, value: String) -> CoreResult<()> {
        self.inner.set_sync_value(key, value).await
    }
}

#[cfg(test)]
mod sync_tests {
    use std::sync::Mutex;

    use futures::executor::block_on;

    use super::*;
    use crate::{
        domain::{DeviceId, SortKey, Timestamp},
        ports::FixedClock,
        sync::{task_fields, HlcState, OpClock, OpKind},
        testing::InMemoryTaskRepository,
    };

    #[derive(Default)]
    struct Recorder(Mutex<Vec<CoreEvent>>);

    impl EventSink for Recorder {
        fn emit(&self, event: CoreEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn api() -> (CoreApi, Arc<Recorder>) {
        let api = CoreApi::new(
            Arc::new(InMemoryTaskRepository::new()),
            Arc::new(FixedClock(Timestamp::from_millis(1_234))),
        );
        let recorder = Arc::new(Recorder::default());
        api.subscribe(Box::new(recorder.clone()));
        (api, recorder)
    }

    #[test]
    fn commits_are_stamped_with_the_api_clock() {
        let (api, _) = api();
        block_on(api.ensure_inbox()).unwrap();
        let ops = block_on(api.repository().pending_ops(10)).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].hlc.wall_ms, 1_234);
    }

    #[test]
    fn remote_changes_are_reported_as_events() {
        let (api, recorder) = api();
        let inbox = block_on(api.ensure_inbox()).unwrap();
        recorder.0.lock().unwrap().clear();

        let clock = FixedClock(Timestamp::from_millis(5));
        let task = Task::new(TaskListId::INBOX, "von B", SortKey::first(), &clock).unwrap();
        let mut remote = OpClock::new(HlcState::new(DeviceId::new()), 0);
        let op = remote.stamp(
            Timestamp::from_millis(10),
            EntityRef::Task(task.id()),
            OpKind::Create {
                fields: task_fields(&task),
            },
        );
        let batch = RemoteBatch {
            ops: vec![op],
            cursors: Vec::new(),
            now: Timestamp::from_millis(20),
        };
        block_on(api.repository().apply_remote(batch)).unwrap();
        assert_eq!(
            *recorder.0.lock().unwrap(),
            [CoreEvent::Changed {
                tasks: vec![task.id().to_string()],
                lists: Vec::new(),
            }]
        );
        assert_eq!(inbox.id, TaskListId::INBOX.to_string());
        assert_eq!(
            block_on(api.get_task(task.id().to_string())).unwrap().title,
            "von B"
        );
    }
}

#[cfg(test)]
mod bulk_tests {
    use std::sync::Mutex;

    use futures::executor::block_on;

    use super::*;
    use crate::{
        domain::Timestamp,
        ports::FixedClock,
        sync::{FieldName, OpKind},
        testing::InMemoryTaskRepository,
    };

    #[derive(Default)]
    struct Recorder(Mutex<Vec<CoreEvent>>);

    impl EventSink for Recorder {
        fn emit(&self, event: CoreEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    struct Setup {
        api: CoreApi,
        events: Arc<Recorder>,
        ids: Vec<String>,
    }

    fn setup() -> Setup {
        let api = CoreApi::new(
            Arc::new(InMemoryTaskRepository::new()),
            Arc::new(FixedClock(Timestamp::from_millis(1_234))),
        );
        let ids = ["a", "b", "c"]
            .iter()
            .map(|t| {
                block_on(api.create_task(CreateTaskDto {
                    title: (*t).into(),
                    list_id: None,
                }))
                .unwrap()
                .id
            })
            .collect();
        block_on(api.repository().mark_ops_pushed(u64::MAX)).unwrap();
        let events = Arc::new(Recorder::default());
        api.subscribe(Box::new(events.clone()));
        Setup { api, events, ids }
    }

    impl Setup {
        /// Events und neue Outbox-Ops seit dem letzten Aufruf.
        fn take(&self) -> (Vec<CoreEvent>, Vec<Op>) {
            let repo = self.api.repository();
            let ops = block_on(repo.pending_ops(u32::MAX)).unwrap();
            block_on(repo.mark_ops_pushed(u64::MAX)).unwrap();
            (std::mem::take(&mut *self.events.0.lock().unwrap()), ops)
        }
    }

    fn changed(ids: &[String]) -> Vec<CoreEvent> {
        vec![CoreEvent::Changed {
            tasks: ids.to_vec(),
            lists: Vec::new(),
        }]
    }

    fn fields(ops: &[Op]) -> Vec<Option<FieldName>> {
        ops.iter()
            .map(|op| match &op.kind {
                OpKind::Set { field, .. } => Some(*field),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn each_bulk_call_is_one_commit_with_one_event() {
        let s = setup();
        let done = block_on(s.api.complete_tasks(s.ids.clone(), true)).unwrap();
        assert_eq!(done.len(), 3);
        let (events, ops) = s.take();
        assert_eq!(events, changed(&s.ids));
        assert_eq!(fields(&ops), vec![Some(FieldName::Status); 3]);

        let moved = block_on(s.api.move_tasks(
            vec![s.ids[0].clone(), s.ids[1].clone()],
            MoveTargetDto {
                after: Some(s.ids[2].clone()),
                ..MoveTargetDto::default()
            },
        ))
        .unwrap();
        assert_eq!(moved.len(), 2);
        let (events, ops) = s.take();
        assert_eq!(events, changed(&s.ids[..2]));
        assert_eq!(fields(&ops), vec![Some(FieldName::SortKey); 2]);

        let deleted = block_on(s.api.delete_tasks(s.ids.clone())).unwrap();
        assert_eq!(deleted.len(), 3);
        let (events, ops) = s.take();
        assert_eq!(events, changed(&s.ids));
        assert!(ops.iter().all(|op| op.kind == OpKind::Delete));
        assert_eq!(ops.len(), 3);

        let restored = block_on(s.api.restore_tasks(deleted.clone())).unwrap();
        let new_ids: Vec<String> = restored.iter().map(|t| t.id.clone()).collect();
        let (events, ops) = s.take();
        assert_eq!(events, changed(&new_ids));
        assert!(ops
            .iter()
            .all(|op| matches!(op.kind, OpKind::Create { .. })));
        for (old, new) in deleted.iter().zip(&restored) {
            assert_eq!(
                (
                    &old.title,
                    &old.sort_key,
                    &old.created_at,
                    &old.completed_at
                ),
                (
                    &new.title,
                    &new.sort_key,
                    &new.created_at,
                    &new.completed_at
                )
            );
        }
    }

    #[test]
    fn noop_and_invalid_calls_emit_nothing() {
        let s = setup();
        assert!(block_on(s.api.complete_tasks(Vec::new(), true))
            .unwrap()
            .is_empty());
        assert!(block_on(s.api.complete_tasks(s.ids.clone(), false))
            .unwrap()
            .is_empty());
        assert!(matches!(
            block_on(s.api.delete_tasks(vec![s.ids[0].clone(), "kaputt".into()])),
            Err(CoreError::Validation(_))
        ));
        assert!(block_on(s.api.restore_tasks(Vec::new()))
            .unwrap()
            .is_empty());
        let (events, ops) = s.take();
        assert!(events.is_empty());
        assert!(ops.is_empty());
        assert!(block_on(s.api.get_task(s.ids[0].clone())).is_ok());
    }
}

#[cfg(test)]
mod list_order_tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        domain::{SortKey, Timestamp},
        ports::FixedClock,
        testing::InMemoryTaskRepository,
    };

    /// ADR-023: Die Inbox steht vorn, auch wenn ihr Sort-Key (z. B. von einem anderen Gerät)
    /// größer ist als der einer Benutzerliste.
    #[test]
    fn inbox_comes_first_and_lists_can_be_reordered() {
        let api = CoreApi::new(
            Arc::new(InMemoryTaskRepository::new()),
            Arc::new(FixedClock(Timestamp::from_millis(1_234))),
        );
        let a = block_on(api.create_task_list("A".into())).unwrap();
        let b = block_on(api.create_task_list("B".into())).unwrap();
        let clock = FixedClock(Timestamp::from_millis(5));
        let late = SortKey::parse("b00").unwrap();
        block_on(
            api.repository()
                .commit(ChangeSet::new().upsert_list(TaskList::inbox(late, &clock))),
        )
        .unwrap();
        let names = || -> Vec<String> {
            block_on(api.list_task_lists())
                .unwrap()
                .into_iter()
                .map(|l| l.name)
                .collect()
        };
        assert_eq!(names(), ["Inbox", "A", "B"]);

        let moved = block_on(api.move_task_list(
            b.id.clone(),
            MoveListTargetDto {
                after: None,
                before: Some(a.id.clone()),
            },
        ))
        .unwrap();
        assert_eq!(moved.name, "B");
        assert_eq!(names(), ["Inbox", "B", "A"]);
        let inbox = block_on(
            api.move_task_list(TaskListId::INBOX.to_string(), MoveListTargetDto::default()),
        );
        assert!(matches!(inbox, Err(CoreError::Validation(_))));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn api_and_futures_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        fn assert_send<T: Send>(_: T) {}
        assert_send_sync::<CoreApi>();
        let api = CoreApi::new(
            Arc::new(crate::testing::InMemoryTaskRepository::new()),
            Arc::new(crate::ports::FixedClock(crate::domain::Timestamp::EPOCH)),
        );
        assert_send(api.ensure_inbox());
        assert_send(api.complete_tasks(Vec::new(), true));
        assert_send(api.delete_tasks(Vec::new()));
        assert_send(api.move_tasks(Vec::new(), MoveTargetDto::default()));
        assert_send(api.restore_tasks(Vec::new()));
        assert_send(api.move_task_list(String::new(), MoveListTargetDto::default()));
        assert_send(api.list_tasks(
            TaskQueryDto {
                view: dto::ViewDto::All,
                status: None,
                tag: None,
            },
            PageRequestDto {
                cursor: None,
                limit: 10,
            },
        ));
    }
}
