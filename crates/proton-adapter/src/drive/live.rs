//! Live-Contract-Test gegen das echte Proton-Testkonto (P6-04). Läuft nur manuell:
//!
//! ```text
//! PROTON_LIVE=1 cargo test -p proton-adapter --lib drive::live -- --ignored --nocapture
//! ```
//!
//! Ein Browser-Login; danach alle `RemoteStore`-Contract-Fälle nacheinander, jeder in einem
//! eigenen Unterordner von `TasksContractTest-<Zeit>/`, der am Ende endgültig gelöscht wird.
//! Ausgabe ohne Geheimnisse und Kontodaten; Sitzung nur im Speicher, am Ende abgemeldet.

use std::{
    process::Command,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use todo_core::{
    ports::{
        AuthProvider, AuthStep, RemoteChanges, RemoteCursor, RemoteDir, RemoteEntry, RemoteListing,
        RemotePath, RemoteStore, Revision,
    },
    testing::{remote_store_contract as contract, InMemoryKeyStore},
    CoreResult,
};
use tokio::runtime::Runtime;

use super::{
    crypto::DriveCrypto,
    store::{Drive, Pgp},
};
use crate::{api_client::ApiConfig, ProtonAuthProvider};

const APP_VERSION: &str = "external-drive-tasks@0.1.0-alpha";

/// Verschiebt alle Pfade unter `prefix` und läuft synchron auf `rt` (für die Contract-Suite).
struct Prefixed<'a, S> {
    rt: &'a Runtime,
    inner: &'a S,
    prefix: String,
}

impl<S> Prefixed<'_, S> {
    fn inner_path(&self, path: &RemotePath) -> RemotePath {
        RemotePath::new(&format!("{}/{path}", self.prefix)).expect("path")
    }

    fn outer_path(&self, path: &RemotePath) -> Option<RemotePath> {
        path.as_str()
            .strip_prefix(&format!("{}/", self.prefix))
            .map(|p| RemotePath::new(p).expect("path"))
    }

    fn outer_entry(&self, mut entry: RemoteEntry) -> Option<RemoteEntry> {
        entry.path = self.outer_path(&entry.path)?;
        Some(entry)
    }
}

#[async_trait::async_trait]
impl<S: RemoteStore> RemoteStore for Prefixed<'_, S> {
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir> {
        let path = RemotePath::new(name)?;
        self.rt
            .block_on(self.inner.open_app_root(self.inner_path(&path).as_str()))?;
        Ok(RemoteDir { path })
    }

    async fn list(
        &self,
        dir: &RemotePath,
        since: Option<&RemoteCursor>,
    ) -> CoreResult<RemoteListing> {
        let mut listing = self
            .rt
            .block_on(self.inner.list(&self.inner_path(dir), since))?;
        listing.entries = listing
            .entries
            .into_iter()
            .filter_map(|e| self.outer_entry(e))
            .collect();
        Ok(listing)
    }

    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>> {
        self.rt.block_on(self.inner.read(&self.inner_path(path)))
    }

    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry> {
        let entry = self
            .rt
            .block_on(self.inner.create(&self.inner_path(path), bytes))?;
        Ok(self.outer_entry(entry).expect("inside prefix"))
    }

    async fn replace(
        &self,
        path: &RemotePath,
        bytes: Vec<u8>,
        expected: Option<&Revision>,
    ) -> CoreResult<RemoteEntry> {
        let entry =
            self.rt
                .block_on(self.inner.replace(&self.inner_path(path), bytes, expected))?;
        Ok(self.outer_entry(entry).expect("inside prefix"))
    }

    async fn delete(&self, path: &RemotePath) -> CoreResult<()> {
        self.rt.block_on(self.inner.delete(&self.inner_path(path)))
    }

    async fn changes_since(
        &self,
        cursor: Option<&RemoteCursor>,
    ) -> CoreResult<Option<RemoteChanges>> {
        let Some(mut changes) = self.rt.block_on(self.inner.changes_since(cursor))? else {
            return Ok(None);
        };
        changes.changed = changes
            .changed
            .into_iter()
            .filter_map(|e| self.outer_entry(e))
            .collect();
        changes.deleted = changes
            .deleted
            .iter()
            .filter_map(|p| self.outer_path(p))
            .collect();
        Ok(Some(changes))
    }
}

