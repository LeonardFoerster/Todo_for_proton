//! Ordner importieren (einmalige Kopie, `notes.md` §8). Die Shell liest die Dateien und filtert
//! (versteckt, Endung, Kodierung, Größe, Symlinks); hier entstehen Ordner und Notizen.

use std::collections::BTreeMap;

use super::{notes::append_key, Deps};
use crate::{
    domain::{
        check_body, note_tree::NOTE_TREE_MAX_DEPTH, Note, NoteFolder, NoteFolderId, NoteKind,
        NoteName, SortKey,
    },
    ports::{Change, ChangeSet},
    CoreResult,
};

/// So viele Elemente gehen höchstens in einen Commit (je ein Upload-Auslöser).
pub const IMPORT_CHUNK: usize = 500;

/// Eine eingelesene Datei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportNoteFile {
    /// Unterordner relativ zum importierten Ordner (leer = direkt darin).
    pub dirs: Vec<String>,
    /// Dateiname ohne Endung.
    pub name: String,
    pub kind: NoteKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportNotesInput {
    /// Name des importierten Ordners; wird ein neuer Ordner in der Wurzel.
    pub root: String,
    pub files: Vec<ImportNoteFile>,
}

/// Ein übersprungenes Element mit Grund.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSkip {
    /// Pfad relativ zum importierten Ordner.
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub root: NoteFolderId,
    pub notes: u32,
    /// Angelegte Ordner (ohne den obersten).
    pub folders: u32,
    pub skipped: Vec<ImportSkip>,
}

/// Importiert Dateien als Notizen unter einem neuen Ordner, in Commits zu je ≤ 500 Elementen.
/// Name = Dateiname, kein Auto-Name.
#[derive(Debug, Clone)]
pub struct ImportNotes {
    deps: Deps,
}

impl ImportNotes {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Fehler des Repositories (bereits geschriebene Blöcke bleiben erhalten).
    pub async fn execute(&self, input: ImportNotesInput) -> CoreResult<ImportReport> {
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        let clock = self.deps.clock.as_ref();
        let root_name =
            NoteName::parse(&input.root).map_or_else(|_| "Import".to_owned(), |n| n.to_string());
        let root = NoteFolder::new(None, &root_name, append_key(repo, None).await?, clock)?;
        let root_id = root.id();

        let mut report = ImportReport {
            root: root_id,
            notes: 0,
            folders: 0,
            skipped: Vec::new(),
        };
        let mut pending: Vec<Change> = vec![Change::UpsertNoteFolder(root)];
        // Ordner je Pfad und letzter Sort-Key je Ordner (neue Ordner sind leer).
        let mut folders: BTreeMap<Vec<String>, NoteFolderId> = BTreeMap::new();
        folders.insert(Vec::new(), root_id);
        let mut last_key: BTreeMap<NoteFolderId, SortKey> = BTreeMap::new();
        let mut next_key = |folder: NoteFolderId| -> CoreResult<SortKey> {
            let key = SortKey::between(last_key.get(&folder), None)?;
            last_key.insert(folder, key.clone());
            Ok(key)
        };

        for file in input.files {
            let path = file
                .dirs
                .iter()
                .chain(std::iter::once(&file.name))
                .cloned()
                .collect::<Vec<_>>()
                .join("/");
            let skip = |reason: &str| ImportSkip {
                path: path.clone(),
                reason: reason.to_owned(),
            };
            // Tiefe: oberster Ordner (1) + Unterordner.
            if file.dirs.len() + 1 > NOTE_TREE_MAX_DEPTH {
                report.skipped.push(skip("zu tief verschachtelt"));
                continue;
            }
            let Ok(name) = NoteName::parse(&file.name) else {
                report.skipped.push(skip("ungültiger Name"));
                continue;
            };
            if check_body(&file.text).is_err() {
                report.skipped.push(skip("größer als 1 MiB"));
                continue;
            }
            // Fehlende Ordner auf dem Pfad anlegen.
            let mut parent = root_id;
            let mut invalid_dir = false;
            for depth in 1..=file.dirs.len() {
                let prefix = file.dirs[..depth].to_vec();
                if let Some(id) = folders.get(&prefix) {
                    parent = *id;
                    continue;
                }
                let Ok(folder) = NoteFolder::new(
                    Some(parent),
                    &file.dirs[depth - 1],
                    next_key(parent)?,
                    clock,
                ) else {
                    invalid_dir = true;
                    break;
                };
                parent = folder.id();
                folders.insert(prefix, parent);
                pending.push(Change::UpsertNoteFolder(folder));
                report.folders += 1;
            }
            if invalid_dir {
                report.skipped.push(skip("ungültiger Ordnername"));
                continue;
            }
            let mut note = Note::new(Some(parent), file.kind, next_key(parent)?, clock);
            note.set_body(&file.text, None, clock)?;
            note.rename(name.as_str(), clock)?;
            pending.push(Change::UpsertNote(note));
            report.notes += 1;
            if pending.len() >= IMPORT_CHUNK {
                commit(repo, &mut pending).await?;
            }
        }
        commit(repo, &mut pending).await?;
        Ok(report)
    }
}

