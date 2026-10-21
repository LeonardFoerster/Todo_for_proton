//! Übersetzt [`TaskQuery`] + Seitenposition in SQL (Keyset-Paginierung, kein `OFFSET`).
//!
//! Die Semantik ist durch [`TaskQuery::matches`] und die Contract-Suite festgelegt; hier steht
//! nur die SQL-Form davon.

use rusqlite::types::Value;
use todo_core::{
    domain::{SortKey, TaskId, TaskStatus, Timestamp},
    ports::{SmartView, TaskQuery, TaskSummary},
    CoreError, CoreResult,
};

use crate::mapping::{encode_date, encode_status, SUMMARY_COLUMNS};

/// Sortierposition einer Task innerhalb einer Ansicht; zugleich Inhalt des (opaken) Cursors.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // „sortiert nach …“; `SortKey` hieße wie der Typ
pub(crate) enum Position {
    BySortKey(SortKey, TaskId),
    /// Erledigt-Ansicht: `completed_at` absteigend, dann `id` aufsteigend.
    ByCompletion(Timestamp, TaskId),
    /// Heute/Geplant: Sortierzeitpunkt der Fälligkeit (ADR-026), dann `sort_key`, `id`.
    ByDue(Timestamp, SortKey, TaskId),
    /// Papierkorb (ADR-031): `trashed_at` absteigend, dann `id` aufsteigend.
    ByTrashed(Timestamp, TaskId),
}

/// Cursor-Kennung der Sortierart einer Ansicht (ein Cursor gehört zu genau einer).
fn cursor_kind(view: &SmartView) -> &'static str {
    match view {
        SmartView::Completed => "c",
        SmartView::Trash => "t",
        SmartView::Today(_) | SmartView::Upcoming(_) => "d",
        _ => "s",
    }
}

/// Sortierzeitpunkt in SQL, wie [`Due::sort_instant`](todo_core::domain::Due::sort_instant):
/// `due_at` bzw. Tagesbeginn ≈ `start + (due_date - date) · 24 h`. Parameter: Datum, Beginn.
const DUE_INSTANT: &str = "COALESCE(due_at, (due_date - ?) * 86400000 + ?)";
/// Keyset-Bedingung „nach (Zeitpunkt, `sort_key`, `id`)" für [`Position::ByDue`].
const DUE_AFTER: &str =
    "(COALESCE(due_at, (due_date - ?) * 86400000 + ?), sort_key, id) > (?, ?, ?)";

impl Position {
    /// Position der letzten gelieferten Zeile – Grundlage für `next_cursor`.
    pub(crate) fn of(summary: &TaskSummary, view: &SmartView) -> Self {
        match view {
            SmartView::Completed => {
                Self::ByCompletion(summary.completed_at.unwrap_or(Timestamp::EPOCH), summary.id)
            }
            SmartView::Trash => {
                Self::ByTrashed(summary.trashed_at.unwrap_or(Timestamp::EPOCH), summary.id)
            }
            SmartView::Today(day) | SmartView::Upcoming(day) => Self::ByDue(
                summary
                    .due
                    .as_ref()
                    .map_or(Timestamp::from_millis(i64::MAX), |d| d.sort_instant(day)),
                summary.sort_key.clone(),
                summary.id,
            ),
            _ => Self::BySortKey(summary.sort_key.clone(), summary.id),
        }
    }

    pub(crate) fn encode(&self) -> String {
        match self {
            Self::BySortKey(key, id) => format!("s:{key}:{id}"),
            Self::ByCompletion(at, id) => format!("c:{}:{id}", at.as_millis()),
            Self::ByDue(at, key, id) => format!("d:{}:{key}:{id}", at.as_millis()),
            Self::ByTrashed(at, id) => format!("t:{}:{id}", at.as_millis()),
        }
    }

