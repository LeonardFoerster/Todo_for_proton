//! Integrationstests der Sync-Engine (P5-08): Bootstrap, Push, Pull gegen
//! `InMemoryRemoteStore` (mit Fehler-Injektion) und mit SQLite-Repositories.

mod common;

use std::{future::Future, pin::pin, sync::Arc, task::Context};

use common::Device;
use futures::{executor::block_on, task::noop_waker_ref};
use todo_core::{
    ports::{KeyStore, RemotePath, RemoteStore},
    testing::{Fault, InMemoryRemoteStore, RemoteCall},
    CoreError,
};
use todo_sync::DeviceInfo;

fn remote() -> Arc<InMemoryRemoteStore> {
    Arc::new(InMemoryRemoteStore::new())
}

fn path(s: &str) -> RemotePath {
    RemotePath::new(s).unwrap()
}

fn segments(remote: &InMemoryRemoteStore) -> Vec<String> {
    remote
        .paths()
        .into_iter()
        .filter(|p| p.as_str().starts_with("Tasks/ops/"))
        .map(|p| p.to_string())
        .collect()
}

/// „Fertig wenn" von P5-08: Gerät A erzeugt Tasks → Sync → Gerät B (frische DB) sieht sie.
#[test]
fn fresh_device_sees_tasks_of_first_device() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    let work = a.list("Arbeit");
    a.add("Milch");
    a.add_to("Bericht", Some(&work.id));
    let report = a.sync();
    assert_eq!(report.pushed, 4, "Inbox, Liste, zwei Tasks");
    assert_eq!(a.pending(), 0);

    let b = Device::in_memory("B", remote.clone());
    let report = b.sync();
    assert!(report.received >= 4, "{report:?}");
    assert_eq!(b.titles(), ["Bericht", "Milch"]);
    let lists = b.lists();
    assert_eq!(lists.iter().filter(|l| l.is_inbox).count(), 1, "eine Inbox");
    assert!(lists.iter().any(|l| l.name == "Arbeit"));

    // Und zurück: B ändert, A sieht es.
    b.add("von B");
    b.sync();
    a.sync();
    assert_eq!(a.titles(), ["Bericht", "Milch", "von B"]);
}

#[test]
fn works_with_sqlite_repositories() {
    let dir = tempfile::tempdir().unwrap();
    let remote = remote();
    let a = Device::sqlite("a", remote.clone(), dir.path());
    a.add("Milch");
    a.sync();
    let b = Device::sqlite("b", remote.clone(), dir.path());
    b.sync();
    assert_eq!(b.titles(), ["Milch"]);
}

#[test]
fn bootstrap_creates_layout_and_shares_the_key() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    a.sync();
    let b = Device::in_memory("B", remote.clone());
    b.sync();
    let paths: Vec<String> = remote.paths().iter().map(ToString::to_string).collect();
    assert!(paths.contains(&"Tasks/manifest.json".to_owned()));
    assert!(paths.contains(&"Tasks/keys/sync-key.enc".to_owned()));
    assert_eq!(
        paths
            .iter()
            .filter(|p| p.starts_with("Tasks/devices/"))
            .count(),
        2
    );
    let key_a = block_on(a.keys.get("sync-key:Tasks")).unwrap().unwrap();
    let key_b = block_on(b.keys.get("sync-key:Tasks")).unwrap().unwrap();
    assert_eq!(key_a, key_b, "zweites Gerät lädt den Schlüssel");
}

#[test]
fn device_file_lists_name_and_cursors() {
    let remote = remote();
    let a = Device::in_memory("Laptop", remote.clone());
    a.add("x");
    a.sync();
    let b = Device::in_memory("Handy", remote.clone());
    b.sync();
    let device_b = block_on(b.api.repository().device_id()).unwrap();
    let device_a = block_on(a.api.repository().device_id()).unwrap();
    let json = block_on(remote.read(&path(&format!("Tasks/devices/{device_b}.json")))).unwrap();
    let info: DeviceInfo = serde_json::from_slice(&json).unwrap();
    assert_eq!(info.name, "Handy");
    assert_eq!(info.last_seq, 1, "B hat seine Inbox hochgeladen");
    assert_eq!(info.cursors.get(&device_a.to_string()), Some(&2));
}

/// Absturz nach dem Upload (Antwort verloren): Der nächste Push findet das Segment, prüft den
/// Inhalt und markiert die Ops als hochgeladen – kein zweites Segment, keine Duplikate.
#[test]
fn lost_upload_response_is_resolved_idempotently() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    a.add("Milch");
    remote.inject_for_path(RemoteCall::Create, "Tasks/ops/", Fault::LostResponse, 1);
    let err = block_on(a.engine.sync_once()).unwrap_err();
    assert!(err.is_retryable(), "{err:?}");
    assert_eq!(a.pending(), 2, "Outbox unverändert");
    assert_eq!(segments(&remote).len(), 1, "Upload ist trotzdem angekommen");

    let report = a.sync();
    assert_eq!(report.pushed, 2);
    assert_eq!(a.pending(), 0);
    assert_eq!(segments(&remote).len(), 1, "kein zweites Segment");

    let b = Device::in_memory("B", remote.clone());
    b.sync();
    assert_eq!(b.titles(), ["Milch"]);
}

