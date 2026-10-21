//! Felder einer Task ändern.

use super::{load_task, Deps};
use crate::{
    domain::{ChecklistItem, Due, Priority, Recurrence, Task, TaskId},
    ports::ChangeSet,
    CoreResult,
};

/// Zu ändernde Felder; `None` = unverändert. `due: Some(None)` entfernt die Fälligkeit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub priority: Option<Priority>,
    pub due: Option<Option<Due>>,
    /// `Some(None)` beendet die Wiederholung (N4).
    pub recurrence: Option<Option<Recurrence>>,
    /// Ersetzt die ganze Checkliste (N8, ADR-034).
    pub checklist: Option<Vec<ChecklistItem>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateTaskInput {
    pub id: TaskId,
    pub patch: TaskPatch,
}

/// Wendet einen Patch an. Ungültige Felder führen zu einem Fehler ohne jede Änderung.
#[derive(Debug, Clone)]
pub struct UpdateTask {
    deps: Deps,
}

impl UpdateTask {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für unbekannte Tasks, `CoreError::Validation` bei ungültigen
    /// Werten; Fehler des Repositories.
    pub async fn execute(&self, input: UpdateTaskInput) -> CoreResult<Task> {
        let original = load_task(self.deps.repo.as_ref(), input.id).await?;
        let clock = self.deps.clock.as_ref();
        let mut task = original.clone();
        let TaskPatch {
            title,
            notes,
            priority,
            due,
            recurrence,
            checklist,
        } = input.patch;
        if let Some(items) = checklist {
            task.set_checklist(items, clock)?;
        }
        if let Some(title) = title {
            task.rename(&title, clock)?;
        }
        if let Some(notes) = notes {
            task.set_notes(&notes, clock);
        }
        if let Some(priority) = priority {
            task.set_priority(priority, clock);
        }
        if let Some(due) = due {
            let removed = due.is_none();
            task.set_due(due, clock)?;
            // Ohne Fälligkeit keine Serie (sonst bliebe eine wirkungslose Regel stehen).
            if removed {
                task.set_recurrence(None, None, clock)?;
            }
        }
        match recurrence {
            Some(Some(rule)) => {
                super::recurring::apply_rule(&mut task, &rule, self.deps.zones.as_deref(), clock)?;
            }
            Some(None) => task.set_recurrence(None, None, clock)?,
            None => {}
        }
        if task != original {
            self.deps
                .repo
                .commit(ChangeSet::new().upsert_task(task.clone()))
                .await?;
        }
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{test_util::deps, CreateTask, CreateTaskInput},
        domain::NaiveDate,
        ports::TaskRepository,
        CoreError,
    };

    fn setup() -> (
        UpdateTask,
        std::sync::Arc<crate::testing::InMemoryTaskRepository>,
        Task,
    ) {
        let (deps, repo) = deps();
        let task = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "Milch".into(),
            list_id: None,
            ..CreateTaskInput::default()
        }))
        .unwrap();
        (UpdateTask::new(deps), repo, task)
    }

    #[test]
    fn applies_all_fields() {
        let (uc, repo, task) = setup();
        let due = Due::Date(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap());
        let patch = TaskPatch {
            title: Some("Hafermilch".into()),
            notes: Some("2 Liter".into()),
            priority: Some(Priority::High),
            due: Some(Some(due.clone())),
            recurrence: None,
            checklist: None,
        };
        let updated = block_on(uc.execute(UpdateTaskInput {
            id: task.id(),
            patch,
        }))
        .unwrap();
        assert_eq!(updated.title(), "Hafermilch");
        assert_eq!(updated.notes(), "2 Liter");
        assert_eq!(updated.priority(), Priority::High);
        assert_eq!(updated.due(), Some(&due));
        assert_eq!(
            block_on(repo.get_task(task.id())).unwrap(),
            Some(updated.clone())
        );

        let cleared = block_on(uc.execute(UpdateTaskInput {
            id: task.id(),
            patch: TaskPatch {
                due: Some(None),
                ..TaskPatch::default()
            },
        }))
        .unwrap();
        assert_eq!(cleared.due(), None);
        assert_eq!(cleared.title(), "Hafermilch", "übrige Felder bleiben");
    }

    #[test]
    fn invalid_field_changes_nothing() {
        let (uc, repo, task) = setup();
        let patch = TaskPatch {
            notes: Some("neu".into()),
            title: Some("  ".into()),
            ..TaskPatch::default()
        };
        let result = block_on(uc.execute(UpdateTaskInput {
            id: task.id(),
            patch,
        }));
        assert!(matches!(result, Err(CoreError::Validation(_))));
        assert_eq!(block_on(repo.get_task(task.id())).unwrap(), Some(task));
    }

    #[test]
    fn empty_patch_is_noop_and_unknown_is_not_found() {
        let (uc, _, task) = setup();
        let same = block_on(uc.execute(UpdateTaskInput {
            id: task.id(),
            patch: TaskPatch::default(),
        }))
        .unwrap();
        assert_eq!(same, task);
        let result = block_on(uc.execute(UpdateTaskInput {
            id: TaskId::new(),
            patch: TaskPatch::default(),
        }));
        assert!(matches!(result, Err(CoreError::NotFound(_))));
    }
}
