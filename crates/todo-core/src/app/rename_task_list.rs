//! Liste umbenennen.

use super::Deps;
use crate::{
    domain::{TaskList, TaskListId},
    ports::ChangeSet,
    CoreError, CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameTaskListInput {
    pub id: TaskListId,
    pub name: String,
}

/// Benennt eine Liste um. Die Inbox kann nicht umbenannt werden.
#[derive(Debug, Clone)]
pub struct RenameTaskList {
    deps: Deps,
}

impl RenameTaskList {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für unbekannte Listen, `CoreError::Validation` für die Inbox oder
    /// einen ungültigen Namen; Fehler des Repositories.
    pub async fn execute(&self, input: RenameTaskListInput) -> CoreResult<TaskList> {
        let mut list = self
            .deps
            .repo
            .get_task_list(input.id)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("list {}", input.id)))?;
        if !list.can_rename() {
            return Err(CoreError::Validation("the inbox cannot be renamed".into()));
        }
        let before = list.clone();
        list.rename(&input.name, self.deps.clock.as_ref())?;
        if list != before {
            self.deps
                .repo
                .commit(ChangeSet::new().upsert_list(list.clone()))
                .await?;
        }
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{test_util::deps, CreateTaskList, CreateTaskListInput, EnsureInbox},
        ports::TaskRepository,
    };

    #[test]
    fn renames_list() {
        let (deps, repo) = deps();
        let list = block_on(
            CreateTaskList::new(deps.clone()).execute(CreateTaskListInput {
                name: "Privat".into(),
            }),
        )
        .unwrap();
        let renamed = block_on(RenameTaskList::new(deps).execute(RenameTaskListInput {
            id: list.id(),
            name: "Zuhause".into(),
        }))
        .unwrap();
        assert_eq!(renamed.name(), "Zuhause");
        assert_eq!(
            block_on(repo.get_task_list(list.id()))
                .unwrap()
                .unwrap()
                .name(),
            "Zuhause"
        );
    }

    #[test]
    fn inbox_cannot_be_renamed() {
        let (deps, _) = deps();
        let inbox = block_on(EnsureInbox::new(deps.clone()).execute()).unwrap();
        let result = block_on(RenameTaskList::new(deps).execute(RenameTaskListInput {
            id: inbox.id(),
            name: "X".into(),
        }));
        assert!(matches!(result, Err(CoreError::Validation(_))));
    }

    #[test]
    fn unknown_list_is_not_found() {
        let (deps, _) = deps();
        let result = block_on(RenameTaskList::new(deps).execute(RenameTaskListInput {
            id: TaskListId::new(),
            name: "X".into(),
        }));
        assert!(matches!(result, Err(CoreError::NotFound(_))));
    }
}
