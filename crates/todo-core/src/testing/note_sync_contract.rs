//! Contract-Fälle für Notizen im Sync (ADR-029, `notes.md` §4.8): zwei echte Repositories
//! tauschen Ops wie über das Backend aus. Werden über [`crate::repository_contract_tests!`] mit
//! ausgeführt.

use std::collections::BTreeSet;

use futures::executor::block_on;

use crate::{
    domain::{Note, NoteFolder, NoteFolderId, NoteId, NoteKind, SortKey, Timestamp},
    ports::{ChangeSet, EntityRef, SteppingClock, TaskRepository},
    sync::{conflict_copy_id, OpKind, RemoteBatch},
};

/// Ein Gerät: Repository plus eigene Uhr.
pub(super) struct Device<R> {
    pub(super) repo: R,
    clock: SteppingClock,
}

impl<R: TaskRepository> Device<R> {
    fn new(repo: R, start: i64) -> Self {
        Self {
            repo,
            clock: SteppingClock::new(Timestamp::from_millis(1_700_000_000_000 + start), 7),
        }
    }

    fn commit(&self, changes: ChangeSet) {
        let now = crate::ports::Clock::now(&self.clock);
        block_on(self.repo.commit(changes.at(now))).expect("commit");
    }

    fn note(&self, id: NoteId) -> Option<Note> {
        block_on(self.repo.get_note(id)).expect("get_note")
    }

    fn folder(&self, id: NoteFolderId) -> Option<NoteFolder> {
        block_on(self.repo.get_note_folder(id)).expect("get_note_folder")
    }

    fn new_note(&self, folder: Option<NoteFolderId>, body: &str) -> Note {
        let mut note = Note::new(folder, NoteKind::Markdown, SortKey::first(), &self.clock);
        note.set_body(body, None, &self.clock).expect("body");
        self.commit(ChangeSet::new().upsert_note(note.clone()));
        self.note(note.id()).expect("saved")
    }

    /// Wie der Editor: auf dem geladenen Stand weiterschreiben.
    fn edit(&self, id: NoteId, body: &str) -> Note {
        let mut note = self.note(id).expect("note");
        let base = note.body_rev();
        note.set_body(body, base, &self.clock).expect("body");
        self.commit(ChangeSet::new().upsert_note(note));
        self.note(id).expect("note")
    }

    fn new_folder(&self, parent: Option<NoteFolderId>, name: &str) -> NoteFolder {
        let folder = NoteFolder::new(parent, name, SortKey::first(), &self.clock).expect("folder");
        self.commit(ChangeSet::new().upsert_note_folder(folder.clone()));
        folder
    }

    fn outbox_kinds(&self) -> Vec<&'static str> {
        block_on(self.repo.pending_ops(10_000))
            .expect("pending")
            .iter()
            .map(|op| match op.kind {
                OpKind::Create { .. } => "create",
                OpKind::Set { .. } => "set",
                OpKind::Delete => "delete",
                OpKind::Revise { .. } => "revise",
            })
            .collect()
    }
}

/// Lädt die Outbox von `from` „hoch" und wendet sie bei `to` an. `true`, wenn etwas übertragen
/// wurde.
fn send<R: TaskRepository>(from: &Device<R>, to: &Device<R>) -> bool {
    let ops = block_on(from.repo.pending_ops(10_000)).expect("pending");
    let Some(last) = ops.iter().map(|op| op.id.seq).max() else {
        return false;
    };
    block_on(from.repo.freeze_outbox(last)).expect("freeze");
    let device = block_on(from.repo.device_id()).expect("device");
    let now = crate::ports::Clock::now(&to.clock);
    block_on(to.repo.apply_remote(RemoteBatch {
        ops,
        cursors: vec![(device, last)],
        now,
    }))
    .expect("apply_remote");
    block_on(from.repo.mark_ops_pushed(last)).expect("mark");
    true
}

