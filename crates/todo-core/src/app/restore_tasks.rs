//! Gelöschte Tasks wiederherstellen (Rückgängig nach Löschen).

use std::collections::HashSet;

use super::{check_bulk_len, Deps, EnsureInbox};
use crate::{
    domain::{Task, TaskId, TaskList, TaskListId, TaskParts},
    ports::{Change, ChangeSet},
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreTasksInput {
    /// Zustand der Tasks vor dem Löschen (z. B. aus [`super::DeleteTasks`]); `id` und
    /// `updated_at` werden ersetzt.
    pub tasks: Vec<TaskParts>,
}

/// Legt Kopien gelöschter Tasks an – in **einem** Commit.
///
/// Eine gelöschte ID ist im Sync-Modell endgültig (Tombstone), deshalb bekommt jede Kopie eine
/// **neue** ID. Alle übrigen Felder bleiben – inkl. `created_at`, Status/`completed_at` und dem
/// alten Sort-Key, damit die Task ohne Nachbar-Suche an ihre alte Stelle kommt (Gleichstand löst
/// der `TaskId`-Tiebreak). Existiert die Liste nicht mehr, landet die Kopie in der Inbox.
#[derive(Debug, Clone)]
pub struct RestoreTasks {
    deps: Deps,
}

impl RestoreTasks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Liefert die neuen Tasks (Reihenfolge wie die Eingabe).
    ///
    /// # Errors
    /// `CoreError::Validation` bei mehr als [`super::BULK_LIMIT_MAX`] Tasks oder verletzter
    /// Invariante (z. B. leerer Titel); Fehler des Repositories. Dann ist nichts geschrieben.
    pub async fn execute(&self, input: RestoreTasksInput) -> CoreResult<Vec<Task>> {
        check_bulk_len(input.tasks.len())?;
        if input.tasks.is_empty() {
            return Ok(Vec::new());
        }
        let repo = self.deps.repo.as_ref();
        let mut lists: HashSet<TaskListId> = repo
            .list_task_lists()
            .await?
            .iter()
            .map(TaskList::id)
            .collect();
        let now = self.deps.clock.now();
        // Neue IDs in der alten Anzeigereihenfolge vergeben (UUIDv7 ist im Prozess monoton): So
        // bleibt die Reihenfolge auch zwischen Tasks mit gleichem Sort-Key (Tiebreak `TaskId`).
        let mut order: Vec<usize> = (0..input.tasks.len()).collect();
        order.sort_by(|&a, &b| {
            let (a, b) = (&input.tasks[a], &input.tasks[b]);
            (&a.sort_key, a.id).cmp(&(&b.sort_key, b.id))
        });
        let mut ids: Vec<Option<TaskId>> = vec![None; input.tasks.len()];
        for i in order {
            ids[i] = Some(TaskId::new());
        }
        let mut restored = Vec::with_capacity(input.tasks.len());
        for (parts, id) in input.tasks.into_iter().zip(ids.into_iter().flatten()) {
            let mut list_id = parts.list_id;
            if !lists.contains(&list_id) {
                list_id = EnsureInbox::new(self.deps.clone()).execute().await?.id();
                lists.insert(list_id);
            }
            restored.push(Task::from_parts(TaskParts {
                id,
                list_id,
                updated_at: now,
                ..parts
            })?);
        }
        let mut changes = ChangeSet::new();
        for task in &restored {
            changes.push(Change::UpsertTask(task.clone()));
        }
        repo.commit(changes).await?;
        Ok(restored)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{
            collect_all, test_util::deps, CompleteTask, CompleteTaskInput, CreateTask,
            CreateTaskInput, CreateTaskList, CreateTaskListInput, DeleteTaskList,
            DeleteTaskListInput, DeleteTasks, DeleteTasksInput, EnsureInbox, TaskPatch, UpdateTask,
            UpdateTaskInput, BULK_LIMIT_MAX,
        },
        domain::{Due, NaiveDate, Priority},
        ports::{SmartView, TaskQuery, TaskRepository},
        CoreError,
    };

    fn parts(task: &Task) -> TaskParts {
        TaskParts {
            id: task.id(),
            list_id: task.list_id(),
            parent_id: task.parent_id(),
            title: task.title().to_owned(),
            notes: task.notes().to_owned(),
            status: task.status(),
            priority: task.priority(),
            due: task.due().cloned(),
            tags: task.tags().to_vec(),
            sort_key: task.sort_key().clone(),
            created_at: task.created_at(),
            updated_at: task.updated_at(),
            completed_at: task.completed_at(),
        }
    }

    #[test]
    fn restores_all_fields_with_new_ids_at_old_position() {
        let (deps, repo) = deps();
        let create = CreateTask::new(deps.clone());
        let mut tasks: Vec<Task> = ["a", "b", "c"]
            .iter()
            .map(|t| {
                block_on(create.execute(CreateTaskInput {
                    title: (*t).into(),
                    list_id: None,
                    ..CreateTaskInput::default()
                }))
                .unwrap()
            })
            .collect();
        tasks[1] = block_on(UpdateTask::new(deps.clone()).execute(UpdateTaskInput {
            id: tasks[1].id(),
            patch: TaskPatch {
                notes: Some("Notiz".into()),
                priority: Some(Priority::High),
                due: Some(Some(Due::Date(
                    NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
                ))),
                ..TaskPatch::default()
            },
        }))
        .unwrap();
        tasks[1] = block_on(
            CompleteTask::new(deps.clone()).execute(CompleteTaskInput { id: tasks[1].id() }),
        )
        .unwrap();

        let deleted = block_on(DeleteTasks::new(deps.clone()).execute(DeleteTasksInput {
            ids: vec![tasks[1].id()],
        }))
        .unwrap();
        let restored = block_on(RestoreTasks::new(deps).execute(RestoreTasksInput {
            tasks: deleted.iter().map(parts).collect(),
        }))
        .unwrap();

        assert_eq!(restored.len(), 1);
        let (old, new) = (&tasks[1], &restored[0]);
        assert_ne!(new.id(), old.id(), "neue ID");
        assert_eq!(
            parts(new),
            TaskParts {
                id: new.id(),
                updated_at: new.updated_at(),
                ..parts(old)
            }
        );
        assert!(new.is_done());
        assert_eq!(
            block_on(repo.get_task(new.id())).unwrap().as_ref(),
            Some(new)
        );
        let order: Vec<String> = block_on(collect_all(
            repo.as_ref(),
            &TaskQuery::view(SmartView::Inbox),
        ))
        .unwrap()
        .into_iter()
        .map(|s| s.title)
        .collect();
        assert_eq!(order, ["a", "b", "c"], "alte Position");
    }

    /// Gleiche Sort-Keys (parallel angelegt, Sync) ordnet die `TaskId`; die neuen IDs müssen die
    /// alte Reihenfolge fortsetzen, auch wenn die Eingabe anders sortiert ist.
    #[test]
    fn keeps_order_among_equal_sort_keys() {
        let (deps, repo) = deps();
        let clock = crate::ports::FixedClock(crate::domain::Timestamp::from_millis(5));
        let key = crate::domain::SortKey::first();
        let mut tasks: Vec<Task> = (0..20)
            .map(|i| {
                Task::new(TaskListId::INBOX, &format!("t{i:02}"), key.clone(), &clock).unwrap()
            })
            .collect();
        tasks.sort_by_key(Task::id);
        block_on(EnsureInbox::new(deps.clone()).execute()).unwrap();
        let mut changes = ChangeSet::new();
        for t in &tasks {
            changes.push(Change::UpsertTask(t.clone()));
        }
        block_on(repo.commit(changes)).unwrap();
        let titles = || -> Vec<String> {
            block_on(collect_all(
                repo.as_ref(),
                &TaskQuery::view(SmartView::Inbox),
            ))
            .unwrap()
            .into_iter()
            .map(|s| s.title)
            .collect()
        };
        let before = titles();

        let mut deleted = block_on(DeleteTasks::new(deps.clone()).execute(DeleteTasksInput {
            ids: tasks.iter().map(Task::id).collect(),
        }))
        .unwrap();
        deleted.reverse();
        block_on(RestoreTasks::new(deps).execute(RestoreTasksInput {
            tasks: deleted.iter().map(parts).collect(),
        }))
        .unwrap();
        assert_eq!(titles(), before);
    }

    #[test]
    fn deleted_list_falls_back_to_inbox() {
        let (deps, repo) = deps();
        let list = block_on(
            CreateTaskList::new(deps.clone()).execute(CreateTaskListInput { name: "B".into() }),
        )
        .unwrap();
        let task = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "x".into(),
            list_id: Some(list.id()),
            ..CreateTaskInput::default()
        }))
        .unwrap();
        let deleted = block_on(DeleteTasks::new(deps.clone()).execute(DeleteTasksInput {
            ids: vec![task.id()],
        }))
        .unwrap();
        block_on(DeleteTaskList::new(deps.clone()).execute(DeleteTaskListInput { id: list.id() }))
            .unwrap();

        let restored = block_on(RestoreTasks::new(deps).execute(RestoreTasksInput {
            tasks: deleted.iter().map(parts).collect(),
        }))
        .unwrap();
        assert_eq!(restored[0].list_id(), TaskListId::INBOX);
        assert!(block_on(repo.get_task(restored[0].id())).unwrap().is_some());
    }

    #[test]
    fn empty_invalid_and_too_many() {
        let (deps, repo) = deps();
        let task = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "x".into(),
            list_id: None,
            ..CreateTaskInput::default()
        }))
        .unwrap();
        let uc = RestoreTasks::new(deps);
        let ops = block_on(repo.pending_op_count()).unwrap();
        assert!(
            block_on(uc.execute(RestoreTasksInput { tasks: Vec::new() }))
                .unwrap()
                .is_empty()
        );

        let bad = TaskParts {
            title: "  ".into(),
            ..parts(&task)
        };
        let result = block_on(uc.execute(RestoreTasksInput {
            tasks: vec![parts(&task), bad],
        }));
        assert!(matches!(result, Err(CoreError::Validation(_))));
        assert_eq!(
            block_on(repo.pending_op_count()).unwrap(),
            ops,
            "nichts geschrieben"
        );

        let many = vec![parts(&task); BULK_LIMIT_MAX + 1];
        assert!(matches!(
            block_on(uc.execute(RestoreTasksInput { tasks: many })),
            Err(CoreError::Validation(_))
        ));
    }
}
