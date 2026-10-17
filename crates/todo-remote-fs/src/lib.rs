//! [`FsRemoteStore`]: der `RemoteStore`-Port auf einem lokalen Verzeichnis.
//!
//! Für Entwicklung, Integrationstests mit mehreren Geräten und die Dev-CLI – zwei Prozesse
//! können sich ein Verzeichnis teilen. Kein Änderungs-Feed (`changes_since` → `None`).
//!
//! - `create` schreibt in eine Temp-Datei und verlinkt sie per `hard_link` unter dem Zielnamen:
//!   atomar, und ein vorhandenes Ziel wird nie überschrieben (`EEXIST` → `Conflict`).
//! - `replace` schreibt eine Temp-Datei und benennt sie atomar um. Die Revisionsprüfung davor
//!   ist nicht gegen gleichzeitige Schreiber geschützt – genügt, weil nur der Besitzer seine
//!   Metadateien ersetzt.
//! - Dateisystemzugriffe sind synchron (kleine Dateien); für Produktion gibt es andere Adapter.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::UNIX_EPOCH,
};

use todo_core::{
    domain::Timestamp,
    ports::{
        RemoteChanges, RemoteCursor, RemoteDir, RemoteEntry, RemoteListing, RemotePath,
        RemoteStore, Revision,
    },
    CoreError, CoreResult,
};

/// Präfix für halbfertige Dateien; `list` überspringt sie.
const TEMP_PREFIX: &str = ".tmp-";

/// Siehe Crate-Doku.
#[derive(Debug)]
pub struct FsRemoteStore {
    root: PathBuf,
    temp_counter: AtomicU64,
}

impl FsRemoteStore {
    /// Store auf dem Verzeichnis `root` (wird bei Bedarf angelegt).
    ///
    /// # Errors
    /// `CoreError::RemoteRejected`, wenn das Verzeichnis nicht angelegt werden kann.
    pub fn new(root: impl Into<PathBuf>) -> CoreResult<Self> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|e| {
            CoreError::RemoteRejected(format!("cannot create {}: {e}", root.display()))
        })?;
        Ok(Self {
            root,
            temp_counter: AtomicU64::new(0),
        })
    }

    fn local(&self, path: &RemotePath) -> PathBuf {
        path.as_str()
            .split('/')
            .fold(self.root.clone(), |p, seg| p.join(seg))
    }

    /// Schreibt `bytes` in eine neue Temp-Datei im Verzeichnis von `target`.
    fn write_temp(&self, target: &Path, bytes: &[u8]) -> CoreResult<PathBuf> {
        let dir = target.parent().unwrap_or(&self.root);
        fs::create_dir_all(dir).map_err(|e| io_error(&e, dir))?;
        let temp = dir.join(format!(
            "{TEMP_PREFIX}{}-{}",
            std::process::id(),
            self.temp_counter.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&temp, bytes).map_err(|e| io_error(&e, &temp))?;
        Ok(temp)
    }

    fn entry(&self, path: &RemotePath) -> CoreResult<RemoteEntry> {
        let local = self.local(path);
        let meta = fs::metadata(&local).map_err(|e| io_error(&e, &local))?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .unwrap_or_default();
        Ok(RemoteEntry {
            path: path.clone(),
            size: meta.len(),
            revision: Revision(format!(
                "{}.{:09}-{}",
                modified.as_secs(),
                modified.subsec_nanos(),
                meta.len()
            )),
            modified: Timestamp::from_millis(
                i64::try_from(modified.as_millis()).unwrap_or(i64::MAX),
            ),
        })
    }

    fn walk(&self, dir: &Path, prefix: &RemotePath, out: &mut Vec<RemoteEntry>) -> CoreResult<()> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(io_error(&e, dir)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| io_error(&e, dir))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with(TEMP_PREFIX) {
                continue;
            }
            let path = prefix.join(name)?;
            let kind = entry.file_type().map_err(|e| io_error(&e, dir))?;
            if kind.is_dir() {
                self.walk(&entry.path(), &path, out)?;
            } else if kind.is_file() {
                out.push(self.entry(&path)?);
            }
        }
        Ok(())
    }
}

fn io_error(err: &io::Error, path: &Path) -> CoreError {
    match err.kind() {
        io::ErrorKind::NotFound => CoreError::NotFound(format!("{}", path.display())),
        io::ErrorKind::AlreadyExists => CoreError::Conflict(format!("{} exists", path.display())),
        io::ErrorKind::PermissionDenied => {
            CoreError::RemoteRejected(format!("{}: {err}", path.display()))
        }
        _ => CoreError::Network(format!("{}: {err}", path.display())),
    }
}

