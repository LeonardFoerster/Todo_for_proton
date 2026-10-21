//! Liste löschen; ihre Tasks wandern in die Inbox.

use super::{collect_all, key_after, Deps, EnsureInbox};
use crate::{
    domain::TaskListId,
    ports::{Change, ChangeSet, EntityRef, SmartView, TaskQuery},
    CoreError, CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteTaskListInput {
    pub id: TaskListId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteTaskListOutput {
    /// Anzahl der Tasks, die in die Inbox verschoben wurden (ohne solche im Papierkorb).
    pub moved_tasks: usize,
}

/// Löscht eine Liste. Ihre Tasks werden in bisheriger Reihenfolge ans Ende der Inbox verschoben –
/// zusammen mit dem Löschen in einem atomaren Commit. Die Inbox selbst ist nicht löschbar.
#[derive(Debug, Clone)]
pub struct DeleteTaskList {
    deps: Deps,
}

impl DeleteTaskList {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für unbekannte Listen, `CoreError::Validation` für die Inbox;
    /// Fehler des Repositories.
    pub async fn execute(&self, input: DeleteTaskListInput) -> CoreResult<DeleteTaskListOutput> {
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        let list = repo
            .get_task_list(input.id)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("list {}", input.id)))?;
        if !list.can_delete() {
            return Err(CoreError::Validation("the inbox cannot be deleted".into()));
        }
        let inbox = EnsureInbox::new(self.deps.clone()).execute().await?;
        let moved_tasks = move_into_and_delete(&self.deps, list.id(), inbox.id()).await?;
        Ok(DeleteTaskListOutput { moved_tasks })
    }
}

