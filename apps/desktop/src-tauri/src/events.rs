//! Core- und Sync-Ereignisse als Tauri-Events an die UI.

use tauri::{AppHandle, Emitter, Runtime};
use todo_core::api::{CoreEvent, EventSink};

use crate::dto::ChangedEventDto;

/// Payload [`ChangedEventDto`]: geschriebene Tasks/Listen (lokal oder aus dem Sync).
pub const CHANGED_EVENT: &str = "core://changed";
/// Payload [`crate::dto::SyncStatusDto`].
pub const SYNC_STATUS_EVENT: &str = "core://sync-status";

/// Leitet `CoreEvent::Changed` an alle Fenster weiter.
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
        let CoreEvent::Changed { tasks, lists } = event;
        let _ = self
            .app
            .emit(CHANGED_EVENT, ChangedEventDto { tasks, lists });
    }
}
