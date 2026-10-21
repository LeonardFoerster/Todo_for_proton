//! Endgültig löschen: einzeln, „Papierkorb leeren", automatisch nach 30 Tagen; dazu die Liste
//! des Papierkorbs (`notes.md` §4.7, §5.3).

use std::collections::BTreeSet;

use super::{
    notes::{all_items, check_note_bulk, NoteItem, Tree},
    trash_notes::dedup,
    Deps,
};
use crate::{
    domain::{note_tree, NoteFolderId, NoteId, Timestamp},
    ports::{ChangeSet, EntityRef, NoteTreeItem, TaskRepository},
    CoreResult,
};

/// Nach so langer Zeit im Papierkorb wird ein Element automatisch gelöscht.
pub const NOTE_TRASH_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Löscht `items` und alle bekannten Nachfahren in **einem** Commit (`Delete`, Delete gewinnt).
/// Nachfahren, die dieses Gerät nicht kennt, werden auf den anderen Geräten zu Waisen in der
/// Wurzel („Notiz retten"). Liefert die Zahl gelöschter Notizen und Ordner.
async fn purge(repo: &dyn TaskRepository, items: &[NoteItem]) -> CoreResult<u32> {
    let tree = Tree::load(repo).await?;
    let mut folders: BTreeSet<NoteFolderId> = BTreeSet::new();
    let mut notes: BTreeSet<NoteId> = BTreeSet::new();
    for item in items {
        match item {
            NoteItem::Note(id) => {
                if repo.get_note(*id).await?.is_some() {
                    notes.insert(*id);
                }
            }
            NoteItem::Folder(id) if tree.folders.contains_key(id) => {
                folders.insert(*id);
                folders.extend(note_tree::descendants(&tree.parents, *id));
            }
            NoteItem::Folder(_) => {}
        }
    }
    for folder in &folders {
        for item in all_items(repo, Some(*folder)).await? {
            if let NoteTreeItem::Note(note) = item {
                notes.insert(note.id);
            }
        }
    }
    let count = u32::try_from(notes.len() + folders.len()).unwrap_or(u32::MAX);
    if count == 0 {
        return Ok(0);
    }
    let mut changes = ChangeSet::new();
    for id in notes {
        changes = changes.delete(EntityRef::Note(id));
    }
    for id in folders {
        changes = changes.delete(EntityRef::NoteFolder(id));
    }
    repo.commit(changes).await?;
    Ok(count)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgeNoteItemsInput {
    pub items: Vec<NoteItem>,
}

/// Löscht Elemente endgültig (aus dem Papierkorb heraus; nicht rückgängig zu machen).
#[derive(Debug, Clone)]
pub struct PurgeNoteItems {
    deps: Deps,
}

impl PurgeNoteItems {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Zahl gelöschter Elemente (inkl. Nachfahren).
    ///
    /// # Errors
    /// `CoreError::Validation` (Obergrenze); Fehler des Repositories.
    pub async fn execute(&self, input: PurgeNoteItemsInput) -> CoreResult<u32> {
        check_note_bulk(input.items.len())?;
        purge(self.deps.repo.as_ref(), &dedup(input.items)).await
    }
}

/// Alle Elemente, die selbst im Papierkorb liegen – optional nur solche, die dort seit vor
/// `before` liegen.
async fn trashed_items(
    repo: &dyn TaskRepository,
    before: Option<Timestamp>,
) -> CoreResult<Vec<NoteItem>> {
    let old_enough = |at: Option<Timestamp>| at.is_some_and(|at| before.is_none_or(|b| at < b));
    let mut items: Vec<NoteItem> = repo
        .list_note_folders()
        .await?
        .iter()
        .filter(|f| old_enough(f.trashed_at()))
        .map(|f| NoteItem::Folder(f.id()))
        .collect();
    items.extend(
        repo.list_trashed_notes()
            .await?
            .iter()
            .filter(|n| old_enough(n.trashed_at))
            .map(|n| NoteItem::Note(n.id)),
    );
    Ok(items)
}

/// Leert den Papierkorb.
#[derive(Debug, Clone)]
pub struct EmptyNoteTrash {
    deps: Deps,
}

impl EmptyNoteTrash {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Zahl gelöschter Elemente.
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self) -> CoreResult<u32> {
        let repo = self.deps.repo.as_ref();
        purge(repo, &trashed_items(repo, None).await?).await
    }
}

