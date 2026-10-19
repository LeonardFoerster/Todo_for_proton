//! Sync-Datenmodell: HLC, Ops, Diff und Merge – reine Logik ohne IO.
//!
//! Liegt im Core (nicht in `todo-sync`), weil der [`TaskRepository`](crate::ports::TaskRepository)-Port
//! Ops zusammen mit dem Zustand atomar schreiben muss und Adapter nur vom Core abhängen dürfen
//! (ADR-015). Die Sync-Engine (Segmente, Push/Pull, Scheduler) liegt in `todo-sync`.

mod diff;
mod hlc;
mod merge;
mod op;
mod replica;

pub use diff::{list_fields, ops_for, task_fields, OpClock};
pub use hlc::{ClockSkew, Hlc, HlcState, MAX_CLOCK_DRIFT_MS, MAX_WALL_MS};
pub use merge::{
    list_id, materialize_list, materialize_task, merge, EntityState, MergeResult, Register,
    Registers,
};
pub use op::{decode_ops, encode_ops, FieldMap, FieldName, FieldValue, Op, OpId, OpKind};
pub use replica::{apply_remote, record_local, RemoteBatch, RemoteReceipt, ReplicaStore};
