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
pub(crate) enum Position {
    BySortKey(SortKey, TaskId),
    /// Erledigt-Ansicht: `completed_at` absteigend, dann `id` aufsteigend.
    ByCompletion(Timestamp, TaskId),
}

impl Position {
    /// Position der letzten gelieferten Zeile – Grundlage für `next_cursor`.
    pub(crate) fn of(summary: &TaskSummary, view: &SmartView) -> Self {
        match view {
            SmartView::Completed => {
                Self::ByCompletion(summary.completed_at.unwrap_or(Timestamp::EPOCH), summary.id)
            }
            _ => Self::BySortKey(summary.sort_key.clone(), summary.id),
        }
    }

    pub(crate) fn encode(&self) -> String {
        match self {
            Self::BySortKey(key, id) => format!("s:{key}:{id}"),
            Self::ByCompletion(at, id) => format!("c:{}:{id}", at.as_millis()),
        }
    }

    /// # Errors
    /// `CoreError::Validation` bei unbekanntem Format oder einem Cursor einer anderen
    /// Sortierart (Erledigt vs. Rest).
    pub(crate) fn decode(cursor: &str, view: &SmartView) -> CoreResult<Self> {
        let invalid = || CoreError::Validation(format!("invalid cursor: {cursor:?}"));
        let mut parts = cursor.splitn(3, ':');
        let (Some(kind), Some(value), Some(id)) = (parts.next(), parts.next(), parts.next()) else {
            return Err(invalid());
        };
        if (kind == "c") != matches!(view, SmartView::Completed) {
            return Err(invalid());
        }
        let id = TaskId::parse(id).map_err(|_| invalid())?;
        match kind {
            "s" => Ok(Self::BySortKey(
                SortKey::parse(value).map_err(|_| invalid())?,
                id,
            )),
            "c" => Ok(Self::ByCompletion(
                Timestamp::from_millis(value.parse().map_err(|_| invalid())?),
                id,
            )),
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
    }
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
        None => {}
    }

    let order = match query.view {
        SmartView::Completed => "completed_at DESC, id",
        _ => "sort_key, id",
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
        for bad in ["", "s:a0", "s:a00:x", "z:a0:x", "c:x:y"] {
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
