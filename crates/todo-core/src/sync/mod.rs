//! Sync-Datenmodell: HLC, Ops, Diff und Merge – reine Logik ohne IO.
//!
//! Liegt im Core (nicht in `todo-sync`), weil der [`TaskRepository`](crate::ports::TaskRepository)-Port
//! Ops zusammen mit dem Zustand atomar schreiben muss und Adapter nur vom Core abhängen dürfen
//! (ADR-015). Die Sync-Engine (Segmente, Push/Pull, Scheduler) liegt in `todo-sync`.

mod diff;
mod hlc;
mod op;

pub use diff::{list_fields, ops_for, task_fields, OpClock};
pub use hlc::{ClockSkew, Hlc, HlcState, MAX_CLOCK_DRIFT_MS};
pub use op::{decode_ops, encode_ops, FieldMap, FieldName, FieldValue, Op, OpId, OpKind};
