//! Papierkorb für Tasks (ADR-031): hineinlegen, zurückholen, endgültig löschen, leeren und nach
//! 30 Tagen automatisch aufräumen. „In den Papierkorb“ ist das LWW-Feld `Trashed` (kein
//! Tombstone), Zurückholen behält die ID; endgültig ist `Delete`.

use super::{bulk_ids, collect_all, load_existing, Deps};
use crate::{
    domain::{Task, TaskId, Timestamp},
    ports::{Change, ChangeSet, EntityRef, SmartView, TaskQuery, TaskRepository},
    CoreResult,
};

/// Nach so langer Zeit im Papierkorb wird eine Task automatisch gelöscht (wie Notizen).
pub const TASK_TRASH_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashTasksInput {
    pub ids: Vec<TaskId>,
}

/// Legt Tasks in den Papierkorb – in **einem** Commit. Unbekannte und schon gelöschte werden
/// übersprungen; liefert die tatsächlich geänderten (für „Rückgängig“ per [`UntrashTasks`]).
#[derive(Debug, Clone)]
pub struct TrashTasks {
    deps: Deps,
}

impl TrashTasks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::Validation` bei mehr als [`super::BULK_LIMIT_MAX`] IDs; Fehler des
    /// Repositories (dann ist nichts geändert).
    pub async fn execute(&self, input: TrashTasksInput) -> CoreResult<Vec<Task>> {
        let now = self.deps.clock.now();
        set_trashed(&self.deps, input.ids, |task| {
            (!task.is_trashed()).then_some(Some(now))
        })
        .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrashTasksInput {
    pub ids: Vec<TaskId>,
}

/// Holt Tasks aus dem Papierkorb zurück (gleiche ID, alter Platz in der Liste). Nicht gelöschte
/// und unbekannte werden übersprungen; liefert die tatsächlich geänderten.
#[derive(Debug, Clone)]
pub struct UntrashTasks {
    deps: Deps,
}

impl UntrashTasks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Wie [`TrashTasks::execute`].
    pub async fn execute(&self, input: UntrashTasksInput) -> CoreResult<Vec<Task>> {
        set_trashed(&self.deps, input.ids, |task| {
            task.is_trashed().then_some(None)
        })
        .await
    }
}

/// Setzt `trashed_at` für alle Tasks, für die `next` einen Wert liefert, in einem Commit.
async fn set_trashed(
    deps: &Deps,
    ids: Vec<TaskId>,
    next: impl Fn(&Task) -> Option<Option<Timestamp>>,
) -> CoreResult<Vec<Task>> {
    let ids = bulk_ids(ids)?;
    let repo = deps.repo.as_ref();
    let mut changes = ChangeSet::new();
    let mut changed = Vec::new();
    for mut task in load_existing(repo, &ids).await? {
        if let Some(at) = next(&task) {
            task.set_trashed(at, deps.clock.as_ref());
            changes.push(Change::UpsertTask(task.clone()));
            changed.push(task);
        }
    }
    if !changes.is_empty() {
        repo.commit(changes).await?;
    }
    Ok(changed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgeTasksInput {
    pub ids: Vec<TaskId>,
}

/// Löscht Tasks endgültig – nur solche, die im Papierkorb liegen (alles andere wird
/// übersprungen). Nicht rückgängig zu machen. Liefert die Zahl gelöschter Tasks.
#[derive(Debug, Clone)]
pub struct PurgeTasks {
    deps: Deps,
}

impl PurgeTasks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Wie [`TrashTasks::execute`].
    pub async fn execute(&self, input: PurgeTasksInput) -> CoreResult<u32> {
        let ids = bulk_ids(input.ids)?;
        let repo = self.deps.repo.as_ref();
        let trashed: Vec<TaskId> = load_existing(repo, &ids)
            .await?
            .iter()
            .filter(|t| t.is_trashed())
            .map(Task::id)
            .collect();
        purge(repo, &trashed).await
    }
}

/// Leert den Papierkorb.
#[derive(Debug, Clone)]
pub struct EmptyTaskTrash {
    deps: Deps,
}

impl EmptyTaskTrash {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Zahl gelöschter Tasks.
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self) -> CoreResult<u32> {
        let repo = self.deps.repo.as_ref();
        purge(repo, &trashed_ids(repo, None).await?).await
    }
}

/// Löscht, was seit mehr als 30 Tagen im Papierkorb liegt. Beim Start und höchstens einmal
/// täglich aufrufen, nie per Timer im Leerlauf. Doppelte Deletes von zwei Geräten sind harmlos.
#[derive(Debug, Clone)]
pub struct PurgeExpiredTasks {
    deps: Deps,
}

impl PurgeExpiredTasks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Zahl gelöschter Tasks.
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self) -> CoreResult<u32> {
        let repo = self.deps.repo.as_ref();
        let before =
            Timestamp::from_millis(self.deps.clock.now().as_millis() - TASK_TRASH_RETENTION_MS);
        purge(repo, &trashed_ids(repo, Some(before)).await?).await
    }
}

/// IDs im Papierkorb, optional nur solche, die dort seit vor `before` liegen.
async fn trashed_ids(
    repo: &dyn TaskRepository,
    before: Option<Timestamp>,
) -> CoreResult<Vec<TaskId>> {
    Ok(collect_all(repo, &TaskQuery::view(SmartView::Trash))
        .await?
        .iter()
        .filter(|t| t.trashed_at.is_some_and(|at| before.is_none_or(|b| at < b)))
        .map(|t| t.id)
        .collect())
}

/// Löscht `ids` endgültig in Stücken zu höchstens [`super::BULK_LIMIT_MAX`] je Commit.
async fn purge(repo: &dyn TaskRepository, ids: &[TaskId]) -> CoreResult<u32> {
    for chunk in ids.chunks(super::BULK_LIMIT_MAX) {
        let mut changes = ChangeSet::new();
        for id in chunk {
            changes = changes.delete(EntityRef::Task(*id));
        }
        repo.commit(changes).await?;
    }
    Ok(u32::try_from(ids.len()).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{
            test_util::deps, CreateTask, CreateTaskInput, CreateTaskList, CreateTaskListInput,
            DeleteTaskList, DeleteTaskListInput,
        },
        ports::PageRequest,
    };

    fn titles(deps: &Deps, view: SmartView) -> Vec<String> {
        let page = block_on(
            deps.repo
                .list_tasks(&TaskQuery::view(view), PageRequest::first(100)),
        )
        .unwrap();
        page.items.into_iter().map(|t| t.title).collect()
    }

    fn create(deps: &Deps, title: &str) -> Task {
        block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: title.into(),
            ..CreateTaskInput::default()
        }))
        .unwrap()
    }

    #[test]
    fn trash_hides_from_views_and_untrash_keeps_the_id() {
        let (deps, _) = deps();
        let a = create(&deps, "A");
        create(&deps, "B");
        let trashed = block_on(TrashTasks::new(deps.clone()).execute(TrashTasksInput {
            ids: vec![a.id(), a.id(), TaskId::new()],
        }))
        .unwrap();
        assert_eq!(trashed.len(), 1);
        assert!(trashed[0].is_trashed());
        assert_eq!(titles(&deps, SmartView::Inbox), ["B"]);
        assert_eq!(titles(&deps, SmartView::All), ["B"]);
        assert_eq!(titles(&deps, SmartView::Trash), ["A"]);
        // Schon im Papierkorb → übersprungen.
        let again =
            block_on(TrashTasks::new(deps.clone()).execute(TrashTasksInput { ids: vec![a.id()] }))
                .unwrap();
        assert!(again.is_empty());

        let back = block_on(
            UntrashTasks::new(deps.clone()).execute(UntrashTasksInput { ids: vec![a.id()] }),
        )
        .unwrap();
        assert_eq!(back.iter().map(Task::id).collect::<Vec<_>>(), [a.id()]);
        assert_eq!(titles(&deps, SmartView::Inbox), ["A", "B"], "alter Platz");
        assert!(titles(&deps, SmartView::Trash).is_empty());
    }

    #[test]
    fn purge_only_deletes_trashed_and_empty_clears_all() {
        let (deps, _) = deps();
        let a = create(&deps, "A");
        let b = create(&deps, "B");
        let c = create(&deps, "C");
        block_on(TrashTasks::new(deps.clone()).execute(TrashTasksInput {
            ids: vec![a.id(), b.id()],
        }))
        .unwrap();
        let n = block_on(PurgeTasks::new(deps.clone()).execute(PurgeTasksInput {
            ids: vec![a.id(), c.id()],
        }))
        .unwrap();
        assert_eq!(n, 1, "C liegt nicht im Papierkorb");
        assert!(block_on(deps.repo.get_task(a.id())).unwrap().is_none());
        assert!(block_on(deps.repo.get_task(c.id())).unwrap().is_some());
        assert_eq!(
            block_on(EmptyTaskTrash::new(deps.clone()).execute()).unwrap(),
            1
        );
        assert!(block_on(deps.repo.get_task(b.id())).unwrap().is_none());
        assert_eq!(titles(&deps, SmartView::All), ["C"]);
    }

    #[test]
    fn expired_trash_is_purged_after_thirty_days() {
        let (deps, _) = deps();
        let old = create(&deps, "Alt");
        let fresh = create(&deps, "Neu");
        let now = deps.clock.now();
        for (task, at) in [
            (&old, now.as_millis() - TASK_TRASH_RETENTION_MS - 1),
            (&fresh, now.as_millis() - 1_000),
        ] {
            let mut t = block_on(deps.repo.get_task(task.id())).unwrap().unwrap();
            t.set_trashed(Some(Timestamp::from_millis(at)), deps.clock.as_ref());
            block_on(deps.repo.commit(ChangeSet::new().upsert_task(t))).unwrap();
        }
        assert_eq!(
            block_on(PurgeExpiredTasks::new(deps.clone()).execute()).unwrap(),
            1
        );
        assert_eq!(titles(&deps, SmartView::Trash), ["Neu"]);
    }

    #[test]
    fn deleting_a_list_moves_its_trashed_tasks_to_the_inbox_too() {
        let (deps, _) = deps();
        let list = block_on(
            CreateTaskList::new(deps.clone()).execute(CreateTaskListInput {
                name: "Projekt".into(),
            }),
        )
        .unwrap();
        let t = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "Weg".into(),
            list_id: Some(list.id()),
            ..CreateTaskInput::default()
        }))
        .unwrap();
        block_on(TrashTasks::new(deps.clone()).execute(TrashTasksInput { ids: vec![t.id()] }))
            .unwrap();
        block_on(DeleteTaskList::new(deps.clone()).execute(DeleteTaskListInput { id: list.id() }))
            .unwrap();
        let moved = block_on(deps.repo.get_task(t.id())).unwrap().unwrap();
        assert!(moved.is_trashed());
        assert_ne!(moved.list_id(), list.id());
        assert_eq!(titles(&deps, SmartView::Trash), ["Weg"]);
    }
}
