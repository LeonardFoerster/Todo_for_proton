//! Angemeldete Sitzung: Access-Token nur im Speicher; UID, Refresh-Token und Schlüssel-Passwort
//! im [`KeyStore`]. Authentifizierte Aufrufe erneuern bei 401 einmal die Tokens.
//!
//! Proton rotiert den Refresh-Token und macht den alten Access-Token sofort ungültig (P2-03).
//! Deshalb läuft ein Refresh immer unter dem Sitzungs-Lock, und neue Tokens werden sofort
//! gespeichert.

use std::sync::Arc;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use todo_core::{
    ports::{AuthError, KeyStore, SecretBytes, SessionInfo},
    CoreError, CoreResult,
};
use tokio::sync::{Mutex, MutexGuard};

use crate::{
    api_client::{ApiClient, Auth, Request},
    error::{code, ApiError},
    secret::Secret,
};

/// Name des Eintrags im KeyStore.
pub(crate) const STORE_KEY: &str = "proton-session";
const STORE_VERSION: u32 = 1;

/// Was im KeyStore liegt (JSON). Ohne `Debug`.
#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    uid: String,
    refresh_token: String,
    key_password: String,
    account_id: String,
    account_label: String,
}

/// Ergebnis eines erfolgreichen Logins.
pub(crate) struct NewSession {
    pub(crate) uid: String,
    pub(crate) access: Secret,
    pub(crate) refresh: Secret,
    pub(crate) key_password: Secret,
    pub(crate) info: SessionInfo,
}

struct Live {
    uid: String,
    /// `None` nach dem Laden aus dem KeyStore, bis zum ersten Refresh.
    access: Option<Secret>,
    refresh: Secret,
    key_password: Secret,
    info: SessionInfo,
}

impl Live {
    fn stored(&self) -> Stored {
        Stored {
            version: STORE_VERSION,
            uid: self.uid.clone(),
            refresh_token: self.refresh.expose().to_owned(),
            key_password: self.key_password.expose().to_owned(),
            account_id: self.info.account_id.clone(),
            account_label: self.info.account_label.clone(),
        }
    }

    fn credentials(&self) -> Option<Credentials> {
        self.access.as_ref().map(|access| Credentials {
            uid: self.uid.clone(),
            access: access.clone(),
        })
    }
}

#[derive(Default)]
struct State {
    loaded: bool,
    live: Option<Live>,
}

/// UID + Access-Token für einen Aufruf.
#[derive(Debug, Clone)]
pub(crate) struct Credentials {
    pub(crate) uid: String,
    pub(crate) access: Secret,
}

impl Credentials {
    pub(crate) fn auth(&self) -> Auth<'_> {
        Auth {
            uid: &self.uid,
            access_token: &self.access,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    #[serde(rename = "UID")]
    uid: Option<String>,
}

pub(crate) struct Session {
    api: ApiClient,
    key_store: Arc<dyn KeyStore>,
    state: Mutex<State>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").finish_non_exhaustive()
    }
}

impl Session {
    pub(crate) fn new(api: ApiClient, key_store: Arc<dyn KeyStore>) -> Self {
        Self {
            api,
            key_store,
            state: Mutex::new(State::default()),
        }
    }

    pub(crate) fn api(&self) -> &ApiClient {
        &self.api
    }

