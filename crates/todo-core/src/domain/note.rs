//! Notizen (`.txt`/`.md`) im Modus „Notes" (`notes.md` §3, ADR-029).

use std::fmt;

use super::{DeviceId, NoteFolderId, NoteId, NoteName, SortKey, Timestamp};
use crate::{ports::Clock, CoreError, CoreResult};

/// Größter Inhalt einer Notiz in Bytes (UTF-8).
pub const NOTE_BODY_MAX_BYTES: usize = 1024 * 1024;

/// Typ des Inhalts; bestimmt Endung, Schrift und ob es eine Vorschau gibt. Nie den Inhalt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NoteKind {
    /// `.txt`
    Text,
    /// `.md`
    #[default]
    Markdown,
}

impl NoteKind {
    /// Dateiendung ohne Punkt.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Markdown => "md",
        }
    }
}

/// Kennung einer Inhalts-Revision: die Op, die sie geschrieben hat (Gerät + Nummer), damit
/// global eindeutig. Entspricht `sync::OpId`; hier als Domain-Wert, damit die Domain den Sync
/// nicht kennt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RevisionId {
    pub device: DeviceId,
    pub seq: u64,
}

impl fmt::Display for RevisionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.device, self.seq)
    }
}

impl RevisionId {
    /// Gegenstück zu [`fmt::Display`] (`<gerät>.<nummer>`).
    ///
    /// # Errors
    /// `CoreError::Validation` bei falschem Format.
    pub fn parse(s: &str) -> CoreResult<Self> {
        let invalid = || CoreError::Validation(format!("invalid revision id: {s:?}"));
        let (device, seq) = s.rsplit_once('.').ok_or_else(invalid)?;
        Ok(Self {
            device: DeviceId::parse(device).map_err(|_| invalid())?,
            seq: seq.parse().map_err(|_| invalid())?,
        })
    }
}

/// Eine Notiz. Der Inhalt (`body`) ist der aufgelöste Text aller Revisionen (§4.3); `body_rev`
/// die Revision, auf der der Editor aufsetzt (`None` = noch keine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    id: NoteId,
    folder: Option<NoteFolderId>,
    name: NoteName,
    auto_name: bool,
    kind: NoteKind,
    body: String,
    body_rev: Option<RevisionId>,
    sort_key: SortKey,
    created_at: Timestamp,
    modified_at: Timestamp,
    trashed_at: Option<Timestamp>,
}

/// Alle Felder einer gespeicherten Notiz, zum Wiederherstellen per [`Note::from_parts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteParts {
    pub id: NoteId,
    pub folder: Option<NoteFolderId>,
    pub name: String,
    pub auto_name: bool,
    pub kind: NoteKind,
    pub body: String,
    pub body_rev: Option<RevisionId>,
    pub sort_key: SortKey,
    pub created_at: Timestamp,
    pub modified_at: Timestamp,
    pub trashed_at: Option<Timestamp>,
}

impl Note {
    /// Stellt eine Notiz aus gespeicherten Feldern wieder her (Name wird geprüft).
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen oder zu großem Inhalt.
    pub fn from_parts(parts: NoteParts) -> CoreResult<Self> {
        check_body(&parts.body)?;
        Ok(Self {
            id: parts.id,
            folder: parts.folder,
            name: NoteName::parse(&parts.name)?,
            auto_name: parts.auto_name,
            kind: parts.kind,
            body: parts.body,
            body_rev: parts.body_rev,
            sort_key: parts.sort_key,
            created_at: parts.created_at,
            modified_at: parts.modified_at,
            trashed_at: parts.trashed_at,
        })
    }

    /// Neue, leere Notiz „Unbenannt" mit Auto-Name.
    #[must_use]
    pub fn new(
        folder: Option<NoteFolderId>,
        kind: NoteKind,
        sort_key: SortKey,
        clock: &(impl Clock + ?Sized),
    ) -> Self {
        Self::with_id(NoteId::new(), folder, kind, sort_key, clock)
    }

