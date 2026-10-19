//! Integrität gegenüber dem Backend (Audit run-1): Ablageort und Header, Schlüsseldatei,
//! Snapshot-Dateinamen. Das Backend darf Dateien kopieren, umbenennen und löschen, kennt aber
//! den Sync-Schlüssel nicht.

mod common;

use std::sync::Arc;

use common::Device;
use futures::executor::block_on;
use todo_core::{
    ports::{KeyStore, RemotePath, RemoteStore, SecretBytes},
    testing::InMemoryRemoteStore,
    CoreError,
};
use todo_sync::SyncConfig;

fn remote() -> Arc<InMemoryRemoteStore> {
    Arc::new(InMemoryRemoteStore::new())
}

fn path(s: &str) -> RemotePath {
    RemotePath::new(s).unwrap()
}

fn files_under(remote: &InMemoryRemoteStore, prefix: &str) -> Vec<RemotePath> {
    remote
        .paths()
        .into_iter()
        .filter(|p| p.as_str().starts_with(prefix))
        .collect()
}

fn device_id(device: &Device) -> String {
    block_on(device.api.repository().device_id())
        .unwrap()
        .to_string()
}

/// Ein Segment von A unter `ops/<B>/` darf B's Cursor nicht vorrücken; B's echte Ops kommen an,
/// sobald das Backend sie wieder zeigt.
#[test]
fn foreign_segment_under_other_device_does_not_advance_its_cursor() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    for title in ["a1", "a2", "a3"] {
        a.add(title);
        a.sync();
    }
    let b = Device::in_memory("B", remote.clone());
    b.add("b-secret");
    b.sync();

    let (a_id, b_id) = (device_id(&a), device_id(&b));
    let a_segments = files_under(&remote, &format!("Tasks/ops/{a_id}/"));
    let newest_a = a_segments.last().unwrap().clone();
    let b_segments = files_under(&remote, &format!("Tasks/ops/{b_id}/"));
    let saved: Vec<(RemotePath, Vec<u8>)> = b_segments
        .iter()
        .map(|p| (p.clone(), block_on(remote.read(p)).unwrap()))
        .collect();

    // Backend: B's Segmente verstecken, A's neuestes Segment unter B's Ordner zeigen.
    for (p, _) in &saved {
        block_on(remote.delete(p)).unwrap();
    }
    let fake = path(&format!("Tasks/ops/{b_id}/{}", newest_a.file_name()));
    block_on(remote.create(&fake, block_on(remote.read(&newest_a)).unwrap())).unwrap();

    let v = Device::in_memory("V", remote.clone());
    let report = v.sync();
    assert!(report.skipped_segments >= 1, "{report:?}");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("does not match header")),
        "{:?}",
        report.warnings
    );

    // Backend wieder ehrlich.
    block_on(remote.delete(&fake)).unwrap();
    for (p, bytes) in saved {
        block_on(remote.create(&p, bytes)).unwrap();
    }
    v.sync();
    assert!(
        v.titles().iter().any(|t| t == "b-secret"),
        "{:?}",
        v.titles()
    );
}

/// Ein lokal gemerkter Schlüssel, der nicht zur Schlüsseldatei passt (z. B. anderes Konto),
/// wird nicht still weiterverwendet.
#[test]
fn local_key_that_differs_from_key_file_fails_closed() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    a.add("eins");
    a.sync();

    let b = Device::in_memory("B", remote.clone());
    let key_name = SyncConfig::default().key_name();
    block_on(b.keys.put(&key_name, SecretBytes::new(vec![9; 32]))).unwrap();
    let err = block_on(b.engine.sync_once()).unwrap_err();
    assert!(matches!(err, CoreError::Validation(_)), "{err:?}");
    assert!(b.titles().is_empty());
}

/// Fehlt die Schlüsseldatei in einem Ordner mit Daten, erzeugt ein neues Gerät keinen eigenen
/// Schlüssel (das spaltete es still von den anderen ab).
#[test]
fn missing_key_file_in_used_folder_is_not_regenerated() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    a.add("eins");
    a.sync();
    let key_file = path("Tasks/keys/sync-key.enc");
    block_on(remote.delete(&key_file)).unwrap();

    let b = Device::in_memory("B", remote.clone());
    let err = block_on(b.engine.sync_once()).unwrap_err();
    assert!(matches!(err, CoreError::Validation(_)), "{err:?}");
    assert!(!remote.paths().contains(&key_file), "kein neuer Schlüssel");

    // Das Gerät mit dem bekannten Schlüssel legt die Datei beim nächsten Start wieder an.
    let restarted = todo_sync::SyncEngine::new(
        a.api.repository(),
        remote.clone(),
        Arc::new(todo_crypto::XChaCha20Poly1305Cipher::new()),
        a.keys.clone(),
        a.clock.clone(),
        SyncConfig::default(),
    );
    block_on(restarted.sync_once()).unwrap();
    assert!(remote.paths().contains(&key_file));
    b.sync();
    assert_eq!(b.titles(), ["eins"]);
}

/// Die Schlüsseldatei muss einen für die Cipher gültigen Schlüssel enthalten.
#[test]
fn key_file_with_wrong_length_is_rejected() {
    let remote = remote();
    block_on(remote.open_app_root("Tasks")).unwrap();
    let a = Device::in_memory("A", remote.clone());
    a.sync();
    let key_file = path("Tasks/keys/sync-key.enc");
    block_on(remote.delete(&key_file)).unwrap();
    block_on(remote.create(&key_file, b"TKEY\x01short".to_vec())).unwrap();

    let b = Device::in_memory("B", remote.clone());
    let err = block_on(b.engine.sync_once()).unwrap_err();
    assert!(matches!(err, CoreError::Validation(_)), "{err:?}");
    let key_name = SyncConfig::default().key_name();
    assert!(block_on(b.keys.get(&key_name)).unwrap().is_none());
}

/// Ein umbenannter Snapshot (Dateiname ≠ authentisierter Header) wird nicht geladen.
#[test]
fn renamed_snapshot_is_not_loaded() {
    let remote = remote();
    let config = SyncConfig {
        snapshot_after_ops: 0,
        ..SyncConfig::default()
    };
    let a = Device::configured(
        "A",
        remote.clone(),
        Arc::new(todo_core::testing::InMemoryTaskRepository::new()),
        common::T0,
        config,
    );
    a.add("eins");
    let report = a.sync();
    assert!(report.snapshot_written, "{report:?}");
    let snapshot = files_under(&remote, "Tasks/snapshots/").pop().unwrap();
    let bytes = block_on(remote.read(&snapshot)).unwrap();
    // Gleiches Gerät, aber „neuere" HLC im Namen (höchstes Byte der Wanduhr gesetzt).
    let name = snapshot.file_name();
    let renamed = path(&format!("Tasks/snapshots/7f{}", &name[2..]));
    block_on(remote.delete(&snapshot)).unwrap();
    block_on(remote.create(&renamed, bytes)).unwrap();

    let b = Device::in_memory("B", remote.clone());
    let report = b.sync();
    assert!(!report.snapshot_loaded, "{report:?}");
    assert_eq!(b.titles(), ["eins"], "Segmente reichen trotzdem");
}