/// Ein abgebrochener, unlesbarer eigener Upload wird ersetzt.
#[test]
fn unreadable_own_segment_is_replaced() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    a.add("Milch");
    remote.inject_for_path(RemoteCall::Create, "Tasks/ops/", Fault::LostResponse, 1);
    let _ = block_on(a.engine.sync_once());
    let segment = path(&segments(&remote)[0]);
    remote.tamper(&segment, |bytes| bytes.truncate(bytes.len() / 2));
    a.sync();
    let b = Device::in_memory("B", remote.clone());
    b.sync();
    assert_eq!(b.titles(), ["Milch"]);
}

#[test]
fn network_errors_keep_the_outbox_for_the_next_attempt() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    a.sync();
    a.add("Milch");
    remote.inject(Some(RemoteCall::Create), Fault::Timeout, 1);
    assert!(matches!(
        block_on(a.engine.sync_once()),
        Err(CoreError::Network(_))
    ));
    remote.inject(None, Fault::RateLimited { retry_after_s: 30 }, 1);
    assert!(matches!(
        block_on(a.engine.sync_once()),
        Err(CoreError::RateLimited { retry_after_s: 30 })
    ));
    assert_eq!(a.pending(), 1);
    assert_eq!(a.sync().pushed, 1);
}

/// Beschädigte Segmente werden übersprungen und gemeldet, der Rest kommt an; der Cursor bleibt
/// vor dem kaputten Segment stehen.
#[test]
fn corrupted_segment_is_skipped_and_reported() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    a.add("eins");
    a.sync();
    a.add("zwei");
    a.sync();
    let first = path(&segments(&remote)[0]);
    remote.tamper(&first, |bytes| {
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
    });

    let b = Device::in_memory("B", remote.clone());
    let report = b.sync();
    assert_eq!(report.skipped_segments, 1, "{report:?}");
    assert!(!report.newer_format);
    assert!(
        report.warnings[0].contains("corrupt"),
        "{:?}",
        report.warnings
    );
    assert_eq!(b.titles(), ["zwei"], "intaktes Segment angewendet");
    let device_a = block_on(a.api.repository().device_id()).unwrap();
    assert_eq!(
        block_on(b.api.repository().sync_cursor(device_a)).unwrap(),
        None,
        "Cursor nicht über das kaputte Segment hinweg"
    );
}

/// Ein App-Ordner einer neueren Version wird weder gelesen noch beschrieben.
#[test]
fn newer_folder_layout_pauses_sync() {
    let remote = remote();
    block_on(remote.open_app_root("Tasks")).unwrap();
    block_on(remote.create(
        &path("Tasks/manifest.json"),
        br#"{"format_version": 99, "app_id": "todo-sync", "created_hlc": "x"}"#.to_vec(),
    ))
    .unwrap();
    let a = Device::in_memory("A", remote.clone());
    a.add("lokal");
    let report = a.sync();
    assert!(report.newer_format);
    assert_eq!(report.pushed, 0);
    assert_eq!(remote.paths().len(), 1, "nichts geschrieben");
    assert_eq!(a.pending(), 2, "Ops bleiben lokal");
}

/// Mit Änderungs-Feed wird im Ruhezustand nichts gelistet; geänderte Geräteordner gezielt.
#[test]
fn change_feed_avoids_listing() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    let b = Device::in_memory("B", remote.clone());
    a.sync();
    b.sync();
    b.sync();
    let lists = remote.calls(RemoteCall::List);
    b.sync();
    assert_eq!(
        remote.calls(RemoteCall::List),
        lists,
        "keine Änderung → kein Listing"
    );

    a.add("neu");
    a.sync();
    let lists = remote.calls(RemoteCall::List);
    b.sync();
    assert_eq!(remote.calls(RemoteCall::List), lists + 1, "nur A's Ordner");
    assert_eq!(b.titles(), ["neu"]);
}

#[test]
fn works_without_change_feed() {
    let remote = remote();
    remote.disable_feed();
    let a = Device::in_memory("A", remote.clone());
    let b = Device::in_memory("B", remote.clone());
    a.add("x");
    a.sync();
    b.sync();
    assert_eq!(b.titles(), ["x"]);
}

#[test]
fn feed_refresh_falls_back_to_listing() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    let b = Device::in_memory("B", remote.clone());
    a.sync();
    b.sync();
    a.add("nach Lücke");
    a.sync();
    remote.truncate_feed();
    b.sync();
    assert_eq!(b.titles(), ["nach Lücke"]);
}

#[test]
fn large_outbox_is_split_into_segments_of_500() {
    let remote = remote();
    let a = Device::in_memory("A", remote.clone());
    for i in 0..1_100 {
        a.add(&format!("Task {i}"));
    }
    assert_eq!(a.sync().pushed, 1_101);
    let names: Vec<String> = segments(&remote)
        .iter()
        .map(|p| p.rsplit('/').next().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["1-500.seg", "1001-1101.seg", "501-1000.seg"]);
    let b = Device::in_memory("B", remote.clone());
    b.sync();
    assert_eq!(b.all().len(), 500, "erste Seite");
}

#[test]
fn parallel_runs_are_rejected() {
    let remote = remote();
    remote.set_latency(3);
    let a = Device::in_memory("A", remote.clone());
    let mut first = pin!(a.engine.sync_once());
    let mut cx = Context::from_waker(noop_waker_ref());
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(matches!(
        block_on(a.engine.sync_once()),
        Err(CoreError::Conflict(_))
    ));
    assert!(block_on(first).is_ok());
}
