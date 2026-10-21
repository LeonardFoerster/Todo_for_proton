//! Neue Task anlegen.

use super::{key_after, Deps, EnsureInbox};
use crate::{
    domain::{Due, Priority, Recurrence, Task, TaskListId},
    ports::ChangeSet,
    CoreError, CoreResult,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateTaskInput {
    pub title: String,
    /// Zielliste; `None` = Inbox.
    pub list_id: Option<TaskListId>,
    pub notes: String,
    pub priority: Priority,
    pub due: Option<Due>,
    /// Wiederholung (N4); ohne `due` wird das erste Vorkommen ab heute die Fälligkeit.
    pub recurrence: Option<Recurrence>,
}

/// Legt eine offene Task am Ende der Zielliste an – mit allen Anfangswerten in einem Commit.
#[derive(Debug, Clone)]
pub struct CreateTask {
    deps: Deps,
}

impl CreateTask {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::Validation` bei ungültigem Titel, `CoreError::NotFound` für eine unbekannte
    /// Liste; Fehler des Repositories.
    pub async fn execute(&self, input: CreateTaskInput) -> CoreResult<Task> {
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        let list_id = match input.list_id {
            Some(id) => repo
                .get_task_list(id)
                .await?
                .ok_or_else(|| CoreError::NotFound(format!("list {id}")))?
                .id(),
            None => EnsureInbox::new(self.deps.clone()).execute().await?.id(),
        };
        let key = key_after(repo.last_sort_key(list_id).await?.as_ref())?;
        let clock = self.deps.clock.as_ref();
        let mut task = Task::new(list_id, &input.title, key, clock)?;
        task.set_notes(&input.notes, clock);
        task.set_priority(input.priority, clock);
        task.set_due(input.due, clock)?;
        if let Some(rule) = input.recurrence {
            super::recurring::apply_rule(&mut task, &rule, self.deps.zones.as_deref(), clock)?;
        }
        repo.commit(ChangeSet::new().upsert_task(task.clone()))
            .await?;
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{collect_all, test_util::deps, CreateTaskList, CreateTaskListInput},
        ports::{SmartView, TaskQuery, TaskRepository},
    };

    fn create(uc: &CreateTask, title: &str, list_id: Option<TaskListId>) -> CoreResult<Task> {
        block_on(uc.execute(CreateTaskInput {
            title: title.into(),
            list_id,
            ..CreateTaskInput::default()
        }))
    }

    #[test]
    fn without_list_goes_to_inbox_and_appends() {
        let (deps, repo) = deps();
        let uc = CreateTask::new(deps.clone());
        let a = create(&uc, "a", None).unwrap();
        let b = create(&uc, "b", None).unwrap();
        let inbox = block_on(EnsureInbox::new(deps).execute()).unwrap();
        assert_eq!(a.list_id(), inbox.id());
        assert!(a.sort_key() < b.sort_key());
        let titles: Vec<String> = block_on(collect_all(
            repo.as_ref(),
            &TaskQuery::view(SmartView::Inbox),
        ))
        .unwrap()
        .into_iter()
        .map(|s| s.title)
        .collect();
        assert_eq!(titles, ["a", "b"]);
    }

    #[test]
    fn into_given_list() {
        let (deps, _) = deps();
        let list = block_on(
            CreateTaskList::new(deps.clone()).execute(CreateTaskListInput {
                name: "Arbeit".into(),
            }),
        )
        .unwrap();
        let task = create(&CreateTask::new(deps), "Bericht", Some(list.id())).unwrap();
        assert_eq!(task.list_id(), list.id());
    }

    #[test]
    fn with_initial_fields_in_one_commit() {
        let (deps, repo) = deps();
        let due = Due::Date(crate::domain::NaiveDate::from_ymd_opt(2026, 10, 24).unwrap());
        let task = block_on(CreateTask::new(deps).execute(CreateTaskInput {
            title: "Übungsblatt 3".into(),
            notes: "Aufgaben 1–4".into(),
            priority: Priority::High,
            due: Some(due.clone()),
            ..CreateTaskInput::default()
        }))
        .unwrap();
        assert_eq!(task.notes(), "Aufgaben 1–4");
        assert_eq!(task.priority(), Priority::High);
        assert_eq!(task.due(), Some(&due));
        assert_eq!(block_on(repo.get_task(task.id())).unwrap(), Some(task));
    }

    #[test]
    fn errors() {
        let (deps, repo) = deps();
        let uc = CreateTask::new(deps);
        assert!(matches!(
            create(&uc, "  ", None),
            Err(CoreError::Validation(_))
        ));
        assert!(matches!(
            create(&uc, "x", Some(TaskListId::new())),
            Err(CoreError::NotFound(_))
        ));
        let all = block_on(collect_all(repo.as_ref(), &TaskQuery::view(SmartView::All))).unwrap();
        assert!(all.is_empty());
    }
}
