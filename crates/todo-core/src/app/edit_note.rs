//! Notiz umbenennen und Typ ändern.

use super::{
    notes::{load_note, Tree},
    Deps,
};
use crate::{
    domain::{Note, NoteId, NoteKind},
    ports::ChangeSet,
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameNoteInput {
    pub id: NoteId,
    pub name: String,
}

/// Benennt eine Notiz von Hand um; der Auto-Name ist danach aus. Gleichnamige Notizen im selben
/// Ordner sind erlaubt (die UI warnt nur).
#[derive(Debug, Clone)]
pub struct RenameNote {
    deps: Deps,
}

impl RenameNote {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound`, `CoreError::Validation` (Name, Papierkorb).
    pub async fn execute(&self, input: RenameNoteInput) -> CoreResult<Note> {
        let repo = self.deps.repo.as_ref();
        let mut note = load_note(repo, input.id).await?;
        Tree::load(repo).await?.check_editable(&note)?;
        let before = note.clone();
        note.rename(&input.name, self.deps.clock.as_ref())?;
        if note != before {
            repo.commit(ChangeSet::new().upsert_note(note.clone()))
                .await?;
        }
        Ok(note)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetNoteKindInput {
    pub id: NoteId,
    pub kind: NoteKind,
}

/// `.txt` ↔ `.md`; der Inhalt bleibt unverändert.
#[derive(Debug, Clone)]
pub struct SetNoteKind {
    deps: Deps,
}

impl SetNoteKind {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound`, `CoreError::Validation` (Papierkorb).
    pub async fn execute(&self, input: SetNoteKindInput) -> CoreResult<Note> {
        let repo = self.deps.repo.as_ref();
        let mut note = load_note(repo, input.id).await?;
        Tree::load(repo).await?.check_editable(&note)?;
        if note.kind() != input.kind {
            note.set_kind(input.kind);
            repo.commit(ChangeSet::new().upsert_note(note.clone()))
                .await?;
        }
        Ok(note)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::notes::test_util::{note, setup},
        ports::TaskRepository,
        CoreError,
    };

    #[test]
    fn rename_turns_auto_name_off_and_kind_keeps_the_body() {
        let (deps, repo) = setup();
        let n = note(&deps, None, "# Titel\nText");
        let renamed = block_on(RenameNote::new(deps.clone()).execute(RenameNoteInput {
            id: n.id(),
            name: "Eigener Name".into(),
        }))
        .unwrap();
        assert_eq!(renamed.name().as_str(), "Eigener Name");
        assert!(!renamed.auto_name());
        assert!(matches!(
            block_on(RenameNote::new(deps.clone()).execute(RenameNoteInput {
                id: n.id(),
                name: ".x".into()
            })),
            Err(CoreError::Validation(_))
        ));
        block_on(repo.mark_ops_pushed(u64::MAX)).unwrap();
        let uc = SetNoteKind::new(deps);
        let text = block_on(uc.execute(SetNoteKindInput {
            id: n.id(),
            kind: NoteKind::Text,
        }))
        .unwrap();
        assert_eq!(text.kind(), NoteKind::Text);
        assert_eq!(text.body(), "# Titel\nText");
        assert_eq!(block_on(repo.pending_op_count()).unwrap(), 1, "ein Set");
        block_on(uc.execute(SetNoteKindInput {
            id: n.id(),
            kind: NoteKind::Text,
        }))
        .unwrap();
        assert_eq!(
            block_on(repo.pending_op_count()).unwrap(),
            1,
            "unverändert: nichts"
        );
    }
}
