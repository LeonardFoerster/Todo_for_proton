//! `ProtonDriveRemoteStore` gegen den Fake-Server: Contract-Suite des Ports plus Proton-Fälle
//! (Entwürfe, verlorene Antworten, Revisionen, Events, Verschlüsselung).

use std::{sync::Arc, time::Duration};

use serde_json::json;
use todo_core::{
    ports::{
        KeyStore, RemoteChanges, RemoteCursor, RemoteDir, RemoteEntry, RemoteListing, RemotePath,
        RemoteStore, Revision, SecretBytes,
    },
    testing::InMemoryKeyStore,
    CoreError, CoreResult,
};
use tokio::runtime::Runtime;
use wiremock::{matchers::any, Mock, MockServer};

use super::{
    crypto::DriveCrypto,
    fake::{FakeDrive, Fault, KEY_PASSWORD},
    store::{Drive, Pgp},
};
use crate::{
    api_client::{tests::APP_VERSION, ApiClient, ApiConfig},
    auth::session::{Session, STORE_KEY},
};

/// Laufzeit, Server und Fake für einen Test.
struct Env {
    rt: Runtime,
    server: MockServer,
    fake: FakeDrive,
}

fn env() -> Env {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let fake = FakeDrive::new();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        fake.set_origin(server.uri());
        Mock::given(any())
            .respond_with(fake.clone())
            .mount(&server)
            .await;
        server
    });
    Env { rt, server, fake }
}

impl Env {
    /// Ein „Gerät": eigener Store (eigene Caches, eigene Sitzung) auf demselben Konto.
    fn device(&self, block_size: usize) -> Drive<impl Pgp> {
        let mut config = ApiConfig::new(APP_VERSION);
        config.base_url = self.server.uri();
        config.timeout = Duration::from_secs(10);
        let store = Arc::new(InMemoryKeyStore::new());
        let stored = json!({
            "version": 1, "uid": "uid-1", "refresh_token": "ref-0", "key_password": KEY_PASSWORD,
            "account_id": "user-1", "account_label": "fake@example.org",
        });
        self.rt
            .block_on(store.put(STORE_KEY, SecretBytes::new(stored.to_string().into_bytes())))
            .unwrap();
        let session = Arc::new(Session::new(ApiClient::new(config).unwrap(), store));
        Drive::new(DriveCrypto::new(proton_crypto::new_pgp_provider()), session)
            .unwrap()
            .with_block_size(block_size)
    }

    fn run<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.rt.block_on(future)
    }
}

/// Für die synchrone Contract-Suite: jeder Aufruf läuft auf der tokio-Runtime des Tests.
struct Blocking<S> {
    env: Env,
    store: S,
}

fn blocking_store() -> Blocking<Drive<impl Pgp>> {
    let env = env();
    let store = env.device(64);
    Blocking { env, store }
}

#[async_trait::async_trait]
impl<S: RemoteStore> RemoteStore for Blocking<S> {
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir> {
        self.env.run(self.store.open_app_root(name))
    }

    async fn list(
        &self,
        dir: &RemotePath,
        since: Option<&RemoteCursor>,
    ) -> CoreResult<RemoteListing> {
        self.env.run(self.store.list(dir, since))
    }

    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>> {
        self.env.run(self.store.read(path))
    }

    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry> {
        self.env.run(self.store.create(path, bytes))
    }

    async fn replace(
        &self,
        path: &RemotePath,
        bytes: Vec<u8>,
        expected: Option<&Revision>,
    ) -> CoreResult<RemoteEntry> {
        self.env.run(self.store.replace(path, bytes, expected))
    }

    async fn delete(&self, path: &RemotePath) -> CoreResult<()> {
        self.env.run(self.store.delete(path))
    }

    async fn changes_since(
        &self,
        cursor: Option<&RemoteCursor>,
    ) -> CoreResult<Option<RemoteChanges>> {
        self.env.run(self.store.changes_since(cursor))
    }
}

mod contract {
    todo_core::remote_store_contract_tests!(super::blocking_store());
}

fn path(rel: &str) -> RemotePath {
    RemotePath::new(&format!("Tasks/{rel}")).unwrap()
}

#[test]
fn multi_block_roundtrip_and_server_sees_no_plaintext() {
    let env = env();
    let drive = env.device(1024);
    let content: Vec<u8> = b"GEHEIMER-INHALT "
        .iter()
        .copied()
        .cycle()
        .take(5000)
        .collect();
    env.run(async {
        drive.open_app_root("Tasks").await.unwrap();
        let entry = drive
            .create(&path("ops/dev-a/1-5.seg"), content.clone())
            .await
            .unwrap();
        assert_eq!(entry.size, 5000);
        assert_eq!(
            drive.read(&path("ops/dev-a/1-5.seg")).await.unwrap(),
            content
        );
    });
    assert_eq!(
        env.fake.count_calls("POST", "/storage/"),
        5,
        "5 Blöcke à 1 KiB"
    );
    let stored = env.fake.stored_text();
    for plain in ["GEHEIMER-INHALT", "Tasks", "dev-a", "1-5.seg"] {
        assert!(!stored.contains(plain), "{plain} im Klartext beim Server");
    }
    // Ein zweites Gerät liest dieselbe Datei.
    let other = env.device(1024);
    assert_eq!(
        env.run(other.read(&path("ops/dev-a/1-5.seg"))).unwrap(),
        content
    );
}

