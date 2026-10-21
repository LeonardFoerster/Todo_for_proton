//! Ordner für Notizen, beliebig verschachtelt (`notes.md` §3.1).

use super::{NoteFolderId, NoteName, SortKey, Timestamp};
use crate::{ports::Clock, CoreResult};

/// Ein Ordner. Ordner und Notizen teilen sich unter einem Elternordner die Reihenfolge
/// (`sort_key`). Ob ein Verschieben einen Zyklus oder eine zu tiefe Verschachtelung ergäbe,
/// prüfen die Use-Cases ([`super::note_tree`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteFolder {
    id: NoteFolderId,
    parent: Option<NoteFolderId>,
    name: NoteName,
    sort_key: SortKey,
    created_at: Timestamp,
    trashed_at: Option<Timestamp>,
}

/// Alle Felder eines gespeicherten Ordners, zum Wiederherstellen per [`NoteFolder::from_parts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteFolderParts {
    pub id: NoteFolderId,
    pub parent: Option<NoteFolderId>,
    pub name: String,
    pub sort_key: SortKey,
    pub created_at: Timestamp,
    pub trashed_at: Option<Timestamp>,
}

impl NoteFolder {
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen.
    pub fn from_parts(parts: NoteFolderParts) -> CoreResult<Self> {
        Ok(Self {
            id: parts.id,
            parent: parts.parent,
            name: NoteName::parse(&parts.name)?,
            sort_key: parts.sort_key,
            created_at: parts.created_at,
            trashed_at: parts.trashed_at,
        })
    }

    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen.
    pub fn new(
        parent: Option<NoteFolderId>,
        name: &str,
        sort_key: SortKey,
        clock: &(impl Clock + ?Sized),
    ) -> CoreResult<Self> {
        Ok(Self {
            id: NoteFolderId::new(),
            parent,
            name: NoteName::parse(name)?,
            sort_key,
            created_at: clock.now(),
            trashed_at: None,
        })
    }

    #[must_use]
    pub fn id(&self) -> NoteFolderId {
        self.id
    }

    #[must_use]
    pub fn parent(&self) -> Option<NoteFolderId> {
        self.parent
    }

    #[must_use]
    pub fn name(&self) -> &NoteName {
        &self.name
    }

    #[must_use]
    pub fn sort_key(&self) -> &SortKey {
        &self.sort_key
    }

    #[must_use]
    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    #[must_use]
    pub fn trashed_at(&self) -> Option<Timestamp> {
        self.trashed_at
    }

    /// Liegt der Ordner **selbst** im Papierkorb?
    #[must_use]
    pub fn is_trashed(&self) -> bool {
        self.trashed_at.is_some()
    }

    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen (der Ordner bleibt unverändert).
    pub fn rename(&mut self, name: &str) -> CoreResult<()> {
        self.name = NoteName::parse(name)?;
        Ok(())
    }

    pub fn move_to(&mut self, parent: Option<NoteFolderId>, sort_key: SortKey) {
        self.parent = parent;
        self.sort_key = sort_key;
    }

    pub fn set_trashed(&mut self, at: Option<Timestamp>) {
        self.trashed_at = at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ports::FixedClock, CoreError};

    #[test]
    fn new_rename_move_trash_and_parts() {
        let c = FixedClock(Timestamp::from_millis(3));
        let parent = NoteFolderId::new();
        let mut folder = NoteFolder::new(Some(parent), " Uni ", SortKey::first(), &c).unwrap();
        assert_eq!(folder.name().as_str(), "Uni");
        assert_eq!(folder.parent(), Some(parent));
        assert_eq!(folder.created_at(), Timestamp::from_millis(3));
        assert!(matches!(
            NoteFolder::new(None, "a/b", SortKey::first(), &c),
            Err(CoreError::Validation(_))
        ));

        folder.rename("Studium").unwrap();
        assert!(folder.rename("").is_err());
        assert_eq!(folder.name().as_str(), "Studium");
        folder.move_to(None, SortKey::first());
        assert_eq!(folder.parent(), None);
        folder.set_trashed(Some(Timestamp::from_millis(5)));
        assert!(folder.is_trashed());

        let parts = NoteFolderParts {
            id: folder.id(),
            parent: folder.parent(),
            name: folder.name().to_string(),
            sort_key: folder.sort_key().clone(),
            created_at: folder.created_at(),
            trashed_at: folder.trashed_at(),
        };
        assert_eq!(NoteFolder::from_parts(parts).unwrap(), folder);
    }
}
