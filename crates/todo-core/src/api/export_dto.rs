//! DTOs für Export und Import (ADR-032). `ExportDto` ist der Inhalt von `tasks.json`.

use serde::{Deserialize, Serialize};

use super::{
    dto::{iso, parse_iso, TaskDto},
    note_dto::NoteKindDto,
};
use crate::{
    app::{DataSnapshot, ExportNote, ImportDataReport, ImportList, ImportTag},
    domain::{TagId, TaskListId, TaskParts, Timestamp},
    CoreError, CoreResult,
};

/// Kennung im Feld `format` von `tasks.json`.
pub const EXPORT_FORMAT: &str = "tasks-export";
/// Aktuelle Version des Exportformats; ältere werden gelesen, neuere abgelehnt.
pub const EXPORT_VERSION: u32 = 1;

/// Inhalt von `tasks.json`: alle Listen und Tasks (auch erledigte und solche im Papierkorb).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExportDto {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub lists: Vec<ExportListDto>,
    pub tasks: Vec<TaskDto>,
    /// Tags (ADR-035, seit N8); ältere Exporte haben keine.
    #[serde(default)]
    pub tags: Vec<ExportTagDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExportTagDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExportListDto {
    pub id: String,
    pub name: String,
    pub is_inbox: bool,
}

impl ExportDto {
    #[must_use]
    pub fn from_snapshot(snapshot: &DataSnapshot, exported_at: Timestamp) -> Self {
        Self {
            format: EXPORT_FORMAT.to_owned(),
            version: EXPORT_VERSION,
            exported_at: iso(exported_at),
            lists: snapshot
                .lists
                .iter()
                .map(|l| ExportListDto {
                    id: l.id().to_string(),
                    // Der Eingang heißt im Core „Inbox“; im Export so, wie die App ihn zeigt.
                    name: if l.is_inbox() {
                        "Eingang".to_owned()
                    } else {
                        l.name().to_owned()
                    },
                    is_inbox: l.is_inbox(),
                })
                .collect(),
            tasks: snapshot.tasks.iter().map(TaskDto::from).collect(),
            tags: snapshot
                .tags
                .iter()
                .map(|t| ExportTagDto {
                    id: t.id().to_string(),
                    name: t.name().to_owned(),
                })
                .collect(),
        }
    }

    /// Prüft Kennung und Version und wandelt in die Eingabe des Imports.
    ///
    /// # Errors
    /// `CoreError::Validation` bei fremdem Format, neuerer Version oder ungültigen Feldern.
    pub fn into_import(self) -> CoreResult<(Vec<ImportList>, Vec<TaskParts>, Vec<ImportTag>)> {
        if self.format != EXPORT_FORMAT {
            return Err(CoreError::Validation(format!(
                "not a tasks export (format {:?})",
                self.format
            )));
        }
        if self.version > EXPORT_VERSION {
            return Err(CoreError::Validation(format!(
                "export version {} is newer than this app ({EXPORT_VERSION}) – please update",
                self.version
            )));
        }
        let lists = self
            .lists
            .iter()
            .map(|l| {
                Ok(ImportList {
                    id: TaskListId::parse(&l.id)
                        .map_err(|e| CoreError::Validation(e.to_string()))?,
                    name: l.name.clone(),
                    is_inbox: l.is_inbox,
                })
            })
            .collect::<CoreResult<_>>()?;
        let tasks = self
            .tasks
            .iter()
            .map(|t| {
                Ok(TaskParts {
                    trashed_at: t.trashed_at.as_deref().map(parse_iso).transpose()?,
                    ..TaskParts::try_from(t)?
                })
            })
            .collect::<CoreResult<_>>()?;
        let tags = self
            .tags
            .iter()
            .map(|t| {
                Ok(ImportTag {
                    id: TagId::parse(&t.id).map_err(|e| CoreError::Validation(e.to_string()))?,
                    name: t.name.clone(),
                })
            })
            .collect::<CoreResult<_>>()?;
        Ok((lists, tasks, tags))
    }
}

/// Ergebnis von `import_data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ImportDataReportDto {
    pub lists_created: u32,
    pub tasks_created: u32,
    pub tasks_skipped: u32,
}

impl From<ImportDataReport> for ImportDataReportDto {
    fn from(r: ImportDataReport) -> Self {
        Self {
            lists_created: r.lists_created,
            tasks_created: r.tasks_created,
            tasks_skipped: r.tasks_skipped,
        }
    }
}

/// Eine Notiz für den Export als Datei (Pfad = Ordnernamen ab der Wurzel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportNoteDto {
    pub path: Vec<String>,
    pub name: String,
    pub kind: NoteKindDto,
    pub body: String,
}

impl From<&ExportNote> for ExportNoteDto {
    fn from(n: &ExportNote) -> Self {
        Self {
            path: n.path.clone(),
            name: n.name.clone(),
            kind: n.kind.into(),
            body: n.body.clone(),
        }
    }
}
