//! `NoteRepository`: Lese-Port für Notizen und Ordner (`notes.md` §2, §7, ADR-029).
//!
//! Eigener Trait, weil die Abfragen andere Formen haben als die der Tasks (Baum statt Ansichten).
//! **Geschrieben** wird weiter über [`TaskRepository::commit`](super::TaskRepository::commit):
//! Zustand und Outbox-Ops entstehen in einer Transaktion (ADR-015). Jedes `TaskRepository` ist
//! auch ein `NoteRepository` (Supertrait), damit Use-Cases und Shells mit einem Repository
//! auskommen.

use std::collections::BTreeSet;

use super::{MaybeSend, MaybeSync, Page, PageRequest, SearchQuery, PAGE_LIMIT_MAX};
use crate::{
    domain::{note_tree, Note, NoteFolder, NoteFolderId, NoteId, NoteKind, SortKey, Timestamp},
    CoreError, CoreResult,
};

/// Notiz ohne Inhalt (Baum, Papierkorb, Suche).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: NoteId,
    pub folder: Option<NoteFolderId>,
    pub name: String,
    pub kind: NoteKind,
    pub sort_key: SortKey,
    pub created_at: Timestamp,
    pub modified_at: Timestamp,
    pub trashed_at: Option<Timestamp>,
    /// Größe des Inhalts in Bytes.
    pub size: u64,
}

impl From<&Note> for NoteSummary {
    fn from(note: &Note) -> Self {
        Self {
            id: note.id(),
            folder: note.folder(),
            name: note.name().to_string(),
            kind: note.kind(),
            sort_key: note.sort_key().clone(),
            created_at: note.created_at(),
            modified_at: note.modified_at(),
            trashed_at: note.trashed_at(),
            size: note.body().len() as u64,
        }
    }
}

/// Ein Eintrag im Baum. Ordner und Notizen teilen sich unter einem Elternordner die Reihenfolge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteTreeItem {
    Folder(NoteFolder),
    Note(NoteSummary),
}

impl NoteTreeItem {
    /// Sortierposition: `sort_key`, bei Gleichstand die ID (als String).
    #[must_use]
    pub fn position(&self) -> (SortKey, String) {
        match self {
            Self::Folder(f) => (f.sort_key().clone(), f.id().to_string()),
            Self::Note(n) => (n.sort_key.clone(), n.id.to_string()),
        }
    }

    #[must_use]
    pub fn is_trashed(&self) -> bool {
        match self {
            Self::Folder(f) => f.is_trashed(),
            Self::Note(n) => n.trashed_at.is_some(),
        }
    }
}

/// Was [`NoteRepository::list_folder_items`] liefert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderItemsQuery {
    /// `None` = Wurzel.
    pub folder: Option<NoteFolderId>,
    /// Auch Elemente, die **selbst** im Papierkorb liegen (z. B. für endgültiges Löschen).
    pub include_trashed: bool,
}

/// Cursor einer Baumseite: Position des letzten Eintrags.
#[must_use]
pub fn encode_tree_cursor(position: &(SortKey, String)) -> String {
    format!("{}:{}", position.0, position.1)
}

/// Gegenstück zu [`encode_tree_cursor`].
///
/// # Errors
/// `CoreError::Validation` bei ungültigem Cursor.
pub fn decode_tree_cursor(cursor: &str) -> CoreResult<(SortKey, String)> {
    let invalid = || CoreError::Validation(format!("invalid cursor: {cursor:?}"));
    let (key, id) = cursor.split_once(':').ok_or_else(invalid)?;
    let key = SortKey::parse(key).map_err(|_| invalid())?;
    NoteId::parse(id).map_err(|_| invalid())?;
    Ok((key, id.to_owned()))
}

/// Prüft das Seitenlimit.
///
/// # Errors
/// `CoreError::Validation` außerhalb `1..=PAGE_LIMIT_MAX`.
pub fn check_page_limit(limit: u32) -> CoreResult<()> {
    if (1..=PAGE_LIMIT_MAX).contains(&limit) {
        Ok(())
    } else {
        Err(CoreError::Validation(format!(
            "page limit must be 1..={PAGE_LIMIT_MAX}, got {limit}"
        )))
    }
}

