//! [`SqliteTaskRepository`]: der [`TaskRepository`]-Port auf SQLite.

use std::path::Path;

use rusqlite::{params_from_iter, Connection, TransactionBehavior};
use todo_core::{
    domain::{Task, TaskId, TaskList, TaskListId},
    ports::{
        Change, ChangeSet, CommitReceipt, EntityRef, Page, PageRequest, TaskQuery, TaskRepository,
        TaskSummary, PAGE_LIMIT_MAX,
    },
    CoreError, CoreResult,
};

use crate::{
    connection::Database,
    error::storage,
    mapping::{
        read_list, read_summary, read_task, write_list, write_task, LIST_COLUMNS, TASK_COLUMNS,
    },
    query::{list_tasks_sql, Position},
};

/// Lokaler Speicher auf einer SQLite-Datei (WAL). Alle Zugriffe laufen über einen eigenen
/// Datenbank-Thread (siehe ADR-014); das Repository ist billig zu teilen (`Arc`).
#[derive(Debug)]
pub struct SqliteTaskRepository {
    db: Database,
}

impl SqliteTaskRepository {
    /// Öffnet oder erzeugt die Datenbank unter `path` und bringt das Schema auf den neuesten
    /// Stand. Das Verzeichnis muss existieren.
    ///
    /// # Errors
    /// `CoreError::Storage`, wenn die Datei nicht geöffnet, konfiguriert oder migriert werden kann.
    pub fn open(path: impl AsRef<Path>) -> CoreResult<Self> {
        Ok(Self {
            db: Database::open(path.as_ref())?,
        })
    }

    /// Flüchtige Datenbank im Speicher (Tests, Experimente).
    ///
    /// # Errors
    /// `CoreError::Storage`, wenn SQLite nicht initialisiert werden kann.
    pub fn open_in_memory() -> CoreResult<Self> {
        Ok(Self {
            db: Database::open_in_memory()?,
        })
    }
}

#[async_trait::async_trait]
impl TaskRepository for SqliteTaskRepository {
    async fn get_task(&self, id: TaskId) -> CoreResult<Option<Task>> {
        self.db
            .call(move |conn| {
                let mut stmt = conn
                    .prepare_cached(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"))
                    .map_err(storage)?;
                let mut rows = stmt.query([id.to_string()]).map_err(storage)?;
                rows.next().map_err(storage)?.map(read_task).transpose()
            })
            .await
    }

    async fn get_task_list(&self, id: TaskListId) -> CoreResult<Option<TaskList>> {
        self.db
            .call(move |conn| {
                let mut stmt = conn
                    .prepare_cached(&format!(
                        "SELECT {LIST_COLUMNS} FROM task_lists WHERE id = ?1"
                    ))
                    .map_err(storage)?;
                let mut rows = stmt.query([id.to_string()]).map_err(storage)?;
                rows.next().map_err(storage)?.map(read_list).transpose()
            })
            .await
    }

    async fn list_task_lists(&self) -> CoreResult<Vec<TaskList>> {
        self.db
            .call(|conn| {
                let mut stmt = conn
                    .prepare_cached(&format!(
                        "SELECT {LIST_COLUMNS} FROM task_lists ORDER BY sort_key, id"
                    ))
                    .map_err(storage)?;
                let mut rows = stmt.query([]).map_err(storage)?;
                let mut lists = Vec::new();
                while let Some(row) = rows.next().map_err(storage)? {
                    lists.push(read_list(row)?);
                }
                Ok(lists)
            })
            .await
    }

    async fn list_tasks(
        &self,
        query: &TaskQuery,
        page: PageRequest,
    ) -> CoreResult<Page<TaskSummary>> {
        if !(1..=PAGE_LIMIT_MAX).contains(&page.limit) {
            return Err(CoreError::Validation(format!(
                "page limit must be 1..={PAGE_LIMIT_MAX}, got {}",
                page.limit
            )));
        }
        let after = page
            .cursor
            .as_deref()
            .map(|c| Position::decode(c, &query.view))
            .transpose()?;
        let view = query.view;
        let limit = page.limit as usize;
        // Eine Zeile mehr holen, um zu wissen, ob es eine weitere Seite gibt.
        let (sql, params) = list_tasks_sql(query, after.as_ref(), page.limit + 1);
        self.db
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(&sql).map_err(storage)?;
                let mut rows = stmt.query(params_from_iter(params)).map_err(storage)?;
                let mut items = Vec::with_capacity(limit + 1);
                while let Some(row) = rows.next().map_err(storage)? {
                    items.push(read_summary(row)?);
                }
                let next_cursor = (items.len() > limit).then(|| {
                    items.truncate(limit);
                    Position::of(&items[limit - 1], &view).encode()
                });
                Ok(Page { items, next_cursor })
            })
            .await
    }

    async fn commit(&self, changes: ChangeSet) -> CoreResult<CommitReceipt> {
        let changed = changes.entities();
        self.db
            .call(move |conn| {
                let tx = conn
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(storage)?;
                for change in changes {
                    apply(&tx, &change)?;
                }
                // Aufgeschobene Fremdschlüssel werden hier geprüft; bei Verstoß rollt der Drop
                // der Transaktion alles zurück.
                tx.commit().map_err(storage)?;
                Ok(CommitReceipt { changed })
            })
            .await
    }
}

