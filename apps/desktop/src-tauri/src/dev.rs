//! Nur in Debug-Builds wirksam: Sync gegen ein lokales Verzeichnis statt Proton und Steuerung des
//! Dev-Selbsttests von außen – für die Zwei-Geräte-Abnahme mit zwei Profilen ohne Konto
//! (`tools/acceptance-two-devices.sh`). In Release-Builds liefern alle Funktionen `None`/nichts.
//!
//! - `TODO_REMOTE_DIR=<ordner>`: Sync-Backend ist dieses Verzeichnis (wie `todo-cli --remote`).
//!   Liegt dort `.auth-revoked`, meldet jeder Lauf „Anmeldung erforderlich" (abgelaufene Sitzung).
//! - `TODO_REMOTE_DELAY_MS=<ms>`: jeder Backend-Aufruf wartet so lange (Zeitfenster für `kill -9`).
//! - `TASKS_SELFTEST=<suiten>`: Selbsttest-Suiten zur Laufzeit (Frontend mit
//!   `VITE_TASKS_SELFTEST=runtime`); Zeilen des Berichts gehen mit Präfix `SELFTEST` auf stdout.

use std::{
    collections::BTreeMap,
    env,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use todo_core::{
    api::CoreApi,
    ports::{
        Clock, KeyStore, RemoteChanges, RemoteCursor, RemoteDir, RemoteEntry, RemoteListing,
        RemotePath, RemoteStore, Revision, SecretBytes,
    },
    CoreError, CoreResult,
};
use todo_crypto::XChaCha20Poly1305Cipher;
use todo_remote_fs::FsRemoteStore;
use todo_sync::{SyncConfig, SyncEngine, SyncReport};

use crate::sync::SyncRunner;

const REMOTE_ENV: &str = "TODO_REMOTE_DIR";
const DELAY_ENV: &str = "TODO_REMOTE_DELAY_MS";
const SELFTEST_ENV: &str = "TASKS_SELFTEST";
/// Datei im Sync-Ordner, die eine abgelaufene Sitzung simuliert.
const AUTH_REVOKED: &str = ".auth-revoked";

fn dev_env(name: &str) -> Option<String> {
    if cfg!(debug_assertions) {
        env::var(name).ok().filter(|v| !v.is_empty())
    } else {
        None
    }
}

/// Selbsttest-Suiten aus der Umgebung.
pub fn selftest_suites() -> Option<String> {
    dev_env(SELFTEST_ENV)
}

/// Eine Zeile des Selbsttest-Berichts auf stdout (nur Debug-Builds).
pub fn selftest_line(line: &str) {
    if cfg!(debug_assertions) {
        for part in line.lines() {
            println!("SELFTEST {part}");
        }
    }
}

/// Sync-Treiber gegen `TODO_REMOTE_DIR`, falls gesetzt.
pub fn dir_runner(
    api: &CoreApi,
    clock: &Arc<dyn Clock>,
) -> Option<CoreResult<Arc<dyn SyncRunner>>> {
    let dir = PathBuf::from(dev_env(REMOTE_ENV)?);
    let delay = dev_env(DELAY_ENV)
        .and_then(|ms| ms.parse().ok())
        .unwrap_or(0);
    Some(FsRemoteStore::new(&dir).map(|store| {
        let remote: Arc<dyn RemoteStore> = Arc::new(Slow {
            inner: store,
            delay: Duration::from_millis(delay),
        });
        let engine = SyncEngine::new(
            api.repository(),
            remote,
            Arc::new(XChaCha20Poly1305Cipher::new()),
            Arc::new(ProcessKeyStore::default()),
            Arc::clone(clock),
            SyncConfig {
                device_name: "dev".into(),
                platform: "linux".into(),
                ..SyncConfig::default()
            },
        );
        Arc::new(DirRunner {
            engine,
            revoked: dir.join(AUTH_REVOKED),
        }) as Arc<dyn SyncRunner>
    }))
}

struct DirRunner {
    engine: SyncEngine,
    revoked: PathBuf,
}

#[async_trait::async_trait]
impl SyncRunner for DirRunner {
    async fn sync_once(&self) -> CoreResult<SyncReport> {
        if self.revoked.exists() {
            return Err(CoreError::AuthRequired);
        }
        self.engine.sync_once().await
    }
}

/// Sync-Schlüssel nur im Prozess; er liegt ohnehin im Sync-Ordner (`keys/sync-key.enc`).
#[derive(Debug, Default)]
struct ProcessKeyStore(Mutex<BTreeMap<String, SecretBytes>>);

impl ProcessKeyStore {
    fn entries(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, SecretBytes>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait::async_trait]
impl KeyStore for ProcessKeyStore {
    async fn get(&self, name: &str) -> CoreResult<Option<SecretBytes>> {
        Ok(self.entries().get(name).cloned())
    }

    async fn put(&self, name: &str, value: SecretBytes) -> CoreResult<()> {
        self.entries().insert(name.to_owned(), value);
        Ok(())
    }

    async fn delete(&self, name: &str) -> CoreResult<()> {
        self.entries().remove(name);
        Ok(())
    }
}

/// Backend mit künstlicher Latenz vor jedem Aufruf.
struct Slow {
    inner: FsRemoteStore,
    delay: Duration,
}

impl Slow {
    async fn wait(&self) {
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
    }
}

#[async_trait::async_trait]
impl RemoteStore for Slow {
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir> {
        self.wait().await;
        self.inner.open_app_root(name).await
    }
    async fn list(
        &self,
        dir: &RemotePath,
        since: Option<&RemoteCursor>,
    ) -> CoreResult<RemoteListing> {
        self.wait().await;
        self.inner.list(dir, since).await
    }
    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>> {
        self.wait().await;
        self.inner.read(path).await
    }
    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry> {
        self.wait().await;
        self.inner.create(path, bytes).await
    }
    async fn replace(
        &self,
        path: &RemotePath,
        bytes: Vec<u8>,
        expected: Option<&Revision>,
    ) -> CoreResult<RemoteEntry> {
        self.wait().await;
        self.inner.replace(path, bytes, expected).await
    }
    async fn delete(&self, path: &RemotePath) -> CoreResult<()> {
        self.wait().await;
        self.inner.delete(path).await
    }
    async fn changes_since(
        &self,
        cursor: Option<&RemoteCursor>,
    ) -> CoreResult<Option<RemoteChanges>> {
        self.wait().await;
        self.inner.changes_since(cursor).await
    }
}
