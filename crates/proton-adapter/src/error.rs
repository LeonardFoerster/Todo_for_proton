//! Fehler der Proton-API und ihre Abbildung auf [`CoreError`].
//!
//! Einordnung: Netzfehler, Zeitüberschreitungen, 5xx und Drosselung sind retrybar
//! (`Network`/`RateLimited`), alles andere endgültig. Meldungstexte des Servers werden bewusst
//! nicht übernommen (lokalisiert, können Namen enthalten) – nur HTTP-Status und Proton-Code.

use std::fmt;

use todo_core::{ports::AuthError, CoreError};

/// Proton-Codes, die der Adapter unterscheidet (Quelle: P2-Proben, `proton-research.md` §5/§13).
pub(crate) mod code {
    /// Erfolg.
    pub(crate) const OK: i64 = 1000;
    /// Erfolg mit Einzelantworten (`Responses`).
    pub(crate) const MULTI: i64 = 1001;
    /// „You are using an outdated version of the app" (z. B. Upload ohne Block-Verifikation).
    pub(crate) const OUTDATED_APP: i64 = 2000;
    /// `x-pm-appversion` mit ungültigem Format.
    pub(crate) const INVALID_APP_VERSION_FORMAT: i64 = 2064;
    /// Name existiert bereits (gleicher Namens-Hash im Elternordner).
    pub(crate) const ALREADY_EXISTS: i64 = 2500;
    /// Link/Entwurf existiert nicht.
    pub(crate) const NOT_EXISTS: i64 = 2501;
    /// `x-pm-appversion` fehlt bzw. ist unbekannt/gesperrt.
    pub(crate) const APP_VERSION_MISSING: i64 = 5002;
    pub(crate) const APP_VERSION_BAD: i64 = 5003;
    /// Refresh-Token ungültig (z. B. nach Logout oder Rotation).
    pub(crate) const INVALID_REFRESH_TOKEN: i64 = 10013;
}

/// Ohne `Retry-After` so lange warten (Wert des offiziellen SDK).
pub(crate) const DEFAULT_RETRY_AFTER_S: u32 = 10;
/// Obergrenze für `Retry-After`, damit ein kaputter Header den Sync nicht tagelang anhält.
const MAX_RETRY_AFTER_S: u32 = 3600;

/// Fehler eines einzelnen API-Aufrufs (crate-intern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ApiError {
    /// Keine Antwort: Verbindung, DNS, TLS, Zeitüberschreitung. Text ohne URL.
    Transport(String),
    /// Antwort mit Fehlerstatus oder Proton-Code ≠ 1000/1001.
    Status {
        status: u16,
        code: Option<i64>,
        retry_after_s: Option<u32>,
    },
    /// Erfolgsantwort, die nicht dem erwarteten Format entspricht.
    Decode(String),
}

impl ApiError {
    pub(crate) fn code(&self) -> Option<i64> {
        match self {
            Self::Status { code, .. } => *code,
            _ => None,
        }
    }

