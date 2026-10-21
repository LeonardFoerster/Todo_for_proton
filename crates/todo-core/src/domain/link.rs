//! Verknüpfung Task ↔ Notiz (N9, ADR-036): ein eigenes Element statt Felder an der Task, damit später
//! weitere Arten (z. B. externe Adressen) dazukommen können. Nur anlegen und löschen.

use super::{LinkId, NoteId, TaskId, Timestamp};
use crate::ports::Clock;

/// Eine Verknüpfung. Die ID ist aus Task und Notiz abgeleitet ([`LinkId::between`]): dieselbe
/// Verknüpfung auf zwei Geräten ist **eine**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    id: LinkId,
    task: TaskId,
    note: NoteId,
    created_at: Timestamp,
}

/// Alle Felder einer gespeicherten Verknüpfung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkParts {
    pub id: LinkId,
    pub task: TaskId,
    pub note: NoteId,
    pub created_at: Timestamp,
}

impl Link {
    #[must_use]
    pub fn from_parts(parts: LinkParts) -> Self {
        Self {
            id: parts.id,
            task: parts.task,
            note: parts.note,
            created_at: parts.created_at,
        }
    }

    #[must_use]
    pub fn new(id: LinkId, task: TaskId, note: NoteId, clock: &(impl Clock + ?Sized)) -> Self {
        Self {
            id,
            task,
            note,
            created_at: clock.now(),
        }
    }

    #[must_use]
    pub fn id(&self) -> LinkId {
        self.id
    }

    #[must_use]
    pub fn task(&self) -> TaskId {
        self.task
    }

    #[must_use]
    pub fn note(&self) -> NoteId {
        self.note
    }

    #[must_use]
    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}
