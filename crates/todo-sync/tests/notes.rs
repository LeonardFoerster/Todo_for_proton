//! Notizen im Op-Log (ADR-029, `notes.md` §4.8): Szenarien über die echte Engine mit
//! gemeinsamem `FsRemoteStore`-Verzeichnis und je Gerät eigener SQLite-Datei.

mod common;

use std::sync::Arc;

use common::Device;
use futures::executor::block_on;
use tempfile::TempDir;
use todo_core::{
    domain::{
        DeviceId, Note, NoteFolder, NoteFolderId, NoteId, NoteKind, SortKey, Task, TaskListId,
    },
    ports::{ChangeSet, Cipher, Clock, EntityRef, KeyStore, RemoteStore},
    sync::{conflict_copy_id, encode_ops, task_fields, HlcState, OpClock, OpKind},
};
use todo_crypto::XChaCha20Poly1305Cipher;
use todo_remote_fs::FsRemoteStore;
use todo_sync::{
    compression,
    segment::{SegmentHeader, HEADER_LEN},
    SyncConfig,
};

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
}

/// Notiz-Hilfen direkt auf dem Repository der `CoreApi` (die Notiz-API kommt mit N5-06).
trait Notes {
    fn commit(&self, changes: ChangeSet);
    fn note(&self, id: NoteId) -> Option<Note>;
    fn new_note(&self, folder: Option<NoteFolderId>, body: &str) -> Note;
    fn edit(&self, id: NoteId, body: &str) -> Note;
    fn new_folder(&self, name: &str) -> NoteFolder;
}

impl Notes for Device {
    fn commit(&self, changes: ChangeSet) {
        block_on(self.api.repository().commit(changes)).expect("commit");
    }

    fn note(&self, id: NoteId) -> Option<Note> {
        block_on(self.api.repository().get_note(id)).expect("get_note")
    }

    fn new_note(&self, folder: Option<NoteFolderId>, body: &str) -> Note {
        let clock = self.clock.as_ref();
        let mut note = Note::new(folder, NoteKind::Markdown, SortKey::first(), clock);
        note.set_body(body, None, clock).unwrap();
        self.commit(ChangeSet::new().upsert_note(note.clone()));
        self.note(note.id()).unwrap()
    }

    fn edit(&self, id: NoteId, body: &str) -> Note {
        let mut note = self.note(id).expect("note");
        let base = note.body_rev();
        note.set_body(body, base, self.clock.as_ref()).unwrap();
        self.commit(ChangeSet::new().upsert_note(note));
        self.note(id).unwrap()
    }

    fn new_folder(&self, name: &str) -> NoteFolder {
        let folder = NoteFolder::new(None, name, SortKey::first(), self.clock.as_ref()).unwrap();
        self.commit(ChangeSet::new().upsert_note_folder(folder.clone()));
        folder
    }
}

/// A ändert Absatz 1, B Absatz 3 (beide offline): ein Text mit beiden, keine Kopie.
#[test]
fn offline_edits_of_different_paragraphs_merge() {
    let world = World::new();
    let (a, b) = (world.device("a"), world.device("b"));
    let note = a.new_note(None, "Absatz 1\n\nAbsatz 2\n\nAbsatz 3");
    a.sync();
    b.sync();
    a.edit(note.id(), "Absatz 1 von A\n\nAbsatz 2\n\nAbsatz 3");
    b.clock.advance(50);
    b.edit(note.id(), "Absatz 1\n\nAbsatz 2\n\nAbsatz 3 von B");
    a.sync();
    b.sync();
    a.sync();
    let merged = "Absatz 1 von A\n\nAbsatz 2\n\nAbsatz 3 von B";
    for device in [&a, &b] {
        assert_eq!(device.note(note.id()).unwrap().body(), merged);
    }
    // Die Merge-Revisionen beider Seiten haben denselben Text: kein weiterer Merge, Ruhe.
    b.sync();
    a.sync();
    assert_eq!(a.pending() + b.pending(), 0);
}

/// A und B ändern dieselbe Zeile: die spätere gewinnt, dieselbe Kopie auf beiden Geräten.
#[test]
fn same_line_conflict_creates_the_same_copy_on_both() {
    let world = World::new();
    let (a, b) = (world.device("a"), world.device("b"));
    let note = a.new_note(None, "Einkauf\nMilch");
    a.sync();
    b.sync();
    let from_a = a.edit(note.id(), "Einkauf\nHafermilch");
    b.clock.advance(10_000);
    let from_b = b.edit(note.id(), "Einkauf\nSojamilch");
    a.sync();
    b.sync();
    a.sync();
    let copy = conflict_copy_id(note.id(), from_a.body_rev().unwrap());
    for device in [&a, &b] {
        assert_eq!(device.note(note.id()).unwrap().body(), from_b.body());
        let copy = device.note(copy).expect("Konfliktkopie");
        assert_eq!(copy.body(), from_a.body());
        assert!(copy.name().as_str().starts_with("Einkauf (Konflikt "));
    }
}

