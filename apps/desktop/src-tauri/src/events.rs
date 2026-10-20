//! Core- und Sync-Ereignisse als Tauri-Events an die UI.

use tauri::{AppHandle, Emitter, Runtime};
use todo_core::api::{CoreEvent, EventSink};

use crate::dto::ChangedEventDto;

/// Payload [`ChangedEventDto`]: geschriebene Tasks/Listen/Notizen/Ordner (lokal oder Sync).
pub const CHANGED_EVENT: &str = "core://changed";
/// Payload `Vec<NoteConflictDto>`: beim Zusammenlegen entstandene Konfliktkopien.
pub const NOTE_CONFLICTS_EVENT: &str = "core://note-conflicts";
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
            } => self.app.emit(
                CHANGED_EVENT,
                ChangedEventDto {
                    tasks,
                    lists,
                    notes,
                    note_folders,
                },
            ),
            CoreEvent::NoteConflicts { conflicts } => {
                self.app.emit(NOTE_CONFLICTS_EVENT, conflicts)
            }
        };
    }
}
