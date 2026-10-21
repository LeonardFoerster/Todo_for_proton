//! Abbildung Tabellenzeile ↔ Domain-Typ. Einzige Stelle, die die Spaltenkodierung kennt
//! (siehe `migrations/0001_init.sql`).

use chrono::Datelike;
use rusqlite::{named_params, Connection, Row};
use todo_core::{
    domain::{
        Due, NaiveDate, Note, NoteFolder, NoteFolderId, NoteFolderParts, NoteId, NoteKind,
        NoteParts, Priority, RevisionId, SortKey, TagId, Task, TaskId, TaskList, TaskListId,
        TaskListParts, TaskParts, TaskStatus, Timestamp,
    },
    ports::{NoteSummary, NoteTreeItem, TaskSummary},
    CoreResult,
};

use crate::error::{corrupt, storage};

pub(crate) const TASK_COLUMNS: &str = "id, list_id, parent_id, title, notes, status, priority, \
     due_date, due_at, due_tz, tags, sort_key, created_at, updated_at, completed_at, trashed_at";

pub(crate) const LIST_COLUMNS: &str = "id, name, sort_key, created_at, updated_at, is_inbox";

/// Spalten für [`TaskSummary`] – ohne Notiztext, nur ob es einen gibt.
pub(crate) const SUMMARY_COLUMNS: &str = "id, list_id, title, status, priority, due_date, due_at, \
     due_tz, notes <> '' AS has_notes, sort_key, completed_at, trashed_at";

pub(crate) const NOTE_COLUMNS: &str = "id, folder_id, name, auto_name, kind, body, body_rev, \
     sort_key, created_at, modified_at, trashed_at";

pub(crate) const FOLDER_COLUMNS: &str = "id, parent_id, name, sort_key, created_at, trashed_at";

/// Spalten für [`NoteSummary`] – ohne Inhalt.
pub(crate) const NOTE_SUMMARY_COLUMNS: &str =
    "id, folder_id, name, sort_key, created_at, trashed_at, kind, modified_at, size";

const UPSERT_NOTE: &str = "\
    INSERT INTO notes (id, folder_id, name, auto_name, kind, body, body_rev, size, sort_key,
                       created_at, modified_at, trashed_at)
    VALUES (:id, :folder_id, :name, :auto_name, :kind, :body, :body_rev, :size, :sort_key,
            :created_at, :modified_at, :trashed_at)
    ON CONFLICT (id) DO UPDATE SET
        folder_id = excluded.folder_id, name = excluded.name, auto_name = excluded.auto_name,
        kind = excluded.kind, body = excluded.body, body_rev = excluded.body_rev,
        size = excluded.size, sort_key = excluded.sort_key, created_at = excluded.created_at,
        modified_at = excluded.modified_at, trashed_at = excluded.trashed_at";

const UPSERT_FOLDER: &str = "\
    INSERT INTO note_folders (id, parent_id, name, sort_key, created_at, trashed_at)
    VALUES (:id, :parent_id, :name, :sort_key, :created_at, :trashed_at)
    ON CONFLICT (id) DO UPDATE SET
        parent_id = excluded.parent_id, name = excluded.name, sort_key = excluded.sort_key,
        created_at = excluded.created_at, trashed_at = excluded.trashed_at";

const UPSERT_TASK: &str = "\
    INSERT INTO tasks (id, list_id, parent_id, title, notes, status, priority, due_date, due_at,
                       due_tz, tags, sort_key, created_at, updated_at, completed_at, trashed_at)
    VALUES (:id, :list_id, :parent_id, :title, :notes, :status, :priority, :due_date, :due_at,
            :due_tz, :tags, :sort_key, :created_at, :updated_at, :completed_at, :trashed_at)
    ON CONFLICT (id) DO UPDATE SET
        list_id = excluded.list_id, parent_id = excluded.parent_id, title = excluded.title,
        notes = excluded.notes, status = excluded.status, priority = excluded.priority,
        due_date = excluded.due_date, due_at = excluded.due_at, due_tz = excluded.due_tz,
        tags = excluded.tags, sort_key = excluded.sort_key, created_at = excluded.created_at,
        updated_at = excluded.updated_at, completed_at = excluded.completed_at,
        trashed_at = excluded.trashed_at";

