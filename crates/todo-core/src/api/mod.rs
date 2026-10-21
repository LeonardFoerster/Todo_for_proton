//! FFI-freundliche Fassade für die UI-Shells (Tauri-Commands, wasm-bindgen, später UniFFI).
//!
//! Nimmt und liefert nur DTOs ([`dto`]); IDs als `String`. Jede erfolgreiche Schreiboperation
//! meldet die tatsächlich geschriebenen Entities als [`CoreEvent::Changed`] an alle [`EventSink`]s.

pub mod dto;
pub mod export_dto;
pub mod note_dto;

use std::{
    fmt,
    sync::{Arc, RwLock},
};

use self::{
    dto::{
        list_id, task_id, CreateTaskDto, MoveListTargetDto, MoveTargetDto, PageDto, PageRequestDto,
        TaskDto, TaskDueDto, TaskListDto, TaskPatchDto, TaskQueryDto, TaskSummaryDto,
    },
    note_dto::{
        folder_id, note_id, revision_id, ImportNoteFileDto, ImportReportDto, NoteConflictDto,
        NoteDto, NoteFolderDto, NoteHitDto, NoteItemDto, NoteKindDto, NoteTargetDto,
        NoteTrashEntryDto, NoteTreeItemDto,
    },
};
use crate::{
    app::{
        check_bulk_len, CompleteTask, CompleteTaskInput, CompleteTasks, CompleteTasksInput,
        CreateTask, CreateTaskInput, CreateTaskList, CreateTaskListInput, DeleteTask,
        DeleteTaskInput, DeleteTaskList, DeleteTaskListInput, DeleteTasks, DeleteTasksInput, Deps,
        EnsureInbox, MoveTask, MoveTaskInput, MoveTaskList, MoveTaskListInput, MoveTasks,
        MoveTasksInput, RenameTaskList, RenameTaskListInput, ReopenTask, ReopenTaskInput,
        RestoreTasks, RestoreTasksInput, SetDues, SetDuesInput, TaskPatch, UpdateTask,
        UpdateTaskInput,
    },
    app::{
        CreateNote, CreateNoteFolder, CreateNoteFolderInput, CreateNoteInput, DiscardEmptyNote,
        DiscardEmptyNoteInput, EmptyNoteTrash, ImportNoteFile, ImportNotes, ImportNotesInput,
        ListNoteTrash, MoveNote, MoveNoteFolder, MoveNoteFolderInput, MoveNoteInput, NoteItem,
        PurgeExpiredNotes, PurgeNoteItems, PurgeNoteItemsInput, RenameNote, RenameNoteFolder,
        RenameNoteFolderInput, RenameNoteInput, RestoreNoteItems, RestoreNoteItemsInput,
        SetNoteKind, SetNoteKindInput, TrashNoteItems, TrashNoteItemsInput, UpdateNoteBody,
        UpdateNoteBodyInput, NOTE_BULK_LIMIT_MAX,
    },
    app::{
        EmptyTaskTrash, PurgeExpiredTasks, PurgeTasks, PurgeTasksInput, TrashTasks,
        TrashTasksInput, UntrashTasks, UntrashTasksInput,
    },
    app::{ExportData, ExportNotes, ImportData, ImportDataInput},
    domain::{
        DeviceId, Note, NoteFolder, NoteFolderId, NoteId, SortKey, Task, TaskId, TaskList,
        TaskListId, TaskParts,
    },
    ports::{
        ChangeSet, Clock, CommitReceipt, EntityRef, FolderItemsQuery, MaybeSend, MaybeSync,
        NoteHit, NoteRepository, NoteSummary, NoteTreeItem, Page, PageRequest, SearchQuery,
        TaskQuery, TaskRepository, TaskSummary,
    },
    sync::{EntityState, NoteConflict, Op, RemoteBatch, RemoteReceipt},
    CoreError, CoreResult,
};

