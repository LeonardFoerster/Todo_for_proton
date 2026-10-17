//! Snapshots und Kompaktierung (P5-11).

mod common;

use std::sync::Arc;

use common::{assert_converged, Device, T0};
use futures::executor::block_on;
use todo_core::{
    ports::{RemotePath, RemoteStore},
    testing::{InMemoryRemoteStore, InMemoryTaskRepository},
};
use todo_sync::{DeviceInfo, SyncConfig};

const DAY: i64 = 24 * 60 * 60 * 1000;

fn device(name: &str, remote: &Arc<InMemoryRemoteStore>, start: i64, config: SyncConfig) -> Device {
    Device::configured(
        name,
        remote.clone(),
        Arc::new(InMemoryTaskRepository::new()),
        start,
        config,
    )
}

/// Gerätedatei sofort aktualisieren, kleine Snapshot-Schwelle.
fn eager(snapshot_after_ops: u64) -> SyncConfig {
    SyncConfig {
        device_file_interval_ms: 0,
        snapshot_after_ops,
        ..SyncConfig::default()
    }
}

fn files(remote: &InMemoryRemoteStore, dir: &str) -> Vec<String> {
    remote
        .paths()
        .iter()
        .filter_map(|p| p.as_str().strip_prefix(dir).map(str::to_owned))
        .collect()
}

fn own_segments(remote: &InMemoryRemoteStore, device: &Device) -> Vec<String> {
    let id = block_on(device.api.repository().device_id()).unwrap();
    files(remote, &format!("Tasks/ops/{id}/"))
}

/// „Fertig wenn" von P5-11: 3 000 Ops → Snapshot → neues Gerät lädt Snapshot + Rest, Zustand
/// identisch.
#[test]
fn new_device_loads_snapshot_and_the_rest() {
    let remote = Arc::new(InMemoryRemoteStore::new());
    let a = device("A", &remote, T0, SyncConfig::default());
    let tasks: Vec<String> = (0..100).map(|i| a.add(&format!("Task {i}")).id).collect();
    for round in 0..29 {
        for (i, id) in tasks.iter().enumerate() {
            a.rename(id, &format!("Task {i} v{round}"));
        }
    }
    assert_eq!(a.pending(), 3_001);
    let report = a.sync();
    assert!(report.snapshot_written, "{report:?}");
    assert_eq!(files(&remote, "Tasks/snapshots/").len(), 1);
    assert_eq!(
        report.segments_deleted, 0,
        "erst der nächste Snapshot gibt frei"
    );

    // Nach dem Snapshot: weitere Änderungen als normales Segment.
    for id in &tasks[..10] {
        a.rename(id, "nach dem Snapshot");
    }
    a.sync();
    assert_eq!(own_segments(&remote, &a).len(), 8, "7 Segmente + 1 neues");

    let c = device("C", &remote, T0 + DAY, SyncConfig::default());
    let report = c.sync();
    assert!(report.snapshot_loaded);
    assert_eq!(report.received, 10, "nur das Segment nach dem Snapshot");
    assert_converged(&[&a, &c]);
    assert_eq!(c.all().len(), 100);
}

