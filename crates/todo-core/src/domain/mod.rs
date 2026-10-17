//! Domain-Modell: Entities und Value Objects (`Task`, `TaskList`, IDs, `SortKey`, `Due`).
//!
//! Reine Logik ohne IO; Invarianten werden in Konstruktoren und Methoden geprüft.

mod due;
mod ids;
mod sort_key;
mod task;
mod task_list;
mod text;
mod timestamp;

pub use due::{Due, LocalDay, NaiveDate, DUE_TZ_MAX_CHARS};
pub use ids::{DeviceId, ParseIdError, TagId, TaskId, TaskListId};
pub use sort_key::SortKey;
pub use task::{Priority, Task, TaskStatus, TASK_TITLE_MAX_CHARS};
pub use task_list::{TaskList, INBOX_NAME, TASK_LIST_NAME_MAX_CHARS};
pub use timestamp::Timestamp;
