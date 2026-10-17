//! `AuthProvider`: Login beim Sync-Backend, anbieterneutral formuliert (`interfaces.md` §3).
//!
//! Login läuft im Browser („Session-Fork", ADR-013): Die App sieht weder Passwort noch zweiten
//! Faktor oder CAPTCHA. Für die UI ist das zweistufig – URL öffnen, dann bis zur Bestätigung pollen.

use std::fmt;

use super::{MaybeSend, MaybeSync};
use crate::CoreResult;

/// Kennung eines laufenden Login-Vorgangs (opak, ohne Geheimnis).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthFlowId(pub String);

impl fmt::Display for AuthFlowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Angemeldetes Konto, wie die UI es anzeigt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// Anzeigename, z. B. die E-Mail-Adresse.
    pub account_label: String,
    pub account_id: String,
}

/// Nächster Schritt eines Logins.
///
/// `url` in `OpenBrowser` enthält einen Einmal-Schlüssel – nie loggen; `Debug` blendet sie aus.
#[derive(Clone, PartialEq, Eq)]
pub enum AuthStep {
    /// Browser mit `url` öffnen; danach alle `poll_interval_s` Sekunden
    /// [`AuthProvider::poll_login`] aufrufen, höchstens `expires_in_s` lang.
    OpenBrowser {
        flow: AuthFlowId,
        url: String,
        poll_interval_s: u32,
        expires_in_s: u32,
    },
    /// Noch nicht im Browser bestätigt.
    Pending {
        flow: AuthFlowId,
    },
    Done(SessionInfo),
}

impl fmt::Debug for AuthStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenBrowser {
                flow,
                poll_interval_s,
                expires_in_s,
                ..
            } => f
                .debug_struct("OpenBrowser")
                .field("flow", flow)
                .field("url", &"<redacted>")
                .field("poll_interval_s", poll_interval_s)
                .field("expires_in_s", expires_in_s)
                .finish(),
            Self::Pending { flow } => f.debug_struct("Pending").field("flow", flow).finish(),
            Self::Done(info) => f.debug_tuple("Done").field(info).finish(),
        }
    }
}

/// Login-/Sitzungsfehler (in [`crate::CoreError::Auth`]). Drosselung ist
/// [`crate::CoreError::RateLimited`], fehlende Anmeldung [`crate::CoreError::AuthRequired`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// Login-Fenster abgelaufen.
    #[error("login expired")]
    Expired,
    /// Login abgebrochen (oder unbekannter Vorgang).
    #[error("login cancelled")]
    Cancelled,
    /// Gespeicherte Sitzung ungültig (abgemeldet, widerrufen) – neu anmelden.
    #[error("session revoked")]
    SessionRevoked,
    /// Backend lehnt diese App-Version ab (fehlt, ungültig, veraltet) – App aktualisieren.
    #[error("client rejected: {0}")]
    ClientRejected(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// Anmeldung beim Sync-Backend. Tokens/Schlüssel verwaltet der Adapter selbst (über den
/// `KeyStore`); sie verlassen ihn nie.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait AuthProvider: MaybeSend + MaybeSync {
    /// Angemeldetes Konto laut lokalem Stand (kein Netz).
    async fn current_session(&self) -> CoreResult<Option<SessionInfo>>;

    /// Startet einen Login; liefert [`AuthStep::OpenBrowser`].
    async fn begin_login(&self) -> CoreResult<AuthStep>;

    /// Fragt einmal den Status ab (blockiert nicht): `Pending` oder `Done`.
    ///
    /// # Errors
    /// `Auth(Expired)` nach Ablauf, `Auth(Cancelled)` für unbekannte/abgebrochene Vorgänge.
    async fn poll_login(&self, flow: &AuthFlowId) -> CoreResult<AuthStep>;

    /// Bricht einen Login ab. Unbekannte Vorgänge sind kein Fehler.
    async fn cancel_login(&self, flow: &AuthFlowId) -> CoreResult<()>;

    /// Stellt eine gespeicherte Sitzung wieder her und erneuert die Tokens, wenn das Netz da
    /// ist. Ohne Netz gilt die gespeicherte Sitzung weiter (offline-first). `None` = keine.
    ///
    /// # Errors
    /// `Auth(SessionRevoked)`, wenn das Backend die Sitzung nicht mehr kennt (lokal gelöscht).
    async fn resume(&self) -> CoreResult<Option<SessionInfo>>;

    /// Meldet ab und löscht die lokalen Sitzungsdaten (auch ohne Netz).
    async fn logout(&self) -> CoreResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_login_url() {
        let step = AuthStep::OpenBrowser {
            flow: AuthFlowId("f1".into()),
            url: "https://login.example/#payload=SECRET".into(),
            poll_interval_s: 5,
            expires_in_s: 600,
        };
        let shown = format!("{step:?}");
        assert!(!shown.contains("SECRET"), "{shown}");
        assert!(shown.contains("f1"));
    }
}
