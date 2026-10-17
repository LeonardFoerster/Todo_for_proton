//! `RemoteStore`: das Sync-Backend als schmaler, dateiorientierter Speicher
//! (siehe `development/interfaces.md` §4).
//!
//! Die Sync-Engine braucht nur einen App-Ordner mit (überwiegend) unveränderlichen Dateien.
//! Alles Backend-Spezifische (Volumes, Shares, Revisionen, Block-Upload, Krypto des Anbieters)
//! bleibt im Adapter. Implementierungen: Proton-Drive-Adapter, `FsRemoteStore` (lokales
//! Verzeichnis), `InMemoryRemoteStore` (Tests, mit Fehler-Injektion).

use std::fmt;

use super::{MaybeSend, MaybeSync};
use crate::{domain::Timestamp, CoreError, CoreResult};

/// Pfad im Backend: relativ, `/`-getrennt, ohne leere Segmente, `.` oder `..`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RemotePath(String);

impl RemotePath {
    /// # Errors
    /// `CoreError::Validation` bei leerem, absolutem oder nicht normalisiertem Pfad.
    pub fn new(path: &str) -> CoreResult<Self> {
        let valid = !path.is_empty()
            && path
                .split('/')
                .all(|seg| !seg.is_empty() && seg != "." && seg != ".." && !seg.contains('\\'));
        if valid {
            Ok(Self(path.to_owned()))
        } else {
            Err(CoreError::Validation(format!(
                "invalid remote path: {path:?}"
            )))
        }
    }

    /// Hängt einen relativen Pfad an.
    ///
    /// # Errors
    /// Wie [`RemotePath::new`] für `relative`.
    pub fn join(&self, relative: &str) -> CoreResult<Self> {
        Self::new(relative).map(|rel| Self(format!("{}/{}", self.0, rel.0)))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Letztes Segment (Dateiname).
    #[must_use]
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// Liegt dieser Pfad in `dir` (oder ist er `dir`)?
    #[must_use]
    pub fn starts_with(&self, dir: &RemotePath) -> bool {
        self.0 == dir.0
            || self
                .0
                .strip_prefix(&dir.0)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    /// Pfad relativ zu `dir`, wenn er darin liegt.
    #[must_use]
    pub fn relative_to(&self, dir: &RemotePath) -> Option<&str> {
        self.0
            .strip_prefix(&dir.0)
            .and_then(|rest| rest.strip_prefix('/'))
    }
}

impl fmt::Display for RemotePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Opaker Versionsstand einer Datei (für Optimistic Concurrency bei [`RemoteStore::replace`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Revision(pub String);

/// Opaker Stand eines Listings oder Änderungs-Feeds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteCursor(pub String);

/// Eine Datei im Backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub path: RemotePath,
    pub size: u64,
    pub revision: Revision,
    pub modified: Timestamp,
}

/// Ergebnis von [`RemoteStore::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteListing {
    /// Alle Dateien unterhalb des Verzeichnisses (rekursiv), nach Pfad sortiert.
    pub entries: Vec<RemoteEntry>,
    pub cursor: RemoteCursor,
}

/// Ergebnis von [`RemoteStore::changes_since`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteChanges {
    /// Angelegte oder geänderte Dateien seit dem Cursor.
    pub changed: Vec<RemoteEntry>,
    /// Gelöschte Dateien seit dem Cursor.
    pub deleted: Vec<RemotePath>,
    /// Stand für den nächsten Aufruf.
    pub cursor: RemoteCursor,
    /// Der Feed kann die Lücke nicht schließen (Cursor zu alt) → einmal komplett `list`en.
    pub refresh: bool,
}

/// Der App-Ordner im Backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteDir {
    pub path: RemotePath,
}

impl RemoteDir {
    /// Pfad einer Datei im App-Ordner.
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungültigem relativem Pfad.
    pub fn join(&self, relative: &str) -> CoreResult<RemotePath> {
        self.path.join(relative)
    }
}

/// Sync-Backend. Fehler: `Network`/`RateLimited` sind retrybar, `RemoteRejected` endgültig,
/// `NotFound` für fehlende Dateien, `Conflict` bei existierender Datei bzw. falscher Revision.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait RemoteStore: MaybeSend + MaybeSync {
    /// Stellt sicher, dass der App-Ordner `name` existiert, und liefert ihn.
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir>;

    /// Alle Dateien unterhalb von `dir` (rekursiv, sortiert). Mit `since` darf das Backend nur
    /// Änderungen seit diesem Stand liefern – oder trotzdem alles (Aufrufer muss beides vertragen).
    async fn list(
        &self,
        dir: &RemotePath,
        since: Option<&RemoteCursor>,
    ) -> CoreResult<RemoteListing>;

    /// Inhalt einer Datei. `NotFound`, wenn sie fehlt.
    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>>;

    /// Schreibt eine **neue** Datei. `Conflict`, wenn sie existiert – überschreibt nie.
    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry>;

    /// Ersetzt eine kleine, gerätelokale Metadatei (z. B. `devices/<id>.json`). Mit `expected`
    /// nur, wenn die aktuelle Revision passt (sonst `Conflict`); ohne legt es an oder ersetzt.
    async fn replace(
        &self,
        path: &RemotePath,
        bytes: Vec<u8>,
        expected: Option<&Revision>,
    ) -> CoreResult<RemoteEntry>;

    /// Löscht eine Datei. Fehlt sie schon, ist das kein Fehler.
    async fn delete(&self, path: &RemotePath) -> CoreResult<()>;

    /// Änderungs-Feed des Backends. `None` = nicht unterstützt (dann `list`). Ohne `cursor`
    /// liefert er nur den aktuellen Stand (keine Einträge) als Startpunkt.
    async fn changes_since(
        &self,
        cursor: Option<&RemoteCursor>,
    ) -> CoreResult<Option<RemoteChanges>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_validated() {
        for ok in ["a", "Apps/Tasks", "ops/dev/1-2.seg", ".hidden"] {
            assert!(RemotePath::new(ok).is_ok(), "{ok}");
        }
        for bad in ["", "/abs", "a//b", "a/", "../x", "a/./b", "a\\b"] {
            assert!(RemotePath::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn join_prefix_and_names() {
        let root = RemotePath::new("Apps/Tasks").unwrap();
        let seg = root.join("ops/d/1-2.seg").unwrap();
        assert_eq!(seg.as_str(), "Apps/Tasks/ops/d/1-2.seg");
        assert_eq!(seg.file_name(), "1-2.seg");
        assert!(seg.starts_with(&root));
        assert!(!RemotePath::new("Apps/TasksX/a").unwrap().starts_with(&root));
        assert_eq!(seg.relative_to(&root), Some("ops/d/1-2.seg"));
        assert!(root.join("../x").is_err());
    }
}
