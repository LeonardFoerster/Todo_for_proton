//! Versioniertes Schema. Die Version steht in `PRAGMA user_version` (0 = leere Datenbank);
//! Migration `n` (1-basiert) hebt von `n - 1` auf `n`.
//!
//! Regeln: Veröffentlichte Migrationen werden nie geändert, nur neue angehängt. Jede Migration
//! läuft zusammen mit dem Versionssprung in einer Transaktion.

use rusqlite::{Connection, TransactionBehavior};
use todo_core::{CoreError, CoreResult};

use crate::error::storage;

/// Alle Migrationen in Reihenfolge.
pub(crate) const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_sync.sql"),
    include_str!("../migrations/0003_search.sql"),
    include_str!("../migrations/0004_notes.sql"),
    include_str!("../migrations/0005_task_trash.sql"),
    include_str!("../migrations/0006_recurrence.sql"),
    include_str!("../migrations/0007_checklist.sql"),
    include_str!("../migrations/0008_tags.sql"),
    include_str!("../migrations/0009_links.sql"),
];

/// Bringt das Schema auf den neuesten Stand. Mehrfacher Aufruf ist harmlos.
///
/// # Errors
/// `CoreError::Storage`, wenn eine Migration scheitert (die Datenbank bleibt dann auf der
/// letzten erfolgreichen Version) oder die Datenbank von einer neueren App-Version stammt.
pub(crate) fn migrate(conn: &mut Connection) -> CoreResult<()> {
    run(conn, MIGRATIONS)
}

pub(crate) fn run(conn: &mut Connection, migrations: &[&str]) -> CoreResult<()> {
    let current = version(conn)?;
    let latest = migrations.len();
    if current > latest {
        return Err(CoreError::Storage(format!(
            "database schema version {current} is newer than supported version {latest}"
        )));
    }
    for (index, sql) in migrations.iter().enumerate().skip(current) {
        let next = index + 1;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        tx.execute_batch(sql)
            .map_err(|e| CoreError::Storage(format!("migration {next} failed: {e}")))?;
        tx.pragma_update(None, "user_version", schema_version(next))
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
    }
    Ok(())
}

fn schema_version(n: usize) -> i64 {
    i64::try_from(n).expect("migration count fits into i64")
}

