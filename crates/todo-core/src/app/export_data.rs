//! Export und Import (ADR-032): alle Listen und Tasks als Datenstand, Notizen als Dateien mit
//! Ordnerpfad. Import legt **neue** IDs an (eine alte ID kann auf einem anderen Gerät schon
//! gelöscht sein – „Delete gewinnt“ nähme die Task dort wieder weg) und überspringt, was es schon gibt.

use std::collections::{HashMap, HashSet};

use super::{
    collect_all, notes::all_items, CreateTaskList, CreateTaskListInput, Deps, EnsureInbox,
    RestoreTasks, RestoreTasksInput, TrashTasks, TrashTasksInput,
};
use crate::{
    domain::{NoteFolderId, NoteKind, Task, TaskId, TaskList, TaskListId, TaskParts, Timestamp},
    ports::{NoteTreeItem, SmartView, TaskQuery, TaskRepository},
    CoreError, CoreResult,
};

/// So viele Tasks je Commit beim Import (wie beim Notiz-Import).
pub const IMPORT_TASK_CHUNK: usize = 500;

/// Alles, was ein Export enthält – Listen, Tasks (offen, erledigt, Papierkorb).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSnapshot {
    pub lists: Vec<TaskList>,
    pub tasks: Vec<Task>,
}

/// Liest den vollständigen Datenstand für den Export.
#[derive(Debug, Clone)]
pub struct ExportData {
    deps: Deps,
}

impl ExportData {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Listen in ihrer Reihenfolge; Tasks je Liste nach `sort_key`, danach der Papierkorb.
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self) -> CoreResult<DataSnapshot> {
        let repo = self.deps.repo.as_ref();
        let lists = repo.list_task_lists().await?;
        let mut ids: Vec<TaskId> = Vec::new();
        for view in [SmartView::All, SmartView::Trash] {
            ids.extend(
                collect_all(repo, &TaskQuery::view(view))
                    .await?
                    .iter()
                    .map(|t| t.id),
            );
        }
        let mut tasks = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(super::BULK_LIMIT_MAX) {
            tasks.extend(repo.get_tasks(chunk).await?);
        }
        Ok(DataSnapshot { lists, tasks })
    }
}

/// Eine Liste im Export: Name und ob sie der Eingang ist (IDs ordnen nur die Tasks zu).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportList {
    pub id: TaskListId,
    pub name: String,
    pub is_inbox: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDataInput {
    pub lists: Vec<ImportList>,
    /// Tasks mit `list_id` aus `lists`; `trashed_at` gesetzt = landet wieder im Papierkorb.
    pub tasks: Vec<TaskParts>,
}

/// Ergebnis eines Imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportDataReport {
    pub lists_created: u32,
    pub tasks_created: u32,
    /// Gab es schon (gleicher Titel, gleiche Liste, gleiches `created_at`).
    pub tasks_skipped: u32,
}

/// Spielt einen Export ein. Listen werden über den Namen zugeordnet (Groß/Klein egal), der
/// Eingang auf den Eingang; fehlende werden angelegt. Tasks bekommen neue IDs, behalten aber alle
/// Felder (wie [`RestoreTasks`]); vorhandene werden übersprungen, so ist ein zweiter Import harmlos.
#[derive(Debug, Clone)]
pub struct ImportData {
    deps: Deps,
}

