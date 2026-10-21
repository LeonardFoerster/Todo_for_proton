//! Contract-Fälle für den [`NoteRepository`]-Port (Lesen) und das Schreiben von Notizen über
//! `commit`. Werden über [`crate::repository_contract_tests!`] mit ausgeführt.

use futures::executor::block_on;

use crate::{
    domain::{Note, NoteFolder, NoteFolderId, NoteKind, SortKey, Timestamp},
    ports::{
        ChangeSet, EntityRef, FolderItemsQuery, NoteTreeItem, PageRequest, SearchQuery,
        SteppingClock, TaskRepository,
    },
    CoreError,
};

struct Notes<R> {
    repo: R,
    clock: SteppingClock,
}

fn notes<R: TaskRepository>(repo: R) -> Notes<R> {
    Notes {
        repo,
        clock: SteppingClock::new(Timestamp::from_millis(1_700_000_000_000), 1),
    }
}

impl<R: TaskRepository> Notes<R> {
    fn commit(&self, changes: ChangeSet) -> Result<(), CoreError> {
        block_on(self.repo.commit(changes)).map(|_| ())
    }

    fn note(&self, folder: Option<NoteFolderId>, key: &str, body: &str) -> Note {
        let mut note = Note::new(
            folder,
            NoteKind::Markdown,
            SortKey::parse(key).unwrap(),
            &self.clock,
        );
        note.set_body(body, None, &self.clock).unwrap();
        self.commit(ChangeSet::new().upsert_note(note.clone()))
            .expect("commit note");
        block_on(self.repo.get_note(note.id())).unwrap().unwrap()
    }

    fn folder(&self, parent: Option<NoteFolderId>, key: &str, name: &str) -> NoteFolder {
        let folder =
            NoteFolder::new(parent, name, SortKey::parse(key).unwrap(), &self.clock).unwrap();
        self.commit(ChangeSet::new().upsert_note_folder(folder.clone()))
            .expect("commit folder");
        folder
    }

    fn items(
        &self,
        folder: Option<NoteFolderId>,
        include_trashed: bool,
        limit: u32,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let mut page = PageRequest::first(limit);
        loop {
            let result = block_on(self.repo.list_folder_items(
                FolderItemsQuery {
                    folder,
                    include_trashed,
                },
                page,
            ))
            .expect("items");
            assert!(result.items.len() <= limit as usize);
            out.extend(result.items.iter().map(|item| match item {
                NoteTreeItem::Folder(f) => format!("F:{}", f.name()),
                NoteTreeItem::Note(n) => format!("N:{}", n.name),
            }));
            match result.next_cursor {
                Some(cursor) => page = PageRequest::after(cursor, limit),
                None => return out,
            }
        }
    }

    fn search(&self, text: &str) -> Vec<String> {
        block_on(self.repo.search_notes(&SearchQuery {
            text: text.into(),
            limit: 50,
        }))
        .expect("search")
        .into_iter()
        .map(|hit| hit.note.name)
        .collect()
    }
}

// --- Fälle -------------------------------------------------------------------------------------

/// Gespeicherte Notizen und Ordner kommen unverändert zurück; Ordnerverweise müssen stimmen.
pub fn notes_and_folders_roundtrip<R: TaskRepository>(repo: R) {
    let n = notes(repo);
    let folder = n.folder(None, "a0", "Uni");
    let note = n.note(Some(folder.id()), "a1", "# Blatt 3\nInhalt");
    assert_eq!(note.name().as_str(), "Blatt 3");
    assert_eq!(note.folder(), Some(folder.id()));
    assert_eq!(note.body(), "# Blatt 3\nInhalt");
    assert!(note.body_rev().is_some());
    assert_eq!(
        block_on(n.repo.get_note_folder(folder.id())).unwrap(),
        Some(folder.clone())
    );
    assert_eq!(block_on(n.repo.list_note_folders()).unwrap(), [folder]);

    let orphan = Note::new(
        Some(NoteFolderId::new()),
        NoteKind::Text,
        SortKey::first(),
        &n.clock,
    );
    let err = n.commit(ChangeSet::new().upsert_note(orphan)).unwrap_err();
    assert!(matches!(err, CoreError::Validation(_)), "{err}");

    n.commit(ChangeSet::new().delete(EntityRef::Note(note.id())))
        .unwrap();
    assert_eq!(block_on(n.repo.get_note(note.id())).unwrap(), None);
}