/// Aktuelle Schema-Version.
pub(crate) fn version(conn: &Connection) -> CoreResult<usize> {
    let v: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(storage)?;
    usize::try_from(v).map_err(|_| CoreError::Storage(format!("invalid schema version {v}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn fresh_database_reaches_latest_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(version(&conn).unwrap(), 0);
        migrate(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), MIGRATIONS.len());
        assert_eq!(
            tables(&conn),
            [
                "field_clocks",
                "links",
                "meta",
                "note_folders",
                "note_revision_parents",
                "note_revisions",
                "notes",
                "notes_fts",
                "notes_fts_config",
                "notes_fts_data",
                "notes_fts_docsize",
                "notes_fts_idx",
                "outbox_ops",
                "remote_cursors",
                "sync_meta",
                "tags",
                "task_lists",
                "tasks",
                "tasks_fts",
                "tasks_fts_config",
                "tasks_fts_data",
                "tasks_fts_docsize",
                "tasks_fts_idx",
                "tombstones"
            ]
        );
    }

    #[test]
    fn migrating_twice_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        conn.execute("INSERT INTO meta VALUES ('k', 'v')", [])
            .unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), MIGRATIONS.len());
        let value: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, "v", "Daten bleiben erhalten");
    }

    #[test]
    fn applies_only_pending_migrations() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn, &["CREATE TABLE a (x INTEGER);"]).unwrap();
        run(
            &mut conn,
            &["CREATE TABLE a (x INTEGER);", "CREATE TABLE b (y INTEGER);"],
        )
        .unwrap();
        assert_eq!(version(&conn).unwrap(), 2);
        assert_eq!(tables(&conn), ["a", "b"]);
    }

    #[test]
    fn failed_migration_is_rolled_back_and_keeps_previous_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        let migrations = [
            "CREATE TABLE a (x INTEGER);",
            "CREATE TABLE b (y INTEGER); THIS IS NOT SQL;",
        ];
        let err = run(&mut conn, &migrations).unwrap_err();
        assert!(
            matches!(&err, CoreError::Storage(m) if m.contains("migration 2")),
            "{err:?}"
        );
        assert_eq!(version(&conn).unwrap(), 1);
        assert_eq!(tables(&conn), ["a"], "Tabelle b zurückgerollt");
    }

    #[test]
    fn rejects_database_from_newer_app() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", schema_version(MIGRATIONS.len() + 1))
            .unwrap();
        let err = migrate(&mut conn).unwrap_err();
        assert!(matches!(err, CoreError::Storage(_)), "{err:?}");
    }

    #[test]
    fn schema_enforces_task_invariants() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO task_lists VALUES ('l', 'Inbox', 'a0', 0, 0, 1)",
            [],
        )
        .unwrap();
        let insert = |status: i64,
                      completed_at: Option<i64>,
                      due_at: Option<i64>,
                      tz: Option<&str>| {
            conn.execute(
                "INSERT INTO tasks (id, list_id, title, notes, status, priority, due_at, due_tz,
                                    tags, sort_key, created_at, updated_at, completed_at)
                 VALUES (lower(hex(randomblob(8))), 'l', 't', '', ?1, 0, ?2, ?3, '', 'a0', 0, 0, ?4)",
                rusqlite::params![status, due_at, tz, completed_at],
            )
        };
        assert!(insert(0, None, None, None).is_ok());
        assert!(insert(1, Some(5), Some(1), Some("UTC")).is_ok());
        assert!(
            insert(1, None, None, None).is_err(),
            "erledigt ohne completed_at"
        );
        assert!(
            insert(0, Some(5), None, None).is_err(),
            "offen mit completed_at"
        );
        assert!(
            insert(0, None, Some(1), None).is_err(),
            "due_at ohne Zeitzone"
        );
    }

    #[test]
    fn search_index_covers_tasks_from_before_the_migration() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn, &MIGRATIONS[..2]).unwrap();
        conn.execute_batch(
            "INSERT INTO task_lists VALUES ('l', 'Inbox', 'a0', 0, 0, 1);
             INSERT INTO tasks (id, list_id, title, notes, status, priority, tags, sort_key,
                                created_at, updated_at)
             VALUES ('a', 'l', 'Übungsblatt', 'alt', 0, 0, '', 'a0', 0, 0);",
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM tasks_fts WHERE tasks_fts MATCH '\"übung\"'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            hits, 1,
            "bestehende Tasks indiziert, Umlaut ohne Groß/Klein"
        );
    }

    #[test]
    fn notes_migration_keeps_registers_tombstones_and_outbox() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn, &MIGRATIONS[..3]).unwrap();
        conn.execute_batch(
            "INSERT INTO field_clocks VALUES ('t', 3, 0, x'00', x'01');
             INSERT INTO tombstones VALUES ('d', 1, x'02');
             INSERT INTO outbox_ops VALUES (1, x'03');",
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(
            count("SELECT count(*) FROM field_clocks WHERE entity_id = 't'"),
            1
        );
        assert_eq!(
            count("SELECT count(*) FROM tombstones WHERE entity_kind = 1"),
            1
        );
        assert_eq!(
            count("SELECT count(*) FROM outbox_ops WHERE entity_id IS NULL"),
            1,
            "alte Ops sind nie ersetzbar"
        );
        conn.execute("INSERT INTO tombstones VALUES ('n', 2, x'04')", [])
            .unwrap();
        // Seit 0008 auch Tags (4); unbekannte Arten bleiben verboten.
        conn.execute("INSERT INTO tombstones VALUES ('t', 4, x'04')", [])
            .unwrap();
        conn.execute("INSERT INTO tombstones VALUES ('l', 5, x'04')", [])
            .unwrap();
        assert!(conn
            .execute("INSERT INTO tombstones VALUES ('x', 6, x'04')", [])
            .is_err());
    }
}