impl ImportData {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::Validation` bei einer Task, deren Liste im Export fehlt, oder ungültigen
    /// Feldern (dann ist von diesem Stück nichts geschrieben); Fehler des Repositories.
    pub async fn execute(&self, input: ImportDataInput) -> CoreResult<ImportDataReport> {
        let repo = self.deps.repo.as_ref();
        let mut report = ImportDataReport::default();
        let inbox = EnsureInbox::new(self.deps.clone()).execute().await?;
        let mut by_name: HashMap<String, TaskListId> = repo
            .list_task_lists()
            .await?
            .iter()
            .filter(|l| !l.is_inbox())
            .map(|l| (l.name().to_lowercase(), l.id()))
            .collect();

        let mut target: HashMap<TaskListId, TaskListId> = HashMap::new();
        for list in &input.lists {
            let id = if list.is_inbox {
                inbox.id()
            } else if let Some(id) = by_name.get(&list.name.trim().to_lowercase()) {
                *id
            } else {
                let created = CreateTaskList::new(self.deps.clone())
                    .execute(CreateTaskListInput {
                        name: list.name.clone(),
                    })
                    .await?;
                report.lists_created += 1;
                by_name.insert(created.name().to_lowercase(), created.id());
                created.id()
            };
            target.insert(list.id, id);
        }

        // Was es schon gibt: (Liste, Titel, angelegt) über alle betroffenen Listen und den Papierkorb.
        let lists: HashSet<TaskListId> = target.values().copied().collect();
        let mut known: HashSet<(TaskListId, String, Timestamp)> = HashSet::new();
        for view in [SmartView::All, SmartView::Trash] {
            let ids: Vec<TaskId> = collect_all(repo, &TaskQuery::view(view))
                .await?
                .iter()
                .filter(|t| lists.contains(&t.list_id))
                .map(|t| t.id)
                .collect();
            for chunk in ids.chunks(super::BULK_LIMIT_MAX) {
                for t in repo.get_tasks(chunk).await? {
                    known.insert((t.list_id(), t.title().to_owned(), t.created_at()));
                }
            }
        }

        let mut fresh = Vec::new();
        for parts in input.tasks {
            let list_id = *target.get(&parts.list_id).ok_or_else(|| {
                CoreError::Validation(format!(
                    "task {}: list {} not in the export",
                    parts.id, parts.list_id
                ))
            })?;
            let parts = TaskParts { list_id, ..parts };
            // Gleich normalisiert vergleichen wie gespeichert (Titel).
            let title = Task::from_parts(parts.clone())?.title().to_owned();
            if known.insert((list_id, title, parts.created_at)) {
                fresh.push(parts);
            } else {
                report.tasks_skipped += 1;
            }
        }

        for chunk in fresh.chunks(IMPORT_TASK_CHUNK) {
            let trashed: HashSet<TaskId> = chunk
                .iter()
                .filter(|p| p.trashed_at.is_some())
                .map(|p| p.id)
                .collect();
            let restored = RestoreTasks::new(self.deps.clone())
                .execute(RestoreTasksInput {
                    tasks: chunk.to_vec(),
                })
                .await?;
            // `RestoreTasks` vergibt neue IDs in Eingabereihenfolge; Papierkorb danach wieder setzen.
            let to_trash: Vec<TaskId> = chunk
                .iter()
                .zip(&restored)
                .filter(|(p, _)| trashed.contains(&p.id))
                .map(|(_, t)| t.id())
                .collect();
            if !to_trash.is_empty() {
                TrashTasks::new(self.deps.clone())
                    .execute(TrashTasksInput { ids: to_trash })
                    .await?;
            }
            report.tasks_created += u32::try_from(restored.len()).unwrap_or(u32::MAX);
        }
        Ok(report)
    }
}

/// Eine Notiz für den Export als Datei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportNote {
    /// Ordnernamen von der Wurzel bis zur Notiz.
    pub path: Vec<String>,
    pub name: String,
    pub kind: NoteKind,
    pub body: String,
}

/// Alle Notizen außerhalb des Papierkorbs mit Ordnerpfad (Tiefensuche, Reihenfolge wie im Baum).
#[derive(Debug, Clone)]
pub struct ExportNotes {
    deps: Deps,
}

impl ExportNotes {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self) -> CoreResult<Vec<ExportNote>> {
        let repo: &dyn TaskRepository = self.deps.repo.as_ref();
        let mut out = Vec::new();
        let mut stack: Vec<(Option<NoteFolderId>, Vec<String>)> = vec![(None, Vec::new())];
        while let Some((folder, path)) = stack.pop() {
            let mut subfolders = Vec::new();
            for item in all_items(repo, folder).await? {
                match item {
                    NoteTreeItem::Folder(f) if !f.is_trashed() => {
                        let mut sub = path.clone();
                        sub.push(f.name().to_string());
                        subfolders.push((Some(f.id()), sub));
                    }
                    NoteTreeItem::Note(n) if n.trashed_at.is_none() => {
                        if let Some(note) = repo.get_note(n.id).await? {
                            out.push(ExportNote {
                                path: path.clone(),
                                name: note.name().to_string(),
                                kind: note.kind(),
                                body: note.body().to_owned(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            // Umgekehrt auf den Stapel, damit die Ordner in Baumreihenfolge drankommen.
            stack.extend(subfolders.into_iter().rev());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{
            test_util::deps, CompleteTask, CompleteTaskInput, CreateTask, CreateTaskInput,
            TrashTasks, TrashTasksInput,
        },
        domain::Priority,
        ports::PageRequest,
    };

    fn snapshot_parts(snapshot: &DataSnapshot) -> (Vec<ImportList>, Vec<TaskParts>) {
        let lists = snapshot
            .lists
            .iter()
            .map(|l| ImportList {
                id: l.id(),
                name: l.name().to_owned(),
                is_inbox: l.is_inbox(),
            })
            .collect();
        let tasks = snapshot
            .tasks
            .iter()
            .map(|t| TaskParts {
                id: t.id(),
                list_id: t.list_id(),
                parent_id: t.parent_id(),
                title: t.title().to_owned(),
                notes: t.notes().to_owned(),
                status: t.status(),
                priority: t.priority(),
                due: t.due().cloned(),
                tags: t.tags().to_vec(),
                sort_key: t.sort_key().clone(),
                created_at: t.created_at(),
                updated_at: t.updated_at(),
                completed_at: t.completed_at(),
                trashed_at: t.trashed_at(),
                recurrence: t.recurrence().cloned(),
            })
            .collect();
        (lists, tasks)
    }

    fn summary(deps: &Deps, view: SmartView) -> Vec<(String, bool)> {
        block_on(
            deps.repo
                .list_tasks(&TaskQuery::view(view), PageRequest::first(100)),
        )
        .unwrap()
        .items
        .into_iter()
        .map(|t| (t.title, t.status == crate::domain::TaskStatus::Done))
        .collect()
    }

    #[test]
    fn export_then_import_into_an_empty_profile_and_again_is_idempotent() {
        let (source, _) = deps();
        let work = block_on(
            CreateTaskList::new(source.clone()).execute(CreateTaskListInput {
                name: "Arbeit".into(),
            }),
        )
        .unwrap();
        let a = block_on(CreateTask::new(source.clone()).execute(CreateTaskInput {
            title: "Bericht".into(),
            list_id: Some(work.id()),
            notes: "Kapitel 2".into(),
            priority: Priority::High,
            ..CreateTaskInput::default()
        }))
        .unwrap();
        let b = block_on(CreateTask::new(source.clone()).execute(CreateTaskInput {
            title: "Milch".into(),
            ..CreateTaskInput::default()
        }))
        .unwrap();
        let c = block_on(CreateTask::new(source.clone()).execute(CreateTaskInput {
            title: "Alt".into(),
            ..CreateTaskInput::default()
        }))
        .unwrap();
        block_on(CompleteTask::new(source.clone()).execute(CompleteTaskInput { id: b.id() }))
            .unwrap();
        block_on(TrashTasks::new(source.clone()).execute(TrashTasksInput { ids: vec![c.id()] }))
            .unwrap();

        let snapshot = block_on(ExportData::new(source.clone()).execute()).unwrap();
        assert_eq!(snapshot.lists.len(), 2);
        assert_eq!(snapshot.tasks.len(), 3);
        let (lists, tasks) = snapshot_parts(&snapshot);

        let (target, _) = deps();
        let report = block_on(ImportData::new(target.clone()).execute(ImportDataInput {
            lists: lists.clone(),
            tasks: tasks.clone(),
        }))
        .unwrap();
        assert_eq!(
            report,
            ImportDataReport {
                lists_created: 1,
                tasks_created: 3,
                tasks_skipped: 0
            }
        );
        let mut live = summary(&target, SmartView::All);
        live.sort();
        assert_eq!(
            live,
            [("Bericht".to_owned(), false), ("Milch".to_owned(), true)]
        );
        assert_eq!(
            summary(&target, SmartView::Trash),
            [("Alt".to_owned(), false)]
        );
        let imported = block_on(ExportData::new(target.clone()).execute()).unwrap();
        let bericht = imported
            .tasks
            .iter()
            .find(|t| t.title() == "Bericht")
            .unwrap();
        assert_ne!(bericht.id(), a.id(), "neue ID");
        assert_eq!(bericht.notes(), "Kapitel 2");
        assert_eq!(bericht.priority(), Priority::High);
        assert_eq!(bericht.created_at(), a.created_at());

        let again =
            block_on(ImportData::new(target.clone()).execute(ImportDataInput { lists, tasks }))
                .unwrap();
        assert_eq!(
            again,
            ImportDataReport {
                lists_created: 0,
                tasks_created: 0,
                tasks_skipped: 3
            }
        );
    }

    #[test]
    fn import_rejects_tasks_of_unknown_lists() {
        let (deps, _) = deps();
        let task = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "X".into(),
            ..CreateTaskInput::default()
        }))
        .unwrap();
        let snapshot = block_on(ExportData::new(deps.clone()).execute()).unwrap();
        let (_, tasks) = snapshot_parts(&snapshot);
        let err = block_on(ImportData::new(deps).execute(ImportDataInput {
            lists: Vec::new(),
            tasks,
        }))
        .unwrap_err();
        assert!(
            matches!(err, CoreError::Validation(_)),
            "{err:?} für {}",
            task.id()
        );
    }

    #[test]
    fn exports_notes_outside_the_trash_with_their_folder_path() {
        use crate::app::{
            CreateNote, CreateNoteFolder, CreateNoteFolderInput, CreateNoteInput, NoteItem,
            TrashNoteItems, TrashNoteItemsInput, UpdateNoteBody, UpdateNoteBodyInput,
        };
        let (deps, _) = deps();
        let note = |folder, text: &str| {
            let n = block_on(CreateNote::new(deps.clone()).execute(CreateNoteInput {
                folder,
                kind: NoteKind::Markdown,
            }))
            .unwrap();
            block_on(
                UpdateNoteBody::new(deps.clone()).execute(UpdateNoteBodyInput {
                    id: n.id(),
                    text: text.into(),
                    base_rev: n.body_rev(),
                }),
            )
            .unwrap()
        };
        let uni = block_on(
            CreateNoteFolder::new(deps.clone()).execute(CreateNoteFolderInput {
                parent: None,
                name: "Uni".into(),
            }),
        )
        .unwrap();
        let ana = block_on(
            CreateNoteFolder::new(deps.clone()).execute(CreateNoteFolderInput {
                parent: Some(uni.id()),
                name: "Analysis".into(),
            }),
        )
        .unwrap();
        note(None, "Einkauf\nMilch");
        note(Some(ana.id()), "Blatt 3\nAufgabe 1");
        let gone = note(Some(uni.id()), "Weg");
        block_on(
            TrashNoteItems::new(deps.clone()).execute(TrashNoteItemsInput {
                items: vec![NoteItem::Note(gone.id())],
            }),
        )
        .unwrap();

        let notes = block_on(ExportNotes::new(deps).execute()).unwrap();
        let mut got: Vec<(String, String)> = notes
            .iter()
            .map(|n| (n.path.join("/"), n.name.clone()))
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                (String::new(), "Einkauf".to_owned()),
                ("Uni/Analysis".to_owned(), "Blatt 3".to_owned())
            ]
        );
        assert!(notes.iter().any(|n| n.body == "Blatt 3\nAufgabe 1"));
    }
}