#[test]
fn failed_commit_removes_draft() {
    let env = env();
    let drive = env.device(64);
    env.run(drive.open_app_root("Tasks")).unwrap();
    env.fake.fail_once(
        "PUT",
        "/revisions/",
        Fault::Respond(500, json!({ "Code": 0 })),
    );
    let result = env.run(drive.create(&path("a.seg"), vec![1, 2, 3]));
    assert!(matches!(result, Err(CoreError::Network(_))), "{result:?}");
    assert_eq!(env.fake.link_states().0, 0, "kein Entwurf bleibt liegen");
    env.run(drive.create(&path("a.seg"), vec![1, 2, 3]))
        .unwrap();
    assert_eq!(env.run(drive.read(&path("a.seg"))).unwrap(), [1, 2, 3]);
}

#[test]
fn stale_draft_from_crash_is_replaced() {
    let env = env();
    let crashed = env.device(64);
    env.run(crashed.open_app_root("Tasks")).unwrap();
    // Absturz nach dem Anlegen: Commit scheitert und das Aufräumen auch.
    env.fake.fail_once(
        "PUT",
        "/revisions/",
        Fault::Respond(500, json!({ "Code": 0 })),
    );
    env.fake.fail_once(
        "POST",
        "delete_multiple",
        Fault::Respond(500, json!({ "Code": 0 })),
    );
    assert!(env.run(crashed.create(&path("a.seg"), vec![1])).is_err());
    assert_eq!(env.fake.link_states().0, 1, "Entwurf liegt noch");

    let restarted = env.device(64);
    env.run(restarted.create(&path("a.seg"), vec![2])).unwrap();
    assert_eq!(env.run(restarted.read(&path("a.seg"))).unwrap(), [2]);
    assert_eq!(
        env.fake.link_states(),
        (0, 2, 0),
        "Tasks und a.seg – kein Entwurf"
    );
}

#[test]
fn lost_commit_response_then_retry_is_conflict_with_same_content() {
    let env = env();
    let drive = env.device(64);
    env.run(drive.open_app_root("Tasks")).unwrap();
    env.fake
        .fail_once("PUT", "/revisions/", Fault::AfterApply(502));
    assert!(env.run(drive.create(&path("s.seg"), vec![7; 10])).is_err());
    // Der Engine-Retry sieht Conflict und liest den Inhalt zum Vergleich.
    let retry = env.run(drive.create(&path("s.seg"), vec![7; 10]));
    assert!(matches!(retry, Err(CoreError::Conflict(_))), "{retry:?}");
    assert_eq!(env.run(drive.read(&path("s.seg"))).unwrap(), vec![7; 10]);
}

#[test]
fn replace_keeps_one_revision_and_detects_stale_writers() {
    let env = env();
    let drive = env.device(64);
    let other = env.device(64);
    env.run(async {
        drive.open_app_root("Tasks").await.unwrap();
        let p = path("devices/a.json");
        let mut last = drive.replace(&p, b"{}".to_vec(), None).await.unwrap();
        for i in 0..3 {
            last = drive
                .replace(
                    &p,
                    format!("{{\"n\":{i}}}").into_bytes(),
                    Some(&last.revision),
                )
                .await
                .unwrap();
        }
        assert_eq!(drive.read(&p).await.unwrap(), b"{\"n\":2}");
        // Ein zweites Gerät mit veralteter Revision.
        let stale = other
            .replace(&p, b"x".to_vec(), Some(&Revision("rev-1".into())))
            .await;
        assert!(matches!(stale, Err(CoreError::Conflict(_))), "{stale:?}");
    });
    assert_eq!(
        env.fake.committed_revisions(),
        1,
        "alte Revisionen werden gelöscht"
    );
}

#[test]
fn replace_recovers_from_own_stale_draft_revision() {
    let env = env();
    let drive = env.device(64);
    env.run(async {
        drive.open_app_root("Tasks").await.unwrap();
        let p = path("devices/a.json");
        let first = drive.replace(&p, b"1".to_vec(), None).await.unwrap();
        env.fake.fail_once(
            "PUT",
            "/revisions/",
            Fault::Respond(500, json!({ "Code": 0 })),
        );
        env.fake.fail_once(
            "DELETE",
            "/revisions/",
            Fault::Respond(500, json!({ "Code": 0 })),
        );
        assert!(drive
            .replace(&p, b"2".to_vec(), Some(&first.revision))
            .await
            .is_err());
        drive
            .replace(&p, b"3".to_vec(), Some(&first.revision))
            .await
            .unwrap();
        assert_eq!(drive.read(&p).await.unwrap(), b"3");
    });
}

