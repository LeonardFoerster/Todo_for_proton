//! Konfigurierte SQLite-Verbindung und der Thread, der sie exklusiv besitzt.
//!
//! # Async-Strategie
//!
//! Die Verbindung lebt in einem **dedizierten Thread**; Aufrufer schicken ihm Closures über einen
//! Kanal und warten asynchron (`oneshot`) auf das Ergebnis. Dadurch
//! - blockiert keine Datenbankarbeit den Executor des Aufrufers (Tauri/tokio, CLI mit `block_on`),
//! - bleiben `todo-core` und dieses Crate frei von einer bestimmten Async-Runtime,
//! - sind alle Zugriffe ohne Lock serialisiert (eine App, ein Schreiber; Lesen ist schnell genug).
//!
//! Alternative wäre `Mutex<Connection>` + `spawn_blocking` in der Shell gewesen – das hätte jede
//! Shell an tokio gebunden und Locks über `.await` hinweg erfordert.

use std::{
    fmt,
    path::Path,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

use futures::channel::oneshot;
use rusqlite::Connection;
use todo_core::{CoreError, CoreResult};

use crate::{error::storage, migrations};

/// Wartezeit, wenn ein anderer Prozess (z. B. die Dev-CLI) die Datei gerade sperrt.
pub(crate) const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Genug Platz für alle Varianten der dynamisch zusammengesetzten Listen-Abfragen.
const STATEMENT_CACHE_CAPACITY: usize = 64;

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

/// Besitzt den Datenbank-Thread. Beim Drop wird der Kanal geschlossen und auf das Ende des
/// Threads gewartet, damit die Verbindung (und der WAL-Checkpoint) sauber abgeschlossen sind.
pub(crate) struct Database {
    jobs: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for Database {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Database {
    /// Öffnet (oder erzeugt) die Datei, konfiguriert und migriert sie und startet den Thread.
    pub(crate) fn open(path: &Path) -> CoreResult<Self> {
        let mut conn = Connection::open(path).map_err(storage)?;
        configure(&conn, true)?;
        migrations::migrate(&mut conn)?;
        Self::start(conn)
    }

    /// Flüchtige Datenbank für Tests (kein WAL – SQLite unterstützt es im Speicher nicht).
    pub(crate) fn open_in_memory() -> CoreResult<Self> {
        let mut conn = Connection::open_in_memory().map_err(storage)?;
        configure(&conn, false)?;
        migrations::migrate(&mut conn)?;
        Self::start(conn)
    }

    fn start(mut conn: Connection) -> CoreResult<Self> {
        let (jobs, rx) = mpsc::channel::<Job>();
        let thread = thread::Builder::new()
            .name("todo-sqlite".into())
            .spawn(move || {
                for job in rx {
                    job(&mut conn);
                }
            })
            .map_err(|e| CoreError::Storage(format!("cannot start database thread: {e}")))?;
        Ok(Self {
            jobs: Some(jobs),
            thread: Some(thread),
        })
    }

    /// Führt `f` auf dem Datenbank-Thread aus.
    ///
    /// # Errors
    /// Fehler von `f`; `CoreError::Storage`, wenn der Thread nicht mehr läuft.
    pub(crate) async fn call<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> CoreResult<R> + Send + 'static,
    ) -> CoreResult<R> {
        let (tx, rx) = oneshot::channel();
        let job: Job = Box::new(move |conn| {
            // Der Aufrufer kann inzwischen aufgegeben haben (Future gedroppt) – dann ist das egal.
            let _ = tx.send(f(conn));
        });
        let jobs = self.jobs.as_ref().ok_or_else(closed)?;
        jobs.send(job).map_err(|_| closed())?;
        rx.await.map_err(|_| closed())?
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(thread) = self.thread.take() {
            // Ein Panic im Thread wurde bereits gemeldet; hier ist nichts mehr zu retten.
            let _ = thread.join();
        }
    }
}

fn closed() -> CoreError {
    CoreError::Storage("database thread is not running".to_owned())
}

/// Setzt die Pragmas, die jede Verbindung braucht (siehe `architecture.md` §6).
fn configure(conn: &Connection, wal: bool) -> CoreResult<()> {
    if wal {
        let mode: String = conn
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
            .map_err(storage)?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(CoreError::Storage(format!(
                "cannot enable WAL journal mode (got {mode:?})"
            )));
        }
    }
    // NORMAL ist mit WAL crash-sicher; nur die letzten Transaktionen vor einem Stromausfall
    // können verloren gehen. FULL würde jeden Commit mit einem fsync bezahlen.
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(storage)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(storage)?;
    conn.busy_timeout(BUSY_TIMEOUT).map_err(storage)?;
    conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE_CAPACITY);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::executor::block_on;

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct Pragmas {
        journal_mode: String,
        synchronous: i64,
        foreign_keys: i64,
        busy_timeout: i64,
    }

    fn pragmas(db: &Database) -> Pragmas {
        block_on(db.call(|conn| {
            let get = |name: &str| conn.pragma_query_value(None, name, |row| row.get::<_, i64>(0));
            Ok(Pragmas {
                journal_mode: conn
                    .pragma_query_value(None, "journal_mode", |row| row.get(0))
                    .map_err(storage)?,
                synchronous: get("synchronous").map_err(storage)?,
                foreign_keys: get("foreign_keys").map_err(storage)?,
                busy_timeout: get("busy_timeout").map_err(storage)?,
            })
        }))
        .unwrap()
    }

    #[test]
    fn file_database_has_required_pragmas() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("todo.sqlite3")).unwrap();
        assert_eq!(
            pragmas(&db),
            Pragmas {
                journal_mode: "wal".into(),
                synchronous: 1, // NORMAL
                foreign_keys: 1,
                busy_timeout: 5_000,
            }
        );
    }

    #[test]
    fn open_migrates_and_reopen_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todo.sqlite3");
        let db = Database::open(&path).unwrap();
        block_on(db.call(|conn| {
            conn.execute("INSERT INTO meta VALUES ('k', 'v')", [])
                .map_err(storage)
        }))
        .unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        let (version, value) = block_on(db.call(|conn| {
            let value: String = conn
                .query_row("SELECT value FROM meta WHERE key = 'k'", [], |r| r.get(0))
                .map_err(storage)?;
            Ok((migrations::version(conn)?, value))
        }))
        .unwrap();
        assert_eq!(version, migrations::MIGRATIONS.len());
        assert_eq!(value, "v");
    }

    #[test]
    fn in_memory_database_enforces_foreign_keys() {
        let db = Database::open_in_memory().unwrap();
        let p = pragmas(&db);
        assert_eq!(p.journal_mode, "memory");
        assert_eq!(p.foreign_keys, 1);
        assert_eq!(p.synchronous, 1);
    }

    #[test]
    fn calls_run_in_order_and_return_results_and_errors() {
        let db = Database::open_in_memory().unwrap();
        block_on(db.call(|conn| {
            conn.execute_batch("CREATE TABLE t (x INTEGER)")
                .map_err(storage)
        }))
        .unwrap();
        for i in 0..3 {
            block_on(db.call(move |conn| {
                conn.execute("INSERT INTO t VALUES (?1)", [i])
                    .map_err(storage)
            }))
            .unwrap();
        }
        let sum: i64 = block_on(db.call(|conn| {
            conn.query_row("SELECT sum(x) FROM t", [], |r| r.get(0))
                .map_err(storage)
        }))
        .unwrap();
        assert_eq!(sum, 3);
        let err = block_on(db.call(|conn| conn.execute_batch("NOT SQL").map_err(storage)));
        assert!(matches!(err, Err(CoreError::Storage(_))), "{err:?}");
    }

    #[test]
    fn usable_from_many_threads() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let db = Arc::clone(&db);
                thread::spawn(move || block_on(db.call(move |_| Ok(i * 2))).unwrap())
            })
            .collect();
        let results: Vec<i32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results, [0, 2, 4, 6]);
    }

    #[test]
    fn panicking_job_reports_storage_error_afterwards() {
        let db = Database::open_in_memory().unwrap();
        let first = block_on(db.call(|_| -> CoreResult<()> { panic!("boom") }));
        assert!(matches!(first, Err(CoreError::Storage(_))), "{first:?}");
        let next = block_on(db.call(|_| Ok(())));
        assert!(matches!(next, Err(CoreError::Storage(_))), "{next:?}");
    }

    #[test]
    fn open_reports_unusable_path() {
        let dir = tempfile::tempdir().unwrap();
        let err = Database::open(&dir.path().join("missing/dir/db.sqlite3")).unwrap_err();
        assert!(matches!(err, CoreError::Storage(_)), "{err:?}");
    }

    #[test]
    fn database_handle_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Database>();
    }
}
