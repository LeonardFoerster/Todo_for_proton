//! `NoteRepository`: Lese-Port für Notizen und Ordner (`notes.md` §2, ADR-029).
//!
//! Eigener Trait, weil die Abfragen andere Formen haben als die der Tasks (Baum statt Ansichten).
//! **Geschrieben** wird weiter über [`TaskRepository::commit`](super::TaskRepository::commit):
//! Zustand und Outbox-Ops entstehen in einer Transaktion (ADR-015). Jedes `TaskRepository` ist
//! auch ein `NoteRepository` (Supertrait), damit Use-Cases und Shells mit einem Repository
//! auskommen.

use super::{MaybeSend, MaybeSync};
use crate::{
    domain::{Note, NoteFolder, NoteFolderId, NoteId},
    CoreResult,
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait NoteRepository: MaybeSend + MaybeSync {
    /// Lädt eine Notiz (mit Inhalt); `None`, wenn sie nicht existiert.
    async fn get_note(&self, id: NoteId) -> CoreResult<Option<Note>>;

    /// Lädt einen Ordner; `None`, wenn er nicht existiert.
    async fn get_note_folder(&self, id: NoteFolderId) -> CoreResult<Option<NoteFolder>>;
}
