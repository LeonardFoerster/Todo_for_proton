//! Notizen und Ordner in den Papierkorb legen und wiederherstellen (`notes.md` §4.7, §5.3).

use std::collections::BTreeSet;

use super::{
    notes::{append_key, check_note_bulk, NoteItem, Tree},
    Deps,
};
use crate::{
    domain::SortKey,
    ports::{Change, ChangeSet, TaskRepository},
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashNoteItemsInput {
    pub items: Vec<NoteItem>,
}

/// Legt Elemente in den Papierkorb (`Trashed = jetzt`, ein LWW-Feld, kein Tombstone). Ein
/// Ordner nimmt seinen Inhalt mit, ohne ihn zu ändern. Unbekannte und schon gelöschte werden
/// übersprungen; liefert die tatsächlich geänderten (für „Rückgängig").
#[derive(Debug, Clone)]
pub struct TrashNoteItems {
    deps: Deps,
}

impl TrashNoteItems {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::Validation` bei mehr als [`super::NOTE_BULK_LIMIT_MAX`] Elementen; Fehler des
    /// Repositories.
    pub async fn execute(&self, input: TrashNoteItemsInput) -> CoreResult<Vec<NoteItem>> {
        check_note_bulk(input.items.len())?;
        let repo = self.deps.repo.as_ref();
        let tree = Tree::load(repo).await?;
        let now = self.deps.clock.now();
        let mut changes = ChangeSet::new();
        let mut changed = Vec::new();
        for item in dedup(input.items) {
            match item {
                NoteItem::Note(id) => {
                    if let Some(mut note) = repo.get_note(id).await? {
                        if !note.is_trashed() {
                            note.set_trashed(Some(now));
                            changes.push(Change::UpsertNote(note));
                            changed.push(item);
                        }
                    }
                }
                NoteItem::Folder(id) => {
                    if let Some(folder) = tree.folders.get(&id) {
                        if !folder.is_trashed() {
                            let mut folder = folder.clone();
                            folder.set_trashed(Some(now));
                            changes.push(Change::UpsertNoteFolder(folder));
                            changed.push(item);
                        }
                    }
                }
            }
        }
        if !changes.is_empty() {
            repo.commit(changes).await?;
        }
        Ok(changed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreNoteItemsInput {
    pub items: Vec<NoteItem>,
}

/// Holt Elemente aus dem Papierkorb. Liegt der Elternordner danach selbst noch im Papierkorb
/// (oder fehlt), landet das Element am Ende der Wurzel. Liefert die tatsächlich geänderten.
#[derive(Debug, Clone)]
pub struct RestoreNoteItems {
    deps: Deps,
}

impl RestoreNoteItems {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::Validation` (Obergrenze); Fehler des Repositories.
    pub async fn execute(&self, input: RestoreNoteItemsInput) -> CoreResult<Vec<NoteItem>> {
        check_note_bulk(input.items.len())?;
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        let items = dedup(input.items);
        // Baum, wie er nach dem Wiederherstellen der Ordner aussieht.
        let mut folders = repo.list_note_folders().await?;
        for folder in &mut folders {
            if items.contains(&NoteItem::Folder(folder.id())) {
                folder.set_trashed(None);
            }
        }
        let tree = Tree::from_folders(folders);
        let mut root_key: Option<SortKey> = None;
        let mut changes = ChangeSet::new();
        let mut changed = Vec::new();
        for item in items {
            match item {
                NoteItem::Note(id) => {
                    let Some(mut note) = repo.get_note(id).await? else {
                        continue;
                    };
                    let homeless = tree.in_trash(note.folder());
                    if !note.is_trashed() && !homeless {
                        continue;
                    }
                    note.set_trashed(None);
                    if homeless {
                        note.move_to(None, next_root_key(repo, &mut root_key).await?);
                    }
                    changes.push(Change::UpsertNote(note));
                }
                NoteItem::Folder(id) => {
                    let Some(mut folder) = tree.folders.get(&id).cloned() else {
                        continue;
                    };
                    let homeless = tree.in_trash(folder.parent());
                    let stored = repo.get_note_folder(id).await?;
                    if !stored.is_some_and(|f| f.is_trashed()) && !homeless {
                        continue;
                    }
                    if homeless {
                        folder.move_to(None, next_root_key(repo, &mut root_key).await?);
                    }
                    changes.push(Change::UpsertNoteFolder(folder));
                }
            }
            changed.push(item);
        }
        if !changes.is_empty() {
            repo.commit(changes).await?;
        }
        Ok(changed)
    }
}

/// Nächster Schlüssel am Ende der Wurzel (mehrere Elemente nacheinander).
async fn next_root_key(
    repo: &dyn TaskRepository,
    last: &mut Option<SortKey>,
) -> CoreResult<SortKey> {
    let key = match last.as_ref() {
        Some(previous) => SortKey::between(Some(previous), None)?,
        None => append_key(repo, None).await?,
    };
    *last = Some(key.clone());
    Ok(key)
}

/// Doppelte entfernen (erstes Vorkommen bleibt).
pub(crate) fn dedup(items: Vec<NoteItem>) -> Vec<NoteItem> {
    let mut seen = BTreeSet::new();
    items.into_iter().filter(|i| seen.insert(*i)).collect()
}
