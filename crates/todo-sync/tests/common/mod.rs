//! Gemeinsame Hilfen der Integrationstests: simulierte Geräte mit `CoreApi` + `SyncEngine`.

#![allow(dead_code)] // nicht jeder Test nutzt jede Hilfe

use std::{
    path::Path,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
};

use futures::executor::block_on;
use todo_core::{
    api::{
        dto::{
            CreateTaskDto, MoveTargetDto, PageRequestDto, TaskDto, TaskListDto, TaskPatchDto,
            TaskQueryDto, TaskSummaryDto, ViewDto,
        },
        CoreApi,
    },
    domain::Timestamp,
    ports::{
        Clock, RemoteChanges, RemoteCursor, RemoteDir, RemoteEntry, RemoteListing, RemotePath,
        RemoteStore, Revision, TaskRepository,
    },
    testing::{InMemoryKeyStore, InMemoryTaskRepository},
    CoreError, CoreResult,
};
use todo_crypto::XChaCha20Poly1305Cipher;
use todo_storage_sqlite::SqliteTaskRepository;
use todo_sync::{SyncConfig, SyncEngine, SyncReport};

/// Startzeit aller Testuhren (2026-10-17).
pub const T0: i64 = 1_792_000_000_000;

/// Uhr, die bei jedem Aufruf 1 ms weiterläuft und sich verstellen lässt.
#[derive(Debug)]
pub struct TestClock(AtomicI64);

impl TestClock {
    pub fn at(ms: i64) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(ms)))
    }

    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(self.0.fetch_add(1, Ordering::SeqCst))
    }
}

/// Ein simuliertes Gerät.
pub struct Device {
    pub api: CoreApi,
    pub engine: SyncEngine,
    pub clock: Arc<TestClock>,
    pub keys: Arc<InMemoryKeyStore>,
}

impl Device {
    /// Gerät mit In-Memory-Speicher.
    pub fn in_memory(name: &str, remote: Arc<dyn RemoteStore>) -> Self {
        Self::with_repo(name, remote, Arc::new(InMemoryTaskRepository::new()), T0)
    }

    /// Gerät mit SQLite-Datei unter `dir`.
    pub fn sqlite(name: &str, remote: Arc<dyn RemoteStore>, dir: &Path) -> Self {
        let repo =
            SqliteTaskRepository::open(dir.join(format!("{name}.sqlite3"))).expect("open db");
        Self::with_repo(name, remote, Arc::new(repo), T0)
    }

    pub fn with_repo(
        name: &str,
        remote: Arc<dyn RemoteStore>,
        repo: Arc<dyn TaskRepository>,
        start_ms: i64,
    ) -> Self {
        Self::configured(name, remote, repo, start_ms, SyncConfig::default())
    }

    /// Wie [`Self::with_repo`] mit eigener Engine-Konfiguration (Name/Plattform werden gesetzt).
    pub fn configured(
        name: &str,
        remote: Arc<dyn RemoteStore>,
        repo: Arc<dyn TaskRepository>,
        start_ms: i64,
        config: SyncConfig,
    ) -> Self {
        let clock = TestClock::at(start_ms);
        let keys = Arc::new(InMemoryKeyStore::new());
        let api = CoreApi::new(repo, clock.clone());
        let engine = SyncEngine::new(
            api.repository(),
            remote,
            Arc::new(XChaCha20Poly1305Cipher::new()),
            keys.clone(),
            clock.clone(),
            SyncConfig {
                device_name: name.into(),
                platform: "test".into(),
                ..config
            },
        );
        block_on(api.ensure_inbox()).expect("inbox");
        Self {
            api,
            engine,
            clock,
            keys,
        }
    }

    pub fn sync(&self) -> SyncReport {
        block_on(self.engine.sync_once()).expect("sync")
    }

    pub fn add(&self, title: &str) -> TaskDto {
        self.add_to(title, None)
    }

    pub fn add_to(&self, title: &str, list: Option<&str>) -> TaskDto {
        block_on(self.api.create_task(CreateTaskDto {
            title: title.into(),
            list_id: list.map(str::to_owned),
        }))
        .expect("create task")
    }

    pub fn list(&self, name: &str) -> TaskListDto {
        block_on(self.api.create_task_list(name.into())).expect("create list")
    }

    pub fn lists(&self) -> Vec<TaskListDto> {
        block_on(self.api.list_task_lists()).expect("lists")
    }

    pub fn task(&self, id: &str) -> Option<TaskDto> {
        block_on(self.api.get_task(id.into())).ok()
    }

    /// Alle Tasks (Ansicht „Alle", Sortierung nach sort_key).
    pub fn all(&self) -> Vec<TaskSummaryDto> {
        block_on(self.api.list_tasks(
            TaskQueryDto {
                view: ViewDto::All,
                status: None,
                tag: None,
            },
            PageRequestDto {
                cursor: None,
                limit: 500,
            },
        ))
        .expect("list tasks")
        .items
    }

