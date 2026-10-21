//! Zwei Geräte über die Use-Cases (`notes.md` §4.8): Ops wie über das Backend austauschen.

use std::sync::Arc;

use futures::executor::block_on;

use super::{
    notes::NoteItem, CreateNote, CreateNoteFolder, CreateNoteFolderInput, CreateNoteInput, Deps,
    EmptyNoteTrash, ListNoteTrash, TrashNoteItems, TrashNoteItemsInput, UpdateNoteBody,
    UpdateNoteBodyInput,
};
use crate::{
    domain::{Note, NoteKind, Timestamp},
    ports::{NoteRepository, SteppingClock, TaskRepository},
    sync::RemoteBatch,
    testing::InMemoryTaskRepository,
};

struct Device {
    deps: Deps,
    repo: Arc<InMemoryTaskRepository>,
}

fn device(start: i64) -> Device {
    let repo = Arc::new(InMemoryTaskRepository::new());
    let clock = Arc::new(SteppingClock::new(
        Timestamp::from_millis(1_700_000_000_000 + start),
        5,
    ));
    Device {
        deps: Deps::new(repo.clone(), clock),
        repo,
    }
}

fn send(from: &Device, to: &Device) -> bool {
    let ops = block_on(from.repo.pending_ops(10_000)).unwrap();
    let Some(last) = ops.iter().map(|op| op.id.seq).max() else {
        return false;
    };
    block_on(from.repo.freeze_outbox(last)).unwrap();
    let device = block_on(from.repo.device_id()).unwrap();
    block_on(to.repo.apply_remote(RemoteBatch {
        ops,
        cursors: vec![(device, last)],
        now: Timestamp::from_millis(1_700_000_100_000),
    }))
    .unwrap();
    block_on(from.repo.mark_ops_pushed(last)).unwrap();
    true
}

fn exchange(a: &Device, b: &Device) {
    for _ in 0..6 {
        if !(send(a, b) | send(b, a)) {
            return;
        }
    }
    panic!("keine Ruhe");
}

impl Device {
    fn create(&self, folder: Option<crate::domain::NoteFolderId>) -> Note {
        block_on(CreateNote::new(self.deps.clone()).execute(CreateNoteInput {
            folder,
            kind: NoteKind::Markdown,
        }))
        .unwrap()
    }

    fn type_text(&self, note: &Note, text: &str) -> Note {
        let current = block_on(self.repo.get_note(note.id())).unwrap().unwrap();
        block_on(
            UpdateNoteBody::new(self.deps.clone()).execute(UpdateNoteBodyInput {
                id: note.id(),
                text: text.into(),
                base_rev: current.body_rev(),
            }),
        )
        .unwrap()
    }

    fn body(&self, note: &Note) -> Option<String> {
        block_on(self.repo.get_note(note.id()))
            .unwrap()
            .map(|n| n.body().to_owned())
    }

    fn note_count(&self) -> usize {
        block_on(self.repo.search_notes(&crate::ports::SearchQuery {
            text: "zeile".into(),
            limit: 100,
        }))
        .unwrap()
        .len()
    }
}

#[test]
fn concurrent_edits_merge_and_conflicts_copy() {
    let (a, b) = (device(0), device(2));
    let note = a.create(None);
    a.type_text(&note, "Zeile 1\nZeile 2\nZeile 3");
    exchange(&a, &b);
    a.type_text(&note, "Zeile 1 A\nZeile 2\nZeile 3");
    b.type_text(&note, "Zeile 1\nZeile 2\nZeile 3 B");
    exchange(&a, &b);
    assert_eq!(a.body(&note).unwrap(), "Zeile 1 A\nZeile 2\nZeile 3 B");
    assert_eq!(a.body(&note), b.body(&note));
    assert_eq!(a.note_count(), 1, "keine Kopie");

    a.type_text(&note, "Zeile 1 A\nZeile 2 von A\nZeile 3 B");
    b.type_text(&note, "Zeile 1 A\nZeile 2 von B\nZeile 3 B");
    exchange(&a, &b);
    assert_eq!(a.body(&note), b.body(&note));
    assert_eq!(a.note_count(), 2, "Original + Kopie");
    assert_eq!(b.note_count(), 2);
}

#[test]
fn trash_and_edit_on_two_devices() {
    let (a, b) = (device(0), device(2));
    let note = a.create(None);
    a.type_text(&note, "Text");
    exchange(&a, &b);
    block_on(
        TrashNoteItems::new(a.deps.clone()).execute(TrashNoteItemsInput {
            items: vec![NoteItem::Note(note.id())],
        }),
    )
    .unwrap();
    b.type_text(&note, "Text, bearbeitet");
    exchange(&a, &b);
    for d in [&a, &b] {
        let trash = block_on(ListNoteTrash::new(d.deps.clone()).execute()).unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(d.body(&note).unwrap(), "Text, bearbeitet");
    }
}

#[test]
fn emptied_trash_rescues_a_note_created_offline() {
    let (a, b) = (device(0), device(2));
    let folder = block_on(
        CreateNoteFolder::new(a.deps.clone()).execute(CreateNoteFolderInput {
            parent: None,
            name: "Uni".into(),
        }),
    )
    .unwrap();
    exchange(&a, &b);
    block_on(
        TrashNoteItems::new(a.deps.clone()).execute(TrashNoteItemsInput {
            items: vec![NoteItem::Folder(folder.id())],
        }),
    )
    .unwrap();
    block_on(EmptyNoteTrash::new(a.deps.clone()).execute()).unwrap();
    let offline = b.create(Some(folder.id()));
    b.type_text(&offline, "offline geschrieben");
    exchange(&a, &b);
    for d in [&a, &b] {
        let n = block_on(d.repo.get_note(offline.id()))
            .unwrap()
            .expect("gerettet");
        assert_eq!(n.folder(), None);
        assert_eq!(n.body(), "offline geschrieben");
        assert!(block_on(d.repo.get_note_folder(folder.id()))
            .unwrap()
            .is_none());
    }
}
