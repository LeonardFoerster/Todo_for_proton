//! Domain-Modell: Entities und Value Objects (`Task`, `TaskList`, IDs, `SortKey`, `Due`).
//!
//! Reine Logik ohne IO; Invarianten werden in Konstruktoren und Methoden geprüft.

mod ids;
mod sort_key;
mod task;
mod text;
mod timestamp;

pub use ids::{DeviceId, ParseIdError, TagId, TaskId, TaskListId};
pub use sort_key::SortKey;
pub use task::{Priority, Task, TaskStatus, TASK_TITLE_MAX_CHARS};
pub use timestamp::Timestamp;
