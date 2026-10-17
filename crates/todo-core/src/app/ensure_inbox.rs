//! Stellt sicher, dass die Inbox existiert.

use super::Deps;
use crate::{
    domain::{SortKey, TaskList},
    ports::ChangeSet,
    CoreResult,
};

/// Liefert die Inbox und legt sie bei Bedarf an (vor allen anderen Listen). Idempotent.
///
/// Gibt es nach einem Sync mehrere Inboxen, wird die erste nach Sortierung verwendet.
#[derive(Debug, Clone)]
pub struct EnsureInbox {
    deps: Deps,
}

impl EnsureInbox {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self) -> CoreResult<TaskList> {
        let lists = self.deps.repo.list_task_lists().await?;
        if let Some(inbox) = lists.iter().find(|l| l.is_inbox()) {
            return Ok(inbox.clone());
        }
        let key = SortKey::between(None, lists.first().map(TaskList::sort_key))?;
        let inbox = TaskList::inbox(key, self.deps.clock.as_ref());
        self.deps
            .repo
            .commit(ChangeSet::new().upsert_list(inbox.clone()))
            .await?;
        Ok(inbox)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{app::test_util::deps, ports::TaskRepository};

    #[test]
    fn creates_inbox_once() {
        let (deps, repo) = deps();
        let uc = EnsureInbox::new(deps);
        let first = block_on(uc.execute()).unwrap();
        let second = block_on(uc.execute()).unwrap();
        assert!(first.is_inbox());
        assert_eq!(first, second);
        assert_eq!(block_on(repo.list_task_lists()).unwrap().len(), 1);
    }

    #[test]
    fn inbox_is_placed_before_existing_lists() {
        let (deps, repo) = deps();
        let other = TaskList::new("Arbeit", SortKey::first(), deps.clock.as_ref()).unwrap();
        block_on(repo.commit(ChangeSet::new().upsert_list(other))).unwrap();
        block_on(EnsureInbox::new(deps).execute()).unwrap();
        let lists = block_on(repo.list_task_lists()).unwrap();
        assert!(lists[0].is_inbox());
        assert_eq!(lists[1].name(), "Arbeit");
    }
}
