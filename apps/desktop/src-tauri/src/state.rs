//! Verdrahtung beim Start: SQLite, `CoreApi`, Sync-Treiber und (im Hintergrund) Schlüsselbund +
//! Proton-Konto. Die App ist sofort lokal benutzbar; Sync startet erst mit einer Sitzung.

use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, PoisonError,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use proton_adapter::{ApiConfig, ProtonAuthProvider, ProtonDriveRemoteStore};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use todo_core::{
    api::CoreApi,
    domain::Timestamp,
    ports::{AuthProvider, Clock, KeyStore},
    CoreError, CoreResult,
};
use todo_crypto::XChaCha20Poly1305Cipher;
use todo_storage_sqlite::SqliteTaskRepository;
use todo_sync::{SchedulerConfig, SyncConfig, SyncEngine, SyncReport};

use crate::{
    dto::ErrorDto,
    events::{ChangedEmitter, SYNC_STATUS_EVENT},
    sync::{self, SyncHandle, SyncRunner},
};

/// Ehrliche Kennung nach den Proton-SDK-Regeln (ADR-013), wie in der Dev-CLI.
const APP_VERSION: &str = "external-drive-tasks@0.1.0-alpha";
/// Eintrag im Schlüsselbund (getrennt von der Dev-CLI); eigene Profile siehe [`keyring_app_id`].
const KEYRING_APP_ID: &str = "dev.tasks.app";
const KEYRING_LABEL: &str = "Tasks";
/// Anbietername und Pflichthinweis für die Login-Ansicht (`interfaces.md` §3).
pub const PROVIDER_NAME: &str = "Proton";
pub const PROVIDER_NOTICE: &str =
    "This is a third-party application not officially supported by Proton.";
const DB_FILE: &str = "tasks.db";

/// Startoptionen; in der App aus der Umgebung, in Tests explizit.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Statt `$XDG_DATA_HOME/<app-id>` (z. B. zweites Profil für Tests: `TODO_DATA_DIR`).
    pub data_dir: Option<PathBuf>,
    /// Schlüsselbund + Konto im Hintergrund laden (in Tests aus).
    pub accounts: bool,
}

impl Options {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            data_dir: env::var_os("TODO_DATA_DIR").map(PathBuf::from),
            accounts: true,
        }
    }
}

#[derive(Debug)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Timestamp::from_millis(i64::try_from(ms).unwrap_or(i64::MAX))
    }
}

/// Tauri-State: alles, was Commands brauchen.
#[derive(Debug)]
pub struct AppState {
    pub api: Arc<CoreApi>,
    pub sync: SyncHandle,
    pub account: Arc<Account>,
}

/// Baut den State und registriert die Event-Emitter.
///
/// # Errors
/// Wenn das Datenverzeichnis oder die Datenbank nicht geöffnet werden kann.
pub fn init<R: Runtime>(app: &AppHandle<R>, options: &Options) -> Result<AppState, String> {
    let dir = match &options.data_dir {
        Some(dir) => dir.clone(),
        None => app.path().app_data_dir().map_err(|e| e.to_string())?,
    };
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let repo = SqliteTaskRepository::open(dir.join(DB_FILE)).map_err(|e| e.to_string())?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let api = Arc::new(CoreApi::new(Arc::new(repo), Arc::clone(&clock)));
    api.subscribe(Box::new(ChangedEmitter::new(app.clone())));

    let emitter = app.clone();
    let sync = sync::spawn(
        tauri::async_runtime::handle().inner(),
        api.repository(),
        Arc::clone(&clock),
        SchedulerConfig::default(),
        Box::new(move |status| {
            let _ = emitter.emit(SYNC_STATUS_EVENT, status);
        }),
    );
    let keyring_id = keyring_app_id(options.data_dir.as_deref());
    let account = Arc::new(Account::new(clock, options.accounts, keyring_id));
    if options.accounts {
        let (account, api, sync) = (Arc::clone(&account), Arc::clone(&api), sync.clone());
        tauri::async_runtime::spawn(async move { account.restore(&api, &sync).await });
    }
    Ok(AppState { api, sync, account })
}

/// Schlüsselbund + Anmeldung beim Sync-Backend. Wird beim ersten Zugriff (oder im Hintergrund
/// nach dem Start) geladen; ohne Schlüsselbund ist nur lokales Arbeiten möglich.
pub struct Account {
    backend: tokio::sync::OnceCell<Option<Backend>>,
    clock: Arc<dyn Clock>,
    /// Login-URLs laufender Vorgänge (für „Browser erneut öffnen").
    login_urls: Mutex<HashMap<String, String>>,
    keyring_id: String,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account").finish_non_exhaustive()
    }
}

