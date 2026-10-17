//! Mehrgeräte-Szenarien (P5-09): echte Offline-Konflikte über ein gemeinsames
//! `FsRemoteStore`-Verzeichnis, jedes Gerät mit eigener SQLite-Datei.

mod common;

use std::{path::Path, sync::Arc};

use common::{assert_converged, Device, Flaky, T0};
use futures::executor::block_on;
use tempfile::TempDir;
use todo_core::{
    api::dto::{DueDto, TaskPatchDto},
    ports::RemoteStore,
};
use todo_remote_fs::FsRemoteStore;
use todo_storage_sqlite::SqliteTaskRepository;

const TWO_HOURS: i64 = 2 * 60 * 60 * 1000;

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

    fn device(&self, name: &str) -> Device {
        Device::sqlite(name, self.remote.clone(), self.dir.path())
    }

    /// Gerät mit eigener Uhr (Start bei `start_ms`) bzw. eigenem Backend-Zugang.
    fn device_with(&self, name: &str, remote: Arc<dyn RemoteStore>, start_ms: i64) -> Device {
        let repo = SqliteTaskRepository::open(db(self.dir.path(), name)).unwrap();
        Device::with_repo(name, remote, Arc::new(repo), start_ms)
    }
}

fn db(dir: &Path, name: &str) -> std::path::PathBuf {
    dir.join(format!("{name}.sqlite3"))
}

fn due(date: &str) -> TaskPatchDto {
    TaskPatchDto {
        due: Some(Some(DueDto::Date { date: date.into() })),
        ..TaskPatchDto::default()
    }
}

/// Szenario 1: Beide offline, A ändert den Titel, B die Fälligkeit → beide Änderungen auf beiden Geräten.
#[test]
fn offline_title_and_due_edits_both_survive() {
    let world = World::new();
    let a = world.device("a");
    let b = world.device("b");
    let task = a.add("Milch");
    a.sync();
    b.sync();

    a.rename(&task.id, "Hafermilch");
    b.clock.advance(50);
    b.update(&task.id, due("2026-10-20"));

    a.sync();
    b.sync();
    a.sync();
    for device in [&a, &b] {
        let t = device.task(&task.id).unwrap();
        assert_eq!(t.title, "Hafermilch");
        assert_eq!(
            t.due,
            Some(DueDto::Date {
                date: "2026-10-20".into()
            })
        );
    }
    assert_converged(&[&a, &b]);
}

/// Szenario 2: A löscht, B bearbeitet dieselbe Task (sogar später) → gelöscht auf beiden.
#[test]
fn delete_beats_concurrent_edit() {
    let world = World::new();
    let a = world.device("a");
    let b = world.device("b");
    let task = a.add("Steuer");
    let keep = a.add("bleibt");
    a.sync();
    b.sync();

    a.delete(&task.id);
    b.clock.advance(60_000);
    b.rename(&task.id, "Steuer – erledigt?");

    b.sync();
    a.sync();
    b.sync();
    for device in [&a, &b] {
        assert_eq!(device.task(&task.id), None);
        assert!(device.task(&keep.id).is_some());
    }
    assert_converged(&[&a, &b]);
}

/// Szenario 3: Beide verschieben verschiedene Tasks in derselben Liste (an dieselbe Stelle → gleiche
/// Sort-Keys) → identische Reihenfolge auf beiden.
#[test]
fn concurrent_moves_give_identical_order() {
    let world = World::new();
    let a = world.device("a");
    let b = world.device("b");
    let ids: Vec<String> = ["eins", "zwei", "drei", "vier"]
        .iter()
        .map(|t| a.add(t).id)
        .collect();
    a.sync();
    b.sync();

    a.move_after(&ids[2], None, Some(&ids[0]));
    b.move_after(&ids[3], None, Some(&ids[0]));
    b.move_after(&ids[1], Some(&ids[2]), None);

    a.sync();
    b.sync();
    a.sync();
    assert_converged(&[&a, &b]);
    let order: Vec<String> = a.all().into_iter().map(|t| t.title).collect();
    assert_eq!(order.len(), 4);
    assert_eq!(order[3], "zwei", "B's Verschiebung ans Ende");
    assert_eq!(order[2], "eins");
}

/// Szenario 4: Push bricht mitten ab (zweites von drei Segmenten; danach eine verlorene Antwort) →
/// der nächste Sync heilt, keine Duplikate.
#[test]
fn aborted_push_heals_without_duplicates() {
    let world = World::new();
    let flaky = Flaky::new(world.remote.clone(), 2, false);
    let a = world.device_with("a", flaky, T0);
    for i in 0..1_200 {
        a.add(&format!("Task {i:04}"));
    }
    assert!(
        block_on(a.engine.sync_once()).is_err(),
        "Abbruch beim zweiten Segment"
    );
    assert_eq!(a.pending(), 701, "erstes Segment ist durch");

    // Neuer Prozess, gleiche Datenbank: diesmal geht die Antwort des ersten Uploads verloren.
    drop(a);
    let lost = Flaky::new(world.remote.clone(), 1, true);
    let a = world.device_with("a", lost, T0 + 10_000);
    assert!(block_on(a.engine.sync_once()).is_err());
    a.sync();
    assert_eq!(a.pending(), 0);

    let b = world.device("b");
    b.sync();
    let mut titles = Vec::new();
    for page in b.all_pages() {
        titles.extend(page.into_iter().map(|t| t.title));
    }
    titles.sort();
    titles.dedup();
    assert_eq!(titles.len(), 1_200, "jede Task genau einmal");
    let lists = b.lists();
    assert_eq!(lists.len(), 1, "eine Inbox");
}

/// Szenario 5: Die Uhr von B geht 2 h vor → Ergebnis trotzdem konsistent; nach dem Sync gewinnt die
/// nächste Änderung von A wieder (HLC hat B's Zeit beobachtet).
#[test]
fn clock_two_hours_ahead_stays_consistent() {
    let world = World::new();
    let a = world.device("a");
    let b = world.device_with("b", world.remote.clone(), T0 + TWO_HOURS);
    let task = a.add("Plan");
    a.sync();
    b.sync();

    b.rename(&task.id, "von B (Uhr vor)");
    a.clock.advance(60_000);
    a.rename(&task.id, "von A (später, echte Zeit)");

    a.sync();
    b.sync();
    a.sync();
    assert_converged(&[&a, &b]);
    assert_eq!(
        a.task(&task.id).unwrap().title,
        "von B (Uhr vor)",
        "höhere HLC gewinnt – auf beiden gleich"
    );

    a.rename(&task.id, "A nach Sync");
    a.sync();
    b.sync();
    assert_eq!(b.task(&task.id).unwrap().title, "A nach Sync");

    let c = world.device("c");
    c.sync();
    assert_converged(&[&a, &b, &c]);
}