async fn commit(
    repo: &dyn crate::ports::TaskRepository,
    pending: &mut Vec<Change>,
) -> CoreResult<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let mut changes = ChangeSet::new();
    for change in pending.drain(..) {
        changes.push(change);
    }
    repo.commit(changes).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::notes::{all_items, test_util::setup},
        domain::NOTE_BODY_MAX_BYTES,
        ports::{NoteRepository, NoteTreeItem, TaskRepository},
    };

    fn file(dirs: &[&str], name: &str, text: &str) -> ImportNoteFile {
        ImportNoteFile {
            dirs: dirs.iter().map(|d| (*d).to_owned()).collect(),
            name: name.into(),
            kind: NoteKind::Markdown,
            text: text.into(),
        }
    }

    #[test]
    fn builds_folders_and_notes_under_a_new_root() {
        let (deps, repo) = setup();
        let report = block_on(ImportNotes::new(deps).execute(ImportNotesInput {
            root: "Vault".into(),
            files: vec![
                file(&[], "Index", "# Start"),
                file(&["Uni", "Analysis"], "Blatt 3", "Text\r\n"),
                file(&["Uni"], "Plan", "Plan"),
                file(&["Uni"], "a/b", "x"),
                file(&[], "Riesig", &"x".repeat(NOTE_BODY_MAX_BYTES + 1)),
            ],
        }))
        .unwrap();
        assert_eq!((report.notes, report.folders), (3, 2));
        assert_eq!(report.skipped.len(), 2);
        assert_eq!(report.skipped[0].path, "Uni/a/b");
        let root = block_on(repo.get_note_folder(report.root))
            .unwrap()
            .unwrap();
        assert_eq!(root.name().as_str(), "Vault");
        assert_eq!(root.parent(), None);
        let items = block_on(all_items(repo.as_ref(), Some(report.root))).unwrap();
        let names: Vec<String> = items
            .iter()
            .map(|i| match i {
                NoteTreeItem::Folder(f) => format!("F:{}", f.name()),
                NoteTreeItem::Note(n) => format!("N:{}", n.name),
            })
            .collect();
        assert_eq!(names, ["N:Index", "F:Uni"], "Reihenfolge wie eingelesen");
        let uni = items
            .iter()
            .find_map(|i| match i {
                NoteTreeItem::Folder(f) => Some(f.id()),
                NoteTreeItem::Note(_) => None,
            })
            .unwrap();
        let inner = block_on(all_items(repo.as_ref(), Some(uni))).unwrap();
        assert_eq!(inner.len(), 2);
        let blatt = block_on(repo.search_notes(&crate::ports::SearchQuery {
            text: "blatt".into(),
            limit: 5,
        }))
        .unwrap();
        let blatt = block_on(repo.get_note(blatt[0].note.id)).unwrap().unwrap();
        assert_eq!(blatt.body(), "Text\n", "LF");
        assert!(!blatt.auto_name());
    }

    #[test]
    fn large_imports_commit_in_chunks() {
        let (deps, repo) = setup();
        let files: Vec<ImportNoteFile> = (0..1_200)
            .map(|i| file(&["A"], &format!("n{i}"), "x"))
            .collect();
        let report = block_on(ImportNotes::new(deps).execute(ImportNotesInput {
            root: ".versteckt".into(),
            files,
        }))
        .unwrap();
        assert_eq!(report.notes, 1_200);
        let root = block_on(repo.get_note_folder(report.root))
            .unwrap()
            .unwrap();
        assert_eq!(root.name().as_str(), "Import", "ungültiger Name ersetzt");
        assert!(block_on(repo.pending_op_count()).unwrap() >= 1_200);
    }
}
