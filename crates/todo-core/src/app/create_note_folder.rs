//! Notiz-Ordner anlegen und umbenennen.

use super::{
    notes::{append_key, Tree},
    Deps,
};
use crate::{
    domain::{note_tree, NoteFolder, NoteFolderId},
    ports::ChangeSet,
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateNoteFolderInput {
    /// Elternordner; `None` = Wurzel.
    pub parent: Option<NoteFolderId>,
    pub name: String,
}

/// Legt einen Ordner am Ende des Elternordners an (Tiefe ≤ 32).
#[derive(Debug, Clone)]
pub struct CreateNoteFolder {
    deps: Deps,
}

impl CreateNoteFolder {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound` für einen unbekannten Elternordner, `CoreError::Validation` bei
    /// ungültigem Namen, zu tiefer Verschachtelung oder einem Elternordner im Papierkorb.
    pub async fn execute(&self, input: CreateNoteFolderInput) -> CoreResult<NoteFolder> {
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        let tree = Tree::load(repo).await?;
        tree.check_place(input.parent)?;
        note_tree::check_new_folder(&tree.parents, input.parent)?;
        let key = append_key(repo, input.parent).await?;
        let folder = NoteFolder::new(input.parent, &input.name, key, self.deps.clock.as_ref())?;
        repo.commit(ChangeSet::new().upsert_note_folder(folder.clone()))
            .await?;
        Ok(folder)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameNoteFolderInput {
    pub id: NoteFolderId,
    pub name: String,
}

/// Benennt einen Ordner um (nicht im Papierkorb).
#[derive(Debug, Clone)]
pub struct RenameNoteFolder {
    deps: Deps,
}

impl RenameNoteFolder {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::NotFound`, `CoreError::Validation` (Name, Papierkorb).
    pub async fn execute(&self, input: RenameNoteFolderInput) -> CoreResult<NoteFolder> {
        let repo = self.deps.repo.as_ref();
        let tree = Tree::load(repo).await?;
        let mut folder = tree.folder(input.id)?.clone();
        tree.check_place(Some(folder.id()))?;
        let before = folder.clone();
        folder.rename(&input.name)?;
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
        app::notes::test_util::{folder, setup},
        domain::note_tree::NOTE_TREE_MAX_DEPTH,
        ports::{NoteRepository, TaskRepository},
        CoreError,
    };

    #[test]
    fn creates_nested_folders_up_to_the_depth_limit() {
        let (deps, repo) = setup();
        let mut parent = None;
        for i in 0..NOTE_TREE_MAX_DEPTH {
            parent = Some(folder(&deps, parent, &format!("Ebene {i}")).id());
        }
        let too_deep = block_on(CreateNoteFolder::new(deps).execute(CreateNoteFolderInput {
            parent,
            name: "zu tief".into(),
        }));
        assert!(matches!(too_deep, Err(CoreError::Validation(_))));
        assert_eq!(
            block_on(repo.list_note_folders()).unwrap().len(),
            NOTE_TREE_MAX_DEPTH
        );
    }

    #[test]
    fn rename_validates_and_skips_noop() {
        let (deps, repo) = setup();
        let uni = folder(&deps, None, "Uni");
        block_on(repo.mark_ops_pushed(u64::MAX)).unwrap();
        let uc = RenameNoteFolder::new(deps);
        let input = |name: &str| RenameNoteFolderInput {
            id: uni.id(),
            name: name.into(),
        };
        assert_eq!(block_on(uc.execute(input("Uni"))).unwrap(), uni);
        assert_eq!(block_on(repo.pending_op_count()).unwrap(), 0);
        assert_eq!(
            block_on(uc.execute(input("Studium")))
                .unwrap()
                .name()
                .as_str(),
            "Studium"
        );
        assert!(matches!(
            block_on(uc.execute(input("a/b"))),
            Err(CoreError::Validation(_))
        ));
        assert!(matches!(
            block_on(uc.execute(RenameNoteFolderInput {
                id: NoteFolderId::new(),
                name: "x".into()
            })),
            Err(CoreError::NotFound(_))
        ));
    }
}