const UPSERT_LIST: &str = "\
    INSERT INTO task_lists (id, name, sort_key, created_at, updated_at, is_inbox)
    VALUES (:id, :name, :sort_key, :created_at, :updated_at, :is_inbox)
    ON CONFLICT (id) DO UPDATE SET
        name = excluded.name, sort_key = excluded.sort_key, created_at = excluded.created_at,
        updated_at = excluded.updated_at, is_inbox = excluded.is_inbox";

// --- Schreiben ---------------------------------------------------------------------------------

/// Legt die Task an oder ersetzt sie vollständig.
pub(crate) fn write_task(conn: &Connection, task: &Task) -> CoreResult<()> {
    let (due_date, due_at, due_tz) = encode_due(task.due());
    let mut stmt = conn.prepare_cached(UPSERT_TASK).map_err(storage)?;
    stmt.execute(named_params! {
        ":id": task.id().to_string(),
        ":list_id": task.list_id().to_string(),
        ":parent_id": task.parent_id().map(|id| id.to_string()),
        ":title": task.title(),
        ":notes": task.notes(),
        ":status": encode_status(task.status()),
        ":priority": encode_priority(task.priority()),
        ":due_date": due_date,
        ":due_at": due_at,
        ":due_tz": due_tz,
        ":tags": encode_tags(task.tags()),
        ":sort_key": task.sort_key().as_str(),
        ":created_at": task.created_at().as_millis(),
        ":updated_at": task.updated_at().as_millis(),
        ":completed_at": task.completed_at().map(Timestamp::as_millis),
        ":trashed_at": task.trashed_at().map(Timestamp::as_millis),
    })
    .map_err(storage)?;
    Ok(())
}

/// Legt die Liste an oder ersetzt sie vollständig.
pub(crate) fn write_list(conn: &Connection, list: &TaskList) -> CoreResult<()> {
    let mut stmt = conn.prepare_cached(UPSERT_LIST).map_err(storage)?;
    stmt.execute(named_params! {
        ":id": list.id().to_string(),
        ":name": list.name(),
        ":sort_key": list.sort_key().as_str(),
        ":created_at": list.created_at().as_millis(),
        ":updated_at": list.updated_at().as_millis(),
        ":is_inbox": list.is_inbox(),
    })
    .map_err(storage)?;
    Ok(())
}

/// Legt die Notiz an oder ersetzt sie vollständig.
pub(crate) fn write_note(conn: &Connection, note: &Note) -> CoreResult<()> {
    let mut stmt = conn.prepare_cached(UPSERT_NOTE).map_err(storage)?;
    stmt.execute(named_params! {
        ":id": note.id().to_string(),
        ":folder_id": note.folder().map(|id| id.to_string()),
        ":name": note.name().as_str(),
        ":auto_name": note.auto_name(),
        ":kind": encode_kind(note.kind()),
        ":body": note.body(),
        ":body_rev": note.body_rev().map(|rev| rev.to_string()),
        ":size": i64::try_from(note.body().len()).unwrap_or(i64::MAX),
        ":sort_key": note.sort_key().as_str(),
        ":created_at": note.created_at().as_millis(),
        ":modified_at": note.modified_at().as_millis(),
        ":trashed_at": note.trashed_at().map(Timestamp::as_millis),
    })
    .map_err(storage)?;
    Ok(())
}

/// Legt den Ordner an oder ersetzt ihn vollständig.
pub(crate) fn write_folder(conn: &Connection, folder: &NoteFolder) -> CoreResult<()> {
    let mut stmt = conn.prepare_cached(UPSERT_FOLDER).map_err(storage)?;
    stmt.execute(named_params! {
        ":id": folder.id().to_string(),
        ":parent_id": folder.parent().map(|id| id.to_string()),
        ":name": folder.name().as_str(),
        ":sort_key": folder.sort_key().as_str(),
        ":created_at": folder.created_at().as_millis(),
        ":trashed_at": folder.trashed_at().map(Timestamp::as_millis),
    })
    .map_err(storage)?;
    Ok(())
}

// --- Lesen -------------------------------------------------------------------------------------

