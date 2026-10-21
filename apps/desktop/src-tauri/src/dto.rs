//! DTOs der Shell, die nicht aus `todo_core::api::dto` kommen: Fehler, Events, Sync-Status und
//! Login. Wie die Core-DTOs: owned, camelCase, Zeitpunkte als ISO-8601.

use serde::Serialize;
use todo_core::{
    api::dto::iso,
    ports::{AuthStep, SessionInfo},
    CoreError,
};
use todo_sync::{SyncPauseReason, SyncStatus};

/// Art eines Fehlers (für Verzweigungen in der UI); der Text ist nur für Logs/Details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    NotFound,
    Validation,
    Storage,
    Network,
    RateLimited,
    RemoteRejected,
    Conflict,
    AuthRequired,
    Auth,
    Internal,
    /// Funktion auf diesem System nicht verfügbar (z. B. kein Schlüsselbund).
    Unavailable,
}

/// Fehler eines Commands (`Promise`-Ablehnung in der UI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ErrorDto {
    pub kind: ErrorKind,
    pub message: String,
}

impl ErrorDto {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Unavailable,
            message: message.into(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Validation,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: message.into(),
        }
    }
}

impl From<CoreError> for ErrorDto {
    fn from(err: CoreError) -> Self {
        let kind = match &err {
            CoreError::NotFound(_) => ErrorKind::NotFound,
            CoreError::Validation(_) => ErrorKind::Validation,
            CoreError::Storage(_) => ErrorKind::Storage,
            CoreError::Network(_) => ErrorKind::Network,
            CoreError::RateLimited { .. } => ErrorKind::RateLimited,
            CoreError::RemoteRejected(_) => ErrorKind::RemoteRejected,
            CoreError::Conflict(_) => ErrorKind::Conflict,
            CoreError::AuthRequired => ErrorKind::AuthRequired,
            CoreError::Auth(_) => ErrorKind::Auth,
            CoreError::Internal(_) => ErrorKind::Internal,
        };
        Self {
            kind,
            message: err.to_string(),
        }
    }
}

/// Payload von `core://changed`: genau die geschriebenen Entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ChangedEventDto {
    pub tasks: Vec<String>,
    pub lists: Vec<String>,
    pub notes: Vec<String>,
    pub note_folders: Vec<String>,
    /// Tags (ADR-035).
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum SyncStateDto {
    /// Nicht angemeldet: Daten bleiben lokal.
    Disabled,
    Idle,
    Syncing,
    Offline,
    AuthRequired,
    RateLimited,
    /// Sync-Ordner stammt von einer neueren App-Version.
    NewerFormat,
    Error,
}

/// Payload von `core://sync-status` und Ergebnis von `sync_status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusDto {
    pub state: SyncStateDto,
    /// Letzter erfolgreicher Sync (ISO-8601).
    pub last_success: Option<String>,
    /// Lokale Änderungen, die noch nicht hochgeladen sind.
    pub pending_ops: u32,
    pub message: Option<String>,
}

