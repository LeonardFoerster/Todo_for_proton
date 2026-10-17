//! Domain-Modell: Entities und Value Objects (`Task`, `TaskList`, IDs, `SortKey`, `Due`).
//!
//! Reine Logik ohne IO; Invarianten werden in Konstruktoren und Methoden geprüft.

mod ids;
mod timestamp;

pub use ids::{DeviceId, ParseIdError, TagId, TaskId, TaskListId};
pub use timestamp::Timestamp;
