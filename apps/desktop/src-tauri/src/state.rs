//! Verdrahtung beim Start: SQLite, `CoreApi`, Sync-Treiber und (im Hintergrund) Schlüsselbund +
//! Proton-Konto. Die App ist sofort lokal benutzbar; Sync startet erst mit einer Sitzung.

use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc, Mutex, PoisonError,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use proton_adapter::{ApiConfig, ProtonAuthProvider, ProtonDriveRemoteStore};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use todo_core::{
    api::CoreApi,
    domain::Timestamp,
    ports::{AuthError, AuthProvider, Clock, KeyStore},
    CoreError, CoreResult,
};
use todo_crypto::XChaCha20Poly1305Cipher;
use todo_storage_sqlite::SqliteTaskRepository;
use todo_sync::{SchedulerConfig, SyncConfig, SyncEngine, SyncReport};

use crate::{
    dto::ErrorDto,
    events::{ChangedEmitter, OPEN_TASK_EVENT, SYNC_STATUS_EVENT},
    reminders::{self, settings::BackgroundSettingsDto, Notifier, Reminders},
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
    /// Letztes automatisches Leeren des Notiz-Papierkorbs (Unix-ms; 0 = noch nie).
    notes_purged_at: AtomicI64,
    /// Schließen läuft (das zweite `CloseRequested` schließt sofort).
    pub closing: AtomicBool,
    /// „Beenden“ aus dem Tray: Schließen beendet wirklich, auch im Hintergrundbetrieb.
    pub quitting: AtomicBool,
    /// Erinnerungen (ADR-033).
    pub reminders: Reminders,
    /// Hintergrundbetrieb und Erinnerungen dieses Geräts (`settings.json`).
    pub settings: Mutex<BackgroundSettingsDto>,
    /// Datenordner (Einstellungen, Erinnerungsstand).
    pub data_dir: PathBuf,
    /// Eigenes Profil (`TODO_DATA_DIR`) – der Autostart startet dasselbe.
    pub profile: Option<PathBuf>,
}

/// Höchstens so oft wird der Notiz-Papierkorb automatisch geleert (`notes.md` §4.7).
const NOTE_PURGE_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

impl AppState {
    /// Beim Start und bei Fokus aufrufen: löscht höchstens einmal täglich, was seit mehr als
    /// 30 Tagen im Papierkorb liegt (Notizen und Tasks) – ohne Timer im Leerlauf.
    pub fn purge_expired_notes_daily(&self) {
        let now = SystemClock.now().as_millis();
        let last = self.notes_purged_at.load(Ordering::Relaxed);
        if now - last < NOTE_PURGE_INTERVAL_MS
            || self
                .notes_purged_at
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
        {
            return;
        }
        let (api, sync) = (Arc::clone(&self.api), self.sync.clone());
        tauri::async_runtime::spawn(async move {
            match api.purge_expired_notes().await {
                Ok(0) => {}
                Ok(n) => {
                    tracing::info!(items = n, "expired notes purged");
                    sync.local_change();
                }
                Err(err) => tracing::warn!(error = %err, "purging expired notes failed"),
            }
            // Papierkorb der Tasks (ADR-031): gleiche Frist, gleicher Takt.
            match api.purge_expired_tasks().await {
                Ok(0) => {}
                Ok(n) => {
                    tracing::info!(tasks = n, "expired tasks purged");
                    sync.local_change();
                }
                Err(err) => tracing::warn!(error = %err, "purging expired tasks failed"),
            }
            // Verknüpfungen mit endgültig gelöschtem Ende (ADR-036).
            match api.purge_dangling_links().await {
                Ok(0) => {}
                Ok(n) => {
                    tracing::info!(links = n, "dangling links purged");
                    sync.local_change();
                }
                Err(err) => tracing::warn!(error = %err, "purging dangling links failed"),
            }
        });
    }
}

