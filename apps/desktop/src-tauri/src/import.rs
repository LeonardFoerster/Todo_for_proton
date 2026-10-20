//! Ordner importieren (`notes.md` §8, nur Desktop): Ordnerauswahl über das XDG-Desktop-Portal,
//! Einlesen hier in der Shell, Anlegen im Core (`ImportNotes`, Blöcke zu je ≤ 500 Elementen).
//! Einmalige Kopie – der Quellordner wird nie verändert.

use std::{
    fs,
    path::{Path, PathBuf},
};

use todo_core::{
    api::{
        note_dto::{ImportNoteFileDto, ImportReportDto, ImportSkipDto, NoteKindDto},
        CoreApi,
    },
    domain::NOTE_BODY_MAX_BYTES,
    CoreResult,
};

/// Tiefer als so viele Unterordner (unter dem neuen obersten Ordner) geht nicht (Core: 32).
const MAX_DIRS: usize = 31;
/// Schutz vor versehentlich gewählten riesigen Ordnern (Home, `/`).
const MAX_FILES: usize = 20_000;

/// Eingelesene Dateien und Übersprungenes (Pfade relativ zum gewählten Ordner).
#[derive(Debug, Default)]
pub struct Scan {
    pub files: Vec<ImportNoteFileDto>,
    pub skipped: Vec<ImportSkipDto>,
}

/// Liest `root` rekursiv: `.md`/`.markdown` → Markdown, `.txt` → Text. Übersprungen (mit Grund)
/// werden versteckte Dateien und Ordner, andere Endungen, Nicht-UTF-8, Dateien über 1 MiB,
/// Symlinks (nie gefolgt) und zu tiefe Ordner. UTF-8-BOM weg, Zeilenenden LF.
pub fn scan(root: &Path) -> Scan {
    let mut out = Scan::default();
    visit(root, &mut Vec::new(), &mut out);
    out
}

fn skip(out: &mut Scan, dirs: &[String], name: &str, reason: &str) {
    let path = dirs
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(name))
        .collect::<Vec<_>>()
        .join("/");
    out.skipped.push(ImportSkipDto {
        path,
        reason: reason.to_owned(),
    });
}

fn visit(dir: &Path, dirs: &mut Vec<String>, out: &mut Scan) {
    let mut entries: Vec<(String, PathBuf, fs::FileType)> = match fs::read_dir(dir) {
        Ok(read) => read
            .filter_map(Result::ok)
            .filter_map(|e| {
                let kind = e.file_type().ok()?;
                Some((e.file_name().to_string_lossy().into_owned(), e.path(), kind))
            })
            .collect(),
        Err(_) => {
            let name = dirs.pop().unwrap_or_default();
            skip(out, dirs, &name, "Ordner nicht lesbar");
            dirs.push(name);
            return;
        }
    };
    // Gleiche Reihenfolge wie im Dateimanager (ohne Groß/Klein), damit der Baum vertraut aussieht.
    entries.sort_by(|a, b| {
        a.0.to_lowercase()
            .cmp(&b.0.to_lowercase())
            .then(a.0.cmp(&b.0))
    });
    for (name, path, kind) in entries {
        if name.starts_with('.') {
            skip(out, dirs, &name, "versteckt");
            continue;
        }
        if kind.is_symlink() {
            skip(out, dirs, &name, "Verknüpfung (nicht gefolgt)");
            continue;
        }
        if kind.is_dir() {
            if dirs.len() >= MAX_DIRS {
                skip(out, dirs, &name, "zu tief verschachtelt");
                continue;
            }
            dirs.push(name);
            visit(&path, dirs, out);
            dirs.pop();
            continue;
        }
        if !kind.is_file() {
            skip(out, dirs, &name, "keine Datei");
            continue;
        }
        let Some((stem, kind)) = note_kind(&name) else {
            skip(out, dirs, &name, "andere Endung");
            continue;
        };
        if out.files.len() >= MAX_FILES {
            skip(out, dirs, &name, "zu viele Dateien");
            continue;
        }
        match fs::metadata(&path) {
            Ok(meta) if meta.len() > NOTE_BODY_MAX_BYTES as u64 => {
                skip(out, dirs, &name, "größer als 1 MiB");
                continue;
            }
            Ok(_) => {}
            Err(_) => {
                skip(out, dirs, &name, "nicht lesbar");
                continue;
            }
        }
        let Ok(bytes) = fs::read(&path) else {
            skip(out, dirs, &name, "nicht lesbar");
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            skip(out, dirs, &name, "kein UTF-8");
            continue;
        };
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        out.files.push(ImportNoteFileDto {
            dirs: dirs.clone(),
            name: stem,
            kind,
            text: text.replace("\r\n", "\n").replace('\r', "\n"),
        });
    }
}