/// Benachrichtigung an die UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    /// Diese Entities wurden geschrieben (angelegt, geändert oder gelöscht).
    Changed {
        tasks: Vec<String>,
        lists: Vec<String>,
        notes: Vec<String>,
        note_folders: Vec<String>,
    },
    /// Beim Zusammenlegen nebenläufiger Bearbeitungen entstanden Konfliktkopien (ADR-029).
    NoteConflicts { conflicts: Vec<NoteConflictDto> },
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

    /// Mit Zeitzonen (Serien mit Uhrzeit, N4). Vor dem ersten Aufruf setzen.
    #[must_use]
    pub fn with_time_zones(mut self, zones: Arc<dyn crate::ports::TimeZones>) -> Self {
        self.deps = self.deps.with_zones(zones);
        self
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
            notes: input.notes.unwrap_or_default(),
            priority: input.priority.map(Into::into).unwrap_or_default(),
            due: input.due.map(TryInto::try_into).transpose()?,
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

    /// Legt Tasks in den Papierkorb (ADR-031); liefert die tatsächlich geänderten (für Rückgängig
    /// per [`Self::untrash_tasks`], gleiche IDs).
    ///
    /// # Errors
    /// `Validation` (ID, Obergrenze).
    pub async fn trash_tasks(&self, ids: Vec<String>) -> CoreResult<Vec<TaskDto>> {
        let input = TrashTasksInput {
            ids: task_ids(&ids)?,
        };
        Ok(dtos(
            &TrashTasks::new(self.deps.clone()).execute(input).await?,
        ))
    }

    /// Holt Tasks aus dem Papierkorb zurück (gleiche ID, alter Platz); liefert die geänderten.
    ///
    /// # Errors
    /// `Validation` (ID, Obergrenze).
    pub async fn untrash_tasks(&self, ids: Vec<String>) -> CoreResult<Vec<TaskDto>> {
        let input = UntrashTasksInput {
            ids: task_ids(&ids)?,
        };
        Ok(dtos(
            &UntrashTasks::new(self.deps.clone()).execute(input).await?,
        ))
    }

    /// Löscht Tasks aus dem Papierkorb endgültig (andere werden übersprungen); Zahl gelöschter.
    ///
    /// # Errors
    /// `Validation` (ID, Obergrenze).
    pub async fn purge_tasks(&self, ids: Vec<String>) -> CoreResult<u32> {
        let input = PurgeTasksInput {
            ids: task_ids(&ids)?,
        };
        PurgeTasks::new(self.deps.clone()).execute(input).await
    }

    /// Leert den Papierkorb der Tasks.
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn empty_task_trash(&self) -> CoreResult<u32> {
        EmptyTaskTrash::new(self.deps.clone()).execute().await
    }

    /// Löscht Tasks, die seit mehr als 30 Tagen im Papierkorb liegen (beim Start, höchstens täglich).
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn purge_expired_tasks(&self) -> CoreResult<u32> {
        PurgeExpiredTasks::new(self.deps.clone()).execute().await
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

    /// Setzt je Task eine eigene Fälligkeit (z. B. „Alle auf heute" mit Uhrzeit je Task, oder
    /// Rückgängig mit den alten Werten); liefert die tatsächlich geänderten.
    ///
    /// # Errors
    /// `Validation` (ID, Fälligkeit, Obergrenze) – dann ist nichts geschrieben.
    pub async fn set_dues(&self, dues: Vec<TaskDueDto>) -> CoreResult<Vec<TaskDto>> {
        check_bulk_len(dues.len())?;
        let dues = dues
            .into_iter()
            .map(|d| Ok((task_id(&d.id)?, d.due.map(TryInto::try_into).transpose()?)))
            .collect::<CoreResult<_>>()?;
        Ok(dtos(
            &SetDues::new(self.deps.clone())
                .execute(SetDuesInput { dues })
                .await?,
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

    /// Alle Listen und Tasks als Inhalt von `tasks.json` (ADR-032).
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn export_data(&self) -> CoreResult<export_dto::ExportDto> {
        let snapshot = ExportData::new(self.deps.clone()).execute().await?;
        Ok(export_dto::ExportDto::from_snapshot(
            &snapshot,
            self.deps.clock.now(),
        ))
    }

    /// Alle Notizen außerhalb des Papierkorbs mit Ordnerpfad (die Shell schreibt sie als Dateien).
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn export_notes(&self) -> CoreResult<Vec<export_dto::ExportNoteDto>> {
        Ok(ExportNotes::new(self.deps.clone())
            .execute()
            .await?
            .iter()
            .map(export_dto::ExportNoteDto::from)
            .collect())
    }

    /// Spielt einen Export ein: neue IDs, Listen über den Namen, Vorhandenes übersprungen.
    ///
    /// # Errors
    /// `Validation` (Format, Version, Felder); Fehler des Repositories.
    pub async fn import_data(
        &self,
        export: export_dto::ExportDto,
    ) -> CoreResult<export_dto::ImportDataReportDto> {
        let (lists, tasks) = export.into_import()?;
        Ok(ImportData::new(self.deps.clone())
            .execute(ImportDataInput { lists, tasks })
            .await?
            .into())
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

    /// Suche über Titel und Notizen (ADR-027): jedes Wort als Teilwort, ohne Groß/Klein; offene
    /// vor erledigten, Titeltreffer zuerst; höchstens `limit` (1..=500).
    ///
    /// # Errors
    /// `Validation` (Limit, Länge des Texts).
    pub async fn search_tasks(&self, text: String, limit: u32) -> CoreResult<Vec<TaskSummaryDto>> {
        let query = SearchQuery { text, limit };
        Ok(self
            .deps
            .repo
            .search_tasks(&query)
            .await?
            .iter()
            .map(TaskSummaryDto::from)
            .collect())
    }
}

impl CoreApi {
    // --- Notizen (ADR-029) ----------------------------------------------------------------------

    /// Alle Ordner (auch im Papierkorb), nach `sortKey`. Wenige; Grundlage für Pfade und
    /// „Verschieben nach…".
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn list_note_folders(&self) -> CoreResult<Vec<NoteFolderDto>> {
        Ok(self
            .deps
            .repo
            .list_note_folders()
            .await?
            .iter()
            .map(NoteFolderDto::from)
            .collect())
    }

    /// Eine Seite des Inhalts eines Ordners (`folderId = null` = Wurzel): Ordner und Notizen
    /// gemischt nach `sortKey`, ohne Elemente im Papierkorb.
    ///
    /// # Errors
    /// `Validation` (ID, Limit, Cursor).
    pub async fn list_note_items(
        &self,
        folder: Option<String>,
        page: PageRequestDto,
    ) -> CoreResult<PageDto<NoteTreeItemDto>> {
        let query = FolderItemsQuery {
            folder: folder.as_deref().map(folder_id).transpose()?,
            include_trashed: false,
        };
        let page = PageRequest {
            cursor: page.cursor,
            limit: page.limit,
        };
        Ok(self.deps.repo.list_folder_items(query, page).await?.into())
    }

    /// Vollständige Notiz (Editor).
    ///
    /// # Errors
    /// `NotFound`, `Validation` (ID).
    pub async fn get_note(&self, id: String) -> CoreResult<NoteDto> {
        let id = note_id(&id)?;
        let note = self
            .deps
            .repo
            .get_note(id)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("note {id}")))?;
        Ok((&note).into())
    }

    /// Neue, leere Notiz am Ende des Ordners.
    ///
    /// # Errors
    /// `NotFound` (Ordner), `Validation` (Papierkorb, ID).
    pub async fn create_note(
        &self,
        folder: Option<String>,
        kind: NoteKindDto,
    ) -> CoreResult<NoteDto> {
        let input = CreateNoteInput {
            folder: folder.as_deref().map(folder_id).transpose()?,
            kind: kind.into(),
        };
        Ok((&CreateNote::new(self.deps.clone()).execute(input).await?).into())
    }

    /// Speichert den Inhalt auf der Basis `baseRev` (zuletzt geliefertes `bodyRev`). Die Antwort
    /// trägt den aufgelösten Inhalt (weicht nur nach einem Merge mit einer Remote-Revision ab)
    /// und das neue `bodyRev`.
    ///
    /// # Errors
    /// `NotFound`, `Validation` (Größe, Papierkorb, IDs).
    pub async fn update_note_body(
        &self,
        id: String,
        text: String,
        base_rev: Option<String>,
    ) -> CoreResult<NoteDto> {
        let input = UpdateNoteBodyInput {
            id: note_id(&id)?,
            text,
            base_rev: base_rev.as_deref().map(revision_id).transpose()?,
        };
        Ok((&UpdateNoteBody::new(self.deps.clone())
            .execute(input)
            .await?)
            .into())
    }

    /// Verwirft eine leer verlassene neue Notiz (leerer Inhalt, Auto-Name); `true` = verworfen.
    ///
    /// # Errors
    /// `Validation` (ID).
    pub async fn discard_empty_note(&self, id: String) -> CoreResult<bool> {
        DiscardEmptyNote::new(self.deps.clone())
            .execute(DiscardEmptyNoteInput { id: note_id(&id)? })
            .await
    }

    /// # Errors
    /// `NotFound`, `Validation` (Name, Papierkorb, ID).
    pub async fn rename_note(&self, id: String, name: String) -> CoreResult<NoteDto> {
        let input = RenameNoteInput {
            id: note_id(&id)?,
            name,
        };
        Ok((&RenameNote::new(self.deps.clone()).execute(input).await?).into())
    }

    /// `.txt` ↔ `.md`; der Inhalt bleibt.
    ///
    /// # Errors
    /// `NotFound`, `Validation` (Papierkorb, ID).
    pub async fn set_note_kind(&self, id: String, kind: NoteKindDto) -> CoreResult<NoteDto> {
        let input = SetNoteKindInput {
            id: note_id(&id)?,
            kind: kind.into(),
        };
        Ok((&SetNoteKind::new(self.deps.clone()).execute(input).await?).into())
    }

    /// # Errors
    /// `NotFound`, `Validation` (Nachbarn, Papierkorb, IDs).
    pub async fn move_note(&self, id: String, target: NoteTargetDto) -> CoreResult<NoteDto> {
        let input = MoveNoteInput {
            id: note_id(&id)?,
            target: (&target).try_into()?,
        };
        Ok((&MoveNote::new(self.deps.clone()).execute(input).await?).into())
    }

    /// # Errors
    /// `NotFound` (Elternordner), `Validation` (Name, Tiefe, Papierkorb).
    pub async fn create_note_folder(
        &self,
        parent_id: Option<String>,
        name: String,
    ) -> CoreResult<NoteFolderDto> {
        let input = CreateNoteFolderInput {
            parent: parent_id.as_deref().map(folder_id).transpose()?,
            name,
        };
        Ok((&CreateNoteFolder::new(self.deps.clone())
            .execute(input)
            .await?)
            .into())
    }

    /// # Errors
    /// `NotFound`, `Validation` (Name, Papierkorb).
    pub async fn rename_note_folder(&self, id: String, name: String) -> CoreResult<NoteFolderDto> {
        let input = RenameNoteFolderInput {
            id: folder_id(&id)?,
            name,
        };
        Ok((&RenameNoteFolder::new(self.deps.clone())
            .execute(input)
            .await?)
            .into())
    }

    /// Verschiebt einen Ordner samt Inhalt (`target.folderId` = neuer Elternordner).
    ///
    /// # Errors
    /// `NotFound`, `Validation` (Zyklus, Tiefe, Nachbarn, Papierkorb).
    pub async fn move_note_folder(
        &self,
        id: String,
        target: NoteTargetDto,
    ) -> CoreResult<NoteFolderDto> {
        let input = MoveNoteFolderInput {
            id: folder_id(&id)?,
            target: (&target).try_into()?,
        };
        Ok((&MoveNoteFolder::new(self.deps.clone())
            .execute(input)
            .await?)
            .into())
    }

    /// In den Papierkorb; liefert die tatsächlich geänderten (für „Rückgängig").
    ///
    /// # Errors
    /// `Validation` (IDs, Obergrenze).
    pub async fn trash_note_items(&self, items: Vec<NoteItemDto>) -> CoreResult<Vec<NoteItemDto>> {
        let items = note_items(&items)?;
        let changed = TrashNoteItems::new(self.deps.clone())
            .execute(TrashNoteItemsInput { items })
            .await?;
        Ok(changed.into_iter().map(Into::into).collect())
    }

    /// Aus dem Papierkorb holen (bei fehlendem Elternordner in die Wurzel).
    ///
    /// # Errors
    /// `Validation` (IDs, Obergrenze).
    pub async fn restore_note_items(
        &self,
        items: Vec<NoteItemDto>,
    ) -> CoreResult<Vec<NoteItemDto>> {
        let items = note_items(&items)?;
        let changed = RestoreNoteItems::new(self.deps.clone())
            .execute(RestoreNoteItemsInput { items })
            .await?;
        Ok(changed.into_iter().map(Into::into).collect())
    }

    /// Endgültig löschen (samt Inhalt); liefert die Zahl gelöschter Elemente.
    ///
    /// # Errors
    /// `Validation` (IDs, Obergrenze).
    pub async fn purge_note_items(&self, items: Vec<NoteItemDto>) -> CoreResult<u32> {
        let items = note_items(&items)?;
        PurgeNoteItems::new(self.deps.clone())
            .execute(PurgeNoteItemsInput { items })
            .await
    }

    /// # Errors
    /// Fehler des Repositories.
    pub async fn empty_note_trash(&self) -> CoreResult<u32> {
        EmptyNoteTrash::new(self.deps.clone()).execute().await
    }

    /// Löscht, was seit mehr als 30 Tagen im Papierkorb liegt (beim Start, höchstens täglich).
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn purge_expired_notes(&self) -> CoreResult<u32> {
        PurgeExpiredNotes::new(self.deps.clone()).execute().await
    }

    /// Oberste Elemente im Papierkorb, neueste zuerst.
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn list_note_trash(&self) -> CoreResult<Vec<NoteTrashEntryDto>> {
        Ok(ListNoteTrash::new(self.deps.clone())
            .execute()
            .await?
            .iter()
            .map(NoteTrashEntryDto::from)
            .collect())
    }

    /// Suche über Name und Inhalt (wie `search_tasks`); mit Ordnerpfad und Ausschnitt.
    ///
    /// # Errors
    /// `Validation` (Limit, Länge).
    pub async fn search_notes(&self, text: String, limit: u32) -> CoreResult<Vec<NoteHitDto>> {
        let repo = self.deps.repo.as_ref();
        let hits = repo.search_notes(&SearchQuery { text, limit }).await?;
        if hits.is_empty() {
            return Ok(Vec::new());
        }
        let folders: std::collections::BTreeMap<_, _> = repo
            .list_note_folders()
            .await?
            .into_iter()
            .map(|f| (f.id(), f))
            .collect();
        let path = |mut folder: Option<NoteFolderId>| {
            let mut names = Vec::new();
            while let Some(id) = folder {
                let Some(f) = folders.get(&id) else { break };
                if names.len() > crate::domain::note_tree::NOTE_TREE_MAX_DEPTH {
                    break;
                }
                names.push(f.name().to_string());
                folder = f.parent();
            }
            names.reverse();
            names
        };
        Ok(hits
            .iter()
            .map(|hit| NoteHitDto::new(hit, path(hit.note.folder)))
            .collect())
    }

    /// Importiert eingelesene Dateien unter einem neuen Ordner `root` in der Wurzel (Commits zu
    /// je ≤ 500 Elementen).
    ///
    /// # Errors
    /// Fehler des Repositories (bis dahin importierte Blöcke bleiben).
    pub async fn import_notes(
        &self,
        root: String,
        files: Vec<ImportNoteFileDto>,
    ) -> CoreResult<ImportReportDto> {
        let input = ImportNotesInput {
            root,
            files: files
                .into_iter()
                .map(|f| ImportNoteFile {
                    dirs: f.dirs,
                    name: f.name,
                    kind: f.kind.into(),
                    text: f.text,
                })
                .collect(),
        };
        Ok((&ImportNotes::new(self.deps.clone()).execute(input).await?).into())
    }
}

fn note_items(items: &[NoteItemDto]) -> CoreResult<Vec<NoteItem>> {
    if items.len() > NOTE_BULK_LIMIT_MAX {
        return Err(CoreError::Validation(format!(
            "at most {NOTE_BULK_LIMIT_MAX} items per call, got {}",
            items.len()
        )));
    }
    items.iter().map(TryInto::try_into).collect()
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

fn emit_conflicts(sinks: &Sinks, conflicts: &[NoteConflict]) {
    if conflicts.is_empty() {
        return;
    }
    let event = CoreEvent::NoteConflicts {
        conflicts: conflicts
            .iter()
            .map(|c| NoteConflictDto {
                note_id: c.note.to_string(),
                copy_id: c.copy.to_string(),
            })
            .collect(),
    };
    let sinks = sinks
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for sink in sinks.iter() {
        sink.emit(event.clone());
    }
}

fn emit(sinks: &Sinks, changed: &[EntityRef]) {
    if changed.is_empty() {
        return;
    }
    let (mut tasks, mut lists, mut notes, mut note_folders) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for entity in changed {
        match entity {
            EntityRef::Task(id) => tasks.push(id.to_string()),
            EntityRef::TaskList(id) => lists.push(id.to_string()),
            EntityRef::Note(id) => notes.push(id.to_string()),
            EntityRef::NoteFolder(id) => note_folders.push(id.to_string()),
        }
    }
    let event = CoreEvent::Changed {
        tasks,
        lists,
        notes,
        note_folders,
    };
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
impl NoteRepository for NotifyingRepository {
    async fn get_note(&self, id: NoteId) -> CoreResult<Option<Note>> {
        self.inner.get_note(id).await
    }

    async fn get_note_folder(&self, id: NoteFolderId) -> CoreResult<Option<NoteFolder>> {
        self.inner.get_note_folder(id).await
    }

    async fn list_note_folders(&self) -> CoreResult<Vec<NoteFolder>> {
        self.inner.list_note_folders().await
    }

    async fn list_folder_items(
        &self,
        query: FolderItemsQuery,
        page: PageRequest,
    ) -> CoreResult<Page<NoteTreeItem>> {
        self.inner.list_folder_items(query, page).await
    }

    async fn list_trashed_notes(&self) -> CoreResult<Vec<NoteSummary>> {
        self.inner.list_trashed_notes().await
    }

    async fn last_note_sort_key(
        &self,
        folder: Option<NoteFolderId>,
    ) -> CoreResult<Option<SortKey>> {
        self.inner.last_note_sort_key(folder).await
    }

    async fn search_notes(&self, query: &SearchQuery) -> CoreResult<Vec<NoteHit>> {
        self.inner.search_notes(query).await
    }
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

    async fn last_sort_key(&self, list_id: TaskListId) -> CoreResult<Option<SortKey>> {
        self.inner.last_sort_key(list_id).await
    }

    // FIX-13: weiterleiten, sonst liefe die O(n)-Standardsuche statt des FTS5-Index (ADR-027).
    async fn search_tasks(&self, query: &SearchQuery) -> CoreResult<Vec<TaskSummary>> {
        self.inner.search_tasks(query).await
    }

    async fn commit(&self, changes: ChangeSet) -> CoreResult<CommitReceipt> {
        let changes = changes.at(self.clock.now());
        let receipt = self.inner.commit(changes).await?;
        emit(&self.sinks, &receipt.changed);
        emit_conflicts(&self.sinks, &receipt.conflicts);
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

    async fn freeze_outbox(&self, up_to: u64) -> CoreResult<()> {
        self.inner.freeze_outbox(up_to).await
    }

    async fn revision_ops(&self) -> CoreResult<Vec<Op>> {
        self.inner.revision_ops().await
    }

    async fn prune_revisions(&self, acked: Vec<(DeviceId, u64)>) -> CoreResult<u32> {
        self.inner.prune_revisions(acked).await
    }

    async fn apply_remote(&self, batch: RemoteBatch) -> CoreResult<RemoteReceipt> {
        let receipt = self.inner.apply_remote(batch).await?;
        emit(&self.sinks, &receipt.changed);
        emit_conflicts(&self.sinks, &receipt.conflicts);
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
                notes: Vec::new(),
                note_folders: Vec::new(),
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
                    ..CreateTaskDto::default()
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
            notes: Vec::new(),
            note_folders: Vec::new(),
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

        let dues: Vec<TaskDueDto> = s
            .ids
            .iter()
            .map(|id| TaskDueDto {
                id: id.clone(),
                due: Some(dto::DueDto::Date {
                    date: "2026-10-20".into(),
                }),
            })
            .collect();
        assert_eq!(block_on(s.api.set_dues(dues)).unwrap().len(), 3);
        let (events, ops) = s.take();
        assert_eq!(events, changed(&s.ids));
        assert_eq!(fields(&ops), vec![Some(FieldName::Due); 3]);

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
        assert_send(api.set_dues(Vec::new()));
        assert_send(api.search_tasks(String::new(), 10));
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

#[cfg(test)]
mod note_tests {
    use std::sync::Mutex;

    use futures::executor::block_on;

    use super::*;
    use crate::{
        domain::Timestamp,
        ports::SteppingClock,
        sync::{OpKind, RemoteBatch},
        testing::InMemoryTaskRepository,
    };

    #[derive(Default)]
    struct Recorder(Mutex<Vec<CoreEvent>>);

    impl EventSink for Recorder {
        fn emit(&self, event: CoreEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn api(start: i64) -> (CoreApi, Arc<Recorder>) {
        let api = CoreApi::new(
            Arc::new(InMemoryTaskRepository::new()),
            Arc::new(SteppingClock::new(Timestamp::from_millis(start), 3)),
        );
        let recorder = Arc::new(Recorder::default());
        api.subscribe(Box::new(recorder.clone()));
        (api, recorder)
    }

    fn take(r: &Recorder) -> Vec<CoreEvent> {
        std::mem::take(&mut *r.0.lock().unwrap())
    }

    /// Ops von `from` nach `to` „hochladen" (wie die Engine, mit Freeze).
    fn send(from: &CoreApi, to: &CoreApi) {
        let (a, b) = (from.repository(), to.repository());
        let ops = block_on(a.pending_ops(1_000)).unwrap();
        let Some(last) = ops.iter().map(|op| op.id.seq).max() else {
            return;
        };
        block_on(a.freeze_outbox(last)).unwrap();
        let device = block_on(a.device_id()).unwrap();
        block_on(b.apply_remote(RemoteBatch {
            ops,
            cursors: vec![(device, last)],
            now: Timestamp::from_millis(1_800_000_000_000),
        }))
        .unwrap();
        block_on(a.mark_ops_pushed(last)).unwrap();
    }

    #[test]
    fn note_lifecycle_emits_note_events() {
        let (api, events) = api(1_700_000_000_000);
        let folder = block_on(api.create_note_folder(None, "Uni".into())).unwrap();
        let note =
            block_on(api.create_note(Some(folder.id.clone()), NoteKindDto::Markdown)).unwrap();
        take(&events);
        let saved = block_on(api.update_note_body(
            note.id.clone(),
            "# Blatt 3\nAufgabe".into(),
            note.body_rev.clone(),
        ))
        .unwrap();
        assert_eq!(saved.name, "Blatt 3");
        assert!(saved.body_rev.is_some());
        assert_eq!(
            take(&events),
            [CoreEvent::Changed {
                tasks: Vec::new(),
                lists: Vec::new(),
                notes: vec![note.id.clone()],
                note_folders: Vec::new(),
            }]
        );
        let page = block_on(api.list_note_items(
            Some(folder.id.clone()),
            PageRequestDto {
                cursor: None,
                limit: 10,
            },
        ))
        .unwrap();
        assert!(matches!(&page.items[..], [NoteTreeItemDto::Note(n)] if n.name == "Blatt 3"));
        let hits = block_on(api.search_notes("aufgabe".into(), 10)).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, ["Uni"]);
        assert_eq!(hits[0].snippet.as_ref().unwrap().offset, 10);

        let item = NoteItemDto::Folder {
            id: folder.id.clone(),
        };
        assert_eq!(
            block_on(api.trash_note_items(vec![item.clone()])).unwrap(),
            std::slice::from_ref(&item)
        );
        let trash = block_on(api.list_note_trash()).unwrap();
        assert_eq!(trash.len(), 1);
        assert!(block_on(api.search_notes("aufgabe".into(), 10))
            .unwrap()
            .is_empty());
        block_on(api.restore_note_items(vec![item.clone()])).unwrap();
        assert_eq!(block_on(api.purge_note_items(vec![item])).unwrap(), 2);
        assert!(matches!(
            block_on(api.get_note(note.id)),
            Err(CoreError::NotFound(_))
        ));
    }

    /// Speichern auf einer alten Basis nach einer Remote-Revision: Die Antwort trägt den
    /// gemergten Text; ein Konflikt meldet die Kopie per Event.
    #[test]
    fn remote_merge_returns_the_merged_body_and_reports_conflicts() {
        let (a, _) = api(1_700_000_000_000);
        let (b, b_events) = api(1_700_000_000_500);
        let note = block_on(a.create_note(None, NoteKindDto::Text)).unwrap();
        let note = block_on(a.update_note_body(note.id, "eins\nzwei\ndrei".into(), note.body_rev))
            .unwrap();
        send(&a, &b);
        let on_b = block_on(b.get_note(note.id.clone())).unwrap();
        block_on(a.update_note_body(
            note.id.clone(),
            "eins\nzwei\ndrei A".into(),
            note.body_rev.clone(),
        ))
        .unwrap();
        send(&a, &b);
        take(&b_events);
        // B tippt noch auf dem alten Stand weiter.
        let merged = block_on(b.update_note_body(
            on_b.id.clone(),
            "eins B\nzwei\ndrei".into(),
            on_b.body_rev.clone(),
        ))
        .unwrap();
        assert_eq!(merged.body, "eins B\nzwei\ndrei A");

        let conflict = block_on(b.update_note_body(
            merged.id.clone(),
            "eins B\nzwei von B\ndrei A".into(),
            merged.body_rev.clone(),
        ))
        .unwrap();
        send(&b, &a);
        let from_a = block_on(a.get_note(note.id.clone())).unwrap();
        block_on(a.update_note_body(
            note.id.clone(),
            from_a.body.replace("zwei", "zwei von A"),
            from_a.body_rev,
        ))
        .unwrap();
        // B bearbeitet dieselbe Zeile erneut, bevor A ankommt.
        block_on(b.update_note_body(
            conflict.id.clone(),
            "eins B\nzwei von B!\ndrei A".into(),
            conflict.body_rev,
        ))
        .unwrap();
        take(&b_events);
        send(&a, &b);
        let reported = take(&b_events)
            .into_iter()
            .any(|e| matches!(e, CoreEvent::NoteConflicts { conflicts } if conflicts.len() == 1));
        assert!(reported, "Konflikt gemeldet");
        let ops = block_on(b.repository().pending_ops(100)).unwrap();
        assert!(
            ops.iter()
                .any(|op| matches!(op.kind, OpKind::Revise { .. })),
            "Merge-Revision"
        );
    }
}