/// Tauscht in beide Richtungen, bis keine Ops mehr entstehen (Folge-Commits eingeschlossen).
fn exchange<R: TaskRepository>(a: &Device<R>, b: &Device<R>) {
    for _ in 0..6 {
        let moved = send(a, b) | send(b, a);
        if !moved {
            return;
        }
    }
    panic!("Geräte kommen nicht zur Ruhe");
}

fn pair<R: TaskRepository>(make: &impl Fn() -> R) -> (Device<R>, Device<R>) {
    (Device::new(make(), 0), Device::new(make(), 3))
}

fn notes_in<R: TaskRepository>(device: &Device<R>, ids: &[NoteId]) -> BTreeSet<NoteId> {
    ids.iter()
        .copied()
        .filter(|id| device.note(*id).is_some())
        .collect()
}

// --- Fälle -------------------------------------------------------------------------------------

/// Autosave: Solange die Revision nicht herausgegeben ist, ersetzt jede Bearbeitung sie
/// (höchstens eine Revision je Upload); danach entsteht eine neue mit ihr als Eltern.
pub fn note_body_edits_coalesce_until_frozen<R: TaskRepository>(make: impl Fn() -> R) {
    let (a, _) = pair(&make);
    let note = a.new_note(None, "");
    a.edit(note.id(), "H");
    a.edit(note.id(), "Ha");
    let saved = a.edit(note.id(), "# Hallo");
    assert_eq!(
        a.outbox_kinds(),
        ["create", "revise"],
        "eine Revision; der Auto-Name ist kein Feld"
    );
    assert_eq!(saved.body(), "# Hallo");
    assert_eq!(saved.name().as_str(), "Hallo", "Auto-Name im Core");
    let first = saved.body_rev().expect("rev");

    let ops = block_on(a.repo.pending_ops(100)).unwrap();
    let last = ops.iter().map(|op| op.id.seq).max().unwrap();
    block_on(a.repo.freeze_outbox(last)).unwrap();
    let next = a.edit(note.id(), "# Hallo\nWelt");
    assert_ne!(next.body_rev(), Some(first));
    let ops = block_on(a.repo.pending_ops(100)).unwrap();
    let OpKind::Revise { parents, .. } = &ops.last().unwrap().kind else {
        panic!("{ops:?}")
    };
    assert_eq!(parents, &[first.into()]);
}

/// A ändert Absatz 1, B Absatz 3: ein Text mit beiden, keine Kopie.
pub fn concurrent_note_edits_merge_cleanly<R: TaskRepository>(make: impl Fn() -> R) {
    let (a, b) = pair(&make);
    let note = a.new_note(None, "Absatz 1\n\nAbsatz 2\n\nAbsatz 3");
    exchange(&a, &b);
    a.edit(note.id(), "Absatz 1 (A)\n\nAbsatz 2\n\nAbsatz 3");
    b.edit(note.id(), "Absatz 1\n\nAbsatz 2\n\nAbsatz 3 (B)");
    exchange(&a, &b);
    let expected = "Absatz 1 (A)\n\nAbsatz 2\n\nAbsatz 3 (B)";
    assert_eq!(a.note(note.id()).unwrap().body(), expected);
    assert_eq!(b.note(note.id()).unwrap().body(), expected);
    // Weiterschreiben auf beiden Seiten bleibt konfliktfrei.
    a.edit(note.id(), &format!("{expected}\n\nAbsatz 4"));
    exchange(&a, &b);
    assert_eq!(
        b.note(note.id()).unwrap().body(),
        format!("{expected}\n\nAbsatz 4")
    );
}

