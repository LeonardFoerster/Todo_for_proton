//! Gemeinsame Hilfen der Notiz-Use-Cases: Baum laden, Orte prüfen, Positionen berechnen.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    domain::{
        note_tree::{self, FolderParents},
        Note, NoteFolder, NoteFolderId, NoteId, SortKey,
    },
    ports::{FolderItemsQuery, NoteTreeItem, PageRequest, TaskRepository, PAGE_LIMIT_MAX},
    CoreError, CoreResult,
};

/// Ein Element im Notiz-Baum (Ordner und Notizen teilen sich die Reihenfolge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NoteItem {
    Note(NoteId),
    Folder(NoteFolderId),
}

/// Höchstzahl an Elementen je Sammelaufruf (Papierkorb, Wiederherstellen, Löschen).
pub const NOTE_BULK_LIMIT_MAX: usize = 2_000;

pub(crate) fn check_note_bulk(len: usize) -> CoreResult<()> {
    if len > NOTE_BULK_LIMIT_MAX {
        return Err(CoreError::Validation(format!(
            "at most {NOTE_BULK_LIMIT_MAX} items per call, got {len}"
        )));
    }
    Ok(())
}

pub(crate) async fn load_note(repo: &dyn TaskRepository, id: NoteId) -> CoreResult<Note> {
    repo.get_note(id)
        .await?
        .ok_or_else(|| CoreError::NotFound(format!("note {id}")))
}

/// Alle Ordner mit Baum-Regeln (die gespeicherten Eltern sind schon wirksam, siehe
/// `sync::replica`).
#[derive(Debug, Clone)]
pub(crate) struct Tree {
    pub(crate) folders: BTreeMap<NoteFolderId, NoteFolder>,
    pub(crate) parents: FolderParents,
    trashed: BTreeSet<NoteFolderId>,
}

impl Tree {
    pub(crate) async fn load(repo: &dyn TaskRepository) -> CoreResult<Self> {
        Ok(Self::from_folders(repo.list_note_folders().await?))
    }

    pub(crate) fn from_folders(list: Vec<NoteFolder>) -> Self {
        let parents = list.iter().map(|f| (f.id(), f.parent())).collect();
        let trashed = list
            .iter()
            .filter(|f| f.is_trashed())
            .map(NoteFolder::id)
            .collect();
        Self {
            folders: list.into_iter().map(|f| (f.id(), f)).collect(),
            parents,
            trashed,
        }
    }

    pub(crate) fn folder(&self, id: NoteFolderId) -> CoreResult<&NoteFolder> {
        self.folders
            .get(&id)
            .ok_or_else(|| CoreError::NotFound(format!("folder {id}")))
    }

    /// Liegt ein Element in `folder` wegen eines Ordners im Papierkorb?
    pub(crate) fn in_trash(&self, folder: Option<NoteFolderId>) -> bool {
        note_tree::in_trashed_folder(&self.parents, &self.trashed, folder)
    }

    /// Ziel für neue oder verschobene Elemente: existiert und liegt nicht im Papierkorb.
    pub(crate) fn check_place(&self, folder: Option<NoteFolderId>) -> CoreResult<()> {
        if let Some(id) = folder {
            self.folder(id)?;
        }
        if self.in_trash(folder) {
            return Err(CoreError::Validation(
                "items in the trash cannot be changed; restore them first".into(),
            ));
        }
        Ok(())
    }

    /// Ist das Element selbst oder über einen Ordner im Papierkorb?
    pub(crate) fn note_in_trash(&self, note: &Note) -> bool {
        note.is_trashed() || self.in_trash(note.folder())
    }

    pub(crate) fn folder_in_trash(&self, folder: &NoteFolder) -> bool {
        folder.is_trashed() || self.in_trash(folder.parent())
    }

    /// Notizen im Papierkorb lassen sich nur lesen (`notes.md` §5.3).
    pub(crate) fn check_editable(&self, note: &Note) -> CoreResult<()> {
        if self.note_in_trash(note) {
            return Err(CoreError::Validation(
                "notes in the trash are read-only; restore them first".into(),
            ));
        }
        Ok(())
    }
}