/// Liest eine Zeile mit [`NOTE_COLUMNS`].
pub(crate) fn read_note(row: &Row<'_>) -> CoreResult<Note> {
    let parts = NoteParts {
        id: note_id(&get::<String>(row, 0)?)?,
        folder: get::<Option<String>>(row, 1)?
            .as_deref()
            .map(folder_id)
            .transpose()?,
        name: get(row, 2)?,
        auto_name: get(row, 3)?,
        kind: decode_kind(get(row, 4)?)?,
        body: get(row, 5)?,
        body_rev: get::<Option<String>>(row, 6)?
            .as_deref()
            .map(|s| RevisionId::parse(s).map_err(|e| corrupt("revision id", e)))
            .transpose()?,
        sort_key: sort_key(&get::<String>(row, 7)?)?,
        created_at: Timestamp::from_millis(get(row, 8)?),
        modified_at: Timestamp::from_millis(get(row, 9)?),
        trashed_at: get::<Option<i64>>(row, 10)?.map(Timestamp::from_millis),
    };
    Note::from_parts(parts).map_err(|e| corrupt("note", e))
}

/// Liest eine Zeile mit [`FOLDER_COLUMNS`].
pub(crate) fn read_folder(row: &Row<'_>) -> CoreResult<NoteFolder> {
    let parts = NoteFolderParts {
        id: folder_id(&get::<String>(row, 0)?)?,
        parent: get::<Option<String>>(row, 1)?
            .as_deref()
            .map(folder_id)
            .transpose()?,
        name: get(row, 2)?,
        sort_key: sort_key(&get::<String>(row, 3)?)?,
        created_at: Timestamp::from_millis(get(row, 4)?),
        trashed_at: get::<Option<i64>>(row, 5)?.map(Timestamp::from_millis),
    };
    NoteFolder::from_parts(parts).map_err(|e| corrupt("note folder", e))
}

/// Liest eine Zeile mit [`NOTE_SUMMARY_COLUMNS`] ab Spalte `at`.
pub(crate) fn read_note_summary(row: &Row<'_>, at: usize) -> CoreResult<NoteSummary> {
    Ok(NoteSummary {
        id: note_id(&get::<String>(row, at)?)?,
        folder: get::<Option<String>>(row, at + 1)?
            .as_deref()
            .map(folder_id)
            .transpose()?,
        name: get(row, at + 2)?,
        sort_key: sort_key(&get::<String>(row, at + 3)?)?,
        created_at: Timestamp::from_millis(get(row, at + 4)?),
        trashed_at: get::<Option<i64>>(row, at + 5)?.map(Timestamp::from_millis),
        kind: decode_kind(get(row, at + 6)?)?,
        modified_at: Timestamp::from_millis(get(row, at + 7)?),
        size: u64::try_from(get::<i64>(row, at + 8)?).map_err(|e| corrupt("note size", e))?,
    })
}

/// Eintrag der Baumseite ab Spalte 1: Ordner ([`FOLDER_COLUMNS`]) bzw. Notiz
/// ([`NOTE_SUMMARY_COLUMNS`] – die ersten sechs Spalten liegen gleich).
pub(crate) fn read_tree_item(row: &Row<'_>, is_note: bool) -> CoreResult<NoteTreeItem> {
    if is_note {
        return read_note_summary(row, 1).map(NoteTreeItem::Note);
    }
    let parts = NoteFolderParts {
        id: folder_id(&get::<String>(row, 1)?)?,
        parent: get::<Option<String>>(row, 2)?
            .as_deref()
            .map(folder_id)
            .transpose()?,
        name: get(row, 3)?,
        sort_key: sort_key(&get::<String>(row, 4)?)?,
        created_at: Timestamp::from_millis(get(row, 5)?),
        trashed_at: get::<Option<i64>>(row, 6)?.map(Timestamp::from_millis),
    };
    NoteFolder::from_parts(parts)
        .map(NoteTreeItem::Folder)
        .map_err(|e| corrupt("note folder", e))
}

pub(crate) fn note_id(s: &str) -> CoreResult<NoteId> {
    NoteId::parse(s).map_err(|e| corrupt("note id", e))
}

pub(crate) fn folder_id(s: &str) -> CoreResult<NoteFolderId> {
    NoteFolderId::parse(s).map_err(|e| corrupt("note folder id", e))
}