fn login(rt: &Runtime, auth: &ProtonAuthProvider) {
    let AuthStep::OpenBrowser {
        flow,
        url,
        poll_interval_s,
        expires_in_s,
    } = rt.block_on(auth.begin_login()).expect("begin_login")
    else {
        panic!("expected OpenBrowser");
    };
    println!("[login] Im Browser mit dem TESTKONTO bestätigen:\n    {url}");
    let _ = Command::new("xdg-open").arg(&url).spawn();
    let started = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(u64::from(poll_interval_s)));
        assert!(
            started.elapsed().as_secs() < u64::from(expires_in_s),
            "Zeitüberschreitung"
        );
        if let AuthStep::Done(_) = rt.block_on(auth.poll_login(&flow)).expect("poll_login") {
            break;
        }
    }
    println!("[login] OK nach {} s", started.elapsed().as_secs());
}

#[test]
#[ignore = "live: braucht PROTON_LIVE=1 und einen Browser-Login"]
fn contract_against_real_drive() {
    if std::env::var("PROTON_LIVE").as_deref() != Ok("1") {
        eprintln!("PROTON_LIVE=1 nicht gesetzt – übersprungen");
        return;
    }
    println!("This is a third-party application not officially supported by Proton.");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let auth = ProtonAuthProvider::new(
        ApiConfig::new(APP_VERSION),
        Arc::new(InMemoryKeyStore::new()),
    )
    .unwrap();
    login(&rt, &auth);
    let drive: Drive<_> = drive(&auth);

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let folder = format!("TasksContractTest-{stamp}");
    type Case<S> = (&'static str, fn(&S));
    let cases: [Case<Prefixed<'_, Drive<_>>>; 8] = [
        (
            "open_app_root_is_idempotent",
            contract::open_app_root_is_idempotent,
        ),
        ("create_then_read", contract::create_then_read),
        ("create_never_overwrites", contract::create_never_overwrites),
        (
            "read_missing_is_not_found",
            contract::read_missing_is_not_found,
        ),
        (
            "list_is_recursive_sorted_and_scoped",
            contract::list_is_recursive_sorted_and_scoped,
        ),
        (
            "replace_checks_expected_revision",
            contract::replace_checks_expected_revision,
        ),
        ("delete_is_idempotent", contract::delete_is_idempotent),
        (
            "changes_feed_if_supported",
            contract::changes_feed_if_supported,
        ),
    ];
    let mut failed = Vec::new();
    for (i, (name, case)) in cases.into_iter().enumerate() {
        let store = Prefixed {
            rt: &rt,
            inner: &drive,
            prefix: format!("{folder}/case-{i}"),
        };
        let started = Instant::now();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| case(&store)));
        let ms = started.elapsed().as_millis();
        match outcome {
            Ok(()) => println!("[contract] {name}: OK ({ms} ms)"),
            Err(_) => {
                println!("[contract] {name}: FEHLER ({ms} ms)");
                failed.push(name);
            }
        }
    }

    let cleanup = rt.block_on(drive.remove_folder(&folder));
    println!("[cleanup] Testordner gelöscht: {}", cleanup.is_ok());
    rt.block_on(auth.logout()).expect("logout");
    println!("[logout] OK");
    assert!(failed.is_empty(), "fehlgeschlagen: {failed:?}");
    println!("ERGEBNIS: OK");
}

fn drive(auth: &ProtonAuthProvider) -> Drive<impl Pgp> {
    Drive::new(
        DriveCrypto::new(proton_crypto::new_pgp_provider()),
        auth.session(),
    )
    .unwrap()
}