    /// Wie [`Note::new`] mit vorgegebener ID (z. B. deterministische Konfliktkopie).
    #[must_use]
    pub fn with_id(
        id: NoteId,
        folder: Option<NoteFolderId>,
        kind: NoteKind,
        sort_key: SortKey,
        clock: &(impl Clock + ?Sized),
    ) -> Self {
        let now = clock.now();
        Self {
            id,
            folder,
            name: NoteName::untitled(),
            auto_name: true,
            kind,
            body: String::new(),
            body_rev: None,
            sort_key,
            created_at: now,
            modified_at: now,
            trashed_at: None,
        }
    }

    #[must_use]
    pub fn id(&self) -> NoteId {
        self.id
    }

    #[must_use]
    pub fn folder(&self) -> Option<NoteFolderId> {
        self.folder
    }

    #[must_use]
    pub fn name(&self) -> &NoteName {
        &self.name
    }

    #[must_use]
    pub fn auto_name(&self) -> bool {
        self.auto_name
    }

    #[must_use]
    pub fn kind(&self) -> NoteKind {
        self.kind
    }

    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    #[must_use]
    pub fn body_rev(&self) -> Option<RevisionId> {
        self.body_rev
    }

    #[must_use]
    pub fn sort_key(&self) -> &SortKey {
        &self.sort_key
    }

    #[must_use]
    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    #[must_use]
    pub fn modified_at(&self) -> Timestamp {
        self.modified_at
    }

    #[must_use]
    pub fn trashed_at(&self) -> Option<Timestamp> {
        self.trashed_at
    }

    /// Liegt die Notiz **selbst** im Papierkorb (nicht nur ein Vorfahr)?
    #[must_use]
    pub fn is_trashed(&self) -> bool {
        self.trashed_at.is_some()
    }

    /// Setzt den Inhalt (Zeilenenden werden LF) und die Revision, auf der er beruht. Mit
    /// Auto-Name folgt der Name der ersten Zeile.
    ///
    /// # Errors
    /// `CoreError::Validation`, wenn der Inhalt größer als [`NOTE_BODY_MAX_BYTES`] ist.
    pub fn set_body(
        &mut self,
        body: &str,
        rev: Option<RevisionId>,
        clock: &(impl Clock + ?Sized),
    ) -> CoreResult<()> {
        let body = normalize_line_endings(body);
        check_body(&body)?;
        self.body_rev = rev;
        if body != self.body {
            self.body = body;
            self.modified_at = clock.now();
        }
        if self.auto_name {
            self.name = NoteName::from_first_line(&self.body);
        }
        Ok(())
    }

    /// Benennt von Hand um; der Auto-Name ist danach aus.
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen (die Notiz bleibt unverändert).
    pub fn rename(&mut self, name: &str, clock: &(impl Clock + ?Sized)) -> CoreResult<()> {
        let name = NoteName::parse(name)?;
        self.auto_name = false;
        if name != self.name {
            self.name = name;
            self.modified_at = clock.now();
        }
        Ok(())
    }

    /// `.txt` ↔ `.md`; der Inhalt bleibt unverändert.
    pub fn set_kind(&mut self, kind: NoteKind) {
        self.kind = kind;
    }

    /// Verschiebt in einen Ordner (`None` = Wurzel) an eine Position.
    pub fn move_to(&mut self, folder: Option<NoteFolderId>, sort_key: SortKey) {
        self.folder = folder;
        self.sort_key = sort_key;
    }

    /// In den Papierkorb (`Some`) bzw. wiederherstellen (`None`).
    pub fn set_trashed(&mut self, at: Option<Timestamp>) {
        self.trashed_at = at;
    }
}