pub(crate) fn encode_kind(kind: NoteKind) -> i64 {
    match kind {
        NoteKind::Text => 0,
        NoteKind::Markdown => 1,
    }
}

fn decode_kind(v: i64) -> CoreResult<NoteKind> {
    match v {
        0 => Ok(NoteKind::Text),
        1 => Ok(NoteKind::Markdown),
        _ => Err(corrupt("note kind", v)),
    }
}

/// Liest eine Zeile mit [`TASK_COLUMNS`].
pub(crate) fn read_task(row: &Row<'_>) -> CoreResult<Task> {
    let parts = TaskParts {
        id: task_id(&get::<String>(row, 0)?)?,
        list_id: list_id(&get::<String>(row, 1)?)?,
        parent_id: get::<Option<String>>(row, 2)?
            .as_deref()
            .map(task_id)
            .transpose()?,
        title: get(row, 3)?,
        notes: get(row, 4)?,
        status: decode_status(get(row, 5)?)?,
        priority: decode_priority(get(row, 6)?)?,
        due: decode_due(get(row, 7)?, get(row, 8)?, get(row, 9)?)?,
        tags: decode_tags(&get::<String>(row, 10)?)?,
        sort_key: sort_key(&get::<String>(row, 11)?)?,
        created_at: Timestamp::from_millis(get(row, 12)?),
        updated_at: Timestamp::from_millis(get(row, 13)?),
        completed_at: get::<Option<i64>>(row, 14)?.map(Timestamp::from_millis),
        trashed_at: get::<Option<i64>>(row, 15)?.map(Timestamp::from_millis),
    };
    Task::from_parts(parts).map_err(|e| corrupt("task", e))
}

/// Liest eine Zeile mit [`LIST_COLUMNS`].
pub(crate) fn read_list(row: &Row<'_>) -> CoreResult<TaskList> {
    let parts = TaskListParts {
        id: list_id(&get::<String>(row, 0)?)?,
        name: get(row, 1)?,
        sort_key: sort_key(&get::<String>(row, 2)?)?,
        created_at: Timestamp::from_millis(get(row, 3)?),
        updated_at: Timestamp::from_millis(get(row, 4)?),
        is_inbox: get(row, 5)?,
    };
    TaskList::from_parts(parts).map_err(|e| corrupt("task list", e))
}

/// Liest eine Zeile mit [`SUMMARY_COLUMNS`].
pub(crate) fn read_summary(row: &Row<'_>) -> CoreResult<TaskSummary> {
    Ok(TaskSummary {
        id: task_id(&get::<String>(row, 0)?)?,
        list_id: list_id(&get::<String>(row, 1)?)?,
        title: get(row, 2)?,
        status: decode_status(get(row, 3)?)?,
        priority: decode_priority(get(row, 4)?)?,
        due: decode_due(get(row, 5)?, get(row, 6)?, get(row, 7)?)?,
        has_notes: get(row, 8)?,
        sort_key: sort_key(&get::<String>(row, 9)?)?,
        completed_at: get::<Option<i64>>(row, 10)?.map(Timestamp::from_millis),
        trashed_at: get::<Option<i64>>(row, 11)?.map(Timestamp::from_millis),
    })
}

/// Liest einen Sort-Key aus Spalte `index`.
pub(crate) fn read_sort_key(row: &Row<'_>, index: usize) -> CoreResult<SortKey> {
    sort_key(&get::<String>(row, index)?)
}

fn get<T: rusqlite::types::FromSql>(row: &Row<'_>, index: usize) -> CoreResult<T> {
    row.get(index).map_err(storage)
}

fn task_id(s: &str) -> CoreResult<TaskId> {
    TaskId::parse(s).map_err(|e| corrupt("task id", e))
}

fn list_id(s: &str) -> CoreResult<TaskListId> {
    TaskListId::parse(s).map_err(|e| corrupt("list id", e))
}

fn sort_key(s: &str) -> CoreResult<SortKey> {
    SortKey::parse(s).map_err(|e| corrupt("sort key", e))
}

// --- Spaltenkodierung --------------------------------------------------------------------------

pub(crate) fn encode_status(status: TaskStatus) -> i64 {
    match status {
        TaskStatus::Open => 0,
        TaskStatus::Done => 1,
    }
}