    /// Gesperrter Zustand; lädt beim ersten Zugriff die gespeicherte Sitzung.
    async fn state(&self) -> CoreResult<MutexGuard<'_, State>> {
        let mut state = self.state.lock().await;
        if !state.loaded {
            state.live = match self.key_store.get(STORE_KEY).await? {
                Some(bytes) => parse_stored(&bytes),
                None => None,
            };
            state.loaded = true;
        }
        Ok(state)
    }

    pub(crate) async fn info(&self) -> CoreResult<Option<SessionInfo>> {
        Ok(self.state().await?.live.as_ref().map(|l| l.info.clone()))
    }

    pub(crate) async fn key_password(&self) -> CoreResult<Secret> {
        self.state()
            .await?
            .live
            .as_ref()
            .map(|l| l.key_password.clone())
            .ok_or(CoreError::AuthRequired)
    }

    /// Übernimmt eine neue Sitzung (ersetzt eine alte) und speichert sie.
    pub(crate) async fn install(&self, new: NewSession) -> CoreResult<SessionInfo> {
        let mut state = self.state().await?;
        let live = Live {
            uid: new.uid,
            access: Some(new.access),
            refresh: new.refresh,
            key_password: new.key_password,
            info: new.info,
        };
        self.persist(&live).await?;
        let info = live.info.clone();
        state.live = Some(live);
        Ok(info)
    }

    /// Vergisst die Sitzung (Speicher + KeyStore). Lässt sich der Eintrag nicht löschen, wird er
    /// mit einem unbrauchbaren Wert überschrieben, damit er beim nächsten Start nicht wieder
    /// auflebt; der Fehler wird trotzdem gemeldet.
    pub(crate) async fn clear(&self) -> CoreResult<()> {
        let mut state = self.state().await?;
        state.live = None;
        match self.key_store.delete(STORE_KEY).await {
            Ok(()) => Ok(()),
            Err(err) => {
                let _ = self
                    .key_store
                    .put(STORE_KEY, SecretBytes::new(b"{}".to_vec()))
                    .await;
                Err(err)
            }
        }
    }

    /// Erneuert die Tokens, sofern der aktuelle Access-Token fehlt oder `stale` ist; sonst
    /// liefert er den (von einem parallelen Aufruf schon erneuerten) aktuellen.
    ///
    /// # Errors
    /// `AuthRequired` ohne Sitzung; `Auth(SessionRevoked)`, wenn das Backend den Refresh-Token
    /// ablehnt (Sitzung wird gelöscht); Netzfehler unverändert (Sitzung bleibt).
    pub(crate) async fn refresh(&self, stale: Option<&Secret>) -> CoreResult<Credentials> {
        let mut state = self.state().await?;
        let live = state.live.as_mut().ok_or(CoreError::AuthRequired)?;
        if let Some(current) = live.credentials() {
            if Some(&current.access) != stale {
                return Ok(current);
            }
        }
        let body = json!({
            "ResponseType": "token",
            "GrantType": "refresh_token",
            "RefreshToken": live.refresh.expose(),
            "RedirectURI": "https://proton.me",
        });
        let result: Result<RefreshResponse, ApiError> = self
            .api
            .send_as(
                Request::post("/auth/v4/refresh", "/auth/v4/refresh")
                    .json(body)
                    .uid(&live.uid),
            )
            .await;
        match result {
            Ok(fresh) => {
                if let Some(uid) = fresh.uid {
                    live.uid = uid;
                }
                live.access = Some(Secret::new(fresh.access_token));
                if let Some(refresh) = fresh.refresh_token {
                    live.refresh = Secret::new(refresh);
                }
                if let Err(err) = self.persist(live).await {
                    // Im Speicher geht es weiter; beim nächsten Start ist der Login evtl. weg.
                    tracing::warn!(error = %err, "rotated session could not be stored");
                }
                Ok(live.credentials().expect("access token just set"))
            }
            Err(err) if is_revocation(&err) => {
                tracing::info!(error = %err, "session revoked by server");
                state.live = None;
                self.key_store.delete(STORE_KEY).await?;
                Err(CoreError::Auth(AuthError::SessionRevoked))
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Authentifizierter Aufruf: bei 401 einmal Tokens erneuern und wiederholen.
    pub(crate) async fn send(&self, req: Request<'static>) -> CoreResult<Value> {
        Ok(self.try_send(req).await??)
    }

    /// Wie [`Self::send`], aber API-Fehler kommen unverändert zurück (z. B. für Konfliktdetails);
    /// der äußere Fehler betrifft nur die Sitzung (Login nötig, Refresh ohne Netz …).
    pub(crate) async fn try_send(
        &self,
        req: Request<'static>,
    ) -> CoreResult<Result<Value, ApiError>> {
        let credentials = self.refresh(None).await?;
        match self.api.send(req.clone().auth(credentials.auth())).await {
            Err(err) if err.is_unauthorized() => {
                let fresh = self.refresh(Some(&credentials.access)).await?;
                Ok(self.api.send(req.auth(fresh.auth())).await)
            }
            result => Ok(result),
        }
    }

    pub(crate) async fn send_as<T: DeserializeOwned>(
        &self,
        req: Request<'static>,
    ) -> CoreResult<T> {
        let what = req.describe();
        let body = self.send(req).await?;
        serde_json::from_value(body).map_err(|e| {
            CoreError::RemoteRejected(
                ApiError::Decode(crate::api_client::decode_error(&what, &e)).to_string(),
            )
        })
    }

    async fn persist(&self, live: &Live) -> CoreResult<()> {
        let json = serde_json::to_vec(&live.stored()).expect("session serializes");
        self.key_store.put(STORE_KEY, SecretBytes::new(json)).await
    }
}

fn parse_stored(bytes: &SecretBytes) -> Option<Live> {
    let Ok(stored) = serde_json::from_slice::<Stored>(bytes.expose()) else {
        tracing::warn!("stored session unreadable; ignoring it");
        return None;
    };
    (stored.version == STORE_VERSION).then(|| Live {
        uid: stored.uid,
        access: None,
        refresh: Secret::new(stored.refresh_token),
        key_password: Secret::new(stored.key_password),
        info: SessionInfo {
            account_label: stored.account_label,
            account_id: stored.account_id,
        },
    })
}

/// Lehnt der Server den Refresh endgültig ab? (z. B. 422/10013, 400, 401) – nicht bei Drosselung,
/// Zeitüberschreitung oder abgelehnter App-Version (dann liegt es nicht an der Sitzung).
fn is_revocation(err: &ApiError) -> bool {
    let client_rejected = matches!(
        err.code(),
        Some(code::APP_VERSION_MISSING | code::APP_VERSION_BAD | code::INVALID_APP_VERSION_FORMAT)
    );
    matches!(err.status(), Some(400..=499))
        && !matches!(err.status(), Some(408 | 429))
        && !client_rejected
}