/// Name ohne Endung und Typ; `None` für andere Endungen.
fn note_kind(name: &str) -> Option<(String, NoteKindDto)> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    let kind = match ext.to_ascii_lowercase().as_str() {
        "md" | "markdown" => NoteKindDto::Markdown,
        "txt" => NoteKindDto::Text,
        _ => return None,
    };
    Some((stem.to_owned(), kind))
}

/// Liest `root` und importiert unter einem neuen Ordner mit dessen Namen. Übersprungenes aus
/// Shell und Core steht gemeinsam im Bericht.
///
/// # Errors
/// Fehler des Cores (bis dahin importierte Blöcke bleiben).
pub async fn import_path(api: &CoreApi, root: PathBuf) -> CoreResult<ImportReportDto> {
    let name = root
        .file_name()
        .map_or_else(|| "Import".to_owned(), |n| n.to_string_lossy().into_owned());
    let scanned = tauri::async_runtime::spawn_blocking(move || scan(&root))
        .await
        .map_err(|e| todo_core::CoreError::Internal(format!("reading the folder failed: {e}")))?;
    let mut report = api.import_notes(name, scanned.files).await?;
    let mut skipped = scanned.skipped;
    skipped.append(&mut report.skipped);
    report.skipped = skipped;
    Ok(report)
}

/// Ordner über das Desktop-Portal wählen; `None`, wenn abgebrochen.
///
/// # Errors
/// Kein Portal verfügbar oder unlesbare Antwort.
#[cfg(target_os = "linux")]
pub async fn pick_folder() -> Result<Option<PathBuf>, String> {
    use ashpd::desktop::{file_chooser::SelectedFiles, ResponseError};
    let request = SelectedFiles::open_file()
        .title("Ordner importieren")
        .accept_label("Importieren")
        .directory(true)
        .modal(true)
        .multiple(false)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    match request.response() {
        Ok(files) => Ok(files.uris().first().and_then(|uri| file_path(uri.as_str()))),
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(target_os = "linux"))]
pub async fn pick_folder() -> Result<Option<PathBuf>, String> {
    Err("folder selection is only available on Linux".into())
}

/// `file:///a/b%20c` → `/a/b c` (Prozent-Kodierung als Bytes, nicht nur UTF-8).
fn file_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(out).ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_become_paths() {
        assert_eq!(
            file_path("file:///home/a/Mein%20Vault"),
            Some(PathBuf::from("/home/a/Mein Vault"))
        );
        assert_eq!(
            file_path("file:///tmp/%C3%BCber"),
            Some(PathBuf::from("/tmp/über"))
        );
        assert_eq!(file_path("https://x"), None);
        assert_eq!(file_path("file:///x%zz"), None);
    }

    #[test]
    fn note_kinds_by_extension() {
        assert!(matches!(note_kind("a.md"), Some((s, NoteKindDto::Markdown)) if s == "a"));
        assert!(matches!(
            note_kind("b.MARKDOWN"),
            Some((_, NoteKindDto::Markdown))
        ));
        assert!(matches!(note_kind("c.d.txt"), Some((s, NoteKindDto::Text)) if s == "c.d"));
        assert!(note_kind("bild.png").is_none());
        assert!(note_kind("ohne").is_none());
        assert!(note_kind(".md").is_none());
    }
}