fn decode_status(v: i64) -> CoreResult<TaskStatus> {
    match v {
        0 => Ok(TaskStatus::Open),
        1 => Ok(TaskStatus::Done),
        _ => Err(corrupt("status", v)),
    }
}

fn encode_priority(priority: Priority) -> i64 {
    match priority {
        Priority::None => 0,
        Priority::Low => 1,
        Priority::Medium => 2,
        Priority::High => 3,
    }
}

fn decode_priority(v: i64) -> CoreResult<Priority> {
    match v {
        0 => Ok(Priority::None),
        1 => Ok(Priority::Low),
        2 => Ok(Priority::Medium),
        3 => Ok(Priority::High),
        _ => Err(corrupt("priority", v)),
    }
}

/// Datum als Tageszahl (1 = 0001-01-01), damit SQLite für jedes `NaiveDate` korrekt vergleicht.
pub(crate) fn encode_date(date: NaiveDate) -> i64 {
    i64::from(date.num_days_from_ce())
}

fn decode_date(days: i64) -> CoreResult<NaiveDate> {
    i32::try_from(days)
        .ok()
        .and_then(NaiveDate::from_num_days_from_ce_opt)
        .ok_or_else(|| corrupt("due date", days))
}

fn encode_due(due: Option<&Due>) -> (Option<i64>, Option<i64>, Option<&str>) {
    match due {
        None => (None, None, None),
        Some(Due::Date(date)) => (Some(encode_date(*date)), None, None),
        Some(Due::DateTime { at, tz }) => (None, Some(at.as_millis()), Some(tz)),
    }
}

fn decode_due(date: Option<i64>, at: Option<i64>, tz: Option<String>) -> CoreResult<Option<Due>> {
    match (date, at, tz) {
        (None, None, None) => Ok(None),
        (Some(days), None, None) => Ok(Some(Due::Date(decode_date(days)?))),
        (None, Some(at), Some(tz)) => Ok(Some(Due::DateTime {
            at: Timestamp::from_millis(at),
            tz,
        })),
        _ => Err(corrupt("due", "inconsistent due columns")),
    }
}

fn encode_tags(tags: &[TagId]) -> String {
    tags.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_tags(s: &str) -> CoreResult<Vec<TagId>> {
    s.split_ascii_whitespace()
        .map(|t| TagId::parse(t).map_err(|e| corrupt("tag id", e)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_encoding_preserves_order_and_roundtrips() {
        let dates = [
            NaiveDate::MIN,
            NaiveDate::from_ymd_opt(-1, 12, 31).unwrap(),
            NaiveDate::from_ymd_opt(1, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 17).unwrap(),
            NaiveDate::from_ymd_opt(12_000, 1, 1).unwrap(),
            NaiveDate::MAX,
        ];
        for pair in dates.windows(2) {
            assert!(encode_date(pair[0]) < encode_date(pair[1]), "{pair:?}");
        }
        for date in dates {
            assert_eq!(decode_date(encode_date(date)).unwrap(), date);
        }
        assert_eq!(encode_date(NaiveDate::from_ymd_opt(1, 1, 1).unwrap()), 1);
    }

    #[test]
    fn enums_roundtrip_and_reject_unknown_codes() {
        for status in [TaskStatus::Open, TaskStatus::Done] {
            assert_eq!(decode_status(encode_status(status)).unwrap(), status);
        }
        for p in [
            Priority::None,
            Priority::Low,
            Priority::Medium,
            Priority::High,
        ] {
            assert_eq!(decode_priority(encode_priority(p)).unwrap(), p);
        }
        assert!(decode_status(2).is_err());
        assert!(decode_priority(-1).is_err());
        assert!(decode_date(i64::MAX).is_err());
    }

    #[test]
    fn tags_roundtrip() {
        let tags = vec![TagId::new(), TagId::new()];
        assert_eq!(decode_tags(&encode_tags(&tags)).unwrap(), tags);
        assert!(decode_tags("").unwrap().is_empty());
        assert!(decode_tags("kaputt").is_err());
    }

    #[test]
    fn due_columns_must_be_consistent() {
        assert!(decode_due(Some(1), Some(1), None).is_err());
        assert!(decode_due(None, Some(1), None).is_err());
        assert!(decode_due(None, None, Some("UTC".into())).is_err());
    }
}