/// Ausschnitt um einen Inhaltstreffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSnippet {
    /// Text um den Treffer (±40 Zeichen, einzeilig), mit „…" an abgeschnittenen Enden.
    pub text: String,
    /// Position des Treffers im Inhalt in UTF-16-Einheiten (für den Cursor im Editor).
    pub offset_utf16: u32,
    /// Länge des Treffers in UTF-16-Einheiten.
    pub len_utf16: u32,
}

/// Ein Suchtreffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteHit {
    pub note: NoteSummary,
    /// Mindestens ein Wort steht im Namen.
    pub name_hit: bool,
    /// Erster Treffer im Inhalt (fehlt bei reinen Namenstreffern).
    pub snippet: Option<NoteSnippet>,
}

/// So viele Zeichen Kontext je Seite hat ein [`NoteSnippet`].
const SNIPPET_CONTEXT: usize = 40;

/// Wörter unter so vielen Zeichen nutzen keinen Trigram-Index. Besteht eine Suche **nur** aus
/// solchen Wörtern, wird nur im Namen gesucht: Ein oder zwei Buchstaben treffen fast jeden
/// Inhalt, und der Vergleich über alle Inhalte sprengte das Budget (ADR-029).
pub const NOTE_SEARCH_MIN_BODY_TERM: usize = 3;

impl NoteHit {
    /// Trifft `note` auf **jedes** Wort (Name oder Inhalt, ohne Groß/Klein)? Dann der Treffer.
    /// Nur kurze Wörter: nur im Namen ([`NOTE_SEARCH_MIN_BODY_TERM`]).
    #[must_use]
    pub fn of(terms: &[String], note: &Note) -> Option<Self> {
        if terms.is_empty() {
            return None;
        }
        let name = note.name().as_str().to_lowercase();
        if terms
            .iter()
            .all(|t| t.chars().count() < NOTE_SEARCH_MIN_BODY_TERM)
        {
            return terms
                .iter()
                .all(|t| name.contains(t.as_str()))
                .then(|| Self {
                    note: NoteSummary::from(note),
                    name_hit: true,
                    snippet: None,
                });
        }
        let lower = note.body().to_lowercase();
        let all = terms
            .iter()
            .all(|t| name.contains(t.as_str()) || lower.contains(t.as_str()));
        if !all {
            return None;
        }
        let name_hit = terms.iter().any(|t| name.contains(t.as_str()));
        let first = terms
            .iter()
            .filter_map(|t| lower.find(t.as_str()).map(|at| (at, t.len())))
            .min();
        Some(Self {
            note: NoteSummary::from(note),
            name_hit,
            snippet: first.map(|(at, len)| snippet::around(note.body(), at, len)),
        })
    }

    /// Rangfolge: Namenstreffer zuerst, dann zuletzt geändert, dann ID.
    #[must_use]
    pub fn rank(&self) -> (bool, std::cmp::Reverse<Timestamp>, NoteId) {
        (
            !self.name_hit,
            std::cmp::Reverse(self.note.modified_at),
            self.note.id,
        )
    }
}

/// Ausschnitt um einen Treffer, der in der Kleinschreibung von `body` bei Byte `at` mit
/// `len` Bytes liegt.
mod snippet {
    use super::{NoteSnippet, SNIPPET_CONTEXT};

    /// Index des Originalzeichens, dessen Kleinschreibung an Byte `at` von `self.text` liegt.
    fn char_at(body: &str, at: usize) -> usize {
        let mut lower_len = 0;
        for (index, c) in body.chars().enumerate() {
            lower_len += c.to_lowercase().map(char::len_utf8).sum::<usize>();
            if lower_len > at {
                return index;
            }
        }
        body.chars().count()
    }