/// Löscht, was seit mehr als 30 Tagen im Papierkorb liegt. Beim Start und höchstens einmal
/// täglich aufrufen, nie per Timer im Leerlauf. Doppelte Deletes von zwei Geräten sind harmlos.
#[derive(Debug, Clone)]
pub struct PurgeExpiredNotes {
    deps: Deps,
}

impl PurgeExpiredNotes {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// Zahl gelöschter Elemente.
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self) -> CoreResult<u32> {
        let repo = self.deps.repo.as_ref();
        let cutoff = self.deps.clock.now().plus_millis(-NOTE_TRASH_RETENTION_MS);
        purge(repo, &trashed_items(repo, Some(cutoff)).await?).await
    }
}

/// Ein Eintrag der Papierkorb-Liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteTrashEntry {
    pub item: NoteTreeItem,
    /// Namen der Ordner über dem Element, von der Wurzel an (Herkunft).
    pub path: Vec<String>,
    pub trashed_at: Timestamp,
}

/// Die **obersten** Elemente im Papierkorb: der Ordner, nicht jede Notiz darin. Neueste zuerst.
#[derive(Debug, Clone)]
pub struct ListNoteTrash {
    deps: Deps,
}

impl ListNoteTrash {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Fehler des Repositories.
    pub async fn execute(&self) -> CoreResult<Vec<NoteTrashEntry>> {
        let repo = self.deps.repo.as_ref();
        let tree = Tree::load(repo).await?;
        let path = |folder: Option<NoteFolderId>| -> Vec<String> {
            let Some(id) = folder else {
                return Vec::new();
            };
            let mut chain = note_tree::ancestors(&tree.parents, id);
            chain.reverse();
            chain.push(id);
            chain
                .iter()
                .filter_map(|f| tree.folders.get(f))
                .map(|f| f.name().to_string())
                .collect()
        };
        let mut out = Vec::new();
        for folder in tree.folders.values() {
            if let Some(at) = folder.trashed_at() {
                if !tree.in_trash(folder.parent()) {
                    out.push(NoteTrashEntry {
                        path: path(folder.parent()),
                        item: NoteTreeItem::Folder(folder.clone()),
                        trashed_at: at,
                    });
                }
            }
        }
        for note in repo.list_trashed_notes().await? {
            if !tree.in_trash(note.folder) {
                out.push(NoteTrashEntry {
                    path: path(note.folder),
                    trashed_at: note.trashed_at.unwrap_or(Timestamp::EPOCH),
                    item: NoteTreeItem::Note(note),
                });
            }
        }
        out.sort_by(|a, b| {
            b.trashed_at
                .cmp(&a.trashed_at)
                .then(a.item.position().cmp(&b.item.position()))
        });
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{
            notes::test_util::{folder, note, setup},
            RestoreNoteItems, RestoreNoteItemsInput, TrashNoteItems, TrashNoteItemsInput,
        },
        ports::NoteRepository,
    };

    fn trash(deps: &Deps, items: Vec<NoteItem>) -> Vec<NoteItem> {
        block_on(TrashNoteItems::new(deps.clone()).execute(TrashNoteItemsInput { items })).unwrap()
    }

    fn restore(deps: &Deps, items: Vec<NoteItem>) -> Vec<NoteItem> {
        block_on(RestoreNoteItems::new(deps.clone()).execute(RestoreNoteItemsInput { items }))
            .unwrap()
    }

    fn trash_names(deps: &Deps) -> Vec<(String, Vec<String>)> {
        block_on(ListNoteTrash::new(deps.clone()).execute())
            .unwrap()
            .into_iter()
            .map(|e| {
                let name = match &e.item {
                    NoteTreeItem::Folder(f) => f.name().to_string(),
                    NoteTreeItem::Note(n) => n.name.clone(),
                };
                (name, e.path)
            })
            .collect()
    }