/// A und B ändern dieselbe Zeile: die neuere gewinnt, auf beiden Geräten dieselbe Kopie.
pub fn conflicting_note_edits_create_one_copy<R: TaskRepository>(make: impl Fn() -> R) {
    let (a, b) = pair(&make);
    let note = a.new_note(None, "Zeile");
    exchange(&a, &b);
    let from_a = a.edit(note.id(), "Zeile von A");
    let from_b = b.edit(note.id(), "Zeile von B");
    exchange(&a, &b);
    let body_a = a.note(note.id()).unwrap().body().to_owned();
    assert_eq!(body_a, b.note(note.id()).unwrap().body());
    // Gewinner = Bearbeitung mit der höheren HLC; der andere Stand wird Kopie.
    let (winner, loser) = if body_a == from_a.body() {
        (from_a, from_b)
    } else {
        (from_b, from_a)
    };
    assert_eq!(body_a, winner.body());
    let copy_id = conflict_copy_id(note.id(), loser.body_rev().unwrap());
    for device in [&a, &b] {
        let copy = device.note(copy_id).expect("Konfliktkopie");
        assert_eq!(copy.body(), loser.body());
        assert!(
            copy.name()
                .as_str()
                .starts_with(&format!("{} (Konflikt ", winner.body())),
            "{}",
            copy.name()
        );
        assert!(
            copy.name().as_str().contains(" (Konflikt "),
            "{}",
            copy.name()
        );
        assert!(!copy.auto_name());
        assert!(copy.sort_key() > note.sort_key());
    }
    let ids: Vec<NoteId> = vec![note.id(), copy_id];
    assert_eq!(notes_in(&a, &ids), notes_in(&b, &ids));
}

/// A leert den Papierkorb (Ordner endgültig gelöscht), B legt offline eine Notiz hinein: Die
/// Notiz landet auf beiden Geräten in der Wurzel.
pub fn purged_folder_rescues_offline_note<R: TaskRepository>(make: impl Fn() -> R) {
    let (a, b) = pair(&make);
    let folder = a.new_folder(None, "Uni");
    exchange(&a, &b);
    let note = b.new_note(Some(folder.id()), "offline");
    a.commit(ChangeSet::new().delete(EntityRef::NoteFolder(folder.id())));
    exchange(&a, &b);
    for device in [&a, &b] {
        assert!(device.folder(folder.id()).is_none());
        let rescued = device.note(note.id()).expect("Notiz gerettet");
        assert_eq!(rescued.folder(), None);
        assert_eq!(rescued.body(), "offline");
    }
}

/// A verschiebt X nach Y, B offline Y nach X: Beide sehen denselben, zyklenfreien Baum.
pub fn folder_cycle_resolves_identically<R: TaskRepository>(make: impl Fn() -> R) {
    let (a, b) = pair(&make);
    let x = a.new_folder(None, "X");
    let y = a.new_folder(None, "Y");
    exchange(&a, &b);
    let mut x_in_y = a.folder(x.id()).unwrap();
    x_in_y.move_to(Some(y.id()), SortKey::first());
    a.commit(ChangeSet::new().upsert_note_folder(x_in_y));
    let mut y_in_x = b.folder(y.id()).unwrap();
    y_in_x.move_to(Some(x.id()), SortKey::first());
    b.commit(ChangeSet::new().upsert_note_folder(y_in_x));
    exchange(&a, &b);
    let tree = |d: &Device<R>| {
        (
            d.folder(x.id()).unwrap().parent(),
            d.folder(y.id()).unwrap().parent(),
        )
    };
    assert_eq!(tree(&a), tree(&b));
    let (px, py) = tree(&a);
    assert!(px.is_none() || py.is_none(), "zyklenfrei: {px:?} {py:?}");
    assert!(
        px.is_some() || py.is_some(),
        "nur ein Ordner wird umgehängt"
    );
}

/// Papierkorb und Bearbeitung stören sich nicht: A legt in den Papierkorb, B bearbeitet.
pub fn trash_and_edit_both_survive<R: TaskRepository>(make: impl Fn() -> R) {
    let (a, b) = pair(&make);
    let note = a.new_note(None, "Text");
    exchange(&a, &b);
    let mut trashed = a.note(note.id()).unwrap();
    trashed.set_trashed(Some(Timestamp::from_millis(1)));
    a.commit(ChangeSet::new().upsert_note(trashed));
    b.edit(note.id(), "Text bearbeitet");
    exchange(&a, &b);
    for device in [&a, &b] {
        let n = device.note(note.id()).unwrap();
        assert!(n.is_trashed());
        assert_eq!(n.body(), "Text bearbeitet");
    }
}

