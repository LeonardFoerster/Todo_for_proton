//! Export und Import (ADR-032, N6-04): `Tasks-Export-JJJJ-MM-TT/` mit `tasks.json` und den Notizen als
//! `.md`/`.txt` in ihrer Ordnerstruktur unter `notizen/`; Import liest eine `tasks.json`. Auswahl
//! über das XDG-Desktop-Portal wie „Ordner importieren…“.

use std::path::{Path, PathBuf};

use serde::Serialize;
use todo_core::{
    api::{
        export_dto::{ExportDto, ImportDataReportDto},
        note_dto::NoteKindDto,
        CoreApi,
    },
    CoreError, CoreResult,
};

/// Größte `tasks.json`, die der Import liest.
const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;

/// Ergebnis eines Exports (für die Meldung in der UI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExportResultDto {
    /// Angelegter Ordner.
    pub path: String,
    pub lists: u32,
    pub tasks: u32,
    pub notes: u32,
}

fn io(what: &str, e: impl std::fmt::Display) -> CoreError {
    CoreError::Internal(format!("{what}: {e}"))
}

/// Schreibt den Export in einen neuen Ordner unter `parent` (bei Namensgleichheit „… (2)“).
///
/// # Errors
/// `Validation` bei ungültigem Datum, `Internal` bei Schreibfehlern.
pub async fn write_export(
    api: &CoreApi,
    parent: &Path,
    today: &str,
) -> CoreResult<ExportResultDto> {
    let valid = today.len() == 10
        && today.char_indices().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    if !valid {
        return Err(CoreError::Validation(format!("invalid date {today:?}")));
    }
    let data = api.export_data().await?;
    let notes = api.export_notes().await?;

    let base = format!("Tasks-Export-{today}");
    let dir = unique(parent, &base, "");
    std::fs::create_dir_all(&dir).map_err(|e| io("creating the export folder failed", e))?;
    let json = serde_json::to_vec_pretty(&data).map_err(|e| io("encoding tasks.json failed", e))?;
    std::fs::write(dir.join("tasks.json"), json).map_err(|e| io("writing tasks.json failed", e))?;

    let root = dir.join("notizen");
    for note in &notes {
        let mut folder = root.clone();
        for part in &note.path {
            folder.push(file_name(part));
        }
        std::fs::create_dir_all(&folder).map_err(|e| io("creating a note folder failed", e))?;
        let ext = match note.kind {
            NoteKindDto::Markdown => ".md",
            NoteKindDto::Text => ".txt",
        };
        // Gleichnamige Notizen im selben Ordner: die vorige liegt schon auf der Platte → „(2)“.
        let file = unique(&folder, &file_name(&note.name), ext);
        std::fs::write(&file, &note.body).map_err(|e| io("writing a note failed", e))?;
    }

    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Ok(ExportResultDto {
        path: dir.to_string_lossy().into_owned(),
        lists: count(data.lists.len()),
        tasks: count(data.tasks.len()),
        notes: count(notes.len()),
    })
}

/// Liest eine `tasks.json` und spielt sie ein.
///
/// # Errors
/// `Validation` bei zu großer oder ungültiger Datei, Fehler des Imports.
pub async fn import_file(api: &CoreApi, file: &Path) -> CoreResult<ImportDataReportDto> {
    let size = std::fs::metadata(file)
        .map_err(|e| io("reading the file failed", e))?
        .len();
    if size > MAX_IMPORT_BYTES {
        return Err(CoreError::Validation(format!(
            "file too large ({size} bytes)"
        )));
    }
    let bytes = std::fs::read(file).map_err(|e| io("reading the file failed", e))?;
    let export: ExportDto = serde_json::from_slice(&bytes)
        .map_err(|e| CoreError::Validation(format!("not a tasks export: {e}")))?;
    api.import_data(export).await
}

/// Gültiger Datei-/Ordnername: ohne `/`, Steuerzeichen und führende Punkte, höchstens 120 Zeichen.
fn file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').trim();
    let short: String = trimmed.chars().take(120).collect();
    if short.is_empty() {
        "Ohne Titel".to_owned()
    } else {
        short
    }
}

/// `dir/stem+ext`, bei Bedarf `stem (2)+ext`, `stem (3)+ext` …, was noch nicht existiert.
fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let stem = stem.trim_end();
    let first = dir.join(format!("{stem}{ext}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// `tasks.json` über das Desktop-Portal wählen; `None`, wenn abgebrochen.
///
/// # Errors
/// Kein Portal verfügbar oder unlesbare Antwort.
#[cfg(target_os = "linux")]
pub async fn pick_export_file() -> Result<Option<PathBuf>, String> {
    use ashpd::desktop::{
        file_chooser::{FileFilter, SelectedFiles},
        ResponseError,
    };
    let filter = FileFilter::new("Tasks-Export (tasks.json)")
        .mimetype("application/json")
        .glob("*.json");
    let request = SelectedFiles::open_file()
        .title("Aus Export wiederherstellen")
        .accept_label("Wiederherstellen")
        .modal(true)
        .multiple(false)
        .filter(filter)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    match request.response() {
        Ok(files) => Ok(files
            .uris()
            .first()
            .and_then(|uri| crate::import::file_path(uri.as_str()))),
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(target_os = "linux"))]
pub async fn pick_export_file() -> Result<Option<PathBuf>, String> {
    Err("file selection is only available on Linux".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe_and_unique() {
        assert_eq!(file_name("a/b\\c"), "a-b-c");
        assert_eq!(file_name("  ..versteckt "), "versteckt");
        assert_eq!(file_name(" "), "Ohne Titel");
        let dir = tempfile::tempdir().unwrap();
        let first = unique(dir.path(), "Blatt", ".md");
        std::fs::write(&first, "").unwrap();
        assert_eq!(
            unique(dir.path(), "Blatt", ".md"),
            dir.path().join("Blatt (2).md")
        );
    }
}
