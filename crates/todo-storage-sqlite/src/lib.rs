//! `TaskRepository` auf SQLite (WAL, versionierte Migrationen).
//!
//! Nur für native Ziele (Linux, später Android). Der Web-Speicher ist offen (Spike S-3).
//!
//! ```no_run
//! use std::sync::Arc;
//! use todo_core::ports::TaskRepository;
//! use todo_storage_sqlite::SqliteTaskRepository;
//!
//! let repo: Arc<dyn TaskRepository> = Arc::new(SqliteTaskRepository::open("todo.sqlite3")?);
//! # Ok::<(), todo_core::CoreError>(())
//! ```

mod connection;
mod error;
mod mapping;
mod migrations;
mod query;
mod repository;

pub use repository::SqliteTaskRepository;