    pub(super) fn around(body: &str, at: usize, len: usize) -> NoteSnippet {
        let start = char_at(body, at);
        let end = char_at(body, at + len - 1) + 1;
        let from = start.saturating_sub(SNIPPET_CONTEXT);
        // Nur den Ausschnitt in Zeichen zerlegen, nicht den ganzen Inhalt.
        let window: Vec<char> = body
            .chars()
            .skip(from)
            .take(end + SNIPPET_CONTEXT - from)
            .collect();
        let has_more = body.chars().nth(end + SNIPPET_CONTEXT).is_some();
        let mut text: String = window
            .iter()
            .map(|c| if c.is_whitespace() { ' ' } else { *c })
            .collect();
        text = text.trim().to_owned();
        if from > 0 {
            text.insert(0, '…');
        }
        if has_more {
            text.push('…');
        }
        let utf16 = |chars: &mut dyn Iterator<Item = char>| -> u32 {
            u32::try_from(chars.map(char::len_utf16).sum::<usize>()).unwrap_or(u32::MAX)
        };
        NoteSnippet {
            text,
            offset_utf16: utf16(&mut body.chars().take(start)),
            len_utf16: utf16(&mut body.chars().skip(start).take(end - start)),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait NoteRepository: MaybeSend + MaybeSync {
    /// Lädt eine Notiz (mit Inhalt); `None`, wenn sie nicht existiert.
    async fn get_note(&self, id: NoteId) -> CoreResult<Option<Note>>;

    /// Lädt einen Ordner; `None`, wenn er nicht existiert.
    async fn get_note_folder(&self, id: NoteFolderId) -> CoreResult<Option<NoteFolder>>;

    /// Alle Ordner (auch im Papierkorb), nach `sort_key`, dann ID. Wenige Einträge, daher ohne
    /// Paginierung; Grundlage für Baum-Regeln und Papierkorb.
    async fn list_note_folders(&self) -> CoreResult<Vec<NoteFolder>>;

    /// Eine Seite des Inhalts eines Ordners: Unterordner und Notizen gemischt nach `sort_key`,
    /// dann ID. Ohne `include_trashed` fehlen Elemente, die selbst im Papierkorb liegen.
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Limit oder Cursor.
    async fn list_folder_items(
        &self,
        query: FolderItemsQuery,
        page: PageRequest,
    ) -> CoreResult<Page<NoteTreeItem>>;

    /// Alle Notizen, die selbst im Papierkorb liegen.
    async fn list_trashed_notes(&self) -> CoreResult<Vec<NoteSummary>>;

    /// Größter `sort_key` unter `folder` (Notizen und Ordner, auch im Papierkorb); Grundlage
    /// für „ans Ende anhängen".
    async fn last_note_sort_key(&self, folder: Option<NoteFolderId>)
        -> CoreResult<Option<SortKey>>;

    /// Suche über Name und Inhalt ([`SearchQuery`], ADR-027): jedes Wort als Teilwort, ohne
    /// Groß/Klein; Notizen im (wirksamen) Papierkorb nie. Rang siehe [`NoteHit::rank`].
    /// Standard: den ganzen Baum laden und vergleichen (O(n)) – Speicher mit Volltextindex
    /// überschreiben das (SQLite: FTS5).
    ///
    /// # Errors
    /// `CoreError::Validation` (Limit, Länge).
    async fn search_notes(&self, query: &SearchQuery) -> CoreResult<Vec<NoteHit>> {
        let terms = query.terms()?;
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let folders = self.list_note_folders().await?;
        let visible = visible_folders(&folders);
        let mut hits = Vec::new();
        let mut places: Vec<Option<NoteFolderId>> = vec![None];
        places.extend(visible.iter().copied().map(Some));
        for folder in places {
            let mut page = PageRequest::first(PAGE_LIMIT_MAX);
            loop {
                let result = self
                    .list_folder_items(
                        FolderItemsQuery {
                            folder,
                            include_trashed: false,
                        },
                        page,
                    )
                    .await?;
                for item in &result.items {
                    if let NoteTreeItem::Note(summary) = item {
                        if let Some(note) = self.get_note(summary.id).await? {
                            hits.extend(NoteHit::of(&terms, &note));
                        }
                    }
                }
                match result.next_cursor {
                    Some(cursor) => page = PageRequest::after(cursor, PAGE_LIMIT_MAX),
                    None => break,
                }
            }
        }
        hits.sort_by_key(NoteHit::rank);
        hits.truncate(query.limit as usize);
        Ok(hits)
    }
}

/// Ordner, die weder selbst noch über einen Vorfahren im Papierkorb liegen.
#[must_use]
pub fn visible_folders(folders: &[NoteFolder]) -> BTreeSet<NoteFolderId> {
    let parents: note_tree::FolderParents = folders.iter().map(|f| (f.id(), f.parent())).collect();
    let trashed: BTreeSet<NoteFolderId> = folders
        .iter()
        .filter(|f| f.is_trashed())
        .map(NoteFolder::id)
        .collect();
    folders
        .iter()
        .map(NoteFolder::id)
        .filter(|id| !note_tree::in_trashed_folder(&parents, &trashed, Some(*id)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FixedClock;

    fn note(name: Option<&str>, body: &str) -> Note {
        let clock = FixedClock(Timestamp::from_millis(5));
        let mut note = Note::new(None, NoteKind::Markdown, SortKey::first(), &clock);
        note.set_body(body, None, &clock).unwrap();
        if let Some(name) = name {
            note.rename(name, &clock).unwrap();
        }
        note
    }

    fn terms(text: &str) -> Vec<String> {
        SearchQuery {
            text: text.into(),
            limit: 10,
        }
        .terms()
        .unwrap()
    }

    #[test]
    fn every_term_in_name_or_body() {
        let n = note(Some("Analysis"), "Grenzwerte und Reihen");
        assert!(NoteHit::of(&terms("ana reihe"), &n).is_some());
        assert!(NoteHit::of(&terms("ana fehlt"), &n).is_none());
        assert!(NoteHit::of(&[], &n).is_none());
        let hit = NoteHit::of(&terms("REIHEN"), &n).unwrap();
        assert!(!hit.name_hit);
        let snippet = hit.snippet.unwrap();
        assert_eq!(snippet.text, "Grenzwerte und Reihen");
        assert_eq!(snippet.offset_utf16, 15);
        assert_eq!(snippet.len_utf16, 6);
        let name_only = NoteHit::of(&terms("analysis"), &n).unwrap();
        assert!(name_only.name_hit);
        assert!(name_only.snippet.is_none());
    }

    #[test]
    fn snippet_cuts_context_and_counts_utf16() {
        let body = format!("{}😀 Treffer {}", "a".repeat(100), "b".repeat(100));
        let hit = NoteHit::of(&terms("treffer"), &note(Some("x"), &body)).unwrap();
        let snippet = hit.snippet.unwrap();
        assert!(snippet.text.starts_with('…') && snippet.text.ends_with('…'));
        assert!(snippet.text.contains("😀 Treffer"));
        assert_eq!(snippet.offset_utf16, 100 + 2 + 1, "Emoji zählt doppelt");
        assert_eq!(snippet.len_utf16, 7);
        // Kleinschreibung, die die Länge ändert (İ → i̇), verschiebt den Treffer nicht.
        let hit = NoteHit::of(&terms("ende"), &note(Some("x"), "İİ Ende")).unwrap();
        assert_eq!(hit.snippet.unwrap().offset_utf16, 3);
    }

    #[test]
    fn ranking_name_hits_then_recent() {
        let a = NoteHit::of(&terms("abc"), &note(Some("abc"), "")).unwrap();
        let b = NoteHit::of(&terms("abc"), &note(Some("y"), "abc")).unwrap();
        assert!(
            NoteHit::of(&terms("ab"), &note(Some("y"), "ab")).is_none(),
            "kurz: nur Name"
        );
        assert!(a.rank() < b.rank());
    }

    #[test]
    fn tree_cursor_roundtrip() {
        let id = NoteId::new().to_string();
        let pos = (SortKey::first(), id);
        assert_eq!(decode_tree_cursor(&encode_tree_cursor(&pos)).unwrap(), pos);
        for bad in ["", "a0", "a0:x", "!!:00000000-0000-0000-0000-000000000000"] {
            assert!(decode_tree_cursor(bad).is_err(), "{bad:?}");
        }
        assert!(check_page_limit(0).is_err());
        assert!(check_page_limit(PAGE_LIMIT_MAX).is_ok());
    }
}