/// Große Revisionen: Segmente bleiben unter 2 MiB Klartext, alles kommt an.
#[test]
fn large_revisions_are_split_across_segments() {
    let world = World::new();
    let (a, b) = (world.device("a"), world.device("b"));
    let big = |c: char| c.to_string().repeat(900 * 1024);
    let notes: Vec<Note> = ['x', 'y', 'z'].map(|c| a.new_note(None, &big(c))).into();
    let report = a.sync();
    assert!(report.pushed >= 6);
    let device = block_on(a.api.repository().device_id()).unwrap();
    let segments = block_on(async {
        let root = world.remote.open_app_root("Tasks").await.unwrap();
        let dir = root.join(&format!("ops/{device}")).unwrap();
        world.remote.list(&dir, None).await.unwrap().entries.len()
    });
    assert!(segments >= 2, "{segments} Segment(e)");
    b.sync();
    for (note, c) in notes.iter().zip(['x', 'y', 'z']) {
        assert_eq!(b.note(note.id()).unwrap().body(), big(c));
    }
}

/// Ein neues Gerät startet aus einem Snapshot, der Revisionen enthält, und kann danach ohne
/// Konflikt weiterschreiben.
#[test]
fn snapshot_carries_note_revisions() {
    let world = World::new();
    let config = SyncConfig {
        snapshot_after_ops: 2,
        ..SyncConfig::default()
    };
    let repo = |name: &str| {
        Arc::new(
            todo_storage_sqlite::SqliteTaskRepository::open(
                world.dir.path().join(format!("{name}.sqlite3")),
            )
            .unwrap(),
        )
    };
    let a = Device::configured(
        "a",
        world.remote.clone(),
        repo("a"),
        common::T0,
        config.clone(),
    );
    let folder = a.new_folder("Uni");
    let note = a.new_note(Some(folder.id()), "# Blatt 3\nAufgabe 1");
    a.edit(note.id(), "# Blatt 3\nAufgabe 1\nAufgabe 2");
    let report = a.sync();
    assert!(report.snapshot_written, "{report:?}");
    let c = Device::configured("c", world.remote.clone(), repo("c"), common::T0, config);
    let report = c.sync();
    assert!(report.snapshot_loaded, "{report:?}");
    let copy = c.note(note.id()).expect("aus dem Snapshot");
    assert_eq!(copy.body(), "# Blatt 3\nAufgabe 1\nAufgabe 2");
    assert_eq!(copy.folder(), Some(folder.id()));
    c.edit(note.id(), "# Blatt 3\nAufgabe 1\nAufgabe 2\nAufgabe 3");
    a.edit(note.id(), "# Blatt 3 (Analysis)\nAufgabe 1\nAufgabe 2");
    c.sync();
    a.sync();
    c.sync();
    let both = "# Blatt 3 (Analysis)\nAufgabe 1\nAufgabe 2\nAufgabe 3";
    assert_eq!(a.note(note.id()).unwrap().body(), both);
    assert_eq!(c.note(note.id()).unwrap().body(), both);
}

/// Ein Segment mit einer unbekannten Variante (neuere App) wird übersprungen, kein Absturz;
/// der Cursor des Geräts bleibt davor stehen.
#[test]
fn segment_with_unknown_variant_is_skipped() {
    let world = World::new();
    let a = world.device("a");
    a.add("vorher");
    a.sync();

    // Ein „neueres" Gerät schreibt ein gültig verschlüsseltes Segment mit unbekannter Entity-Art.
    let newer = DeviceId::new();
    let mut clock = OpClock::new(HlcState::new(newer), 0);
    let task = Task::new(TaskListId::INBOX, "x", SortKey::first(), a.clock.as_ref()).unwrap();
    let op = clock.stamp(
        a.clock.now(),
        EntityRef::Task(task.id()),
        OpKind::Create {
            fields: task_fields(&task),
        },
    );
    let mut plain = encode_ops(&[op]);
    // Vec-Länge (1) + OpId (18) + HLC (29) → Varianten-Byte der Entity.
    assert_eq!(plain[48], 0, "Task-Variante");
    plain[48] = 9;
    let header = SegmentHeader {
        device: newer,
        seq_from: 1,
        seq_to: 1,
    };
    let key = block_on(a.keys.get(&SyncConfig::default().key_name()))
        .unwrap()
        .unwrap();
    let aad = header.to_bytes();
    let sealed = XChaCha20Poly1305Cipher::new()
        .seal(&key, &compression::compress(&plain).unwrap(), &aad)
        .unwrap();
    assert_eq!(aad.len(), HEADER_LEN);
    block_on(async {
        let root = world.remote.open_app_root("Tasks").await.unwrap();
        let path = root.join(&format!("ops/{newer}/1-1.seg")).unwrap();
        world
            .remote
            .create(&path, [aad.as_slice(), &sealed].concat())
            .await
            .unwrap();
    });

    let b = world.device("b");
    let report = b.sync();
    assert_eq!(report.skipped_segments, 1, "{report:?}");
    assert!(b.titles().contains(&"vorher".to_owned()));
    assert_eq!(
        block_on(b.api.repository().sync_cursor(newer)).unwrap(),
        None
    );
}
