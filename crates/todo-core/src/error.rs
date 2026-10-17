//! Gemeinsamer Fehlertyp des Cores (`CoreError`). Siehe `development/interfaces.md` §0.

use crate::domain::ParseIdError;

/// Ergebnis-Typ aller öffentlichen Core-Funktionen.
pub type CoreResult<T> = Result<T, CoreError>;

/// Fehler, die den Core verlassen. Adapter bilden ihre eigenen Fehler hierauf ab.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("storage: {0}")]
    Storage(String),
    /// Vorübergehendes Netzwerkproblem – erneuter Versuch sinnvoll.
    #[error("network: {0}")]
    Network(String),
    /// Gegenseite drosselt (HTTP 429); erst nach `retry_after_s` Sekunden erneut versuchen.
    #[error("rate limited, retry after {retry_after_s} s")]
    RateLimited { retry_after_s: u32 },
    /// Gegenseite hat endgültig abgelehnt – erneuter Versuch zwecklos.
    #[error("remote rejected: {0}")]
    RemoteRejected(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl CoreError {
    /// Lohnt ein späterer erneuter Versuch (Netz weg, Zeitüberschreitung, Drosselung)?
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_) | Self::RateLimited { .. })
    }
}

impl From<ParseIdError> for CoreError {
    fn from(err: ParseIdError) -> Self {
        Self::Validation(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TaskId;

    #[test]
    fn parse_id_error_becomes_validation() {
        let err: CoreError = TaskId::parse("x").unwrap_err().into();
        assert!(matches!(err, CoreError::Validation(_)));
    }

    #[test]
    fn only_network_and_rate_limits_are_retryable() {
        assert!(CoreError::Network("x".into()).is_retryable());
        assert!(CoreError::RateLimited { retry_after_s: 3 }.is_retryable());
        assert!(!CoreError::RemoteRejected("x".into()).is_retryable());
        assert!(!CoreError::Conflict("x".into()).is_retryable());
    }

    #[test]
    fn display_includes_context() {
        assert_eq!(
            CoreError::NotFound("task 1".into()).to_string(),
            "not found: task 1"
        );
    }
}