/// Baut den State und registriert die Event-Emitter.
///
/// # Errors
/// Wenn das Datenverzeichnis oder die Datenbank nicht geöffnet werden kann.
pub fn init<R: Runtime>(app: &AppHandle<R>, options: &Options) -> Result<AppState, String> {
    let dir = data_dir(app, options)?;
    let repo = SqliteTaskRepository::open(dir.join(DB_FILE)).map_err(|e| e.to_string())?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let api = Arc::new(
        CoreApi::new(Arc::new(repo), Arc::clone(&clock))
            .with_time_zones(Arc::new(crate::zones::JiffZones)),
    );
    api.subscribe(Box::new(ChangedEmitter::new(app.clone())));
    let settings = reminders::settings::load(&dir);
    let reminders = reminders::spawn(
        Arc::clone(&api),
        dir.clone(),
        settings.clone(),
        notifier(app),
    );
    api.subscribe(reminders.sink());

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
    // Dev-Build mit `TODO_REMOTE_DIR`: Verzeichnis statt Konto (Zwei-Geräte-Abnahme).
    let dev_remote = crate::dev::dir_runner(&api, &clock)
        .transpose()
        .map_err(|e| e.to_string())?;
    let accounts = options.accounts && dev_remote.is_none();
    if dev_remote.is_some() {
        sync.set_runner(dev_remote);
    }
    let keyring_id = keyring_app_id(options.data_dir.as_deref());
    let account = Arc::new(Account::new(clock, accounts, keyring_id));
    if accounts {
        let (account, api, sync) = (Arc::clone(&account), Arc::clone(&api), sync.clone());
        tauri::async_runtime::spawn(async move { account.restore(&api, &sync).await });
    }
    Ok(AppState {
        api,
        sync,
        account,
        notes_purged_at: AtomicI64::new(0),
        closing: AtomicBool::new(false),
        quitting: AtomicBool::new(false),
        reminders,
        settings: Mutex::new(settings),
        data_dir: dir,
        profile: options.data_dir.clone(),
    })
}

/// Datenordner des Profils (angelegt, nur für den Besitzer lesbar).
///
/// # Errors
/// Wenn er nicht bestimmt oder angelegt werden kann.
pub fn data_dir<R: Runtime>(app: &AppHandle<R>, options: &Options) -> Result<PathBuf, String> {
    let dir = match &options.data_dir {
        Some(dir) => dir.clone(),
        None => app.path().app_data_dir().map_err(|e| e.to_string())?,
    };
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    restrict_to_owner(&dir);
    Ok(dir)
}

/// Benachrichtigungen über das Portal; Klick zeigt das Fenster und öffnet die Task. Im Debug-Build
/// mit `TODO_NOTIFY=log` nur ins Log (Selbsttest ohne Meldungen auf dem Desktop).
fn notifier<R: Runtime>(app: &AppHandle<R>) -> Arc<dyn Notifier> {
    if cfg!(debug_assertions) && env::var("TODO_NOTIFY").is_ok_and(|v| v == "log") {
        return Arc::new(reminders::LogNotifier);
    }
    #[cfg(target_os = "linux")]
    {
        let app = app.clone();
        Arc::new(reminders::PortalNotifier::new(Arc::new(
            move |task_id: String| {
                crate::tray::show_window(&app);
                let _ = app.emit(OPEN_TASK_EVENT, task_id);
            },
        )))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        Arc::new(reminders::LogNotifier)
    }
}

