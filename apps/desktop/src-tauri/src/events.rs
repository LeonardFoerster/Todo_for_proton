//! Core- und Sync-Ereignisse als Tauri-Events an die UI.

use tauri::{AppHandle, Emitter, Runtime};
use todo_core::api::{CoreEvent, EventSink};

use crate::dto::ChangedEventDto;

/// Payload [`ChangedEventDto`]: geschriebene Tasks/Listen/Notizen/Ordner (lokal oder Sync).
pub const CHANGED_EVENT: &str = "core://changed";
/// Payload `Vec<NoteConflictDto>`: beim Zusammenlegen entstandene Konfliktkopien.
pub const NOTE_CONFLICTS_EVENT: &str = "core://note-conflicts";
/// Ohne Payload: Das Fenster soll schließen – UI speichert und meldet `close_ready`.
pub const CLOSE_REQUESTED_EVENT: &str = "app://close-requested";
/// Payload: Task-ID – Klick auf eine Erinnerung (ADR-033): Task öffnen.
pub const OPEN_TASK_EVENT: &str = "app://open-task";
/// Ohne Payload: „Neue Task“ im Tray.
pub const NEW_TASK_EVENT: &str = "app://new-task";
/// Payload [`crate::dto::SyncStatusDto`].
pub const SYNC_STATUS_EVENT: &str = "core://sync-status";

/// Leitet Core-Events an alle Fenster weiter.
#[derive(Debug)]
pub struct ChangedEmitter<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> ChangedEmitter<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> EventSink for ChangedEmitter<R> {
    fn emit(&self, event: CoreEvent) {
        let _ = match event {
            CoreEvent::Changed {
                tasks,
                lists,
                notes,
                note_folders,
                tags,
                links,
            } => self.app.emit(
                CHANGED_EVENT,
                ChangedEventDto {
                    tasks,
                    lists,
                    notes,
                    note_folders,
                    tags,
                    links,
                },
            ),
            CoreEvent::NoteConflicts { conflicts } => {
                self.app.emit(NOTE_CONFLICTS_EVENT, conflicts)
            }
        };
    }
}