    /// # Errors
    /// `CoreError::Validation` bei unbekanntem Format oder einem Cursor einer anderen
    /// Sortierart (z. B. Erledigt vs. Rest).
    pub(crate) fn decode(cursor: &str, view: &SmartView) -> CoreResult<Self> {
        let invalid = || CoreError::Validation(format!("invalid cursor: {cursor:?}"));
        let parts: Vec<&str> = cursor.split(':').collect();
        if parts.first() != Some(&cursor_kind(view)) {
            return Err(invalid());
        }
        let millis = |v: &str| v.parse().map(Timestamp::from_millis).map_err(|_| invalid());
        let key = |v: &str| SortKey::parse(v).map_err(|_| invalid());
        let id = |v: &str| TaskId::parse(v).map_err(|_| invalid());
        match parts.as_slice() {
            ["s", k, i] => Ok(Self::BySortKey(key(k)?, id(i)?)),
            ["c", at, i] => Ok(Self::ByCompletion(millis(at)?, id(i)?)),
            ["d", at, k, i] => Ok(Self::ByDue(millis(at)?, key(k)?, id(i)?)),
            ["t", at, i] => Ok(Self::ByTrashed(millis(at)?, id(i)?)),
            _ => Err(invalid()),
        }
    }
}

/// SQL samt Parametern für eine Seite mit höchstens `fetch` Zeilen.
///
/// Gleiche Abfrageform ⇒ gleicher SQL-Text, damit `prepare_cached` greift.
pub(crate) fn list_tasks_sql(
    query: &TaskQuery,
    after: Option<&Position>,
    fetch: u32,
) -> (String, Vec<Value>) {
    let mut conditions: Vec<&str> = Vec::new();
    let mut params: Vec<Value> = Vec::new();

    match query.view {
        SmartView::Inbox => {
            conditions.push("list_id IN (SELECT id FROM task_lists WHERE is_inbox = 1)");
        }
        SmartView::List(id) => {
            conditions.push("list_id = ?");
            params.push(Value::Text(id.to_string()));
        }
        SmartView::Today(day) => {
            conditions.push("(due_date <= ? OR due_at < ?)");
            params.push(Value::Integer(encode_date(day.date())));
            params.push(Value::Integer(day.span().end.as_millis()));
        }
        SmartView::Upcoming(day) => {
            conditions.push("(due_date > ? OR due_at >= ?)");
            params.push(Value::Integer(encode_date(day.date())));
            params.push(Value::Integer(day.span().end.as_millis()));
        }
        SmartView::All => {}
        SmartView::Completed => {
            conditions.push("status = ?");
            params.push(Value::Integer(encode_status(TaskStatus::Done)));
        }
        SmartView::Trash => {}
    }
    // Papierkorb nur in seiner Ansicht (ADR-031, wie `TaskQuery::matches`).
    conditions.push(if query.view == SmartView::Trash {
        "trashed_at IS NOT NULL"
    } else {
        "trashed_at IS NULL"
    });
    if let Some(status) = query.status {
        conditions.push("status = ?");
        params.push(Value::Integer(encode_status(status)));
    }
    if let Some(tag) = query.tag {
        // IDs haben feste Länge und enthalten kein Leerzeichen → kein Teiltreffer möglich.
        conditions.push("instr(tags, ?) > 0");
        params.push(Value::Text(tag.to_string()));
    }
    match after {
        Some(Position::BySortKey(key, id)) => {
            conditions.push("(sort_key, id) > (?, ?)");
            params.push(Value::Text(key.as_str().to_owned()));
            params.push(Value::Text(id.to_string()));
        }
        Some(Position::ByCompletion(at, id)) => {
            conditions.push("(completed_at < ? OR (completed_at = ? AND id > ?))");
            params.push(Value::Integer(at.as_millis()));
            params.push(Value::Integer(at.as_millis()));
            params.push(Value::Text(id.to_string()));
        }
        Some(Position::ByTrashed(at, id)) => {
            conditions.push("(trashed_at < ? OR (trashed_at = ? AND id > ?))");
            params.push(Value::Integer(at.as_millis()));
            params.push(Value::Integer(at.as_millis()));
            params.push(Value::Text(id.to_string()));
        }
        Some(Position::ByDue(at, key, id)) => {
            let (SmartView::Today(day) | SmartView::Upcoming(day)) = query.view else {
                unreachable!("ByDue nur für Heute/Geplant (decode prüft die Art)")
            };
            conditions.push(DUE_AFTER);
            params.push(Value::Integer(encode_date(day.date())));
            params.push(Value::Integer(day.span().start.as_millis()));
            params.push(Value::Integer(at.as_millis()));
            params.push(Value::Text(key.as_str().to_owned()));
            params.push(Value::Text(id.to_string()));
        }
        None => {}
    }

    let order = match query.view {
        SmartView::Completed => "completed_at DESC, id".to_owned(),
        SmartView::Trash => "trashed_at DESC, id".to_owned(),
        SmartView::Today(day) | SmartView::Upcoming(day) => {
            // Parameter des ORDER BY stehen im SQL nach denen des WHERE, vor LIMIT.
            params.push(Value::Integer(encode_date(day.date())));
            params.push(Value::Integer(day.span().start.as_millis()));
            format!("{DUE_INSTANT}, sort_key, id")
        }
        _ => "sort_key, id".to_owned(),
    };
    let filter = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    params.push(Value::Integer(i64::from(fetch)));
    let sql = format!("SELECT {SUMMARY_COLUMNS} FROM tasks{filter} ORDER BY {order} LIMIT ?");
    (sql, params)
}

