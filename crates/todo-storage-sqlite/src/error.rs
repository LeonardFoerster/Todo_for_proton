//! Abbildung von `rusqlite`-Fehlern auf [`CoreError`] (kein `rusqlite::Error` verlässt das Crate).

use rusqlite::ErrorCode;
use todo_core::CoreError;

/// Allgemeiner Speicherfehler. Verletzte Fremdschlüssel sind dagegen Validierungsfehler des
/// Aufrufers (z. B. Task in unbekannter Liste) – siehe [`TaskRepository::commit`].
///
/// [`TaskRepository::commit`]: todo_core::ports::TaskRepository::commit
#[allow(clippy::needless_pass_by_value)] // Signatur passend für `map_err(storage)`
pub(crate) fn storage(err: rusqlite::Error) -> CoreError {
    match err.sqlite_error_code() {
        Some(ErrorCode::ConstraintViolation) if is_foreign_key(&err) => CoreError::Validation(
            "a task or note refers to a list or folder that does not exist (or one with \
             content was deleted)"
                .to_owned(),
        ),
        _ => CoreError::Storage(err.to_string()),
    }
}

fn is_foreign_key(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _)
            if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    )
}

/// Gespeicherte Daten passen nicht zum Domain-Modell (manuell editiert, Bug in einer Migration …).
pub(crate) fn corrupt(what: &str, detail: impl std::fmt::Display) -> CoreError {
    CoreError::Storage(format!("corrupt {what}: {detail}"))
}