fn apply(conn: &Connection, change: &Change) -> CoreResult<()> {
    match change {
        Change::UpsertTask(task) => write_task(conn, task),
        Change::UpsertList(list) => write_list(conn, list),
        Change::Delete(EntityRef::Task(id)) => delete(conn, "tasks", &id.to_string()),
        Change::Delete(EntityRef::TaskList(id)) => delete(conn, "task_lists", &id.to_string()),
    }
}

fn delete(conn: &Connection, table: &str, id: &str) -> CoreResult<()> {
    conn.prepare_cached(&format!("DELETE FROM {table} WHERE id = ?1"))
        .and_then(|mut stmt| stmt.execute([id]))
        .map_err(storage)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use tempfile::TempDir;
    use todo_core::{
        domain::{SortKey, Timestamp},
        ports::{FixedClock, SmartView},
    };

    use super::*;

    mod contract_in_memory {
        todo_core::repository_contract_tests!(
            crate::SqliteTaskRepository::open_in_memory().expect("open")
        );
    }

    mod contract_on_disk {
        todo_core::repository_contract_tests!(super::OnDisk::new());
    }

    /// Repository auf einer Temp-Datei; hält das Verzeichnis so lange am Leben wie die Datenbank.
    #[derive(Debug)]
    struct OnDisk {
        repo: SqliteTaskRepository,
        _dir: TempDir,
    }

    impl OnDisk {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let repo = SqliteTaskRepository::open(dir.path().join("todo.sqlite3")).expect("open");
            Self { repo, _dir: dir }
        }
    }

    #[async_trait::async_trait]
    impl TaskRepository for OnDisk {
        async fn get_task(&self, id: TaskId) -> CoreResult<Option<Task>> {
            self.repo.get_task(id).await
        }
        async fn get_task_list(&self, id: TaskListId) -> CoreResult<Option<TaskList>> {
            self.repo.get_task_list(id).await
        }
        async fn list_task_lists(&self) -> CoreResult<Vec<TaskList>> {
            self.repo.list_task_lists().await
        }
        async fn list_tasks(
            &self,
            query: &TaskQuery,
            page: PageRequest,
        ) -> CoreResult<Page<TaskSummary>> {
            self.repo.list_tasks(query, page).await
        }
        async fn commit(&self, changes: ChangeSet) -> CoreResult<CommitReceipt> {
            self.repo.commit(changes).await
        }
    }

    fn inbox_with_task(clock: &FixedClock) -> (TaskList, Task) {
        let inbox = TaskList::inbox(SortKey::first(), clock);
        let task = Task::new(inbox.id(), "Milch", SortKey::first(), clock).unwrap();
        (inbox, task)
    }

    #[test]
    fn data_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todo.sqlite3");
        let clock = FixedClock(Timestamp::from_millis(42));
        let (inbox, task) = inbox_with_task(&clock);
        {
            let repo = SqliteTaskRepository::open(&path).unwrap();
            block_on(
                repo.commit(
                    ChangeSet::new()
                        .upsert_list(inbox.clone())
                        .upsert_task(task.clone()),
                ),
            )
            .unwrap();
        }
        let repo = SqliteTaskRepository::open(&path).unwrap();
        assert_eq!(block_on(repo.get_task(task.id())).unwrap(), Some(task));
        assert_eq!(block_on(repo.list_task_lists()).unwrap(), [inbox]);
    }

    #[test]
    fn failed_commit_leaves_connection_usable() {
        let repo = SqliteTaskRepository::open_in_memory().unwrap();
        let clock = FixedClock(Timestamp::EPOCH);
        let (inbox, task) = inbox_with_task(&clock);
        let orphan = block_on(repo.commit(ChangeSet::new().upsert_task(task.clone())));
        assert!(
            matches!(orphan, Err(CoreError::Validation(_))),
            "{orphan:?}"
        );
        block_on(
            repo.commit(
                ChangeSet::new()
                    .upsert_list(inbox)
                    .upsert_task(task.clone()),
            ),
        )
        .expect("Transaktion wurde sauber zurückgerollt");
        assert_eq!(block_on(repo.get_task(task.id())).unwrap(), Some(task));
    }

    #[test]
    fn corrupt_rows_are_storage_errors() {
        let repo = SqliteTaskRepository::open_in_memory().unwrap();
        let clock = FixedClock(Timestamp::EPOCH);
        let (inbox, task) = inbox_with_task(&clock);
        block_on(
            repo.commit(
                ChangeSet::new()
                    .upsert_list(inbox)
                    .upsert_task(task.clone()),
            ),
        )
        .unwrap();
        block_on(repo.db.call(|conn| {
            conn.execute("UPDATE tasks SET sort_key = 'kaputt!', title = ''", [])
                .map_err(storage)
        }))
        .unwrap();
        let get = block_on(repo.get_task(task.id()));
        assert!(matches!(get, Err(CoreError::Storage(_))), "{get:?}");
        let page =
            block_on(repo.list_tasks(&TaskQuery::view(SmartView::All), PageRequest::first(10)));
        assert!(matches!(page, Err(CoreError::Storage(_))), "{page:?}");
    }

    #[test]
    fn repository_and_futures_are_send_sync() {
        fn assert_send<T: Send>(_: &T) {}
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SqliteTaskRepository>();
        let repo = SqliteTaskRepository::open_in_memory().unwrap();
        let fut = repo.list_task_lists();
        assert_send(&fut);
    }
}
