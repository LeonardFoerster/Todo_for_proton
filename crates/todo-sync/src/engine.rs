//! Sync-Engine: ein Durchlauf aus Bootstrap, Push und Pull (`sync-strategy.md` §3–§5).
//!
//! Layout im App-Ordner:
//!
//! ```text
//! manifest.json                      { format_version, app_id, created_hlc }
//! keys/sync-key.enc                  Sync-Schlüssel (geschützt durch die E2E des Backends)
//! devices/<device_id>.json           { name, platform, last_seen, last_seq, cursors } – nur Besitzer schreibt
//! ops/<device_id>/<from>-<to>.seg    unveränderliche Segmente (segment.rs)
//! ```
//!
//! Die Engine hält keinen Sync-Zustand im Speicher außer der Sitzung (App-Ordner, Schlüssel):
//! Outbox, Cursor, HLC und Feed-Cursor liegen im Repository. Laufzeitunabhängig – Timer und
//! Trigger liefert der Scheduler über die Shell.

use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use futures::future::join_all;
use serde::{Deserialize, Serialize};
use todo_core::{
    domain::{DeviceId, Timestamp},
    ports::{
        Cipher, Clock, KeyStore, RemoteCursor, RemoteDir, RemotePath, RemoteStore, SecretBytes,
        TaskRepository,
    },
    sync::{Hlc, Op, RemoteBatch},
    CoreError, CoreResult,
};

use crate::{
    segment::{open_segment, seal_segment, SegmentError, SegmentHeader, MAX_OPS},
    snapshot::{open_snapshot, seal_snapshot, SnapshotBody, SnapshotHeader},
};

/// Version des Ordner-Layouts (`manifest.json`). Neuere Ordner werden nicht beschrieben.
pub const LAYOUT_VERSION: u32 = 1;
/// Kennung im Manifest.
pub const APP_ID: &str = "todo-sync";

const MANIFEST: &str = "manifest.json";
const KEY_FILE: &str = "keys/sync-key.enc";
const KEY_MAGIC: &[u8; 4] = b"TKEY";
const OPS_DIR: &str = "ops";
const DEVICES_DIR: &str = "devices";
/// Schlüssel im Repository (`sync_value`).
const FEED_CURSOR: &str = "feed_cursor";
const DEVICE_FILE_AT: &str = "device_file_at";
const PUSHED_UP_TO: &str = "pushed_up_to";
const OPS_SINCE_SNAPSHOT: &str = "ops_since_snapshot";
const LAST_SNAPSHOT_AT: &str = "last_snapshot_at";
/// Eigene Op-Nummer, bis zu der der letzte eigene Snapshot reicht …
const SNAPSHOT_SEQ: &str = "snapshot_seq";
/// … und der vorletzte. Nur was dieser abdeckt, wird kompaktiert: So bleibt ein Ersatz, falls
/// der neueste Snapshot unlesbar ist.
const PREV_SNAPSHOT_SEQ: &str = "prev_snapshot_seq";
/// Erster Pull erledigt (ab dann keine Snapshot-Basis mehr nötig, außer bei Lücken).
const INITIAL_PULL_DONE: &str = "initial_pull_done";
const SNAPSHOTS_DIR: &str = "snapshots";
/// So viele eigene Snapshots bleiben liegen (ein Ersatz, falls der neueste unlesbar ist).
const KEEP_OWN_SNAPSHOTS: usize = 2;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// Höchstens so viele Downloads gleichzeitig.
const PARALLEL_DOWNLOADS: usize = 4;
/// So viele Segmente werden je `apply_remote` (eine Transaktion) zusammengefasst.
const SEGMENTS_PER_APPLY: usize = 20;

/// Einstellungen der Engine (aus der Composition Root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConfig {
    /// Name des App-Ordners im Backend.
    pub app_folder: String,
    /// Anzeigename dieses Geräts (in `devices/<id>.json`).
    pub device_name: String,
    pub platform: String,
    /// `devices/<id>.json` wird höchstens so oft aktualisiert (jeder Upload kostet).
    pub device_file_interval_ms: i64,
    /// Snapshot schreiben, wenn seit dem letzten mehr Ops gepusht/empfangen wurden …
    pub snapshot_after_ops: u64,
    /// … oder er älter ist als das (und seitdem etwas passiert ist).
    pub snapshot_max_age_ms: i64,
    /// Geräte, die so lange nicht gesehen wurden, bremsen die Kompaktierung nicht mehr
    /// (sie brauchen dann einen Full-Resync – noch nicht umgesetzt).
    pub inactive_device_ms: i64,
}