/// Ordnerinhalt: Unterordner und Notizen gemischt nach `sort_key`, seitenweise lückenlos;
/// Papierkorb nur auf Wunsch.
pub fn folder_items_are_mixed_paged_and_filter_trash<R: TaskRepository>(repo: R) {
    let n = notes(repo);
    let uni = n.folder(None, "a2", "Uni");
    n.note(None, "a1", "Einkauf");
    n.note(None, "a3", "Ideen");
    let mut trashed = n.note(None, "a4", "Alt");
    trashed.set_trashed(Some(Timestamp::from_millis(1)));
    n.commit(ChangeSet::new().upsert_note(trashed)).unwrap();
    n.note(Some(uni.id()), "a0", "Blatt 3");

    let root = ["N:Einkauf", "F:Uni", "N:Ideen"];
    for limit in [1, 2, 10] {
        assert_eq!(n.items(None, false, limit), root, "limit {limit}");
    }
    assert_eq!(
        n.items(None, true, 2),
        ["N:Einkauf", "F:Uni", "N:Ideen", "N:Alt"]
    );
    assert_eq!(n.items(Some(uni.id()), false, 10), ["N:Blatt 3"]);
    assert_eq!(
        block_on(n.repo.list_trashed_notes())
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect::<Vec<_>>(),
        ["Alt"]
    );
    assert_eq!(
        block_on(n.repo.last_note_sort_key(None)).unwrap(),
        Some(SortKey::parse("a4").unwrap())
    );
    assert_eq!(
        block_on(n.repo.last_note_sort_key(Some(uni.id()))).unwrap(),
        Some(SortKey::parse("a0").unwrap())
    );
    assert_eq!(
        block_on(n.repo.last_note_sort_key(Some(NoteFolderId::new()))).unwrap(),
        None
    );
    let bad = block_on(n.repo.list_folder_items(
        FolderItemsQuery {
            folder: None,
            include_trashed: false,
        },
        PageRequest::after("kaputt".into(), 10),
    ));
    assert!(matches!(bad, Err(CoreError::Validation(_))));
    let bad = block_on(n.repo.list_folder_items(
        FolderItemsQuery {
            folder: None,
            include_trashed: false,
        },
        PageRequest::first(0),
    ));
    assert!(matches!(bad, Err(CoreError::Validation(_))));
}

/// Suche: jedes Wort als Teilwort in Name oder Inhalt, Namenstreffer zuerst, nie Papierkorb
/// (auch nicht über einen Ordner im Papierkorb), kurze Wörter funktionieren.
pub fn search_notes_by_name_and_body<R: TaskRepository>(repo: R) {
    let n = notes(repo);
    let analysis = n.note(None, "a1", "# Analysis\nGrenzwerte von Folgen");
    let folgen = n.note(None, "a2", "# Folgen und Reihen\nKonvergenz");
    let old = n.folder(None, "a3", "Archiv");
    n.note(Some(old.id()), "a0", "# Folgen alt");
    let mut archived = old.clone();
    archived.set_trashed(Some(Timestamp::from_millis(1)));
    n.commit(ChangeSet::new().upsert_note_folder(archived))
        .unwrap();

    assert_eq!(n.search("folgen"), ["Folgen und Reihen", "Analysis"]);
    assert_eq!(n.search("FOLG grenz"), ["Analysis"]);
    assert_eq!(n.search("von"), ["Analysis"], "genau drei Zeichen");
    assert_eq!(
        n.search("re"),
        ["Folgen und Reihen"],
        "nur kurze Wörter: nur im Namen"
    );
    assert!(n.search("ve").is_empty(), "„ve“ steht nur im Inhalt");
    assert_eq!(
        n.search("ve konvergenz"),
        ["Folgen und Reihen"],
        "gemischt: auch Inhalt"
    );
    assert!(n.search("fehlt").is_empty());
    assert!(n.search("   ").is_empty());
    let hits = block_on(n.repo.search_notes(&SearchQuery {
        text: "konvergenz".into(),
        limit: 5,
    }))
    .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(!hits[0].name_hit);
    let snippet = hits[0].snippet.as_ref().unwrap();
    assert!(snippet.text.contains("Konvergenz"));
    assert_eq!(
        snippet.offset_utf16 as usize,
        folgen.body().find("Konvergenz").unwrap()
    );
    let mut trashed = analysis;
    trashed.set_trashed(Some(Timestamp::from_millis(1)));
    n.commit(ChangeSet::new().upsert_note(trashed)).unwrap();
    assert_eq!(n.search("folgen"), ["Folgen und Reihen"]);
    let err = block_on(n.repo.search_notes(&SearchQuery {
        text: "x".into(),
        limit: 0,
    }));
    assert!(matches!(err, Err(CoreError::Validation(_))));
}

/// Inhalt und Name lassen sich nach dem Bearbeiten wiederfinden (der Index folgt Änderungen).
pub fn search_follows_edits_and_deletes<R: TaskRepository>(repo: R) {
    let n = notes(repo);
    let note = n.note(None, "a0", "alt");
    assert_eq!(n.search("alt").len(), 1);
    let mut edited = block_on(n.repo.get_note(note.id())).unwrap().unwrap();
    let base = edited.body_rev();
    edited.set_body("neu geschrieben", base, &n.clock).unwrap();
    n.commit(ChangeSet::new().upsert_note(edited)).unwrap();
    assert!(n.search("alt").is_empty());
    assert_eq!(n.search("geschrieben"), ["neu geschrieben"]);
    let mut renamed = block_on(n.repo.get_note(note.id())).unwrap().unwrap();
    renamed.rename("Tagebuch", &n.clock).unwrap();
    n.commit(ChangeSet::new().upsert_note(renamed)).unwrap();
    assert_eq!(n.search("tageb"), ["Tagebuch"]);
    n.commit(ChangeSet::new().delete(EntityRef::Note(note.id())))
        .unwrap();
    assert!(n.search("tageb").is_empty());
}
