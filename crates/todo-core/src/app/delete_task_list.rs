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
    /// Anzahl der Tasks, die in die Inbox verschoben wurden.
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
        let repo = self.deps.repo.as_ref();
        let list = repo
            .get_task_list(input.id)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("list {}", input.id)))?;
        if !list.can_delete() {
            return Err(CoreError::Validation("the inbox cannot be deleted".into()));
        }
        let inbox = EnsureInbox::new(self.deps.clone()).execute().await?;

        let inbox_tasks = collect_all(repo, &TaskQuery::view(SmartView::Inbox)).await?;
        let mut last_key = inbox_tasks.last().map(|s| s.sort_key.clone());
        let to_move = collect_all(repo, &TaskQuery::view(SmartView::List(list.id()))).await?;

        let mut changes = ChangeSet::new();
        for summary in &to_move {
            let mut task = repo
                .get_task(summary.id)
                .await?
                .ok_or_else(|| CoreError::Conflict(format!("task {} vanished", summary.id)))?;
            let key = key_after(last_key.as_ref())?;
            task.move_to(inbox.id(), key.clone(), self.deps.clock.as_ref());
            last_key = Some(key);
            changes.push(Change::UpsertTask(task));
        }
        changes.push(Change::Delete(EntityRef::TaskList(list.id())));
        repo.commit(changes).await?;
        Ok(DeleteTaskListOutput {
            moved_tasks: to_move.len(),
        })
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
}