/// Endgültig löschen gewinnt gegen eine Offline-Bearbeitung (User-Entscheidung).
pub fn purge_wins_over_offline_edit<R: TaskRepository>(make: impl Fn() -> R) {
    let (a, b) = pair(&make);
    let note = a.new_note(None, "weg");
    exchange(&a, &b);
    a.commit(ChangeSet::new().delete(EntityRef::Note(note.id())));
    b.edit(note.id(), "noch da?");
    exchange(&a, &b);
    assert_eq!(notes_in(&a, &[note.id()]).len(), 0);
    assert_eq!(notes_in(&b, &[note.id()]).len(), 0);
    assert!(block_on(b.repo.revision_ops())
        .unwrap()
        .iter()
        .all(|op| op.entity != EntityRef::Note(note.id())));
}

/// Revisionen, die alle aktiven Geräte hinter sich haben, werden aufgeräumt; Köpfe bleiben,
/// und der Snapshot-Export enthält genau die gespeicherten.
pub fn acked_revisions_are_pruned<R: TaskRepository>(make: impl Fn() -> R) {
    let (a, b) = pair(&make);
    let note = a.new_note(None, "1");
    for text in ["1\n2", "1\n2\n3", "1\n2\n3\n4"] {
        exchange(&a, &b);
        a.edit(note.id(), text);
    }
    exchange(&a, &b);
    let before = block_on(a.repo.revision_ops()).unwrap().len();
    assert!(before >= 4, "{before}");
    let device = block_on(a.repo.device_id()).unwrap();
    let removed = block_on(a.repo.prune_revisions(vec![(device, u64::MAX)])).unwrap();
    let after = block_on(a.repo.revision_ops()).unwrap();
    assert_eq!(after.len(), 1, "nur der Kopf bleibt");
    assert_eq!(removed as usize, before - 1);
    assert_eq!(a.note(note.id()).unwrap().body(), "1\n2\n3\n4");
    // Weiter bearbeiten und mergen geht ohne die alten Revisionen.
    b.edit(note.id(), "0\n1\n2\n3\n4");
    exchange(&a, &b);
    assert_eq!(a.note(note.id()).unwrap().body(), "0\n1\n2\n3\n4");
}

/// Snapshot-Import (Zustand + Revisionen als Ops) ergibt dieselbe Notiz.
pub fn note_state_survives_snapshot_roundtrip<R: TaskRepository>(make: impl Fn() -> R) {
    let (a, b) = pair(&make);
    let folder = a.new_folder(None, "Uni");
    let note = a.new_note(Some(folder.id()), "# Blatt 3");
    let mut ops = Vec::new();
    for (entity, state) in block_on(a.repo.entity_states()).unwrap() {
        if let crate::sync::EntityState::Live(registers) = state {
            for (field, register) in registers {
                ops.push(crate::sync::Op {
                    id: crate::sync::OpId {
                        device: register.hlc.device,
                        seq: 0,
                    },
                    hlc: register.hlc,
                    entity,
                    kind: OpKind::Set {
                        field,
                        value: register.value,
                    },
                });
            }
        }
    }
    ops.extend(block_on(a.repo.revision_ops()).unwrap());
    let now = crate::ports::Clock::now(&b.clock);
    block_on(b.repo.apply_remote(RemoteBatch {
        ops,
        cursors: Vec::new(),
        now,
    }))
    .unwrap();
    let copy = b.note(note.id()).expect("note");
    assert_eq!(copy.body(), "# Blatt 3");
    assert_eq!(copy.folder(), Some(folder.id()));
    assert_eq!(copy.name().as_str(), "Blatt 3");
    assert_eq!(copy.body_rev(), a.note(note.id()).unwrap().body_rev());
}
