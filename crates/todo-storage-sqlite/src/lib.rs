//! `TaskRepository` auf SQLite (WAL, versionierte Migrationen).
//!
//! Nur für native Ziele (Linux, später Android). Der Web-Speicher ist offen (Spike S-3).

// Erster Nutzer ist `SqliteTaskRepository` (P4-03).
#[allow(dead_code)]
mod connection;
mod error;
mod migrations;
