//! Neue Notiz anlegen.

use super::{
    notes::{append_key, Tree},
    Deps,
};
use crate::{
    domain::{Note, NoteFolderId, NoteKind},
    ports::ChangeSet,
    CoreResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateNoteInput {
    /// Zielordner; `None` = Wurzel.
    pub folder: Option<NoteFolderId>,
    pub kind: NoteKind,
}

/// Legt eine leere Notiz „Unbenannt" (Auto-Name) am Ende des Ordners an.
#[derive(Debug, Clone)]
pub struct CreateNote {
    deps: Deps,
}

impl CreateNote {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für einen unbekannten Ordner, `CoreError::Validation` für einen
    /// Ordner im Papierkorb; Fehler des Repositories.
    pub async fn execute(&self, input: CreateNoteInput) -> CoreResult<Note> {
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        Tree::load(repo).await?.check_place(input.folder)?;
        let key = append_key(repo, input.folder).await?;
        let note = Note::new(input.folder, input.kind, key, self.deps.clock.as_ref());
        repo.commit(ChangeSet::new().upsert_note(note.clone()))
            .await?;
        Ok(note)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::notes::test_util::{folder, note, setup},
        domain::{NoteFolderId, Timestamp},
        ports::{EntityRef, NoteRepository, TaskRepository},
        CoreError,
    };

    #[test]
    fn appends_untitled_notes_in_the_folder() {
        let (deps, repo) = setup();
        let uni = folder(&deps, None, "Uni");
        let a = note(&deps, Some(uni.id()), "");
        let b = note(&deps, Some(uni.id()), "");
        assert_eq!(a.folder(), Some(uni.id()));
        assert!(a.sort_key() < b.sort_key());
        assert_eq!(a.name().as_str(), "Unbenannt");
        assert!(a.auto_name());
        assert_eq!(block_on(repo.get_note(a.id())).unwrap(), Some(a));
        let root = note(&deps, None, "");
        assert!(
            root.sort_key() > uni.sort_key(),
            "Ordner und Notizen teilen die Reihenfolge"
        );
    }

    #[test]
    fn rejects_unknown_or_trashed_folders() {
        let (deps, repo) = setup();
        let uc = CreateNote::new(deps.clone());
        let input = |folder| CreateNoteInput {
            folder,
            kind: NoteKind::Text,
        };
        assert!(matches!(
            block_on(uc.execute(input(Some(NoteFolderId::new())))),
            Err(CoreError::NotFound(_))
        ));
        let mut old = folder(&deps, None, "Alt");
        old.set_trashed(Some(Timestamp::from_millis(1)));
        block_on(repo.commit(ChangeSet::new().upsert_note_folder(old.clone()))).unwrap();
        assert!(matches!(
            block_on(uc.execute(input(Some(old.id())))),
            Err(CoreError::Validation(_))
        ));
        let pending = block_on(repo.pending_ops(100)).unwrap();
        assert!(pending
            .iter()
            .all(|op| !matches!(op.entity, EntityRef::Note(_))));
    }
}