struct Backend {
    auth: Arc<ProtonAuthProvider>,
    key_store: Arc<dyn KeyStore>,
}

impl Account {
    fn new(clock: Arc<dyn Clock>, enabled: bool, keyring_id: String) -> Self {
        Self {
            backend: if enabled {
                tokio::sync::OnceCell::new()
            } else {
                tokio::sync::OnceCell::new_with(Some(None))
            },
            clock,
            login_urls: Mutex::default(),
            keyring_id,
        }
    }

    async fn backend(&self) -> Option<&Backend> {
        self.backend
            .get_or_init(|| connect(&self.keyring_id))
            .await
            .as_ref()
    }

    /// Wie [`Self::backend`], aber als Command-Fehler, wenn nicht verfügbar.
    async fn require(&self) -> Result<&Backend, ErrorDto> {
        self.backend().await.ok_or_else(|| {
            ErrorDto::unavailable(
                "Kein Schlüsselbund (Secret Service) verfügbar – Sync nicht möglich",
            )
        })
    }

    /// Ist Login auf diesem System möglich?
    pub async fn available(&self) -> bool {
        self.backend().await.is_some()
    }

    /// Nach dem Start: gespeicherte Sitzung → Sync einschalten. Kein Netz nötig.
    async fn restore(&self, api: &CoreApi, sync: &SyncHandle) {
        let Some(backend) = self.backend().await else {
            return;
        };
        match backend.auth.current_session().await {
            Ok(Some(_)) => match self.runner(backend, api) {
                Ok(runner) => sync.set_runner(Some(runner)),
                Err(err) => eprintln!("sync: backend not available: {err}"),
            },
            Ok(None) => {}
            Err(err) => eprintln!("sync: reading session failed: {err}"),
        }
    }

    fn runner(&self, backend: &Backend, api: &CoreApi) -> CoreResult<Arc<dyn SyncRunner>> {
        let remote = ProtonDriveRemoteStore::new(&backend.auth)?;
        let engine = SyncEngine::new(
            api.repository(),
            Arc::new(remote),
            Arc::new(XChaCha20Poly1305Cipher::new()),
            Arc::clone(&backend.key_store),
            Arc::clone(&self.clock),
            SyncConfig {
                device_name: device_name(),
                platform: "linux".into(),
                ..SyncConfig::default()
            },
        );
        Ok(Arc::new(ProtonRunner {
            auth: Arc::clone(&backend.auth),
            engine,
            resumed: AtomicBool::new(false),
        }))
    }

    pub async fn current(&self) -> Result<Option<crate::dto::SessionDto>, ErrorDto> {
        let Some(backend) = self.backend().await else {
            return Ok(None);
        };
        Ok(backend.auth.current_session().await?.map(Into::into))
    }

    pub async fn begin_login(&self) -> Result<crate::dto::AuthStepDto, ErrorDto> {
        let backend = self.require().await?;
        let step = backend.auth.begin_login().await?;
        if let todo_core::ports::AuthStep::OpenBrowser { flow, url, .. } = &step {
            self.urls().insert(flow.0.clone(), url.clone());
            open_browser(url);
        }
        Ok(step.into())
    }

    /// Öffnet die Login-Seite eines laufenden Vorgangs erneut.
    pub fn reopen_browser(&self, flow: &str) -> Result<(), ErrorDto> {
        let url = self
            .urls()
            .get(flow)
            .cloned()
            .ok_or_else(|| ErrorDto::from(CoreError::NotFound(format!("login flow {flow}"))))?;
        open_browser(&url);
        Ok(())
    }

    /// Fragt den Login-Status ab; bei Erfolg startet der Sync.
    pub async fn poll_login(
        &self,
        flow: String,
        api: &CoreApi,
        sync: &SyncHandle,
    ) -> Result<crate::dto::AuthStepDto, ErrorDto> {
        let backend = self.require().await?;
        let flow_id = todo_core::ports::AuthFlowId(flow);
        let result = backend.auth.poll_login(&flow_id).await;
        if !matches!(result, Ok(todo_core::ports::AuthStep::Pending { .. })) {
            self.urls().remove(&flow_id.0);
        }
        let step = result?;
        if matches!(step, todo_core::ports::AuthStep::Done(_)) {
            sync.set_runner(Some(self.runner(backend, api)?));
        }
        Ok(step.into())
    }

