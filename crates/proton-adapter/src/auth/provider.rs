use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use reqwest::Method;
use serde::Deserialize;
use todo_core::{
    ports::{AuthError, AuthFlowId, AuthProvider, AuthStep, KeyStore, SessionInfo},
    CoreError, CoreResult,
};

use super::{
    fork::{self, ForkKey},
    session::{NewSession, Session},
};
use crate::{
    api_client::{ApiClient, ApiConfig, Auth, Request},
    secret::Secret,
};

/// Wie das offizielle CLI: alle 5 s fragen, höchstens 10 min.
const POLL_INTERVAL_S: u32 = 5;
const LOGIN_EXPIRES_S: u32 = 600;

struct Flow {
    selector: Secret,
    key: ForkKey,
    started: Instant,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ForkInit {
    selector: String,
    user_code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ForkDone {
    #[serde(rename = "UID")]
    uid: String,
    access_token: String,
    refresh_token: String,
    payload: String,
    #[serde(rename = "UserID", default)]
    user_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UsersResponse {
    user: User,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct User {
    #[serde(rename = "ID")]
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

/// [`AuthProvider`] für Proton per Browser-Session-Fork (ADR-013).
///
/// Die Sitzung teilt er mit dem Drive-Store desselben Kontos.
pub struct ProtonAuthProvider {
    session: Arc<Session>,
    flows: Mutex<HashMap<String, Flow>>,
    login_expires: Duration,
}

impl std::fmt::Debug for ProtonAuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtonAuthProvider").finish_non_exhaustive()
    }
}

impl ProtonAuthProvider {
    /// `key_store` nimmt UID, Refresh-Token und Schlüssel-Passwort auf.
    ///
    /// # Errors
    /// `CoreError::Internal`, wenn der HTTP-Client nicht gebaut werden kann.
    pub fn new(config: ApiConfig, key_store: Arc<dyn KeyStore>) -> CoreResult<Self> {
        Ok(Self {
            session: Arc::new(Session::new(ApiClient::new(config)?, key_store)),
            flows: Mutex::new(HashMap::new()),
            login_expires: Duration::from_secs(u64::from(LOGIN_EXPIRES_S)),
        })
    }

    pub(crate) fn session(&self) -> Arc<Session> {
        Arc::clone(&self.session)
    }

    #[cfg(test)]
    pub(crate) fn set_login_expiry(&mut self, expires: Duration) {
        self.login_expires = expires;
    }

    fn flows(&self) -> std::sync::MutexGuard<'_, HashMap<String, Flow>> {
        self.flows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn complete(&self, done: ForkDone, key: &ForkKey) -> CoreResult<SessionInfo> {
        let key_password = fork::decrypt_payload(key, &done.payload)?;
        let access = Secret::new(done.access_token);
        let users: UsersResponse = self
            .session
            .api()
            .send_as(Request::get("/core/v4/users", "/core/v4/users").auth(Auth {
                uid: &done.uid,
                access_token: &access,
            }))
            .await?;
        let user = users.user;
        let account_label = [user.email, user.name]
            .into_iter()
            .flatten()
            .find(|s| !s.is_empty())
            .unwrap_or_else(|| user.id.clone());
        let account_id = done.user_id.unwrap_or(user.id);
        // Eine noch bestehende Sitzung ersetzen: serverseitig beenden, damit kein gültiges
        // Token-Paar zurückbleibt (Fehler egal, sie verfällt sonst von selbst).
        if self.session.info().await?.is_some() {
            let request = Request::new(Method::DELETE, "/auth/v4", "/auth/v4");
            if let Err(err) = self.session.send(request).await {
                tracing::debug!(error = %err, "previous session not revoked");
            }
        }
        self.session
            .install(NewSession {
                uid: done.uid,
                access,
                refresh: Secret::new(done.refresh_token),
                key_password,
                info: SessionInfo {
                    account_label,
                    account_id,
                },
            })
            .await
    }
}

#[async_trait::async_trait]
impl AuthProvider for ProtonAuthProvider {
    async fn current_session(&self) -> CoreResult<Option<SessionInfo>> {
        self.session.info().await
    }

    async fn begin_login(&self) -> CoreResult<AuthStep> {
        let init: ForkInit = self
            .session
            .api()
            .send_as(Request::get(
                "/auth/v4/sessions/forks",
                "/auth/v4/sessions/forks",
            ))
            .await?;
        // Werte vom Server landen in URL-Pfad bzw. -Fragment: nur einfache Kennungen zulassen.
        if !is_token(&init.selector) || !is_token(&init.user_code) {
            return Err(CoreError::RemoteRejected("unexpected fork response".into()));
        }
        let key = ForkKey::generate()?;
        let url = fork::login_url(&init.user_code, &key);
        let flow = AuthFlowId(random_id()?);
        let mut flows = self.flows();
        // Abgelaufene, nie abgefragte Vorgänge nicht ewig behalten.
        flows.retain(|_, f| f.started.elapsed() <= self.login_expires);
        flows.insert(
            flow.0.clone(),
            Flow {
                selector: Secret::new(init.selector),
                key,
                started: Instant::now(),
            },
        );
        drop(flows);
        Ok(AuthStep::OpenBrowser {
            flow,
            url: url.expose().to_owned(),
            poll_interval_s: POLL_INTERVAL_S,
            expires_in_s: u32::try_from(self.login_expires.as_secs()).unwrap_or(u32::MAX),
        })
    }

    async fn poll_login(&self, flow: &AuthFlowId) -> CoreResult<AuthStep> {
        let (selector, key) = {
            let mut flows = self.flows();
            let Some(entry) = flows.get(&flow.0) else {
                return Err(CoreError::Auth(AuthError::Cancelled));
            };
            if entry.started.elapsed() > self.login_expires {
                flows.remove(&flow.0);
                return Err(CoreError::Auth(AuthError::Expired));
            }
            (entry.selector.clone(), entry.key.clone())
        };
        let path = format!(
            "/auth/v4/sessions/forks/{}",
            fork::encode_uri_component(selector.expose())
        );
        let result = self
            .session
            .api()
            .send_as::<ForkDone>(Request::get("/auth/v4/sessions/forks/{selector}", path))
            .await;
        match result {
            // 422 = im Browser noch nicht bestätigt.
            Err(err) if err.status() == Some(422) => Ok(AuthStep::Pending { flow: flow.clone() }),
            Err(err) => Err(err.into()),
            Ok(done) => {
                // Der Selector gilt nur einmal – Vorgang ist ab hier beendet, auch bei Fehlern.
                self.flows().remove(&flow.0);
                self.complete(done, &key).await.map(AuthStep::Done)
            }
        }
    }

    async fn cancel_login(&self, flow: &AuthFlowId) -> CoreResult<()> {
        let Some(entry) = self.flows().remove(&flow.0) else {
            return Ok(());
        };
        // Selector serverseitig verwerfen; scheitert das, verfällt er von selbst.
        let path = format!(
            "/auth/v4/sessions/forks/{}",
            fork::encode_uri_component(entry.selector.expose())
        );
        let request = Request::new(Method::DELETE, "/auth/v4/sessions/forks/{selector}", path);
        if let Err(err) = self.session.api().send(request).await {
            tracing::debug!(error = %err, "fork selector not discarded");
        }
        Ok(())
    }

    async fn resume(&self) -> CoreResult<Option<SessionInfo>> {
        let Some(info) = self.session.info().await? else {
            return Ok(None);
        };
        match self.session.refresh(None).await {
            Ok(_) => Ok(Some(info)),
            // Offline-first: gespeicherte Sitzung gilt, Refresh beim ersten Aufruf mit Netz.
            Err(err) if err.is_retryable() => Ok(Some(info)),
            Err(err) => Err(err),
        }
    }

    async fn logout(&self) -> CoreResult<()> {
        if self.session.info().await?.is_some() {
            let request = Request::new(Method::DELETE, "/auth/v4", "/auth/v4");
            if let Err(err) = self.session.send(request).await {
                // Lokal trotzdem abmelden; die Sitzung verfällt serverseitig.
                tracing::warn!(error = %err, "server-side logout failed");
            }
        }
        self.session.clear().await
    }
}

/// Selector und User-Code: sichtbares ASCII ohne Leerzeichen, höchstens 256 Zeichen. In URLs
/// werden sie zusätzlich prozentkodiert.
fn is_token(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && value.bytes().all(|b| b.is_ascii_graphic())
}

fn random_id() -> CoreResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| CoreError::Internal(format!("random: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