impl SyncStatusDto {
    pub fn from_status(
        status: &SyncStatus,
        pending_ops: u64,
        last_success: Option<String>,
    ) -> Self {
        let (state, message) = match status {
            SyncStatus::Disabled => (SyncStateDto::Disabled, None),
            SyncStatus::Idle { .. } => (SyncStateDto::Idle, None),
            SyncStatus::Syncing => (SyncStateDto::Syncing, None),
            SyncStatus::Offline { .. } => (SyncStateDto::Offline, None),
            SyncStatus::Paused(SyncPauseReason::AuthRequired) => (SyncStateDto::AuthRequired, None),
            SyncStatus::Paused(SyncPauseReason::RateLimited) => (SyncStateDto::RateLimited, None),
            SyncStatus::Paused(SyncPauseReason::NewerFormat) => (SyncStateDto::NewerFormat, None),
            SyncStatus::Error { message } => (SyncStateDto::Error, Some(message.clone())),
        };
        let last_success = match status {
            SyncStatus::Idle {
                last_success: Some(at),
            } => Some(iso(*at)),
            _ => last_success,
        };
        Self {
            state,
            last_success,
            pending_ops: u32::try_from(pending_ops).unwrap_or(u32::MAX),
            message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionDto {
    /// Anzeigename, z. B. die E-Mail-Adresse.
    pub account_label: String,
}

impl From<SessionInfo> for SessionDto {
    fn from(info: SessionInfo) -> Self {
        Self {
            account_label: info.account_label,
        }
    }
}

/// Schritt des Login-Ablaufs (`interfaces.md` §3): Browser öffnen → warten → fertig.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AuthStepDto {
    /// Die Shell hat den Browser bereits geöffnet; `url` nur zum Kopieren als Ausweg.
    #[serde(rename_all = "camelCase")]
    OpenBrowser {
        flow: String,
        url: String,
        poll_interval_s: u32,
        expires_in_s: u32,
    },
    Pending {
        flow: String,
    },
    Done {
        session: SessionDto,
    },
}

impl From<AuthStep> for AuthStepDto {
    fn from(step: AuthStep) -> Self {
        match step {
            AuthStep::OpenBrowser {
                flow,
                url,
                poll_interval_s,
                expires_in_s,
            } => Self::OpenBrowser {
                flow: flow.0,
                url,
                poll_interval_s,
                expires_in_s,
            },
            AuthStep::Pending { flow } => Self::Pending { flow: flow.0 },
            AuthStep::Done(info) => Self::Done {
                session: info.into(),
            },
        }
    }
}

/// Anbieter-Texte für die Login-Ansicht; die UI kennt den Anbieter sonst nicht.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AuthInfoDto {
    /// Name des Sync-Anbieters (für „Mit … anmelden").
    pub provider: String,
    /// Pflichthinweis des Anbieters.
    pub notice: String,
    /// `false`, wenn Login hier nicht möglich ist (z. B. kein Schlüsselbund).
    pub available: bool,
}

#[cfg(test)]
mod tests {
    use todo_core::domain::Timestamp;

    use super::*;

    #[test]
    fn core_errors_keep_their_kind() {
        let dto = ErrorDto::from(CoreError::NotFound("task x".into()));
        assert_eq!(dto.kind, ErrorKind::NotFound);
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            serde_json::json!({"kind": "notFound", "message": "not found: task x"})
        );
        assert_eq!(
            ErrorDto::from(CoreError::RateLimited { retry_after_s: 3 }).kind,
            ErrorKind::RateLimited
        );
    }

    #[test]
    fn sync_status_is_flat() {
        let idle = SyncStatus::Idle {
            last_success: Some(Timestamp::from_millis(0)),
        };
        let dto = SyncStatusDto::from_status(&idle, 2, None);
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            serde_json::json!({
                "state": "idle",
                "lastSuccess": "1970-01-01T00:00:00.000Z",
                "pendingOps": 2,
                "message": null
            })
        );
        let paused = SyncStatus::Paused(SyncPauseReason::AuthRequired);
        let dto = SyncStatusDto::from_status(&paused, 0, Some("x".into()));
        assert_eq!(dto.state, SyncStateDto::AuthRequired);
        assert_eq!(dto.last_success.as_deref(), Some("x"));
    }

    #[test]
    fn auth_step_is_tagged() {
        let step = AuthStepDto::OpenBrowser {
            flow: "f".into(),
            url: "https://example.invalid".into(),
            poll_interval_s: 3,
            expires_in_s: 600,
        };
        assert_eq!(
            serde_json::to_value(&step).unwrap(),
            serde_json::json!({
                "kind": "openBrowser", "flow": "f", "url": "https://example.invalid",
                "pollIntervalS": 3, "expiresInS": 600
            })
        );
    }
}
