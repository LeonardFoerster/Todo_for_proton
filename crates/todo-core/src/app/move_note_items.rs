//! Notizen und Ordner verschieben bzw. umsortieren (Drag & Drop, `Alt+↑/↓`, „Verschieben nach…").

use super::{
    notes::{key_between, load_note, NoteItem, Tree},
    Deps,
};
use crate::{
    domain::{note_tree, Note, NoteFolder, NoteFolderId, NoteId},
    ports::ChangeSet,
    CoreResult,
};

/// Zielposition: Ordner (`None` = Wurzel) und Nachbarn darin (Notizen oder Ordner). Nur
/// `after` = direkt dahinter, nur `before` = direkt davor, beide = dazwischen, keins = ans Ende.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteTarget {
    pub folder: Option<NoteFolderId>,
    pub after: Option<NoteItem>,
    pub before: Option<NoteItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveNoteInput {
    pub id: NoteId,
    pub target: NoteTarget,
}

/// Verschiebt eine Notiz; ändert nur Ordner und `sort_key`.
#[derive(Debug, Clone)]
pub struct MoveNote {
    deps: Deps,
}

impl MoveNote {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` (Notiz, Ordner), `CoreError::Validation` (Papierkorb, Nachbarn).
    pub async fn execute(&self, input: MoveNoteInput) -> CoreResult<Note> {
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        let mut note = load_note(repo, input.id).await?;
        let tree = Tree::load(repo).await?;
        tree.check_editable(&note)?;
        tree.check_place(input.target.folder)?;
        let t = input.target;
        let key = key_between(repo, t.folder, NoteItem::Note(note.id()), t.after, t.before).await?;
        let before = note.clone();
        note.move_to(t.folder, key);
        if note != before {
            repo.commit(ChangeSet::new().upsert_note(note.clone()))
                .await?;
        }
        Ok(note)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveNoteFolderInput {
    pub id: NoteFolderId,
    /// `target.folder` = neuer Elternordner.
    pub target: NoteTarget,
}

/// Verschiebt einen Ordner samt Inhalt – nicht in sich selbst oder einen Unterordner, Tiefe ≤ 32.
#[derive(Debug, Clone)]
pub struct MoveNoteFolder {
    deps: Deps,
}

impl MoveNoteFolder {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound`, `CoreError::Validation` (Zyklus, Tiefe, Papierkorb, Nachbarn).
    pub async fn execute(&self, input: MoveNoteFolderInput) -> CoreResult<NoteFolder> {
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        let tree = Tree::load(repo).await?;
        let mut folder = tree.folder(input.id)?.clone();
        if tree.folder_in_trash(&folder) {
            tree.check_place(Some(folder.id()))?;
        }
        let t = input.target;
        tree.check_place(t.folder)?;
        note_tree::check_folder_move(&tree.parents, folder.id(), t.folder)?;
        let key = key_between(
            repo,
            t.folder,
            NoteItem::Folder(folder.id()),
            t.after,
            t.before,
        )
        .await?;
        let before = folder.clone();
        folder.move_to(t.folder, key);
        if folder != before {
            repo.commit(ChangeSet::new().upsert_note_folder(folder.clone()))
                .await?;
        }
        Ok(folder)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::notes::{
            all_items,
            test_util::{folder, note, setup},
        },
        ports::{NoteTreeItem, TaskRepository},
        CoreError,
    };

    fn names(repo: &dyn TaskRepository, folder: Option<NoteFolderId>) -> Vec<String> {
        block_on(all_items(repo, folder))
            .unwrap()
            .into_iter()
            .map(|item| match item {
                NoteTreeItem::Folder(f) => f.name().to_string(),
                NoteTreeItem::Note(n) => n.name,
            })
            .collect()
    }

    fn to(
        folder: Option<NoteFolderId>,
        after: Option<NoteItem>,
        before: Option<NoteItem>,
    ) -> NoteTarget {
        NoteTarget {
            folder,
            after,
            before,
        }
    }

    #[test]
    fn notes_reorder_and_move_between_folders() {
        let (deps, repo) = setup();
        let a = note(&deps, None, "a");
        let b = note(&deps, None, "b");
        let uni = folder(&deps, None, "Uni");
        let uc = MoveNote::new(deps.clone());
        let mv = |id, target| block_on(uc.execute(MoveNoteInput { id, target }));
        mv(b.id(), to(None, None, Some(NoteItem::Note(a.id())))).unwrap();
        assert_eq!(names(repo.as_ref(), None), ["b", "a", "Uni"]);
        mv(a.id(), to(None, Some(NoteItem::Folder(uni.id())), None)).unwrap();
        assert_eq!(names(repo.as_ref(), None), ["b", "Uni", "a"]);
        let moved = mv(a.id(), to(Some(uni.id()), None, None)).unwrap();
        assert_eq!(moved.folder(), Some(uni.id()));
        assert_eq!(names(repo.as_ref(), Some(uni.id())), ["a"]);
        assert!(matches!(
            mv(
                b.id(),
                to(Some(uni.id()), Some(NoteItem::Note(b.id())), None)
            ),
            Err(CoreError::Validation(_))
        ));
        assert!(
            matches!(
                mv(b.id(), to(None, Some(NoteItem::Note(a.id())), None)),
                Err(CoreError::Validation(_)),
            ),
            "Nachbar in anderem Ordner"
        );
        assert!(matches!(
            mv(b.id(), to(Some(NoteFolderId::new()), None, None)),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn folders_cannot_move_into_themselves() {
        let (deps, repo) = setup();
        let uni = folder(&deps, None, "Uni");
        let ana = folder(&deps, Some(uni.id()), "Analysis");
        let uc = MoveNoteFolder::new(deps);
        let mv = |id, target| block_on(uc.execute(MoveNoteFolderInput { id, target }));
        assert!(matches!(
            mv(uni.id(), to(Some(ana.id()), None, None)),
            Err(CoreError::Validation(_))
        ));
        assert!(matches!(
            mv(uni.id(), to(Some(uni.id()), None, None)),
            Err(CoreError::Validation(_))
        ));
        let out = mv(ana.id(), to(None, None, Some(NoteItem::Folder(uni.id())))).unwrap();
        assert_eq!(out.parent(), None);
        assert_eq!(names(repo.as_ref(), None), ["Analysis", "Uni"]);
    }
}