    pub async fn cancel_login(&self, flow: String) -> Result<(), ErrorDto> {
        let backend = self.require().await?;
        self.urls().remove(&flow);
        backend
            .auth
            .cancel_login(&todo_core::ports::AuthFlowId(flow))
            .await?;
        Ok(())
    }

    /// Meldet ab und entfernt den Sync-Schlüssel; lokale Daten bleiben.
    pub async fn logout(&self, sync: &SyncHandle) -> Result<(), ErrorDto> {
        let backend = self.require().await?;
        sync.set_runner(None);
        backend.auth.logout().await?;
        backend
            .key_store
            .delete(&SyncConfig::default().key_name())
            .await?;
        Ok(())
    }

    fn urls(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.login_urls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Schlüsselbund-Kennung eines Profils. Das Standardprofil nutzt [`KEYRING_APP_ID`]; ein eigenes
/// Datenverzeichnis (`TODO_DATA_DIR`, z. B. zweites Profil für den Zwei-Geräte-Test) bekommt eine
/// eigene Sitzung und einen eigenen Sync-Schlüssel – zwei Profile mit einer Sitzung würden sich
/// beim Token-Refresh gegenseitig abmelden (der Refresh entwertet das alte Token-Paar).
fn keyring_app_id(data_dir: Option<&Path>) -> String {
    match data_dir {
        None => KEYRING_APP_ID.to_owned(),
        Some(dir) => {
            let dir = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
            format!("{KEYRING_APP_ID}@{}", dir.display())
        }
    }
}

#[cfg(target_os = "linux")]
async fn connect(keyring_id: &str) -> Option<Backend> {
    use todo_keystore_secret_service::SecretServiceKeyStore;
    let key_store: Arc<dyn KeyStore> =
        match SecretServiceKeyStore::connect(keyring_id, KEYRING_LABEL).await {
            Ok(store) => Arc::new(store),
            Err(err) => {
                eprintln!("keyring not available, sync disabled: {err}");
                return None;
            }
        };
    match ProtonAuthProvider::new(ApiConfig::new(APP_VERSION), Arc::clone(&key_store)) {
        Ok(auth) => Some(Backend {
            auth: Arc::new(auth),
            key_store,
        }),
        Err(err) => {
            eprintln!("sync backend not available: {err}");
            None
        }
    }
}

#[cfg(not(target_os = "linux"))]
async fn connect(keyring_id: &str) -> Option<Backend> {
    let _ = (keyring_id, KEYRING_LABEL, APP_VERSION);
    None
}

/// Sync gegen Proton Drive. Stellt vor dem ersten Lauf die gespeicherte Sitzung wieder her
/// (Refresh, wenn Netz da ist); Engine und Remote-Store leben so lange wie die Anmeldung, damit
/// Schlüssel und Ordner-Cache nur einmal geladen werden (P6-05).
struct ProtonRunner {
    auth: Arc<ProtonAuthProvider>,
    engine: SyncEngine,
    resumed: AtomicBool,
}

#[async_trait::async_trait]
impl SyncRunner for ProtonRunner {
    async fn sync_once(&self) -> CoreResult<SyncReport> {
        if !self.resumed.load(Ordering::Acquire) {
            match self.auth.resume().await? {
                Some(_) => self.resumed.store(true, Ordering::Release),
                None => return Err(CoreError::AuthRequired),
            }
        }
        self.engine.sync_once().await
    }
}

fn device_name() -> String {
    fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "Linux".into())
}

/// Öffnet die Login-Seite im Standardbrowser (Fehler egal: die UI zeigt die URL zum Kopieren).
fn open_browser(url: &str) {
    match std::process::Command::new("xdg-open").arg(url).spawn() {
        // Abwarten, damit kein Zombie-Prozess bleibt.
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(err) => eprintln!("xdg-open failed: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_get_their_own_keyring_entry() {
        assert_eq!(keyring_app_id(None), KEYRING_APP_ID);
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let id_a = keyring_app_id(Some(a.path()));
        assert_ne!(id_a, KEYRING_APP_ID);
        assert_ne!(id_a, keyring_app_id(Some(b.path())));
        // Relativ oder absolut angegeben: dasselbe Profil.
        let nested = a.path().join("x");
        fs::create_dir(&nested).unwrap();
        assert_eq!(id_a, keyring_app_id(Some(&nested.join(".."))));
    }
}
