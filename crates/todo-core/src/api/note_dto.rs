//! DTOs für Notizen und Ordner (ADR-029). Gleiche Regeln wie [`super::dto`]: owned, IDs als
//! `String`, Zeitpunkte ISO-8601, camelCase. Revisions-IDs als `"<gerät>.<nummer>"`.

use serde::{Deserialize, Serialize};

use super::dto::iso;
use crate::{
    app::{ImportReport, NoteItem, NoteTarget, NoteTrashEntry},
    domain::{Note, NoteFolder, NoteFolderId, NoteId, NoteKind, RevisionId},
    ports::{NoteHit, NoteSummary, NoteTreeItem, Page},
    CoreResult,
};

pub(crate) fn note_id(s: &str) -> CoreResult<NoteId> {
    Ok(NoteId::parse(s)?)
}

pub(crate) fn folder_id(s: &str) -> CoreResult<NoteFolderId> {
    Ok(NoteFolderId::parse(s)?)
}

pub(crate) fn revision_id(s: &str) -> CoreResult<RevisionId> {
    RevisionId::parse(s)
}

/// Typ des Inhalts: `"text"` (`.txt`) oder `"markdown"` (`.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum NoteKindDto {
    Text,
    Markdown,
}

impl From<NoteKind> for NoteKindDto {
    fn from(kind: NoteKind) -> Self {
        match kind {
            NoteKind::Text => Self::Text,
            NoteKind::Markdown => Self::Markdown,
        }
    }
}

impl From<NoteKindDto> for NoteKind {
    fn from(kind: NoteKindDto) -> Self {
        match kind {
            NoteKindDto::Text => Self::Text,
            NoteKindDto::Markdown => Self::Markdown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteFolderDto {
    pub id: String,
    /// `null` = Wurzel.
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_key: String,
    pub created_at: String,
    /// Gesetzt = liegt selbst im Papierkorb.
    pub trashed_at: Option<String>,
}

impl From<&NoteFolder> for NoteFolderDto {
    fn from(folder: &NoteFolder) -> Self {
        Self {
            id: folder.id().to_string(),
            parent_id: folder.parent().map(|p| p.to_string()),
            name: folder.name().to_string(),
            sort_key: folder.sort_key().to_string(),
            created_at: iso(folder.created_at()),
            trashed_at: folder.trashed_at().map(iso),
        }
    }
}

/// Notiz ohne Inhalt (Baum, Papierkorb, Suche).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteSummaryDto {
    pub id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub kind: NoteKindDto,
    pub sort_key: String,
    pub created_at: String,
    pub modified_at: String,
    pub trashed_at: Option<String>,
    /// Größe des Inhalts in Bytes (≤ 1 MiB).
    pub size: u32,
}

impl From<&NoteSummary> for NoteSummaryDto {
    fn from(n: &NoteSummary) -> Self {
        Self {
            id: n.id.to_string(),
            folder_id: n.folder.map(|f| f.to_string()),
            name: n.name.clone(),
            kind: n.kind.into(),
            sort_key: n.sort_key.to_string(),
            created_at: iso(n.created_at),
            modified_at: iso(n.modified_at),
            trashed_at: n.trashed_at.map(iso),
            size: u32::try_from(n.size).unwrap_or(u32::MAX),
        }
    }
}

/// Vollständige Notiz (Editor). `bodyRev` beim nächsten Speichern als `baseRev` mitgeben.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteDto {
    pub id: String,
    pub folder_id: Option<String>,
    pub name: String,
    /// Name folgt der ersten Zeile, bis von Hand umbenannt wird.
    pub auto_name: bool,
    pub kind: NoteKindDto,
    pub body: String,
    pub body_rev: Option<String>,
    pub sort_key: String,
    pub created_at: String,
    pub modified_at: String,
    pub trashed_at: Option<String>,
}

impl From<&Note> for NoteDto {
    fn from(n: &Note) -> Self {
        Self {
            id: n.id().to_string(),
            folder_id: n.folder().map(|f| f.to_string()),
            name: n.name().to_string(),
            auto_name: n.auto_name(),
            kind: n.kind().into(),
            body: n.body().to_owned(),
            body_rev: n.body_rev().map(|r| r.to_string()),
            sort_key: n.sort_key().to_string(),
            created_at: iso(n.created_at()),
            modified_at: iso(n.modified_at()),
            trashed_at: n.trashed_at().map(iso),
        }
    }
}

/// Ein Eintrag im Baum: `{"type":"folder",…}` oder `{"type":"note",…}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NoteTreeItemDto {
    Folder(NoteFolderDto),
    Note(NoteSummaryDto),
}

impl From<&NoteTreeItem> for NoteTreeItemDto {
    fn from(item: &NoteTreeItem) -> Self {
        match item {
            NoteTreeItem::Folder(f) => Self::Folder(f.into()),
            NoteTreeItem::Note(n) => Self::Note(n.into()),
        }
    }
}

impl From<Page<NoteTreeItem>> for super::dto::PageDto<NoteTreeItemDto> {
    fn from(page: Page<NoteTreeItem>) -> Self {
        Self {
            items: page.items.iter().map(NoteTreeItemDto::from).collect(),
            next_cursor: page.next_cursor,
        }
    }
}