    #[test]
    fn trash_lists_top_items_and_restore_undoes() {
        let (deps, repo) = setup();
        let uni = folder(&deps, None, "Uni");
        let ana = folder(&deps, Some(uni.id()), "Analysis");
        let blatt = note(&deps, Some(ana.id()), "Blatt 3");
        let lose = note(&deps, Some(uni.id()), "Lose");
        assert_eq!(
            trash(
                &deps,
                vec![NoteItem::Note(lose.id()), NoteItem::Note(lose.id())]
            ),
            [NoteItem::Note(lose.id())]
        );
        trash(&deps, vec![NoteItem::Folder(ana.id())]);
        assert!(
            trash(&deps, vec![NoteItem::Folder(ana.id())]).is_empty(),
            "schon drin"
        );
        let listed = trash_names(&deps);
        assert_eq!(listed.len(), 2);
        assert!(listed.contains(&("Analysis".into(), vec!["Uni".into()])));
        assert!(listed.contains(&("Lose".into(), vec!["Uni".into()])));
        // Eine Notiz im Papierkorb-Ordner ist nicht selbst gelistet.
        assert!(!listed.iter().any(|(n, _)| n == "Blatt 3"));

        assert_eq!(restore(&deps, vec![NoteItem::Folder(ana.id())]).len(), 1);
        let back = block_on(repo.get_note(blatt.id())).unwrap().unwrap();
        assert_eq!(back.folder(), Some(ana.id()), "Inhalt kommt mit");
        assert_eq!(trash_names(&deps).len(), 1);
    }

    #[test]
    fn restore_into_trashed_folder_goes_to_the_root() {
        let (deps, repo) = setup();
        let uni = folder(&deps, None, "Uni");
        let n = note(&deps, Some(uni.id()), "Text");
        trash(&deps, vec![NoteItem::Note(n.id())]);
        trash(&deps, vec![NoteItem::Folder(uni.id())]);
        restore(&deps, vec![NoteItem::Note(n.id())]);
        let back = block_on(repo.get_note(n.id())).unwrap().unwrap();
        assert_eq!(back.folder(), None);
        assert!(!back.is_trashed());
        assert!(back.sort_key() > uni.sort_key(), "ans Ende der Wurzel");
    }

    #[test]
    fn purge_takes_all_descendants_and_empty_trash_everything() {
        let (deps, repo) = setup();
        let uni = folder(&deps, None, "Uni");
        let ana = folder(&deps, Some(uni.id()), "Analysis");
        let deep = note(&deps, Some(ana.id()), "tief");
        let other = note(&deps, None, "bleibt");
        let gone = note(&deps, None, "weg");
        trash(
            &deps,
            vec![NoteItem::Folder(uni.id()), NoteItem::Note(gone.id())],
        );
        let n = block_on(
            PurgeNoteItems::new(deps.clone()).execute(PurgeNoteItemsInput {
                items: vec![NoteItem::Folder(uni.id())],
            }),
        )
        .unwrap();
        assert_eq!(n, 3, "zwei Ordner und eine Notiz");
        assert!(block_on(repo.get_note(deep.id())).unwrap().is_none());
        assert!(block_on(repo.get_note_folder(ana.id())).unwrap().is_none());
        assert_eq!(
            block_on(EmptyNoteTrash::new(deps.clone()).execute()).unwrap(),
            1
        );
        assert!(block_on(repo.get_note(gone.id())).unwrap().is_none());
        assert!(block_on(repo.get_note(other.id())).unwrap().is_some());
        assert_eq!(block_on(EmptyNoteTrash::new(deps).execute()).unwrap(), 0);
    }

    #[test]
    fn expired_items_are_purged_after_30_days() {
        use std::sync::Arc;

        use crate::{
            app::Deps,
            ports::{ChangeSet, FixedClock},
        };
        let (deps, repo) = setup();
        let old = note(&deps, None, "alt");
        let fresh = note(&deps, None, "frisch");
        let now = Timestamp::from_millis(100 * 24 * 60 * 60 * 1000);
        for (n, age_days) in [(&old, 31), (&fresh, 29)] {
            let mut n = block_on(repo.get_note(n.id())).unwrap().unwrap();
            n.set_trashed(Some(now.plus_millis(-age_days * 24 * 60 * 60 * 1000)));
            block_on(repo.commit(ChangeSet::new().upsert_note(n))).unwrap();
        }
        let later = Deps::new(repo.clone(), Arc::new(FixedClock(now)));
        assert_eq!(
            block_on(PurgeExpiredNotes::new(later).execute()).unwrap(),
            1
        );
        assert!(block_on(repo.get_note(old.id())).unwrap().is_none());
        assert!(block_on(repo.get_note(fresh.id())).unwrap().is_some());
    }
}