#[async_trait::async_trait]
impl RemoteStore for FsRemoteStore {
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir> {
        let path = RemotePath::new(name)?;
        let local = self.local(&path);
        fs::create_dir_all(&local).map_err(|e| io_error(&e, &local))?;
        Ok(RemoteDir { path })
    }

    async fn list(
        &self,
        dir: &RemotePath,
        _since: Option<&RemoteCursor>,
    ) -> CoreResult<RemoteListing> {
        let mut entries = Vec::new();
        self.walk(&self.local(dir), dir, &mut entries)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(RemoteListing {
            entries,
            cursor: RemoteCursor(String::new()),
        })
    }

    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>> {
        let local = self.local(path);
        fs::read(&local).map_err(|e| io_error(&e, &local))
    }

    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry> {
        let target = self.local(path);
        let temp = self.write_temp(&target, &bytes)?;
        let linked = fs::hard_link(&temp, &target);
        // Die Temp-Datei wird in jedem Fall entfernt; ein Fehler dabei ändert nichts am Ergebnis.
        let _ = fs::remove_file(&temp);
        linked.map_err(|e| io_error(&e, &target))?;
        self.entry(path)
    }

    async fn replace(
        &self,
        path: &RemotePath,
        bytes: Vec<u8>,
        expected: Option<&Revision>,
    ) -> CoreResult<RemoteEntry> {
        if let Some(expected) = expected {
            let current = match self.entry(path) {
                Ok(entry) => Some(entry.revision),
                Err(CoreError::NotFound(_)) => None,
                Err(e) => return Err(e),
            };
            if current.as_ref() != Some(expected) {
                return Err(CoreError::Conflict(format!("{path}: revision changed")));
            }
        }
        let target = self.local(path);
        let temp = self.write_temp(&target, &bytes)?;
        fs::rename(&temp, &target).map_err(|e| {
            let _ = fs::remove_file(&temp);
            io_error(&e, &target)
        })?;
        self.entry(path)
    }

    async fn delete(&self, path: &RemotePath) -> CoreResult<()> {
        let local = self.local(path);
        match fs::remove_file(&local) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_error(&e, &local)),
        }
    }

    async fn changes_since(
        &self,
        _cursor: Option<&RemoteCursor>,
    ) -> CoreResult<Option<RemoteChanges>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use tempfile::TempDir;

    use super::*;

    /// Store auf einem Temp-Verzeichnis, das so lange lebt wie der Store.
    #[derive(Debug)]
    struct Temp {
        store: FsRemoteStore,
        _dir: TempDir,
    }

    impl Temp {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            Self {
                store: FsRemoteStore::new(dir.path().join("remote")).expect("store"),
                _dir: dir,
            }
        }
    }

    impl std::ops::Deref for Temp {
        type Target = FsRemoteStore;

        fn deref(&self) -> &FsRemoteStore {
            &self.store
        }
    }

    mod contract {
        todo_core::remote_store_contract_tests!(*super::Temp::new());
    }

    fn path(s: &str) -> RemotePath {
        RemotePath::new(s).unwrap()
    }

    #[test]
    fn two_stores_share_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let a = FsRemoteStore::new(dir.path()).unwrap();
        let b = FsRemoteStore::new(dir.path()).unwrap();
        block_on(a.create(&path("x/1.seg"), vec![1])).unwrap();
        assert_eq!(block_on(b.read(&path("x/1.seg"))).unwrap(), [1]);
        assert!(matches!(
            block_on(b.create(&path("x/1.seg"), vec![2])),
            Err(CoreError::Conflict(_))
        ));
    }

    #[test]
    fn listing_skips_temp_files_and_leaves_none_behind() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsRemoteStore::new(dir.path()).unwrap();
        block_on(store.create(&path("ops/1.seg"), vec![1])).unwrap();
        let _ = block_on(store.create(&path("ops/1.seg"), vec![2]));
        block_on(store.replace(&path("ops/meta"), vec![3], None)).unwrap();
        fs::write(dir.path().join("ops/.tmp-crashed"), b"half").unwrap();
        let listing = block_on(store.list(&path("ops"), None)).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.path.file_name()).collect();
        assert_eq!(names, ["1.seg", "meta"]);
        let leftovers = fs::read_dir(dir.path().join("ops"))
            .unwrap()
            .filter(|e| {
                let name = e.as_ref().unwrap().file_name();
                name.to_str().unwrap().starts_with(TEMP_PREFIX) && name != ".tmp-crashed"
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn has_no_change_feed() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsRemoteStore::new(dir.path()).unwrap();
        assert_eq!(block_on(store.changes_since(None)).unwrap(), None);
    }
}
