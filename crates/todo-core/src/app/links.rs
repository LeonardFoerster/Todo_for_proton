//! Verknüpfungen Task ↔ Notiz (N9, ADR-036): anlegen, lösen, auflisten, verwaiste aufräumen.

use super::Deps;
use crate::{
    domain::{Link, LinkId, Note, NoteId, Task, TaskId},
    ports::{ChangeSet, EntityRef},
    sync::EntityState,
    CoreError, CoreResult,
};

/// Verknüpft eine Task mit einer Notiz. Gibt es die Verknüpfung schon, kommt sie zurück (kein Op).
/// Die ID ist aus beiden Enden abgeleitet: Dieselbe Verknüpfung auf zwei Geräten ist eine.
#[derive(Debug, Clone)]
pub struct LinkTaskNote {
    deps: Deps,
}

impl LinkTaskNote {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `NotFound` für eine unbekannte Task oder Notiz; Fehler des Repositories.
    pub async fn execute(&self, task: TaskId, note: NoteId) -> CoreResult<Link> {
        let repo = self.deps.repo.as_ref();
        if repo.get_task(task).await?.is_none() {
            return Err(CoreError::NotFound(format!("task {task}")));
        }
        if repo.get_note(note).await?.is_none() {
            return Err(CoreError::NotFound(format!("note {note}")));
        }
        if let Some(existing) = repo
            .list_links(Some(task), Some(note))
            .await?
            .into_iter()
            .next()
        {
            return Ok(existing);
        }
        // Nach dem Lösen (Tombstone) die nächste Variante – sonst bliebe sie für immer gelöscht.
        let mut id = None;
        for attempt in 0u8..16 {
            let candidate = LinkId::between(task, note, attempt);
            if !matches!(
                repo.entity_state(EntityRef::Link(candidate)).await?,
                Some(EntityState::Deleted(_))
            ) {
                id = Some(candidate);
                break;
            }
        }
        let link = Link::new(
            id.unwrap_or_else(LinkId::new),
            task,
            note,
            self.deps.clock.as_ref(),
        );
        repo.commit(ChangeSet::new().upsert_link(link)).await?;
        Ok(link)
    }
}

/// Löst eine Verknüpfung (unbekannte ID: kein Fehler).
#[derive(Debug, Clone)]
pub struct UnlinkTaskNote {
    deps: Deps,
}

impl UnlinkTaskNote {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self, id: LinkId) -> CoreResult<()> {
        self.deps
            .repo
            .commit(ChangeSet::new().delete(EntityRef::Link(id)))
            .await?;
        Ok(())
    }
}

/// Verknüpfte Notizen einer Task bzw. Tasks einer Notiz – nur solche, deren anderes Ende es gibt und
/// das nicht im Papierkorb liegt.
#[derive(Debug, Clone)]
pub struct ListLinks {
    deps: Deps,
}

impl ListLinks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Fehler des Repositories.
    pub async fn notes_of(&self, task: TaskId) -> CoreResult<Vec<(LinkId, Note)>> {
        let repo = self.deps.repo.as_ref();
        let mut out = Vec::new();
        for link in repo.list_links(Some(task), None).await? {
            if let Some(note) = repo.get_note(link.note()).await? {
                if !note.is_trashed() {
                    out.push((link.id(), note));
                }
            }
        }
        Ok(out)
    }

    /// # Errors
    /// Fehler des Repositories.
    pub async fn tasks_of(&self, note: NoteId) -> CoreResult<Vec<(LinkId, Task)>> {
        let repo = self.deps.repo.as_ref();
        let links = repo.list_links(None, Some(note)).await?;
        let ids: Vec<TaskId> = links.iter().map(Link::task).collect();
        let tasks = repo.get_tasks(&ids).await?;
        Ok(links
            .iter()
            .filter_map(|l| {
                let task = tasks.iter().find(|t| t.id() == l.task())?;
                (!task.is_trashed()).then(|| (l.id(), task.clone()))
            })
            .collect())
    }
}

/// Entfernt Verknüpfungen, deren Task oder Notiz es nicht mehr gibt (endgültig gelöscht, auch per
/// Sync). Liefert ihre Zahl.
#[derive(Debug, Clone)]
pub struct PurgeDanglingLinks {
    deps: Deps,
}

impl PurgeDanglingLinks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self) -> CoreResult<usize> {
        let repo = self.deps.repo.as_ref();
        let mut changes = ChangeSet::new();
        let mut n = 0;
        for link in repo.list_links(None, None).await? {
            let gone = repo.get_task(link.task()).await?.is_none()
                || repo.get_note(link.note()).await?.is_none();
            if gone {
                changes.push(crate::ports::Change::Delete(EntityRef::Link(link.id())));
                n += 1;
            }
        }
        if n > 0 {
            repo.commit(changes).await?;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{
            test_util::deps, CreateNote, CreateNoteInput, CreateTask, CreateTaskInput, DeleteTask,
            DeleteTaskInput,
        },
        domain::NoteKind,
        ports::TaskRepository,
    };

    #[test]
    fn link_is_idempotent_listed_both_ways_unlinked_and_purged() {
        let (deps, repo) = deps();
        let task = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "Blatt 3".into(),
            ..CreateTaskInput::default()
        }))
        .unwrap();
        let note = block_on(CreateNote::new(deps.clone()).execute(CreateNoteInput {
            folder: None,
            kind: NoteKind::Markdown,
        }))
        .unwrap();
        let link = block_on(LinkTaskNote::new(deps.clone()).execute(task.id(), note.id())).unwrap();
        assert_eq!(link.id(), LinkId::between(task.id(), note.id(), 0));
        let again =
            block_on(LinkTaskNote::new(deps.clone()).execute(task.id(), note.id())).unwrap();
        assert_eq!(again, link, "schon verknüpft");
        let list = ListLinks::new(deps.clone());
        assert_eq!(
            block_on(list.notes_of(task.id())).unwrap()[0].1.id(),
            note.id()
        );
        assert_eq!(
            block_on(list.tasks_of(note.id())).unwrap()[0].1.id(),
            task.id()
        );

        // Lösen und neu verknüpfen nimmt die nächste ID-Variante (Tombstone).
        block_on(UnlinkTaskNote::new(deps.clone()).execute(link.id())).unwrap();
        assert!(block_on(list.notes_of(task.id())).unwrap().is_empty());
        let relinked =
            block_on(LinkTaskNote::new(deps.clone()).execute(task.id(), note.id())).unwrap();
        assert_eq!(relinked.id(), LinkId::between(task.id(), note.id(), 1));

        // Endgültig gelöschte Task → Verknüpfung verwaist → beim Aufräumen weg.
        block_on(DeleteTask::new(deps.clone()).execute(DeleteTaskInput { id: task.id() })).unwrap();
        assert!(
            block_on(list.tasks_of(note.id())).unwrap().is_empty(),
            "ausgeblendet"
        );
        assert_eq!(
            block_on(PurgeDanglingLinks::new(deps).execute()).unwrap(),
            1
        );
        assert!(block_on(repo.list_links(None, None)).unwrap().is_empty());
    }

    #[test]
    fn unknown_ends_are_not_found() {
        let (deps, _) = deps();
        assert!(matches!(
            block_on(LinkTaskNote::new(deps).execute(TaskId::new(), NoteId::new())),
            Err(CoreError::NotFound(_))
        ));
    }
}
