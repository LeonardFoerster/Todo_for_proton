//! Offline-first Sync-Engine (Op-Log pro Gerät, HLC, feldweises LWW-Merge).
//!
//! Spricht ausschließlich über Ports aus `todo-core`. Das Datenmodell (HLC, Ops, Merge) liegt in
//! `todo_core::sync` (ADR-015); hier liegen Dateiformate, Ablauf und Planung des Syncs.
//! Siehe `development/sync-strategy.md`.

pub mod compression;
pub mod engine;
pub mod scheduler;
pub mod segment;
pub mod snapshot;

pub use engine::{Bootstrap, DeviceInfo, SyncConfig, SyncEngine, SyncReport};
pub use scheduler::{
    Actions, Scheduler, SchedulerConfig, SchedulerEvent, SyncOutcome, SyncPauseReason, SyncStatus,
};