/// Hängt alle Tasks von `source` (auch erledigte und solche im Papierkorb, ADR-031) in bisheriger
/// Reihenfolge ans Ende von `target` und löscht `source` – ein Commit. Liefert die Zahl der
/// verschobenen Tasks außerhalb des Papierkorbs. Die Reihenfolge-Sperre hält der Aufrufer.
pub(super) async fn move_into_and_delete(
    deps: &Deps,
    source: TaskListId,
    target: TaskListId,
) -> CoreResult<usize> {
    let repo = deps.repo.as_ref();
    let mut last_key = repo.last_sort_key(target).await?;
    let to_move = collect_all(repo, &TaskQuery::view(SmartView::List(source))).await?;
    // Auch Tasks im Papierkorb gehören zur Liste – sonst zeigten sie danach ins Leere.
    let trashed: Vec<_> = collect_all(repo, &TaskQuery::view(SmartView::Trash))
        .await?
        .into_iter()
        .filter(|t| t.list_id == source)
        .collect();

    let mut changes = ChangeSet::new();
    for summary in to_move.iter().chain(&trashed) {
        let mut task = repo
            .get_task(summary.id)
            .await?
            .ok_or_else(|| CoreError::Conflict(format!("task {} vanished", summary.id)))?;
        let key = key_after(last_key.as_ref())?;
        task.move_to(target, key.clone(), deps.clock.as_ref());
        last_key = Some(key);
        changes.push(Change::UpsertTask(task));
    }
    changes.push(Change::Delete(EntityRef::TaskList(source)));
    repo.commit(changes).await?;
    Ok(to_move.len())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeTaskListsInput {
    /// Wird aufgelöst (nicht die Inbox).
    pub source: TaskListId,
    /// Bekommt die Tasks (auch die Inbox).
    pub target: TaskListId,
}

/// Führt zwei Listen zusammen (ADR-020, N10-03): die Tasks von `source` ans Ende von `target`,
/// `source` wird gelöscht – ein Commit. Gedacht für gleichnamige Listen nach dem ersten Login.
#[derive(Debug, Clone)]
pub struct MergeTaskLists {
    deps: Deps,
}

impl MergeTaskLists {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Liefert die Zahl der verschobenen Tasks (ohne Papierkorb).
    ///
    /// # Errors
    /// `NotFound` für eine unbekannte Liste, `Validation` für die Inbox als Quelle oder gleiche
    /// Listen; Fehler des Repositories.
    pub async fn execute(&self, input: MergeTaskListsInput) -> CoreResult<usize> {
        if input.source == input.target {
            return Err(CoreError::Validation(
                "a list cannot be merged into itself".into(),
            ));
        }
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        let source = repo
            .get_task_list(input.source)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("list {}", input.source)))?;
        if !source.can_delete() {
            return Err(CoreError::Validation(
                "the inbox cannot be merged away".into(),
            ));
        }
        repo.get_task_list(input.target)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("list {}", input.target)))?;
        move_into_and_delete(&self.deps, source.id(), input.target).await
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{test_util::deps, CreateTaskList, CreateTaskListInput},
        domain::{SortKey, Task},
        ports::TaskRepository,
    };

    fn titles(repo: &dyn TaskRepository, view: SmartView) -> Vec<String> {
        block_on(collect_all(repo, &TaskQuery::view(view)))
            .unwrap()
            .into_iter()
            .map(|s| s.title)
            .collect()
    }

    #[test]
    fn moves_tasks_to_end_of_inbox_in_order_and_deletes_list() {
        let (deps, repo) = deps();
        let clock = deps.clock.clone();
        let inbox = block_on(EnsureInbox::new(deps.clone()).execute()).unwrap();
        let work = block_on(
            CreateTaskList::new(deps.clone()).execute(CreateTaskListInput {
                name: "Arbeit".into(),
            }),
        )
        .unwrap();
        let mut set = ChangeSet::new();
        let mut key = SortKey::first();
        for title in ["i1", "i2"] {
            set =
                set.upsert_task(Task::new(inbox.id(), title, key.clone(), clock.as_ref()).unwrap());
            key = key_after(Some(&key)).unwrap();
        }
        // Absichtlich kleinere Keys als in der Inbox: Verschieben muss neue Keys vergeben.
        let mut key = SortKey::between(None, Some(&SortKey::first())).unwrap();
        for title in ["w1", "w2", "w3"] {
            set =
                set.upsert_task(Task::new(work.id(), title, key.clone(), clock.as_ref()).unwrap());
            key = key_after(Some(&key)).unwrap();
        }
        block_on(repo.commit(set)).unwrap();

        let out =
            block_on(DeleteTaskList::new(deps).execute(DeleteTaskListInput { id: work.id() }))
                .unwrap();
        assert_eq!(out.moved_tasks, 3);
        assert_eq!(block_on(repo.get_task_list(work.id())).unwrap(), None);
        assert_eq!(
            titles(repo.as_ref(), SmartView::Inbox),
            ["i1", "i2", "w1", "w2", "w3"]
        );
    }

    #[test]
    fn deleting_empty_list_creates_inbox_if_missing() {
        let (deps, repo) = deps();
        let list = block_on(
            CreateTaskList::new(deps.clone()).execute(CreateTaskListInput {
                name: "Leer".into(),
            }),
        )
        .unwrap();
        let out =
            block_on(DeleteTaskList::new(deps).execute(DeleteTaskListInput { id: list.id() }))
                .unwrap();
        assert_eq!(out.moved_tasks, 0);
        let lists = block_on(repo.list_task_lists()).unwrap();
        assert_eq!(lists.len(), 1);
        assert!(lists[0].is_inbox());
    }

    #[test]
    fn inbox_cannot_be_deleted() {
        let (deps, repo) = deps();
        let inbox = block_on(EnsureInbox::new(deps.clone()).execute()).unwrap();
        let result =
            block_on(DeleteTaskList::new(deps).execute(DeleteTaskListInput { id: inbox.id() }));
        assert!(matches!(result, Err(CoreError::Validation(_))));
        assert!(block_on(repo.get_task_list(inbox.id())).unwrap().is_some());
    }

    #[test]
    fn unknown_list_is_not_found() {
        let (deps, _) = deps();
        let result = block_on(DeleteTaskList::new(deps).execute(DeleteTaskListInput {
            id: TaskListId::new(),
        }));
        assert!(matches!(result, Err(CoreError::NotFound(_))));
    }

    #[test]
    fn merge_moves_everything_and_rejects_inbox_and_self() {
        let (deps, repo) = deps();
        let clock = deps.clock.clone();
        let inbox = block_on(EnsureInbox::new(deps.clone()).execute()).unwrap();
        let make = |name: &str| {
            block_on(
                CreateTaskList::new(deps.clone())
                    .execute(CreateTaskListInput { name: name.into() }),
            )
            .unwrap()
        };
        let (a, b) = (make("Arbeit"), make("Arbeit"));
        let mut set = ChangeSet::new();
        let mut key = SortKey::first();
        for (list, title) in [(a.id(), "a1"), (b.id(), "b1"), (b.id(), "b2")] {
            set = set.upsert_task(Task::new(list, title, key.clone(), clock.as_ref()).unwrap());
            key = key_after(Some(&key)).unwrap();
        }
        block_on(repo.commit(set)).unwrap();
        let merge = MergeTaskLists::new(deps.clone());
        let n = block_on(merge.execute(MergeTaskListsInput {
            source: b.id(),
            target: a.id(),
        }))
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            titles(repo.as_ref(), SmartView::List(a.id())),
            ["a1", "b1", "b2"]
        );
        assert!(block_on(repo.get_task_list(b.id())).unwrap().is_none());
        assert!(matches!(
            block_on(merge.execute(MergeTaskListsInput {
                source: inbox.id(),
                target: a.id()
            })),
            Err(CoreError::Validation(_))
        ));
        assert!(matches!(
            block_on(merge.execute(MergeTaskListsInput {
                source: a.id(),
                target: a.id()
            })),
            Err(CoreError::Validation(_))
        ));
    }
}