/// Verweis auf ein Element: `{"type":"note","id":…}` oder `{"type":"folder","id":…}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NoteItemDto {
    Note { id: String },
    Folder { id: String },
}

impl From<NoteItem> for NoteItemDto {
    fn from(item: NoteItem) -> Self {
        match item {
            NoteItem::Note(id) => Self::Note { id: id.to_string() },
            NoteItem::Folder(id) => Self::Folder { id: id.to_string() },
        }
    }
}

impl TryFrom<&NoteItemDto> for NoteItem {
    type Error = crate::CoreError;

    fn try_from(dto: &NoteItemDto) -> CoreResult<Self> {
        Ok(match dto {
            NoteItemDto::Note { id } => Self::Note(note_id(id)?),
            NoteItemDto::Folder { id } => Self::Folder(folder_id(id)?),
        })
    }
}

/// Zielposition beim Verschieben: Ordner (`null` = Wurzel) und Nachbarn darin. Nur `after` =
/// direkt dahinter, nur `before` = direkt davor, beide = dazwischen, keins = ans Ende.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteTargetDto {
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub folder_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub after: Option<NoteItemDto>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub before: Option<NoteItemDto>,
}

impl TryFrom<&NoteTargetDto> for NoteTarget {
    type Error = crate::CoreError;

    fn try_from(dto: &NoteTargetDto) -> CoreResult<Self> {
        Ok(Self {
            folder: dto.folder_id.as_deref().map(folder_id).transpose()?,
            after: dto.after.as_ref().map(TryInto::try_into).transpose()?,
            before: dto.before.as_ref().map(TryInto::try_into).transpose()?,
        })
    }
}

/// Ausschnitt um einen Inhaltstreffer; `offset`/`length` in UTF-16-Einheiten des Inhalts
/// (direkt für `setSelectionRange` im Editor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteSnippetDto {
    pub text: String,
    pub offset: u32,
    pub length: u32,
}

/// Suchtreffer mit Ordnerpfad (Namen von der Wurzel an).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteHitDto {
    pub note: NoteSummaryDto,
    pub path: Vec<String>,
    pub name_hit: bool,
    pub snippet: Option<NoteSnippetDto>,
}

impl NoteHitDto {
    pub(crate) fn new(hit: &NoteHit, path: Vec<String>) -> Self {
        Self {
            note: (&hit.note).into(),
            path,
            name_hit: hit.name_hit,
            snippet: hit.snippet.as_ref().map(|s| NoteSnippetDto {
                text: s.text.clone(),
                offset: s.offset_utf16,
                length: s.len_utf16,
            }),
        }
    }
}

/// Eintrag im Papierkorb (nur oberste Elemente).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteTrashEntryDto {
    pub item: NoteTreeItemDto,
    /// Herkunft: Ordnernamen von der Wurzel an.
    pub path: Vec<String>,
    pub trashed_at: String,
}

impl From<&NoteTrashEntry> for NoteTrashEntryDto {
    fn from(e: &NoteTrashEntry) -> Self {
        Self {
            item: (&e.item).into(),
            path: e.path.clone(),
            trashed_at: iso(e.trashed_at),
        }
    }
}

/// Eine eingelesene Datei für `import_notes` (Shell → Core).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ImportNoteFileDto {
    /// Unterordner relativ zum importierten Ordner.
    pub dirs: Vec<String>,
    /// Dateiname ohne Endung.
    pub name: String,
    pub kind: NoteKindDto,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ImportSkipDto {
    pub path: String,
    pub reason: String,
}

/// Ergebnis eines Imports: „412 Notizen in 37 Ordnern importiert, 9 übersprungen".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ImportReportDto {
    /// Der neue oberste Ordner.
    pub folder_id: String,
    pub notes: u32,
    pub folders: u32,
    pub skipped: Vec<ImportSkipDto>,
}

impl From<&ImportReport> for ImportReportDto {
    fn from(r: &ImportReport) -> Self {
        Self {
            folder_id: r.root.to_string(),
            notes: r.notes,
            folders: r.folders,
            skipped: r
                .skipped
                .iter()
                .map(|s| ImportSkipDto {
                    path: s.path.clone(),
                    reason: s.reason.clone(),
                })
                .collect(),
        }
    }
}

/// Eine beim Zusammenlegen entstandene Konfliktkopie (`noteId` = Original).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteConflictDto {
    pub note_id: String,
    pub copy_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shapes() {
        let item: NoteItemDto = serde_json::from_str(
            r#"{"type":"folder","id":"00000000-0000-7000-8000-000000000001"}"#,
        )
        .unwrap();
        assert!(matches!(
            NoteItem::try_from(&item).unwrap(),
            NoteItem::Folder(_)
        ));
        let target: NoteTargetDto = serde_json::from_str("{}").unwrap();
        assert_eq!(target, NoteTargetDto::default());
        assert_eq!(
            serde_json::to_string(&NoteKindDto::Markdown).unwrap(),
            r#""markdown""#
        );
        let bad: NoteItemDto = serde_json::from_str(r#"{"type":"note","id":"x"}"#).unwrap();
        assert!(NoteItem::try_from(&bad).is_err());
    }
}
