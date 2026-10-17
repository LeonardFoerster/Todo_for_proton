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
            "a task refers to a list that does not exist (or a list with tasks was deleted)"
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