/// Ab so vielen Zeichen nutzt ein Suchwort den Trigram-Index; kürzere gehen über `LIKE`.
const TRIGRAM: usize = 3;

/// Ein Suchwort als FTS5-Zeichenkette (Anführungszeichen verdoppelt) – Operatoren wie `OR`, `*`
/// oder Klammern sind darin normaler Text.
fn fts_string(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// `%wort%` mit maskierten `LIKE`-Platzhaltern (Escape-Zeichen `\`).
fn like_pattern(term: &str) -> String {
    let escaped = term
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// SQL samt Parametern für die Suche (ADR-027): Wörter ab 3 Zeichen über den Trigram-Index
/// `tasks_fts` (Teilwort, Groß/Klein egal, auch Umlaute), kürzere über `LIKE` (Groß/Klein nur bei
/// ASCII egal). Alle Wörter müssen passen; Rang: offen vor erledigt, Titeltreffer zuerst.
pub(crate) fn search_tasks_sql(terms: &[String], limit: u32) -> (String, Vec<Value>) {
    let (long, short): (Vec<&String>, Vec<&String>) =
        terms.iter().partition(|t| t.chars().count() >= TRIGRAM);
    // Papierkorb wird nicht durchsucht (ADR-031).
    let mut conditions: Vec<String> = vec!["trashed_at IS NULL".into()];
    let mut params: Vec<Value> = Vec::new();
    if !long.is_empty() {
        conditions.push("rowid IN (SELECT rowid FROM tasks_fts WHERE tasks_fts MATCH ?)".into());
        let all: Vec<String> = long.iter().map(|t| fts_string(t)).collect();
        params.push(Value::Text(all.join(" ")));
    }
    for term in &short {
        conditions.push(
            r"(title LIKE ? ESCAPE '\' OR notes LIKE ? ESCAPE '\' OR checklist_text LIKE ? ESCAPE '\')"
                .into(),
        );
        for _ in 0..3 {
            params.push(Value::Text(like_pattern(term)));
        }
    }

    // Titeltreffer: irgendein Wort im Titel.
    let mut in_title: Vec<String> = Vec::new();
    if !long.is_empty() {
        in_title.push("rowid IN (SELECT rowid FROM tasks_fts WHERE tasks_fts MATCH ?)".into());
        let any: Vec<String> = long.iter().map(|t| fts_string(t)).collect();
        params.push(Value::Text(format!("title : ({})", any.join(" OR "))));
    }
    for term in &short {
        in_title.push(r"title LIKE ? ESCAPE '\'".into());
        params.push(Value::Text(like_pattern(term)));
    }
    params.push(Value::Integer(i64::from(limit)));
    let sql = format!(
        "SELECT {SUMMARY_COLUMNS} FROM tasks WHERE {} \
         ORDER BY status, NOT ({}), sort_key, id LIMIT ?",
        conditions.join(" AND "),
        in_title.join(" OR "),
    );
    (sql, params)
}

/// Notizen, die nicht im wirksamen Papierkorb liegen (selbst oder über einen Ordner).
const VISIBLE_NOTES: &str = "trashed_at IS NULL AND (folder_id IS NULL OR folder_id NOT IN (
        WITH RECURSIVE gone (id) AS (
            SELECT id FROM note_folders WHERE trashed_at IS NOT NULL
            UNION SELECT f.id FROM note_folders f JOIN gone ON f.parent_id = gone.id)
        SELECT id FROM gone))";

