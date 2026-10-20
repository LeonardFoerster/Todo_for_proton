//! Inhalt speichern (Autosave) und leere neue Notizen verwerfen.

use super::{
    notes::{load_note, Tree},
    Deps,
};
use crate::{
    domain::{Note, NoteId, RevisionId},
    ports::{ChangeSet, EntityRef},
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateNoteBodyInput {
    pub id: NoteId,
    pub text: String,
    /// Revision, die der Editor geladen hatte (`notes.md` §4.2); `None` = noch keine. Muss die
    /// zuletzt vom Core gelieferte sein: Speichern läuft je Notiz nacheinander (§6.2), denn eine
    /// noch nicht hochgeladene Revision wird beim nächsten Speichern ersetzt (§4.6).
    pub base_rev: Option<RevisionId>,
}

/// Speichert den Inhalt als neue Revision auf der Basis `base_rev`. Liefert die Notiz danach:
/// `body` weicht vom gesendeten Text nur ab, wenn inzwischen eine Remote-Revision eintraf und
/// gemergt wurde; `body_rev` ist die neue Basis für das nächste Speichern.
#[derive(Debug, Clone)]
pub struct UpdateNoteBody {
    deps: Deps,
}

impl UpdateNoteBody {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound`, `CoreError::Validation` (größer als 1 MiB, Papierkorb); Fehler des
    /// Repositories.
    pub async fn execute(&self, input: UpdateNoteBodyInput) -> CoreResult<Note> {
        let repo = self.deps.repo.as_ref();
        let mut note = load_note(repo, input.id).await?;
        Tree::load(repo).await?.check_editable(&note)?;
        note.set_body(&input.text, input.base_rev, self.deps.clock.as_ref())?;
        repo.commit(ChangeSet::new().upsert_note(note)).await?;
        load_note(repo, input.id).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscardEmptyNoteInput {
    pub id: NoteId,
}

/// Verwirft eine Notiz, die leer verlassen wird („keine leeren Leichen", `notes.md` §5.3): nur
/// wenn der Inhalt leer ist und der Name noch automatisch. Endgültig, ohne Papierkorb.
#[derive(Debug, Clone)]
pub struct DiscardEmptyNote {
    deps: Deps,
}

impl DiscardEmptyNote {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// `true`, wenn verworfen.
    ///
    /// # Errors
    /// Fehler des Repositories. Eine unbekannte Notiz ist kein Fehler (`false`).
    pub async fn execute(&self, input: DiscardEmptyNoteInput) -> CoreResult<bool> {
        let repo = self.deps.repo.as_ref();
        let Some(note) = repo.get_note(input.id).await? else {
            return Ok(false);
        };
        if !note.body().is_empty() || !note.auto_name() {
            return Ok(false);
        }
        repo.commit(ChangeSet::new().delete(EntityRef::Note(note.id())))
            .await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::notes::test_util::{note, setup},
        domain::{Timestamp, NOTE_BODY_MAX_BYTES},
        ports::{NoteRepository, TaskRepository},
        CoreError,
    };

    #[test]
    fn saves_with_base_and_follows_the_first_line() {
        let (deps, repo) = setup();
        let n = note(&deps, None, "");
        let uc = UpdateNoteBody::new(deps);
        let first = block_on(uc.execute(UpdateNoteBodyInput {
            id: n.id(),
            text: "# Blatt 3\r\n".into(),
            base_rev: None,
        }))
        .unwrap();
        assert_eq!(first.body(), "# Blatt 3\n");
        assert_eq!(first.name().as_str(), "Blatt 3");
        let rev = first.body_rev().expect("rev");
        let second = block_on(uc.execute(UpdateNoteBodyInput {
            id: n.id(),
            text: "# Blatt 3\nAufgabe 1".into(),
            base_rev: Some(rev),
        }))
        .unwrap();
        assert_eq!(second.body(), "# Blatt 3\nAufgabe 1");
        assert_eq!(block_on(repo.get_note(n.id())).unwrap(), Some(second));
    }

    /// Eine veraltete Basis (Remote-Änderung während des Tippens) wird gemergt, nicht
    /// überschrieben.
    #[test]
    fn stale_base_is_merged() {
        let (deps, repo) = setup();
        let n = note(&deps, None, "eins\nzwei\ndrei");
        let base = n.body_rev();
        // Wie nach einem Upload: Die Basis ist herausgegeben, also unveränderlich.
        block_on(repo.freeze_outbox(u64::MAX)).unwrap();
        let uc = UpdateNoteBody::new(deps);
        let newer = block_on(uc.execute(UpdateNoteBodyInput {
            id: n.id(),
            text: "eins\nzwei\ndrei!".into(),
            base_rev: base,
        }))
        .unwrap();
        block_on(repo.freeze_outbox(u64::MAX)).unwrap();
        // Ein zweiter Stand auf derselben (alten) Basis – wie eine Remote-Revision.
        let merged = block_on(uc.execute(UpdateNoteBodyInput {
            id: n.id(),
            text: "eins!\nzwei\ndrei".into(),
            base_rev: base,
        }))
        .unwrap();
        assert_eq!(merged.body(), "eins!\nzwei\ndrei!");
        assert_ne!(merged.body_rev(), newer.body_rev());
    }

    #[test]
    fn rejects_trashed_and_too_large() {
        let (deps, repo) = setup();
        let mut n = note(&deps, None, "x");
        let uc = UpdateNoteBody::new(deps);
        let big = block_on(uc.execute(UpdateNoteBodyInput {
            id: n.id(),
            text: "x".repeat(NOTE_BODY_MAX_BYTES + 1),
            base_rev: n.body_rev(),
        }));
        assert!(matches!(big, Err(CoreError::Validation(_))));
        n.set_trashed(Some(Timestamp::from_millis(1)));
        block_on(repo.commit(ChangeSet::new().upsert_note(n.clone()))).unwrap();
        let trashed = block_on(uc.execute(UpdateNoteBodyInput {
            id: n.id(),
            text: "y".into(),
            base_rev: n.body_rev(),
        }));
        assert!(matches!(trashed, Err(CoreError::Validation(_))));
        assert!(matches!(
            block_on(uc.execute(UpdateNoteBodyInput {
                id: NoteId::new(),
                text: String::new(),
                base_rev: None
            })),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn discards_only_empty_untitled_notes() {
        let (deps, repo) = setup();
        let empty = note(&deps, None, "");
        let full = note(&deps, None, "Inhalt");
        let uc = DiscardEmptyNote::new(deps);
        let run = |id| block_on(uc.execute(DiscardEmptyNoteInput { id })).unwrap();
        assert!(run(empty.id()));
        assert!(!run(empty.id()), "schon weg");
        assert!(!run(full.id()));
        assert!(block_on(repo.get_note(empty.id())).unwrap().is_none());
        assert!(block_on(repo.get_note(full.id())).unwrap().is_some());
    }
}
