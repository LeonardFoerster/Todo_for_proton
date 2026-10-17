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
    list_id, task_id, CreateTaskDto, MoveTargetDto, PageDto, PageRequestDto, TaskDto, TaskListDto,
    TaskPatchDto, TaskQueryDto, TaskSummaryDto,
};
use crate::{
    app::{
        CompleteTask, CompleteTaskInput, CreateTask, CreateTaskInput, CreateTaskList,
        CreateTaskListInput, DeleteTask, DeleteTaskInput, DeleteTaskList, DeleteTaskListInput,
        Deps, EnsureInbox, MoveTask, MoveTaskInput, RenameTaskList, RenameTaskListInput,
        ReopenTask, ReopenTaskInput, TaskPatch, UpdateTask, UpdateTaskInput,
    },
    domain::{Task, TaskId, TaskList, TaskListId},
    ports::{
        ChangeSet, Clock, CommitReceipt, EntityRef, MaybeSend, MaybeSync, Page, PageRequest,
        TaskQuery, TaskRepository, TaskSummary,
    },
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
            sinks: sinks.clone(),
        });
        Self {
            deps: Deps::new(repo, clock),
            sinks,
        }
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

    /// # Errors
    /// Fehler des Repositories.
    pub async fn list_task_lists(&self) -> CoreResult<Vec<TaskListDto>> {
        Ok(self
            .deps
            .repo
            .list_task_lists()
            .await?
            .iter()
            .map(TaskListDto::from)
            .collect())
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

fn write_sinks(sinks: &Sinks) -> std::sync::RwLockWriteGuard<'_, Vec<Box<dyn EventSink>>> {
    sinks
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn emit(sinks: &Sinks, receipt: &CommitReceipt) {
    if receipt.changed.is_empty() {
        return;
    }
    let (mut tasks, mut lists) = (Vec::new(), Vec::new());
    for entity in &receipt.changed {
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

/// Decorator: meldet jeden erfolgreichen Commit an die Sinks. So muss kein Use-Case Events kennen.
struct NotifyingRepository {
    inner: Arc<dyn TaskRepository>,
    sinks: Sinks,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl TaskRepository for NotifyingRepository {
    async fn get_task(&self, id: TaskId) -> CoreResult<Option<Task>> {
        self.inner.get_task(id).await
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
        let receipt = self.inner.commit(changes).await?;
        emit(&self.sinks, &receipt);
        Ok(receipt)
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