impl SyncConfig {
    /// Name des Sync-Schlüssels im `KeyStore` (beim Abmelden zu löschen).
    #[must_use]
    pub fn key_name(&self) -> String {
        format!("sync-key:{}", self.app_folder)
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            app_folder: "Tasks".into(),
            device_name: "unknown".into(),
            platform: "unknown".into(),
            device_file_interval_ms: 10 * 60 * 1000,
            snapshot_after_ops: 2_000,
            snapshot_max_age_ms: 7 * DAY_MS,
            inactive_device_ms: 90 * DAY_MS,
        }
    }
}

/// Ergebnis eines Sync-Durchlaufs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Hochgeladene Ops.
    pub pushed: u32,
    /// Empfangene Ops (inkl. Duplikate).
    pub received: u32,
    /// Lokal geänderte Entities.
    pub changed: u32,
    /// Übersprungene (beschädigte oder zu neue) Segmente.
    pub skipped_segments: u32,
    /// Ordner oder Segmente einer neueren App-Version gesehen → „App aktualisieren".
    pub newer_format: bool,
    /// Ops mit Uhrzeit mehr als 24 h in der Zukunft.
    pub clock_skews: u32,
    /// Hinweise ohne Inhalte (für Logs/Status).
    pub warnings: Vec<String>,
    /// Ein Snapshot wurde als Basis importiert.
    pub snapshot_loaded: bool,
    /// Ein eigener Snapshot wurde geschrieben.
    pub snapshot_written: bool,
    /// Durch Kompaktierung gelöschte eigene Segmente.
    pub segments_deleted: u32,
}

impl SyncReport {
    fn merge(&mut self, other: SyncReport) {
        self.pushed += other.pushed;
        self.received += other.received;
        self.changed += other.changed;
        self.skipped_segments += other.skipped_segments;
        self.newer_format |= other.newer_format;
        self.clock_skews += other.clock_skews;
        self.warnings.extend(other.warnings);
        self.snapshot_loaded |= other.snapshot_loaded;
        self.snapshot_written |= other.snapshot_written;
        self.segments_deleted += other.segments_deleted;
    }
}

/// Ausgang von [`SyncEngine::bootstrap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bootstrap {
    Ready,
    /// Der App-Ordner stammt von einer neueren App-Version; Sync pausiert, nichts wird geschrieben.
    NewerFormat(u32),
}

#[derive(Debug, Deserialize)]
struct ManifestVersion {
    format_version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format_version: u32,
    app_id: String,
    created_hlc: Hlc,
}

/// Inhalt von `devices/<id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: DeviceId,
    pub name: String,
    pub platform: String,
    /// Letzter Kontakt (Unix-ms).
    pub last_seen: i64,
    /// Höchste hochgeladene Op-Nummer.
    pub last_seq: u64,
    /// Bis wohin dieses Gerät die Ops anderer Geräte verarbeitet hat (Geräte-ID → Nummer).
    pub cursors: BTreeMap<String, u64>,
}

#[derive(Clone)]
struct Session {
    root: RemoteDir,
    key: SecretBytes,
    device: DeviceId,
}

/// Die Sync-Engine eines Geräts. Siehe Moduldoku.
pub struct SyncEngine {
    repo: Arc<dyn TaskRepository>,
    remote: Arc<dyn RemoteStore>,
    cipher: Arc<dyn Cipher>,
    key_store: Arc<dyn KeyStore>,
    clock: Arc<dyn Clock>,
    config: SyncConfig,
    session: Mutex<Option<Session>>,
    running: AtomicBool,
}

impl fmt::Debug for SyncEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncEngine")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Ein heruntergeladenes Segment (oder warum es fehlt).
enum Fetched {
    Ops(Vec<Op>),
    Missing,
    Unreadable(SegmentError),
}

/// Ein Segment eines fremden Geräts, das noch nicht verarbeitet ist.
#[derive(Debug, Clone)]
struct Planned {
    from: u64,
    to: u64,
    path: RemotePath,
}

impl SyncEngine {
    /// `repo` muss das Repository der `CoreApi` sein (`CoreApi::repository()`), damit die UI
    /// Remote-Änderungen als Events sieht.
    #[must_use]
    pub fn new(
        repo: Arc<dyn TaskRepository>,
        remote: Arc<dyn RemoteStore>,
        cipher: Arc<dyn Cipher>,
        key_store: Arc<dyn KeyStore>,
        clock: Arc<dyn Clock>,
        config: SyncConfig,
    ) -> Self {
        Self {
            repo,
            remote,
            cipher,
            key_store,
            clock,
            config,
            session: Mutex::new(None),
            running: AtomicBool::new(false),
        }
    }

    fn key_name(&self) -> String {
        self.config.key_name()
    }