    /// Alle Tasks seitenweise (für mehr als 500).
    pub fn all_pages(&self) -> Vec<Vec<TaskSummaryDto>> {
        let mut pages = Vec::new();
        let mut cursor = None;
        loop {
            let page = block_on(self.api.list_tasks(
                TaskQueryDto {
                    view: ViewDto::All,
                    status: None,
                    tag: None,
                },
                PageRequestDto { cursor, limit: 500 },
            ))
            .expect("list tasks");
            pages.push(page.items);
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => return pages,
            }
        }
    }

    pub fn titles(&self) -> Vec<String> {
        let mut titles: Vec<String> = self.all().into_iter().map(|t| t.title).collect();
        titles.sort();
        titles
    }

    pub fn pending(&self) -> u64 {
        block_on(self.engine.pending()).expect("pending")
    }

    pub fn update(&self, id: &str, patch: TaskPatchDto) -> TaskDto {
        block_on(self.api.update_task(id.into(), patch)).expect("update task")
    }

    pub fn rename(&self, id: &str, title: &str) -> TaskDto {
        self.update(
            id,
            TaskPatchDto {
                title: Some(title.into()),
                ..TaskPatchDto::default()
            },
        )
    }

    pub fn move_after(&self, id: &str, after: Option<&str>, before: Option<&str>) -> TaskDto {
        block_on(self.api.move_task(
            id.into(),
            MoveTargetDto {
                list_id: None,
                after: after.map(str::to_owned),
                before: before.map(str::to_owned),
            },
        ))
        .expect("move task")
    }

    pub fn delete(&self, id: &str) {
        block_on(self.api.delete_task(id.into())).expect("delete task");
    }
}

/// Prüft, dass alle Geräte dieselben Listen und Tasks (in derselben Reihenfolge) sehen.
pub fn assert_converged(devices: &[&Device]) {
    let first = devices[0];
    for other in &devices[1..] {
        assert_eq!(first.all(), other.all(), "Tasks unterscheiden sich");
        let names = |d: &Device| -> Vec<(String, String, bool)> {
            d.lists()
                .into_iter()
                .map(|l| (l.id, l.name, l.is_inbox))
                .collect()
        };
        assert_eq!(names(first), names(other), "Listen unterscheiden sich");
        for task in first.all() {
            let (a, b) = (first.task(&task.id).unwrap(), other.task(&task.id).unwrap());
            assert_eq!(
                (&a.notes, &a.created_at, &a.list_id),
                (&b.notes, &b.created_at, &b.list_id)
            );
        }
    }
}

/// Wrapper um ein Backend, der den `n`-ten Upload eines Segments scheitern lässt – wahlweise
/// vor dem Schreiben (Timeout) oder danach (Antwort verloren).
pub struct Flaky {
    pub inner: Arc<dyn RemoteStore>,
    fail_at: AtomicI64,
    after_write: bool,
}

impl Flaky {
    /// Der `nth` Segment-Upload (1-basiert) scheitert.
    pub fn new(inner: Arc<dyn RemoteStore>, nth: i64, after_write: bool) -> Arc<Self> {
        Arc::new(Self {
            inner,
            fail_at: AtomicI64::new(nth),
            after_write,
        })
    }
}

#[async_trait::async_trait]
impl RemoteStore for Flaky {
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir> {
        self.inner.open_app_root(name).await
    }
    async fn list(
        &self,
        dir: &RemotePath,
        since: Option<&RemoteCursor>,
    ) -> CoreResult<RemoteListing> {
        self.inner.list(dir, since).await
    }
    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>> {
        self.inner.read(path).await
    }
    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry> {
        let is_segment = path.as_str().contains("/ops/");
        if is_segment && self.fail_at.fetch_sub(1, Ordering::SeqCst) == 1 {
            if self.after_write {
                self.inner.create(path, bytes).await?;
            }
            return Err(CoreError::Network("simulierter Abbruch".into()));
        }
        self.inner.create(path, bytes).await
    }
    async fn replace(
        &self,
        path: &RemotePath,
        bytes: Vec<u8>,
        expected: Option<&Revision>,
    ) -> CoreResult<RemoteEntry> {
        self.inner.replace(path, bytes, expected).await
    }
    async fn delete(&self, path: &RemotePath) -> CoreResult<()> {
        self.inner.delete(path).await
    }
    async fn changes_since(
        &self,
        cursor: Option<&RemoteCursor>,
    ) -> CoreResult<Option<RemoteChanges>> {
        self.inner.changes_since(cursor).await
    }
}
