//! [`SqliteTaskRepository`]: der [`TaskRepository`]-Port auf SQLite.

use std::path::Path;

use rusqlite::{params_from_iter, Connection, TransactionBehavior};
use todo_core::{
    domain::{Task, TaskId, TaskList, TaskListId},
    ports::{
        Change, ChangeSet, CommitReceipt, EntityRef, Page, PageRequest, SmartView, TaskQuery,
        TaskRepository, TaskSummary, PAGE_LIMIT_MAX,
    },
    CoreError, CoreResult,
};

use crate::{
    connection::Database,
    error::{corrupt, storage},
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
        let query = *query;
        let limit = page.limit as usize;
        self.db
            .call(move |conn| {
                let Some(query) = resolve_inbox(conn, query)? else {
                    return Ok(Page {
                        items: Vec::new(),
                        next_cursor: None,
                    });
                };
                // Eine Zeile mehr holen, um zu wissen, ob es eine weitere Seite gibt.
                let (sql, params) = list_tasks_sql(&query, after.as_ref(), page.limit + 1);
                let mut stmt = conn.prepare_cached(&sql).map_err(storage)?;
                let mut rows = stmt.query(params_from_iter(params)).map_err(storage)?;
                let mut items = Vec::with_capacity(limit + 1);
                while let Some(row) = rows.next().map_err(storage)? {
                    items.push(read_summary(row)?);
                }
                let next_cursor = (items.len() > limit).then(|| {
                    items.truncate(limit);
                    Position::of(&items[limit - 1], &query.view).encode()
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

/// Ersetzt die Inbox-Ansicht durch die Ansicht der (einen) Inbox-Liste: `list_id = ?` liefert
/// die Zeilen direkt in Index-Reihenfolge, `list_id IN (…)` bräuchte einen Sortierschritt über
/// alle Treffer. `None`, wenn es keine Inbox gibt (leeres Ergebnis).
fn resolve_inbox(conn: &Connection, query: TaskQuery) -> CoreResult<Option<TaskQuery>> {
    if query.view != SmartView::Inbox {
        return Ok(Some(query));
    }
    let mut stmt = conn
        .prepare_cached("SELECT id FROM task_lists WHERE is_inbox = 1 LIMIT 2")
        .map_err(storage)?;
    let ids = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    match ids.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(TaskQuery {
            view: SmartView::List(TaskListId::parse(id).map_err(|e| corrupt("list id", e))?),
            ..query
        })),
        // Mehrere Inboxen (z. B. nach einem Sync-Konflikt): allgemeine, langsamere Form.
        _ => Ok(Some(query)),
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
        ports::FixedClock,
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

    /// Die häufigen Listenabfragen lesen in Index-Reihenfolge – kein Sortierschritt, kein
    /// Tabellen-Scan (Frühwarnung, falls ein Schema- oder Abfrageumbau das bricht).
    #[test]
    fn hot_queries_use_indexes_without_sorting() {
        use todo_core::domain::TaskStatus;

        use crate::query::list_tasks_sql;

        let repo = SqliteTaskRepository::open_in_memory().unwrap();
        let clock = FixedClock(Timestamp::EPOCH);
        let (inbox, task) = inbox_with_task(&clock);
        block_on(
            repo.commit(
                ChangeSet::new()
                    .upsert_list(inbox.clone())
                    .upsert_task(task),
            ),
        )
        .unwrap();
        let open = Some(TaskStatus::Open);
        let sort_cursor = Position::BySortKey(SortKey::first(), TaskId::new());
        let done_cursor = Position::ByCompletion(Timestamp::EPOCH, TaskId::new());
        let cases = [
            (SmartView::Inbox, None, None),
            (SmartView::Inbox, open, Some(&sort_cursor)),
            (SmartView::List(inbox.id()), open, Some(&sort_cursor)),
            (SmartView::All, None, Some(&sort_cursor)),
            (SmartView::All, open, None),
            (SmartView::Completed, None, None),
            (SmartView::Completed, None, Some(&done_cursor)),
        ];
        for (view, status, cursor) in cases {
            let cursor = cursor.cloned();
            let plan = block_on(repo.db.call(move |conn| {
                let query = TaskQuery {
                    view,
                    status,
                    tag: None,
                };
                let query = resolve_inbox(conn, query)?.expect("inbox exists");
                let (sql, params) = list_tasks_sql(&query, cursor.as_ref(), 51);
                let mut stmt = conn
                    .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                    .map_err(storage)?;
                let rows = stmt
                    .query_map(params_from_iter(params), |r| r.get::<_, String>(3))
                    .map_err(storage)?;
                rows.collect::<Result<Vec<_>, _>>().map_err(storage)
            }))
            .unwrap();
            let text = plan.join(" | ");
            assert!(text.contains("USING INDEX"), "{view:?} {status:?}: {text}");
            assert!(!text.contains("TEMP B-TREE"), "{view:?} {status:?}: {text}");
            assert!(
                !text.contains("SCAN task_lists"),
                "{view:?} {status:?}: {text}"
            );
        }
    }

    #[test]
    fn inbox_view_without_inbox_is_empty_and_with_two_inboxes_shows_both() {
        let repo = SqliteTaskRepository::open_in_memory().unwrap();
        let clock = FixedClock(Timestamp::EPOCH);
        let inbox_view = || {
            block_on(repo.list_tasks(&TaskQuery::view(SmartView::Inbox), PageRequest::first(10)))
                .unwrap()
                .items
        };
        assert!(inbox_view().is_empty());
        let (a, task_a) = inbox_with_task(&clock);
        let (b, task_b) = inbox_with_task(&clock);
        block_on(
            repo.commit(
                ChangeSet::new()
                    .upsert_list(a)
                    .upsert_list(b)
                    .upsert_task(task_a.clone())
                    .upsert_task(task_b.clone()),
            ),
        )
        .unwrap();
        let mut expected = [task_a.id(), task_b.id()];
        expected.sort();
        let ids: Vec<TaskId> = inbox_view().iter().map(|s| s.id).collect();
        assert_eq!(ids, expected);
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