#[test]
fn changes_feed_reports_other_devices_files_with_paging() {
    let env = env();
    let a = env.device(64);
    let b = env.device(64);
    env.run(async {
        a.open_app_root("Tasks").await.unwrap();
        b.open_app_root("Tasks").await.unwrap();
        let start = b.changes_since(None).await.unwrap().unwrap();
        // Mehr Events als eine Seite (Fake: 3 je Seite), neuer Geräteordner inklusive.
        for i in 1..=4 {
            a.create(&path(&format!("ops/dev-a/{i}-{i}.seg")), vec![i])
                .await
                .unwrap();
        }
        env.fake.touch_root_event();
        let changes = b.changes_since(Some(&start.cursor)).await.unwrap().unwrap();
        assert!(!changes.refresh);
        let mut seen: Vec<&str> = changes.changed.iter().map(|e| e.path.as_str()).collect();
        seen.sort_unstable();
        assert_eq!(
            seen,
            [
                "Tasks/ops/dev-a/1-1.seg",
                "Tasks/ops/dev-a/2-2.seg",
                "Tasks/ops/dev-a/3-3.seg",
                "Tasks/ops/dev-a/4-4.seg"
            ]
        );
        // Das neue Segment ist sofort lesbar (Ordner-Cache wurde verworfen).
        assert_eq!(b.read(&path("ops/dev-a/4-4.seg")).await.unwrap(), [4]);

        a.delete(&path("ops/dev-a/1-1.seg")).await.unwrap();
        let after = b
            .changes_since(Some(&changes.cursor))
            .await
            .unwrap()
            .unwrap();
        assert!(after.changed.is_empty(), "{:?}", after.changed);
        assert_eq!(after.deleted, [path("ops/dev-a/1-1.seg")]);
        let idle = b.changes_since(Some(&after.cursor)).await.unwrap().unwrap();
        assert!(idle.changed.is_empty() && idle.deleted.is_empty());
        assert_eq!(idle.cursor, after.cursor);
    });
}

#[test]
fn unknown_event_cursor_requests_refresh() {
    let env = env();
    let drive = env.device(64);
    let changes = env
        .run(drive.changes_since(Some(&RemoteCursor("gone".into()))))
        .unwrap()
        .unwrap();
    assert!(changes.refresh);
    assert!(changes.changed.is_empty());
}

#[test]
fn concurrent_folder_creation_by_two_devices_converges() {
    let env = env();
    let a = env.device(64);
    let b = env.device(64);
    env.run(async {
        let (ra, rb) = futures::join!(a.open_app_root("Tasks"), b.open_app_root("Tasks"));
        ra.unwrap();
        rb.unwrap();
        a.create(&path("ops/x/1-1.seg"), vec![1]).await.unwrap();
        b.create(&path("ops/x/2-2.seg"), vec![2]).await.unwrap();
        let listing = a.list(&path("ops"), None).await.unwrap();
        assert_eq!(listing.entries.len(), 2);
    });
    let (_, active, _) = env.fake.link_states();
    assert_eq!(active, 5, "genau ein Tasks-, ops- und x-Ordner");
}

#[test]
fn delete_purges_from_trash() {
    let env = env();
    let drive = env.device(64);
    env.run(async {
        drive.open_app_root("Tasks").await.unwrap();
        drive.create(&path("a.seg"), vec![1]).await.unwrap();
        drive.delete(&path("a.seg")).await.unwrap();
    });
    assert_eq!(env.fake.link_states().2, 0, "nichts bleibt im Papierkorb");
}

#[test]
fn rate_limit_is_propagated() {
    let env = env();
    let drive = env.device(64);
    env.run(drive.open_app_root("Tasks")).unwrap();
    env.fake.fail_once(
        "GET",
        "/children",
        Fault::Respond(429, json!({ "Code": 2028 })),
    );
    let result = env.run(drive.list(&path("ops"), None));
    assert_eq!(result, Err(CoreError::RateLimited { retry_after_s: 10 }));
}

#[test]
fn wrong_key_password_is_rejected() {
    let env = env();
    let mut config = ApiConfig::new(APP_VERSION);
    config.base_url = env.server.uri();
    let store = Arc::new(InMemoryKeyStore::new());
    let stored = json!({
        "version": 1, "uid": "uid-1", "refresh_token": "r", "key_password": "falsch",
        "account_id": "u", "account_label": "l",
    });
    env.run(store.put(STORE_KEY, SecretBytes::new(stored.to_string().into_bytes())))
        .unwrap();
    let session = Arc::new(Session::new(ApiClient::new(config).unwrap(), store));
    let drive = Drive::new(DriveCrypto::new(proton_crypto::new_pgp_provider()), session).unwrap();
    assert!(matches!(
        env.run(drive.open_app_root("Tasks")),
        Err(CoreError::RemoteRejected(_))
    ));
}