    pub(crate) fn status(&self) -> Option<u16> {
        match self {
            Self::Status { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Access-Token abgelaufen oder ungültig.
    pub(crate) fn is_unauthorized(&self) -> bool {
        self.status() == Some(401)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(msg) => write!(f, "transport: {msg}"),
            Self::Status { status, code, .. } => match code {
                Some(code) => write!(f, "HTTP {status}, code {code}"),
                None => write!(f, "HTTP {status}"),
            },
            Self::Decode(msg) => write!(f, "unexpected response: {msg}"),
        }
    }
}

impl From<ApiError> for CoreError {
    fn from(err: ApiError) -> Self {
        let text = err.to_string();
        let ApiError::Status {
            status,
            code,
            retry_after_s,
        } = err
        else {
            return match err {
                ApiError::Transport(_) => CoreError::Network(text),
                _ => CoreError::RemoteRejected(text),
            };
        };
        match (status, code) {
            (429, _) => CoreError::RateLimited {
                retry_after_s: retry_after_s.unwrap_or(DEFAULT_RETRY_AFTER_S),
            },
            (503, _) if retry_after_s.is_some() => CoreError::RateLimited {
                retry_after_s: retry_after_s.unwrap_or(DEFAULT_RETRY_AFTER_S),
            },
            (408 | 500..=599, _) => CoreError::Network(text),
            // Die Session-Schicht erneuert vorher einmal die Tokens; kommt 401 trotzdem an,
            // hilft nur ein neuer Login.
            (401, _) => CoreError::AuthRequired,
            (
                _,
                Some(
                    code::APP_VERSION_MISSING
                    | code::APP_VERSION_BAD
                    | code::INVALID_APP_VERSION_FORMAT,
                ),
            )
            | (400, Some(code::OUTDATED_APP)) => CoreError::Auth(AuthError::ClientRejected(text)),
            (_, Some(code::ALREADY_EXISTS)) => CoreError::Conflict(text),
            (404, _) | (_, Some(code::NOT_EXISTS)) => CoreError::NotFound(text),
            _ => CoreError::RemoteRejected(text),
        }
    }
}

/// Wertet einen `Retry-After`-Header aus (Sekunden; HTTP-Datum → Standardwert).
pub(crate) fn parse_retry_after(value: &str) -> u32 {
    value
        .trim()
        .parse::<u32>()
        .map_or(DEFAULT_RETRY_AFTER_S, |s| s.clamp(1, MAX_RETRY_AFTER_S))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(status: u16, code: Option<i64>) -> ApiError {
        ApiError::Status {
            status,
            code,
            retry_after_s: None,
        }
    }

    #[test]
    fn retryable_errors() {
        assert!(matches!(
            CoreError::from(ApiError::Transport("timeout".into())),
            CoreError::Network(_)
        ));
        for s in [408, 500, 502, 503, 504] {
            assert!(CoreError::from(status(s, None)).is_retryable(), "{s}");
        }
        let limited = ApiError::Status {
            status: 429,
            code: None,
            retry_after_s: Some(42),
        };
        assert_eq!(
            CoreError::from(limited),
            CoreError::RateLimited { retry_after_s: 42 }
        );
        assert_eq!(
            CoreError::from(status(429, Some(2028))),
            CoreError::RateLimited {
                retry_after_s: DEFAULT_RETRY_AFTER_S
            }
        );
        let unavailable = ApiError::Status {
            status: 503,
            code: None,
            retry_after_s: Some(5),
        };
        assert_eq!(
            CoreError::from(unavailable),
            CoreError::RateLimited { retry_after_s: 5 }
        );
    }

    #[test]
    fn final_errors() {
        assert!(matches!(
            CoreError::from(status(422, Some(code::ALREADY_EXISTS))),
            CoreError::Conflict(_)
        ));
        assert!(matches!(
            CoreError::from(status(422, Some(code::NOT_EXISTS))),
            CoreError::NotFound(_)
        ));
        assert!(matches!(
            CoreError::from(status(404, None)),
            CoreError::NotFound(_)
        ));
        assert_eq!(
            CoreError::from(status(401, Some(401))),
            CoreError::AuthRequired
        );
        for (s, c) in [
            (400, code::APP_VERSION_MISSING),
            (400, code::APP_VERSION_BAD),
            (400, code::INVALID_APP_VERSION_FORMAT),
            (400, code::OUTDATED_APP),
        ] {
            let err = CoreError::from(status(s, Some(c)));
            assert!(
                matches!(err, CoreError::Auth(AuthError::ClientRejected(_))),
                "{s}/{c}: {err:?}"
            );
        }
        for (s, c) in [(422, 2001), (422, code::OUTDATED_APP)] {
            let err = CoreError::from(status(s, Some(c)));
            assert!(matches!(err, CoreError::RemoteRejected(_)), "{s}/{c}");
            assert!(!err.is_retryable());
        }
        assert!(matches!(
            CoreError::from(ApiError::Decode("x".into())),
            CoreError::RemoteRejected(_)
        ));
    }

    #[test]
    fn message_carries_status_and_code_only() {
        assert_eq!(
            CoreError::from(status(422, Some(2001))).to_string(),
            "remote rejected: HTTP 422, code 2001"
        );
    }

    #[test]
    fn retry_after_parsing() {
        assert_eq!(parse_retry_after("7"), 7);
        assert_eq!(parse_retry_after(" 0 "), 1);
        assert_eq!(parse_retry_after("999999"), MAX_RETRY_AFTER_S);
        assert_eq!(
            parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"),
            DEFAULT_RETRY_AFTER_S
        );
    }
}
