//! Neue Liste anlegen.

use super::{key_after, Deps};
use crate::{domain::TaskList, ports::ChangeSet, CoreResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTaskListInput {
    pub name: String,
}

/// Legt eine Liste am Ende der Listenreihenfolge an.
#[derive(Debug, Clone)]
pub struct CreateTaskList {
    deps: Deps,
}

impl CreateTaskList {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen; Fehler des Repositories.
    pub async fn execute(&self, input: CreateTaskListInput) -> CoreResult<TaskList> {
        let lists = self.deps.repo.list_task_lists().await?;
        let key = key_after(lists.last().map(TaskList::sort_key))?;
        let list = TaskList::new(&input.name, key, self.deps.clock.as_ref())?;
        self.deps
            .repo
            .commit(ChangeSet::new().upsert_list(list.clone()))
            .await?;
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{test_util::deps, EnsureInbox},
        ports::TaskRepository,
        CoreError,
    };

    fn create(uc: &CreateTaskList, name: &str) -> CoreResult<TaskList> {
        block_on(uc.execute(CreateTaskListInput { name: name.into() }))
    }

    #[test]
    fn appends_lists_in_creation_order() {
        let (deps, repo) = deps();
        block_on(EnsureInbox::new(deps.clone()).execute()).unwrap();
        let uc = CreateTaskList::new(deps);
        create(&uc, "Privat").unwrap();
        create(&uc, " Arbeit ").unwrap();
        let names: Vec<String> = block_on(repo.list_task_lists())
            .unwrap()
            .iter()
            .map(|l| l.name().to_owned())
            .collect();
        assert_eq!(names, ["Inbox", "Privat", "Arbeit"]);
    }

    #[test]
    fn rejects_invalid_name_without_writing() {
        let (deps, repo) = deps();
        let uc = CreateTaskList::new(deps);
        assert!(matches!(create(&uc, "  "), Err(CoreError::Validation(_))));
        assert!(block_on(repo.list_task_lists()).unwrap().is_empty());
    }
}