/// Suche über Name und Inhalt der Notizen (gleiche Regeln wie [`search_tasks_sql`]). Liefert
/// [`NOTE_COLUMNS`](crate::mapping::NOTE_COLUMNS); Ausschnitte berechnet der Core.
pub(crate) fn search_notes_sql(terms: &[String], limit: u32) -> (String, Vec<Value>) {
    let (long, short): (Vec<&String>, Vec<&String>) =
        terms.iter().partition(|t| t.chars().count() >= TRIGRAM);
    let mut conditions: Vec<String> = vec![VISIBLE_NOTES.into()];
    let mut params: Vec<Value> = Vec::new();
    if !long.is_empty() {
        conditions.push("rowid IN (SELECT rowid FROM notes_fts WHERE notes_fts MATCH ?)".into());
        let all: Vec<String> = long.iter().map(|t| fts_string(t)).collect();
        params.push(Value::Text(all.join(" ")));
    }
    for term in &short {
        if long.is_empty() {
            // Nur kurze Wörter: nur im Namen (NOTE_SEARCH_MIN_BODY_TERM).
            conditions.push(r"name LIKE ? ESCAPE '\'".into());
        } else {
            conditions.push(r"(name LIKE ? ESCAPE '\' OR body LIKE ? ESCAPE '\')".into());
            params.push(Value::Text(like_pattern(term)));
        }
        params.push(Value::Text(like_pattern(term)));
    }
    let mut in_name: Vec<String> = Vec::new();
    for term in terms {
        in_name.push(r"name LIKE ? ESCAPE '\'".into());
        params.push(Value::Text(like_pattern(term)));
    }
    params.push(Value::Integer(i64::from(limit)));
    // Erst nur IDs ranken (ohne die großen Inhalte durch den Sortierer zu schieben), dann die
    // höchstens `limit` Treffer vollständig lesen.
    let sql = format!(
        "SELECT {} FROM notes WHERE rowid IN (
             SELECT rowid FROM notes WHERE {} ORDER BY NOT ({}), modified_at DESC, id LIMIT ?)",
        crate::mapping::NOTE_COLUMNS,
        conditions.join(" AND "),
        in_name.join(" OR "),
    );
    (sql, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrip_and_kind_check() {
        let id = TaskId::new();
        let s = Position::BySortKey(SortKey::first(), id);
        let c = Position::ByCompletion(Timestamp::from_millis(-5), id);
        assert_eq!(Position::decode(&s.encode(), &SmartView::All).unwrap(), s);
        assert_eq!(
            Position::decode(&c.encode(), &SmartView::Completed).unwrap(),
            c
        );
        assert!(Position::decode(&s.encode(), &SmartView::Completed).is_err());
        assert!(Position::decode(&c.encode(), &SmartView::Inbox).is_err());
        let day = todo_core::domain::LocalDay::new(
            todo_core::domain::NaiveDate::from_ymd_opt(2026, 10, 17).unwrap(),
            Timestamp::from_millis(0),
            Timestamp::from_millis(1),
        )
        .unwrap();
        let d = Position::ByDue(Timestamp::from_millis(-7), SortKey::first(), id);
        assert_eq!(
            Position::decode(&d.encode(), &SmartView::Upcoming(day)).unwrap(),
            d
        );
        assert!(Position::decode(&d.encode(), &SmartView::All).is_err());
        assert!(Position::decode(&s.encode(), &SmartView::Today(day)).is_err());
        for bad in ["", "s:a0", "s:a00:x", "z:a0:x", "c:x:y", "d:1:a0"] {
            assert!(Position::decode(bad, &SmartView::All).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn same_query_shape_gives_same_sql() {
        let q = TaskQuery::open(SmartView::List(todo_core::domain::TaskListId::new()));
        let other = TaskQuery::open(SmartView::List(todo_core::domain::TaskListId::new()));
        assert_eq!(
            list_tasks_sql(&q, None, 51).0,
            list_tasks_sql(&other, None, 11).0
        );
    }
}