impl AppState {
    /// Aktuelle Einstellungen des Hintergrundbetriebs.
    #[must_use]
    pub fn settings(&self) -> BackgroundSettingsDto {
        self.settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Hintergrundbetrieb an (Schließen versteckt nur)?
    #[must_use]
    pub fn background(&self) -> bool {
        self.settings().background
    }
}

/// Schlüsselbund + Anmeldung beim Sync-Backend. Wird beim ersten Zugriff (oder im Hintergrund
/// nach dem Start) geladen; ohne Schlüsselbund ist nur lokales Arbeiten möglich.
pub struct Account {
    backend: tokio::sync::OnceCell<Option<Backend>>,
    clock: Arc<dyn Clock>,
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
                Err(err) => tracing::warn!(error = %err, "sync: backend not available"),
            },
            Ok(None) => {}
            Err(err) => tracing::warn!(error = %err, "sync: reading session failed"),
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
            key_store: Arc::clone(&backend.key_store),
            engine,
            resumed: AtomicBool::new(false),
            account_id: Mutex::new(None),
        }))
    }

    pub async fn current(&self) -> Result<Option<crate::dto::SessionDto>, ErrorDto> {
        let Some(backend) = self.backend().await else {
            return Ok(None);
        };
        Ok(backend.auth.current_session().await?.map(Into::into))
    }

    /// Startet den Login. Die Shell öffnet keinen Browser: Die UI zeigt die URL zum Kopieren
    /// (ADR-038, `xdg-open` landete je nach Dateizuordnung in fremden Programmen).
    pub async fn begin_login(&self) -> Result<crate::dto::AuthStepDto, ErrorDto> {
        let backend = self.require().await?;
        Ok(backend.auth.begin_login().await?.into())
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
        let step = backend.auth.poll_login(&flow_id).await?;
        if matches!(step, todo_core::ports::AuthStep::Done(_)) {
            sync.set_runner(Some(self.runner(backend, api)?));
        }
        Ok(step.into())
    }

    pub async fn cancel_login(&self, flow: String) -> Result<(), ErrorDto> {
        let backend = self.require().await?;
        backend
            .auth
            .cancel_login(&todo_core::ports::AuthFlowId(flow))
            .await?;
        Ok(())
    }

    /// Meldet ab und entfernt den Sync-Schlüssel; lokale Daten bleiben. Sitzung und Schlüssel
    /// werden unabhängig voneinander entfernt (scheitert eins, wird das andere trotzdem gelöscht).
    pub async fn logout(&self, sync: &SyncHandle) -> Result<(), ErrorDto> {
        let backend = self.require().await?;
        sync.set_runner(None);
        let session = backend.auth.logout().await;
        let key = backend
            .key_store
            .delete(&SyncConfig::default().key_name())
            .await;
        session?;
        key?;
        Ok(())
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
                tracing::warn!(error = %err, "keyring not available, sync disabled");
                return None;
            }
        };
    match ProtonAuthProvider::new(ApiConfig::new(APP_VERSION), Arc::clone(&key_store)) {
        Ok(auth) => Some(Backend {
            auth: Arc::new(auth),
            key_store,
        }),
        Err(err) => {
            tracing::warn!(error = %err, "sync backend not available");
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
    key_store: Arc<dyn KeyStore>,
    engine: SyncEngine,
    resumed: AtomicBool,
    /// Konto, für das Engine und Store (Schlüssel, Ordner-Cache) geladen wurden.
    account_id: Mutex<Option<String>>,
}

impl ProtonRunner {
    async fn run(&self) -> CoreResult<SyncReport> {
        if !self.resumed.load(Ordering::Acquire) {
            match self.auth.resume().await? {
                Some(_) => self.resumed.store(true, Ordering::Release),
                None => return Err(CoreError::AuthRequired),
            }
        }
        // Ein Runner gehört zu genau einem Konto: Nach Abmelden und Anmelden mit einem anderen
        // Konto läuft er nicht mehr (die Shell setzt dann einen neuen).
        let current = self.auth.current_session().await?.map(|s| s.account_id);
        {
            let mut bound = self
                .account_id
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match (&*bound, &current) {
                (_, None) => return Err(CoreError::AuthRequired),
                (None, Some(id)) => *bound = Some(id.clone()),
                (Some(bound), Some(id)) if bound != id => return Err(CoreError::AuthRequired),
                _ => {}
            }
        }
        self.engine.sync_once().await
    }
}

#[async_trait::async_trait]
impl SyncRunner for ProtonRunner {
    async fn sync_once(&self) -> CoreResult<SyncReport> {
        let result = self.run().await;
        if matches!(result, Err(CoreError::Auth(AuthError::SessionRevoked))) {
            // Wie beim Abmelden: Mit der Sitzung geht auch der Sync-Schlüssel (sonst nutzte ein
            // späterer Login mit einem anderen Konto still den alten Schlüssel weiter).
            if let Err(err) = self
                .key_store
                .delete(&SyncConfig::default().key_name())
                .await
            {
                tracing::warn!(error = %err, "sync key not removed after revocation");
            }
        }
        result
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

/// Datenverzeichnis nur für den Besitzer (die Datenbank enthält alle Tasks im Klartext).
fn restrict_to_owner(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(err) = fs::set_permissions(dir, fs::Permissions::from_mode(0o700)) {
            tracing::warn!(error = %err, "data directory permissions not restricted");
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
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