/// CRLF und einzelne CR werden zu LF (gleicher Text auf allen Geräten, saubere Zeilen-Diffs).
#[must_use]
pub fn normalize_line_endings(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Prüft die Größe eines Inhalts.
///
/// # Errors
/// `CoreError::Validation`, wenn `body` größer als [`NOTE_BODY_MAX_BYTES`] ist.
pub fn check_body(body: &str) -> CoreResult<()> {
    if body.len() > NOTE_BODY_MAX_BYTES {
        return Err(CoreError::Validation(format!(
            "note body must be at most {NOTE_BODY_MAX_BYTES} bytes, got {}",
            body.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::UNTITLED_NOTE_NAME,
        ports::{FixedClock, SteppingClock},
    };

    fn clock() -> SteppingClock {
        SteppingClock::new(Timestamp::from_millis(100), 1)
    }

    fn rev(seq: u64) -> RevisionId {
        RevisionId {
            device: DeviceId::new(),
            seq,
        }
    }

    #[test]
    fn new_note_is_untitled_with_auto_name() {
        let c = FixedClock(Timestamp::from_millis(7));
        let folder = NoteFolderId::new();
        let note = Note::new(Some(folder), NoteKind::Text, SortKey::first(), &c);
        assert_eq!(note.name().as_str(), UNTITLED_NOTE_NAME);
        assert!(note.auto_name());
        assert_eq!(note.folder(), Some(folder));
        assert_eq!(note.kind().extension(), "txt");
        assert_eq!(note.body(), "");
        assert_eq!(note.body_rev(), None);
        assert_eq!(note.created_at(), Timestamp::from_millis(7));
        assert_eq!(note.modified_at(), note.created_at());
        assert!(!note.is_trashed());
        assert_eq!(NoteKind::default(), NoteKind::Markdown);
    }

    #[test]
    fn body_follows_first_line_until_renamed() {
        let c = clock();
        let mut note = Note::new(None, NoteKind::Markdown, SortKey::first(), &c);
        let r1 = rev(1);
        note.set_body("# Blatt 3\r\nText", Some(r1), &c).unwrap();
        assert_eq!(note.body(), "# Blatt 3\nText", "LF");
        assert_eq!(note.body_rev(), Some(r1));
        assert_eq!(note.name().as_str(), "Blatt 3");
        assert!(note.modified_at() > note.created_at());

        note.rename("Analysis", &c).unwrap();
        assert!(!note.auto_name());
        note.set_body("# Etwas anderes", Some(rev(2)), &c).unwrap();
        assert_eq!(note.name().as_str(), "Analysis");

        let before = note.clone();
        assert!(note.rename("a/b", &c).is_err());
        assert_eq!(note, before);
    }

    #[test]
    fn body_size_is_limited() {
        let c = clock();
        let mut note = Note::new(None, NoteKind::Text, SortKey::first(), &c);
        assert!(note
            .set_body(&"x".repeat(NOTE_BODY_MAX_BYTES), None, &c)
            .is_ok());
        let before = note.clone();
        let err = note
            .set_body(&"x".repeat(NOTE_BODY_MAX_BYTES + 1), None, &c)
            .unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));
        assert_eq!(note, before);
    }

    #[test]
    fn kind_move_and_trash() {
        let c = clock();
        let mut note = Note::new(None, NoteKind::Text, SortKey::first(), &c);
        note.set_body("abc", None, &c).unwrap();
        note.set_kind(NoteKind::Markdown);
        assert_eq!(note.body(), "abc");
        let folder = NoteFolderId::new();
        let key = SortKey::between(Some(&SortKey::first()), None).unwrap();
        note.move_to(Some(folder), key.clone());
        assert_eq!((note.folder(), note.sort_key()), (Some(folder), &key));
        note.set_trashed(Some(Timestamp::from_millis(9)));
        assert!(note.is_trashed());
        note.set_trashed(None);
        assert!(!note.is_trashed());
    }

    #[test]
    fn parts_roundtrip_and_validation() {
        let c = clock();
        let mut note = Note::new(None, NoteKind::Markdown, SortKey::first(), &c);
        note.set_body("Hallo", Some(rev(4)), &c).unwrap();
        let parts = NoteParts {
            id: note.id(),
            folder: note.folder(),
            name: note.name().to_string(),
            auto_name: note.auto_name(),
            kind: note.kind(),
            body: note.body().to_owned(),
            body_rev: note.body_rev(),
            sort_key: note.sort_key().clone(),
            created_at: note.created_at(),
            modified_at: note.modified_at(),
            trashed_at: note.trashed_at(),
        };
        assert_eq!(Note::from_parts(parts.clone()).unwrap(), note);
        assert!(Note::from_parts(NoteParts {
            name: ".x".into(),
            ..parts
        })
        .is_err());
    }

    #[test]
    fn revision_id_roundtrips_as_string() {
        let r = rev(42);
        assert_eq!(RevisionId::parse(&r.to_string()).unwrap(), r);
        for bad in ["", "x.1", "42", &format!("{}.x", r.device)] {
            assert!(RevisionId::parse(bad).is_err(), "{bad:?}");
        }
    }
}
