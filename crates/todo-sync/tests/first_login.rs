//! Lokale Daten beim ersten Login (P8-01): Was vor der ersten Anmeldung angelegt wurde, geht
//! nicht verloren, und zwei Bestände werden ohne doppelte Inbox vereint. Echte SQLite-Dateien und
//! ein gemeinsames `FsRemoteStore`-Verzeichnis wie zwischen zwei Geräten.

mod common;

use std::sync::Arc;

use common::{assert_converged, Device, T0};
use futures::executor::block_on;
use tempfile::TempDir;
use todo_core::{
    api::dto::{
        PageRequestDto, TaskPatchDto, TaskQueryDto, TaskStatusDto, TaskSummaryDto, ViewDto,
    },
    domain::TaskListId,
    ports::RemoteStore,
};
use todo_remote_fs::FsRemoteStore;
use todo_storage_sqlite::SqliteTaskRepository;
use todo_sync::SyncConfig;

struct World {
    dir: TempDir,
    remote: Arc<dyn RemoteStore>,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let remote = Arc::new(FsRemoteStore::new(dir.path().join("remote")).unwrap());
        Self { dir, remote }
    }

    fn device(&self, name: &str, start_ms: i64, config: SyncConfig) -> Device {
        let db = self.dir.path().join(format!("{name}.sqlite3"));
        let repo = SqliteTaskRepository::open(db).unwrap();
        Device::configured(name, self.remote.clone(), Arc::new(repo), start_ms, config)
    }
}

fn inbox_view(device: &Device) -> Vec<TaskSummaryDto> {
    block_on(device.api.list_tasks(
        TaskQueryDto {
            view: ViewDto::Inbox,
            status: None,
            tag: None,
            priority: None,
            sort: None,
        },
        PageRequestDto {
            cursor: None,
            limit: 500,
        },
    ))
    .unwrap()
    .items
}

fn assert_one_inbox(device: &Device) {
    let inboxes: Vec<_> = device.lists().into_iter().filter(|l| l.is_inbox).collect();
    assert_eq!(inboxes.len(), 1, "genau eine Inbox: {inboxes:?}");
    assert_eq!(inboxes[0].id, TaskListId::INBOX.to_string());
}

/// Szenario 1: Tasks offline angelegt (nie angemeldet) → erster Sync → zweites Gerät sieht alles.
#[test]
fn offline_data_before_first_login_reaches_the_second_device() {
    let world = World::new();
    let a = world.device("a", T0, SyncConfig::default());
    let work = a.list("Arbeit");
    let milk = a.add("Milch");
    let report = a.add_to("Bericht", Some(&work.id));
    let done = a.add("Erledigt");
    a.update(
        &milk.id,
        TaskPatchDto {
            notes: Some("Hafer".into()),
            ..TaskPatchDto::default()
        },
    );
    block_on(a.api.complete_task(done.id.clone(), true)).unwrap();
    assert!(
        a.pending() > 0,
        "ohne Anmeldung sammelt sich alles in der Outbox"
    );

    // „Login": erster Sync dieses Geräts.
    a.sync();
    assert_eq!(a.pending(), 0);

    let b = world.device("b", T0 + 60_000, SyncConfig::default());
    b.sync();
    assert_eq!(b.titles(), ["Bericht", "Erledigt", "Milch"]);
    assert_eq!(b.task(&milk.id).unwrap().notes, "Hafer");
    assert_eq!(b.task(&done.id).unwrap().status, TaskStatusDto::Done);
    assert_eq!(b.task(&report.id).unwrap().list_id, work.id);
    assert_one_inbox(&b);
    assert_converged(&[&a, &b]);
}

/// Szenario 2: Beide Geräte hatten lokale Daten → nach dem ersten Login beider ist alles vereint,
/// die Inbox gibt es genau einmal, keine Task doppelt.
#[test]
fn local_data_of_two_devices_is_united_with_one_inbox() {
    let world = World::new();
    let a = world.device("a", T0, SyncConfig::default());
    let b = world.device("b", T0 + 5_000, SyncConfig::default());
    // Gleiche Positionen in beiden Inboxen (beide fangen bei derselben Sortierung an).
    let a_tasks: Vec<String> = ["A1", "A2", "A3"].iter().map(|t| a.add(t).id).collect();
    let b_tasks: Vec<String> = ["B1", "B2"].iter().map(|t| b.add(t).id).collect();
    let a_work = a.list("Arbeit");
    let b_work = b.list("Arbeit");
    a.add_to("A-Arbeit", Some(&a_work.id));
    b.add_to("B-Arbeit", Some(&b_work.id));

    a.sync();
    b.sync();
    a.sync();

    for device in [&a, &b] {
        assert_one_inbox(device);
        assert_eq!(
            device.titles(),
            ["A-Arbeit", "A1", "A2", "A3", "B-Arbeit", "B1", "B2"]
        );
        let inbox: Vec<String> = inbox_view(device).into_iter().map(|t| t.title).collect();
        assert_eq!(
            inbox.len(),
            5,
            "beide Inbox-Bestände in der einen Inbox: {inbox:?}"
        );
        // Gleichnamige Listen sind verschiedene Listen (eigene IDs) – nichts geht verloren.
        let work: Vec<_> = device
            .lists()
            .into_iter()
            .filter(|l| l.name == "Arbeit")
            .collect();
        assert_eq!(work.len(), 2);
    }
    assert_converged(&[&a, &b]);

    // Umsortieren zwischen Tasks mit gleichen Sort-Keys (A1/B1) klappt und konvergiert.
    let order: Vec<String> = inbox_view(&b).into_iter().map(|t| t.id).collect();
    b.move_after(&a_tasks[2], None, Some(&order[0]));
    b.move_after(&b_tasks[1], Some(&order[0]), Some(&order[1]));
    b.sync();
    a.sync();
    assert_converged(&[&a, &b]);
    assert_eq!(inbox_view(&a)[0].id, a_tasks[2]);
}

/// Szenario 2 mit Snapshot: Das zweite Gerät steigt über einen Snapshot ein und bringt eigene
/// lokale Daten mit; ein drittes, frisches Gerät sieht danach beide Bestände.
#[test]
fn device_with_local_data_joins_via_snapshot() {
    let world = World::new();
    let eager = SyncConfig {
        device_file_interval_ms: 0,
        snapshot_after_ops: 10,
        ..SyncConfig::default()
    };
    let a = world.device("a", T0, eager);
    for i in 0..20 {
        a.add(&format!("A{i:02}"));
    }
    assert!(a.sync().snapshot_written);

    let b = world.device("b", T0 + 5_000, SyncConfig::default());
    b.add("B lokal");
    let report = b.sync();
    assert!(report.snapshot_loaded, "{report:?}");
    a.sync();

    let c = world.device("c", T0 + 10_000, SyncConfig::default());
    c.sync();
    for device in [&a, &b, &c] {
        assert_one_inbox(device);
        assert_eq!(device.all().len(), 21);
        assert!(device.titles().contains(&"B lokal".to_owned()));
    }
    assert_converged(&[&a, &b, &c]);
}