/// Eigene Segmente werden erst gelöscht, wenn alle aktiven Geräte sie gelesen haben; inaktive
/// Geräte (> 90 Tage) bremsen nicht.
#[test]
fn compaction_waits_for_active_devices() {
    let remote = Arc::new(InMemoryRemoteStore::new());
    let a = device("A", &remote, T0, eager(50));
    let b = device("B", &remote, T0, eager(1_000_000));
    a.sync();
    b.sync();
    let stale = DeviceInfo {
        device_id: todo_core::domain::DeviceId::new(),
        name: "altes Handy".into(),
        platform: "test".into(),
        last_seen: T0 - 100 * DAY,
        last_seq: 0,
        cursors: Default::default(),
    };
    block_on(remote.replace(
        &RemotePath::new(&format!("Tasks/devices/{}.json", stale.device_id)).unwrap(),
        serde_json::to_vec(&stale).unwrap(),
        None,
    ))
    .unwrap();

    let add_60 = |prefix: &str| {
        for i in 0..60 {
            a.add(&format!("{prefix}{i}"));
        }
        a.sync()
    };
    let first = add_60("a");
    assert!(first.snapshot_written);
    assert_eq!(first.segments_deleted, 0, "kein vorheriger Snapshot");

    let second = add_60("b");
    assert!(second.snapshot_written);
    assert_eq!(second.segments_deleted, 1, "nur 1-1: B hat 2-61 noch nicht");
    assert_eq!(own_segments(&remote, &a), ["2-61.seg", "62-121.seg"]);

    b.sync();
    let third = add_60("c");
    assert_eq!(
        third.segments_deleted, 2,
        "B hat alles bis 121, inaktives Gerät zählt nicht"
    );
    assert_eq!(own_segments(&remote, &a), ["122-181.seg"]);

    b.sync();
    let c = device("C", &remote, T0, SyncConfig::default());
    let report = c.sync();
    assert!(report.snapshot_loaded);
    assert_converged(&[&a, &b, &c]);
    assert_eq!(c.all().len(), 180);
}

/// Ein Gerät, das so lange weg war, dass seine Segmente kompaktiert wurden, holt per Snapshot auf.
#[test]
fn device_behind_compaction_catches_up_via_snapshot() {
    let remote = Arc::new(InMemoryRemoteStore::new());
    // B's Uhr liegt 100 Tage zurück → A hält B für inaktiv.
    let b = device("B", &remote, T0 - 100 * DAY, SyncConfig::default());
    let a = device("A", &remote, T0, eager(10));
    a.sync();
    b.add("von B");
    b.sync();
    for round in 0..2 {
        for i in 0..20 {
            a.add(&format!("a{round}-{i}"));
        }
        assert!(a.sync().snapshot_written);
    }
    assert_eq!(own_segments(&remote, &a), ["22-41.seg"], "B bremst nicht");

    let report = b.sync();
    assert!(report.snapshot_loaded, "{report:?}");
    assert_converged(&[&a, &b]);
    assert_eq!(b.all().len(), 41);
}

/// Pro Gerät bleiben höchstens zwei eigene Snapshots liegen.
#[test]
fn old_own_snapshots_are_pruned() {
    let remote = Arc::new(InMemoryRemoteStore::new());
    let a = device("A", &remote, T0, eager(1));
    for round in 0..4 {
        a.add(&format!("r{round}a"));
        a.add(&format!("r{round}b"));
        a.clock.advance(1_000);
        assert!(a.sync().snapshot_written);
    }
    assert_eq!(files(&remote, "Tasks/snapshots/").len(), 2);
}

/// Ein unlesbarer neuester Snapshot wird übersprungen, der ältere plus die noch vorhandenen
/// Segmente genutzt.
#[test]
fn unreadable_newest_snapshot_falls_back_to_older() {
    let remote = Arc::new(InMemoryRemoteStore::new());
    let a = device("A", &remote, T0, eager(1));
    a.add("eins");
    a.add("zwei");
    a.sync();
    a.clock.advance(1_000);
    a.add("drei");
    a.add("vier");
    a.sync();
    let mut snaps = files(&remote, "Tasks/snapshots/");
    snaps.sort();
    let newest = RemotePath::new(&format!("Tasks/snapshots/{}", snaps[1])).unwrap();
    remote.tamper(&newest, |bytes| {
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
    });
    let c = device("C", &remote, T0, SyncConfig::default());
    let report = c.sync();
    assert!(report.snapshot_loaded);
    assert_eq!(report.skipped_segments, 1);
    assert_converged(&[&a, &c]);
    assert_eq!(c.all().len(), 4);
}