/// Alle Einträge eines Ordners (auch im Papierkorb), seitenweise geladen.
pub(crate) async fn all_items(
    repo: &dyn TaskRepository,
    folder: Option<NoteFolderId>,
) -> CoreResult<Vec<NoteTreeItem>> {
    let mut items = Vec::new();
    let mut page = PageRequest::first(PAGE_LIMIT_MAX);
    loop {
        let result = repo
            .list_folder_items(
                FolderItemsQuery {
                    folder,
                    include_trashed: true,
                },
                page,
            )
            .await?;
        items.extend(result.items);
        match result.next_cursor {
            Some(cursor) => page = PageRequest::after(cursor, PAGE_LIMIT_MAX),
            None => return Ok(items),
        }
    }
}

/// Schlüssel für „ans Ende von `folder`".
pub(crate) async fn append_key(
    repo: &dyn TaskRepository,
    folder: Option<NoteFolderId>,
) -> CoreResult<SortKey> {
    SortKey::between(repo.last_note_sort_key(folder).await?.as_ref(), None)
}

fn item_of(entry: &NoteTreeItem) -> NoteItem {
    match entry {
        NoteTreeItem::Folder(f) => NoteItem::Folder(f.id()),
        NoteTreeItem::Note(n) => NoteItem::Note(n.id),
    }
}

/// Schlüssel für eine Position in `folder` neben `after`/`before` (Semantik wie bei Tasks:
/// nur `after` = direkt dahinter, nur `before` = direkt davor, beide = dazwischen, keins = ans
/// Ende). `moving` wird bei der Nachbarsuche ausgelassen.
pub(crate) async fn key_between(
    repo: &dyn TaskRepository,
    folder: Option<NoteFolderId>,
    moving: NoteItem,
    after: Option<NoteItem>,
    before: Option<NoteItem>,
) -> CoreResult<SortKey> {
    if after == Some(moving) || before == Some(moving) {
        return Err(CoreError::Validation(
            "an item cannot be its own neighbour".into(),
        ));
    }
    if after.is_none() && before.is_none() {
        return append_key(repo, folder).await;
    }
    let siblings: Vec<NoteTreeItem> = all_items(repo, folder)
        .await?
        .into_iter()
        .filter(|entry| item_of(entry) != moving)
        .collect();
    let index_of = |item: NoteItem| {
        siblings
            .iter()
            .position(|entry| item_of(entry) == item)
            .ok_or_else(|| {
                CoreError::Validation(format!("neighbour {item:?} is not in the folder"))
            })
    };
    let (lower, upper) = match (after, before) {
        (Some(a), Some(b)) => (Some(index_of(a)?), Some(index_of(b)?)),
        (Some(a), None) => {
            let a = index_of(a)?;
            (Some(a), (a + 1 < siblings.len()).then_some(a + 1))
        }
        (None, Some(b)) => {
            let b = index_of(b)?;
            (b.checked_sub(1), Some(b))
        }
        (None, None) => unreachable!("oben behandelt"),
    };
    let key = |i: Option<usize>| i.map(|i| siblings[i].position().0);
    let (lower, upper) = (key(lower), key(upper));
    // Gleiche Keys der Nachbarn (parallele Verschiebungen): direkt hinter den unteren.
    match (&lower, &upper) {
        (Some(l), Some(u)) if l >= u => SortKey::between(Some(l), None),
        _ => SortKey::between(lower.as_ref(), upper.as_ref()),
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use futures::executor::block_on;

    use crate::{
        app::{
            test_util::deps, CreateNote, CreateNoteFolder, CreateNoteFolderInput, CreateNoteInput,
            Deps, UpdateNoteBody, UpdateNoteBodyInput,
        },
        domain::{Note, NoteFolder, NoteFolderId, NoteKind},
        testing::InMemoryTaskRepository,
    };

    pub(crate) fn setup() -> (Deps, std::sync::Arc<InMemoryTaskRepository>) {
        deps()
    }

    pub(crate) fn note(deps: &Deps, folder: Option<NoteFolderId>, body: &str) -> Note {
        let note = block_on(CreateNote::new(deps.clone()).execute(CreateNoteInput {
            folder,
            kind: NoteKind::Markdown,
        }))
        .unwrap();
        if body.is_empty() {
            return note;
        }
        block_on(
            UpdateNoteBody::new(deps.clone()).execute(UpdateNoteBodyInput {
                id: note.id(),
                text: body.into(),
                base_rev: note.body_rev(),
            }),
        )
        .unwrap()
    }

    pub(crate) fn folder(deps: &Deps, parent: Option<NoteFolderId>, name: &str) -> NoteFolder {
        block_on(
            CreateNoteFolder::new(deps.clone()).execute(CreateNoteFolderInput {
                parent,
                name: name.into(),
            }),
        )
        .unwrap()
    }
}
