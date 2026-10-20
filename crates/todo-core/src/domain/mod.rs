//! Domain-Modell: Entities und Value Objects (`Task`, `TaskList`, `Note`, `NoteFolder`, IDs,
//! `SortKey`, `Due`).
//!
//! Reine Logik ohne IO; Invarianten werden in Konstruktoren und Methoden geprüft.

mod due;
mod ids;
mod note;
mod note_folder;
mod note_name;
pub mod note_tree;
mod sort_key;
mod task;
mod task_list;
mod text;
mod timestamp;

pub use due::{Due, LocalDay, NaiveDate, DUE_TZ_MAX_CHARS};
pub use ids::{DeviceId, NoteFolderId, NoteId, ParseIdError, TagId, TaskId, TaskListId};
pub use note::{
    check_body, normalize_line_endings, Note, NoteKind, NoteParts, RevisionId, NOTE_BODY_MAX_BYTES,
};
pub use note_folder::{NoteFolder, NoteFolderParts};
pub use note_name::{NoteName, NOTE_NAME_MAX_CHARS, UNTITLED_NOTE_NAME};
pub use sort_key::SortKey;
pub use task::{Priority, Task, TaskParts, TaskStatus, TASK_TITLE_MAX_CHARS};
pub use task_list::{TaskList, TaskListParts, INBOX_NAME, TASK_LIST_NAME_MAX_CHARS};
pub use timestamp::Timestamp;