    fn session(&self) -> CoreResult<Session> {
        self.session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| CoreError::Internal("sync engine not bootstrapped".into()))
    }

    fn is_bootstrapped(&self) -> bool {
        self.session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// Richtet den App-Ordner ein bzw. tritt ihm bei: Manifest, Sync-Schlüssel (erstes Gerät
    /// erzeugt ihn, weitere laden ihn), `devices/<id>.json`. Idempotent.
    ///
    /// # Errors
    /// Fehler von Backend, Repository oder KeyStore; `Validation` bei kaputtem Manifest/Schlüssel.
    pub async fn bootstrap(&self) -> CoreResult<Bootstrap> {
        let outcome = self.open_session().await?;
        if outcome == Bootstrap::Ready {
            self.write_device_file(true).await?;
        }
        Ok(outcome)
    }

    /// Bootstrap ohne `devices/<id>.json` (das schreibt [`Self::sync_once`] nach Push/Pull).
    async fn open_session(&self) -> CoreResult<Bootstrap> {
        let device = self.repo.device_id().await?;
        let root = self.remote.open_app_root(&self.config.app_folder).await?;
        if let Some(version) = self.ensure_manifest(&root, device).await? {
            return Ok(Bootstrap::NewerFormat(version));
        }
        let key = self.ensure_key(&root).await?;
        *self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(Session { root, key, device });
        Ok(Bootstrap::Ready)
    }

    /// Liest oder legt das Manifest an. `Some(version)`, wenn es neuer ist als unterstützt.
    async fn ensure_manifest(&self, root: &RemoteDir, device: DeviceId) -> CoreResult<Option<u32>> {
        let path = root.join(MANIFEST)?;
        let bytes = match self.remote.read(&path).await {
            Ok(bytes) => bytes,
            Err(CoreError::NotFound(_)) => {
                let manifest = Manifest {
                    format_version: LAYOUT_VERSION,
                    app_id: APP_ID.into(),
                    created_hlc: Hlc {
                        wall_ms: u64::try_from(self.clock.now().as_millis()).unwrap_or(0),
                        counter: 0,
                        device,
                    },
                };
                let json = serde_json::to_vec_pretty(&manifest).expect("manifest serializes");
                match self.remote.create(&path, json).await {
                    Ok(_) => return Ok(None),
                    // Ein anderes Gerät war schneller.
                    Err(CoreError::Conflict(_)) => self.remote.read(&path).await?,
                    Err(e) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        };
        // Erst nur die Version lesen: Eine neuere App darf den Rest beliebig ändern.
        let version: ManifestVersion = serde_json::from_slice(&bytes)
            .map_err(|e| CoreError::Validation(format!("invalid {MANIFEST}: {e}")))?;
        if version.format_version > LAYOUT_VERSION {
            return Ok(Some(version.format_version));
        }
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .map_err(|e| CoreError::Validation(format!("invalid {MANIFEST}: {e}")))?;
        if manifest.app_id != APP_ID {
            return Err(CoreError::Validation(format!(
                "{MANIFEST} belongs to another app ({:?})",
                manifest.app_id
            )));
        }
        Ok(None)
    }

    /// Lokaler Schlüssel, sonst der im App-Ordner, sonst ein neuer (und dort abgelegt).
    async fn ensure_key(&self, root: &RemoteDir) -> CoreResult<SecretBytes> {
        if let Some(key) = self.key_store.get(&self.key_name()).await? {
            return Ok(key);
        }
        let path = root.join(KEY_FILE)?;
        let key = match self.remote.read(&path).await {
            Ok(bytes) => decode_key(&bytes)?,
            Err(CoreError::NotFound(_)) => {
                let key = self.cipher.generate_key()?;
                match self.remote.create(&path, encode_key(&key)).await {
                    Ok(_) => key,
                    Err(CoreError::Conflict(_)) => decode_key(&self.remote.read(&path).await?)?,
                    Err(e) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        };
        self.key_store.put(&self.key_name(), key.clone()).await?;
        Ok(key)
    }

    /// Schreibt `devices/<id>.json`, wenn `force` oder das Intervall abgelaufen ist.
    async fn write_device_file(&self, force: bool) -> CoreResult<()> {
        let session = self.session()?;
        let now = self.clock.now().as_millis();
        if !force {
            let last: Option<i64> = self
                .repo
                .sync_value(DEVICE_FILE_AT)
                .await?
                .and_then(|v| v.parse().ok());
            if last.is_some_and(|at| now - at < self.config.device_file_interval_ms) {
                return Ok(());
            }
        }
        let info = DeviceInfo {
            device_id: session.device,
            name: self.config.device_name.clone(),
            platform: self.config.platform.clone(),
            last_seen: now,
            last_seq: self.number(PUSHED_UP_TO).await?,
            cursors: self
                .repo
                .sync_cursors()
                .await?
                .into_iter()
                .map(|(device, seq)| (device.to_string(), seq))
                .collect(),
        };
        let path = session
            .root
            .join(&format!("{DEVICES_DIR}/{}.json", session.device))?;
        let json = serde_json::to_vec_pretty(&info).expect("device info serializes");
        self.remote.replace(&path, json, None).await?;
        self.repo
            .set_sync_value(DEVICE_FILE_AT, now.to_string())
            .await
    }

    /// Lädt die Outbox in Segmenten (≤ 500 Ops) hoch. Liefert die Zahl hochgeladener Ops.
    ///
    /// Existiert ein Segment schon (Absturz nach dem Upload), wird sein Inhalt geprüft und die
    /// Ops gelten als hochgeladen – keine Duplikate.
    ///
    /// # Errors
    /// Fehler von Backend oder Repository; die Outbox bleibt dann ab dem fehlgeschlagenen
    /// Segment erhalten.
    pub async fn push(&self) -> CoreResult<u32> {
        let session = self.session()?;
        let mut pushed = 0;
        loop {
            let ops = self
                .repo
                .pending_ops(u32::try_from(MAX_OPS).unwrap_or(u32::MAX))
                .await?;
            if ops.is_empty() {
                return Ok(pushed);
            }
            let (header, bytes) = seal_segment(self.cipher.as_ref(), &session.key, &ops)?;
            let path = session.root.join(&format!(
                "{OPS_DIR}/{}/{}",
                session.device,
                header.file_name()
            ))?;
            match self.remote.create(&path, bytes.clone()).await {
                Ok(_) => {}
                Err(CoreError::Conflict(_)) => {
                    self.resolve_existing_segment(&session, &path, &ops, bytes)
                        .await?;
                }
                Err(e) => return Err(e),
            }
            self.repo.mark_ops_pushed(header.seq_to).await?;
            self.repo
                .set_sync_value(PUSHED_UP_TO, header.seq_to.to_string())
                .await?;
            pushed += u32::try_from(ops.len()).unwrap_or(u32::MAX);
        }
    }

    /// Das eigene Segment existiert schon: gleicher Inhalt → fertig; unlesbar (abgebrochener
    /// Upload) → ersetzen; anderer Inhalt → Fehler (sollte nie passieren).
    async fn resolve_existing_segment(
        &self,
        session: &Session,
        path: &RemotePath,
        ops: &[Op],
        bytes: Vec<u8>,
    ) -> CoreResult<()> {
        let existing = self.remote.read(path).await?;
        match open_segment(self.cipher.as_ref(), &session.key, &existing) {
            Ok((_, remote_ops)) if remote_ops == ops => Ok(()),
            Ok(_) => Err(CoreError::Conflict(format!(
                "segment {path} exists with different ops"
            ))),
            Err(_) => {
                self.remote.delete(path).await?;
                self.remote.create(path, bytes).await.map(|_| ())
            }
        }
    }

    /// Holt neue Segmente anderer Geräte, mergt sie und setzt die Cursor.
    ///
    /// Beschädigte oder zu neue Segmente werden übersprungen (Report), der Cursor des Geräts
    /// bleibt davor stehen. Ein Netzwerkfehler bricht ab; bereits Angewendetes bleibt.
    ///
    /// # Errors
    /// Fehler von Backend oder Repository.
    pub async fn pull(&self) -> CoreResult<SyncReport> {
        Ok(self.pull_inner().await?.0)
    }

    /// Wie [`Self::pull`]; zweiter Wert: Bei mindestens einem Gerät fehlen Segmente am Anfang
    /// der Kette (kompaktiert) – dann hilft nur ein Snapshot.
    async fn pull_inner(&self) -> CoreResult<(SyncReport, bool)> {
        let session = self.session()?;
        let ops_dir = session.root.join(OPS_DIR)?;
        let (scope, next_feed) = self.changed_devices(&ops_dir, session.device).await?;
        let listing = match scope {
            None => self.remote.list(&ops_dir, None).await?.entries,
            Some(devices) => {
                let mut entries = Vec::new();
                for device in devices {
                    let dir = ops_dir.join(&device.to_string())?;
                    entries.extend(self.remote.list(&dir, None).await?.entries);
                }
                entries
            }
        };

        let mut plan: BTreeMap<DeviceId, Vec<Planned>> = BTreeMap::new();
        for entry in listing {
            let Some((device, from, to)) = parse_segment_path(&entry.path, &ops_dir) else {
                continue;
            };
            if device != session.device {
                plan.entry(device).or_default().push(Planned {
                    from,
                    to,
                    path: entry.path,
                });
            }
        }

        let mut report = SyncReport::default();
        let mut behind = false;
        for (device, mut segments) in plan {
            let cursor = self.repo.sync_cursor(device).await?.unwrap_or(0);
            segments.retain(|s| s.to > cursor);
            segments.sort_by_key(|s| (s.from, s.to));
            behind |= segments.first().is_some_and(|s| s.from > cursor + 1);
            report.merge(
                self.pull_device(&session, device, cursor, &segments)
                    .await?,
            );
        }
        if let Some(cursor) = next_feed {
            self.repo.set_sync_value(FEED_CURSOR, cursor.0).await?;
        }
        Ok((report, behind))
    }

    /// Welche Geräteordner zu prüfen sind: `None` = alle (Listing), sonst laut Änderungs-Feed.
    /// Zweiter Wert: Feed-Cursor, der nach erfolgreichem Pull gilt.
    async fn changed_devices(
        &self,
        ops_dir: &RemotePath,
        own: DeviceId,
    ) -> CoreResult<(Option<Vec<DeviceId>>, Option<RemoteCursor>)> {
        let stored = self.repo.sync_value(FEED_CURSOR).await?.map(RemoteCursor);
        let Some(changes) = self.remote.changes_since(stored.as_ref()).await? else {
            return Ok((None, None));
        };
        if stored.is_none() || changes.refresh {
            // Erster Lauf oder Feed-Lücke: einmal alles listen, danach dem Feed folgen.
            return Ok((None, Some(changes.cursor)));
        }
        let mut devices: Vec<DeviceId> = changes
            .changed
            .iter()
            .filter_map(|entry| parse_segment_path(&entry.path, ops_dir))
            .map(|(device, _, _)| device)
            .filter(|device| *device != own)
            .collect();
        devices.sort();
        devices.dedup();
        Ok((Some(devices), Some(changes.cursor)))
    }

    /// Lädt die Segmente eines Geräts (parallel, max. 4) und wendet sie gruppenweise an.
    async fn pull_device(
        &self,
        session: &Session,
        device: DeviceId,
        cursor: u64,
        segments: &[Planned],
    ) -> CoreResult<SyncReport> {
        let mut report = SyncReport::default();
        // Nächste erwartete Nummer; der Cursor rückt nur über lückenlos gelesene Segmente vor.
        let mut next = cursor + 1;
        let mut chain_intact = true;
        for group in segments.chunks(SEGMENTS_PER_APPLY) {
            let mut ops = Vec::new();
            for downloads in group.chunks(PARALLEL_DOWNLOADS) {
                let fetched = join_all(downloads.iter().map(|s| self.fetch(session, s))).await;
                for (segment, result) in downloads.iter().zip(fetched) {
                    match result? {
                        Fetched::Ops(segment_ops) => {
                            if chain_intact && segment.from <= next {
                                next = next.max(segment.to + 1);
                            } else {
                                chain_intact = false;
                            }
                            ops.extend(segment_ops);
                        }
                        // Zwischen Listing und Lesen gelöscht (Kompaktierung) – später erneut.
                        Fetched::Missing => chain_intact = false,
                        Fetched::Unreadable(err) => {
                            chain_intact = false;
                            report.skipped_segments += 1;
                            report.newer_format |= matches!(err, SegmentError::NewerFormat(_));
                            report
                                .warnings
                                .push(format!("skipped {}: {err}", segment.path.file_name()));
                        }
                    }
                }
            }
            report.received += u32::try_from(ops.len()).unwrap_or(u32::MAX);
            let cursors = if next > cursor + 1 {
                vec![(device, next - 1)]
            } else {
                Vec::new()
            };
            if ops.is_empty() && cursors.is_empty() {
                continue;
            }
            let receipt = self
                .repo
                .apply_remote(RemoteBatch {
                    ops,
                    cursors,
                    now: self.clock.now(),
                })
                .await?;
            report.changed += u32::try_from(receipt.changed.len()).unwrap_or(u32::MAX);
            report.clock_skews += receipt.clock_skews;
        }
        Ok(report)
    }

    async fn fetch(&self, session: &Session, segment: &Planned) -> CoreResult<Fetched> {
        let bytes = match self.remote.read(&segment.path).await {
            Ok(bytes) => bytes,
            Err(CoreError::NotFound(_)) => return Ok(Fetched::Missing),
            Err(e) => return Err(e),
        };
        Ok(
            match open_segment(self.cipher.as_ref(), &session.key, &bytes) {
                Ok((header, ops))
                    if (header.seq_from, header.seq_to) == (segment.from, segment.to) =>
                {
                    Fetched::Ops(ops)
                }
                Ok(_) => Fetched::Unreadable(SegmentError::Corrupt(
                    "file name does not match header".into(),
                )),
                Err(err) => Fetched::Unreadable(err),
            },
        )
    }

    /// Bootstrap (falls nötig), Push, Pull und gelegentlich `devices/<id>.json`.
    ///
    /// # Errors
    /// Fehler, die den ganzen Durchlauf verhindern (Netz, Drosselung, Speicher); einzelne
    /// unlesbare Segmente stehen stattdessen im Report. `Conflict`, wenn bereits ein Durchlauf
    /// läuft.
    pub async fn sync_once(&self) -> CoreResult<SyncReport> {
        if self.running.swap(true, Ordering::Acquire) {
            return Err(CoreError::Conflict("sync already running".into()));
        }
        let result = self.run().await;
        self.running.store(false, Ordering::Release);
        result
    }

    async fn run(&self) -> CoreResult<SyncReport> {
        let first_run = !self.is_bootstrapped();
        if first_run {
            if let Bootstrap::NewerFormat(version) = self.open_session().await? {
                return Ok(SyncReport {
                    newer_format: true,
                    warnings: vec![format!(
                        "sync folder layout {version} is newer than supported"
                    )],
                    ..SyncReport::default()
                });
            }
        }
        let pushed = self.push().await?;
        let mut report = SyncReport::default();
        // Neues Gerät: Snapshot als Basis statt aller Segmente.
        let fresh = self.repo.sync_value(INITIAL_PULL_DONE).await?.is_none()
            && self.repo.sync_cursors().await?.is_empty();
        if fresh {
            report.merge(self.load_snapshot().await?);
        }
        let (pulled, behind) = self.pull_inner().await?;
        report.merge(pulled);
        if behind {
            // Segmente wurden kompaktiert, bevor dieses Gerät sie las.
            let loaded = self.load_snapshot().await?;
            if loaded.snapshot_loaded {
                report.merge(loaded);
                report.merge(self.pull_inner().await?.0);
            }
        }
        self.repo
            .set_sync_value(INITIAL_PULL_DONE, "1".into())
            .await?;
        report.pushed = pushed;
        report.merge(
            self.maybe_snapshot(u64::from(pushed) + u64::from(report.received))
                .await?,
        );
        self.write_device_file(first_run).await?;
        Ok(report)
    }

    /// Importiert den neuesten lesbaren Snapshot (State-Join über `apply_remote`) und setzt die
    /// Cursor auf seinen Vektor, soweit sie dadurch vorrücken.
    async fn load_snapshot(&self) -> CoreResult<SyncReport> {
        let session = self.session()?;
        let mut report = SyncReport::default();
        for path in self.snapshot_paths(&session, None).await?.into_iter().rev() {
            let bytes = match self.remote.read(&path).await {
                Ok(bytes) => bytes,
                Err(CoreError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            };
            let body = match open_snapshot(self.cipher.as_ref(), &session.key, &bytes) {
                Ok((_, body)) => body,
                Err(err) => {
                    report.skipped_segments += 1;
                    report.newer_format |= matches!(err, SegmentError::NewerFormat(_));
                    report
                        .warnings
                        .push(format!("skipped {}: {err}", path.file_name()));
                    continue;
                }
            };
            let local: BTreeMap<DeviceId, u64> =
                self.repo.sync_cursors().await?.into_iter().collect();
            let cursors = body
                .vector
                .iter()
                .filter(|(device, seq)| {
                    *device != session.device && local.get(device).is_none_or(|c| seq > c)
                })
                .copied()
                .collect();
            let receipt = self
                .repo
                .apply_remote(RemoteBatch {
                    ops: body.to_ops(),
                    cursors,
                    now: self.clock.now(),
                })
                .await?;
            report.changed += u32::try_from(receipt.changed.len()).unwrap_or(u32::MAX);
            report.clock_skews += receipt.clock_skews;
            report.snapshot_loaded = true;
            return Ok(report);
        }
        Ok(report)
    }

    /// Snapshot-Dateien, nach Alter aufsteigend; mit `device` nur die dieses Geräts.
    async fn snapshot_paths(
        &self,
        session: &Session,
        device: Option<DeviceId>,
    ) -> CoreResult<Vec<RemotePath>> {
        let dir = session.root.join(SNAPSHOTS_DIR)?;
        let mut found: Vec<(Hlc, RemotePath)> = self
            .remote
            .list(&dir, None)
            .await?
            .entries
            .into_iter()
            .filter_map(|entry| {
                let (hlc, owner) = SnapshotHeader::parse_file_name(entry.path.file_name())?;
                device
                    .is_none_or(|d| d == owner)
                    .then_some((hlc, entry.path))
            })
            .collect();
        found.sort();
        Ok(found.into_iter().map(|(_, path)| path).collect())
    }

    /// Schreibt einen Snapshot, wenn genug passiert ist (`sync-strategy.md` §4), und kompaktiert
    /// danach. Nur mit leerer Outbox: Dann ist klar, bis wohin die eigenen Ops reichen.
    async fn maybe_snapshot(&self, activity: u64) -> CoreResult<SyncReport> {
        let now = self.clock.now().as_millis();
        let since = self.number(OPS_SINCE_SNAPSHOT).await? + activity;
        self.repo
            .set_sync_value(OPS_SINCE_SNAPSHOT, since.to_string())
            .await?;
        let last_at = match self.repo.sync_value(LAST_SNAPSHOT_AT).await? {
            Some(at) => at.parse().unwrap_or(now),
            None => {
                self.repo
                    .set_sync_value(LAST_SNAPSHOT_AT, now.to_string())
                    .await?;
                now
            }
        };
        let due = since > self.config.snapshot_after_ops
            || (since > 0 && now - last_at > self.config.snapshot_max_age_ms);
        if !due || self.repo.pending_op_count().await? > 0 {
            return Ok(SyncReport::default());
        }
        let session = self.session()?;
        let own_seq = self.number(PUSHED_UP_TO).await?;
        let mut vector = self.repo.sync_cursors().await?;
        if own_seq > 0 {
            vector.push((session.device, own_seq));
            vector.sort();
        }
        let body = SnapshotBody {
            vector,
            entities: self.repo.entity_states().await?,
        };
        let header = SnapshotHeader {
            device: session.device,
            hlc: Hlc {
                wall_ms: u64::try_from(now).unwrap_or(0),
                counter: 0,
                device: session.device,
            },
        };
        let bytes = seal_snapshot(self.cipher.as_ref(), &session.key, &header, &body)?;
        let path = session
            .root
            .join(&format!("{SNAPSHOTS_DIR}/{}", header.file_name()))?;
        match self.remote.create(&path, bytes).await {
            Ok(_) | Err(CoreError::Conflict(_)) => {}
            Err(e) => return Err(e),
        }
        self.repo
            .set_sync_value(OPS_SINCE_SNAPSHOT, "0".into())
            .await?;
        self.repo
            .set_sync_value(LAST_SNAPSHOT_AT, now.to_string())
            .await?;
        let previous = self.number(SNAPSHOT_SEQ).await?;
        self.repo
            .set_sync_value(PREV_SNAPSHOT_SEQ, previous.to_string())
            .await?;
        self.repo
            .set_sync_value(SNAPSHOT_SEQ, own_seq.to_string())
            .await?;

        let own = self.snapshot_paths(&session, Some(session.device)).await?;
        for old in own.iter().rev().skip(KEEP_OWN_SNAPSHOTS) {
            self.remote.delete(old).await?;
        }
        Ok(SyncReport {
            snapshot_written: true,
            segments_deleted: self.compact(&session).await?,
            ..SyncReport::default()
        })
    }

    /// Löscht eigene Segmente, die der vorletzte eigene Snapshot enthält **und** die alle
    /// aktiven Geräte (laut `devices/*.json`) schon gelesen haben. Das jüngste eigene Segment
    /// bleibt immer liegen: Daran erkennen zurückliegende Geräte die Lücke (→ Snapshot laden).
    /// Liefert die Zahl gelöschter Segmente.
    async fn compact(&self, session: &Session) -> CoreResult<u32> {
        let now = self.clock.now().as_millis();
        let mut bound = self.number(PREV_SNAPSHOT_SEQ).await?;
        if bound == 0 {
            return Ok(0);
        }
        let devices_dir = session.root.join(DEVICES_DIR)?;
        for entry in self.remote.list(&devices_dir, None).await?.entries {
            let bytes = match self.remote.read(&entry.path).await {
                Ok(bytes) => bytes,
                Err(CoreError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            };
            // Unlesbare Gerätedateien gelten als aktiv, ohne etwas gelesen zu haben.
            let (device, last_seen, cursor) = match serde_json::from_slice::<DeviceInfo>(&bytes) {
                Ok(info) => (
                    Some(info.device_id),
                    info.last_seen,
                    info.cursors
                        .get(&session.device.to_string())
                        .copied()
                        .unwrap_or(0),
                ),
                Err(_) => (None, now, 0),
            };
            if device == Some(session.device) || now - last_seen > self.config.inactive_device_ms {
                continue;
            }
            bound = bound.min(cursor);
        }
        if bound == 0 {
            return Ok(0);
        }
        let own_dir = session
            .root
            .join(&format!("{OPS_DIR}/{}", session.device))?;
        let segments: Vec<(u64, RemotePath)> = self
            .remote
            .list(&own_dir, None)
            .await?
            .entries
            .into_iter()
            .filter_map(|entry| {
                let (_, to) = SegmentHeader::parse_file_name(entry.path.file_name())?;
                Some((to, entry.path))
            })
            .collect();
        let newest = segments.iter().map(|(to, _)| *to).max().unwrap_or(0);
        let mut deleted = 0;
        for (to, path) in segments {
            if to <= bound && to < newest {
                self.remote.delete(&path).await?;
                deleted += 1;
            }
        }
        Ok(deleted)
    }

    async fn number(&self, key: &str) -> CoreResult<u64> {
        Ok(self
            .repo
            .sync_value(key)
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }

    /// Anzahl noch nicht hochgeladener Ops (für `SyncStatus::Offline`).
    ///
    /// # Errors
    /// Fehler des Repositories.
    pub async fn pending(&self) -> CoreResult<u64> {
        self.repo.pending_op_count().await
    }

    /// Aktuelle Zeit der Engine-Uhr (für den Scheduler in der Shell).
    #[must_use]
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }
}

/// `ops/<device>/<from>-<to>.seg` → (Gerät, from, to).
fn parse_segment_path(path: &RemotePath, ops_dir: &RemotePath) -> Option<(DeviceId, u64, u64)> {
    let (device, name) = path.relative_to(ops_dir)?.split_once('/')?;
    let device = DeviceId::parse(device).ok()?;
    let (from, to) = SegmentHeader::parse_file_name(name)?;
    Some((device, from, to))
}

fn encode_key(key: &SecretBytes) -> Vec<u8> {
    let mut out = KEY_MAGIC.to_vec();
    out.push(1);
    out.extend_from_slice(key.expose());
    out
}

fn decode_key(bytes: &[u8]) -> CoreResult<SecretBytes> {
    match bytes {
        [m0, m1, m2, m3, 1, key @ ..] if [*m0, *m1, *m2, *m3] == *KEY_MAGIC && !key.is_empty() => {
            Ok(SecretBytes::new(key.to_vec()))
        }
        _ => Err(CoreError::Validation(format!("invalid {KEY_FILE}"))),
    }
}

#[cfg(test)]
mod tests {
    use todo_core::{
        domain::Timestamp,
        ports::FixedClock,
        testing::{InMemoryKeyStore, InMemoryRemoteStore, InMemoryTaskRepository},
    };
    use todo_crypto::XChaCha20Poly1305Cipher;

    use super::*;

    fn engine() -> SyncEngine {
        SyncEngine::new(
            Arc::new(InMemoryTaskRepository::new()),
            Arc::new(InMemoryRemoteStore::new()),
            Arc::new(XChaCha20Poly1305Cipher::new()),
            Arc::new(InMemoryKeyStore::new()),
            Arc::new(FixedClock(Timestamp::EPOCH)),
            SyncConfig::default(),
        )
    }

    /// Tauri-Commands und tokio-Tasks brauchen `Send`-Futures.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn engine_and_futures_are_send_sync() {
        fn assert_send<T: Send>(_: &T) {}
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SyncEngine>();
        let engine = engine();
        assert_send(&engine.sync_once());
        assert_send(&engine.bootstrap());
    }

    #[test]
    fn push_and_pull_need_bootstrap() {
        let engine = engine();
        assert!(matches!(
            futures::executor::block_on(engine.push()),
            Err(CoreError::Internal(_))
        ));
    }

    #[test]
    fn key_file_roundtrip_and_validation() {
        let key = SecretBytes::new(vec![7; 32]);
        assert_eq!(decode_key(&encode_key(&key)).unwrap(), key);
        assert!(decode_key(b"TKEY").is_err());
        assert!(decode_key(b"XXXX\x01abc").is_err());
        assert!(decode_key(b"TKEY\x02abc").is_err());
    }

    #[test]
    fn segment_paths_are_parsed() {
        let ops = RemotePath::new("Tasks/ops").unwrap();
        let device = DeviceId::new();
        let ok = ops.join(&format!("{device}/3-7.seg")).unwrap();
        assert_eq!(parse_segment_path(&ok, &ops), Some((device, 3, 7)));
        for bad in ["x/3-7.seg", "nested/a/b", "3-7.seg"] {
            let p = ops.join(bad).unwrap();
            assert_eq!(parse_segment_path(&p, &ops), None, "{bad}");
        }
    }
}
