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
            CreateTaskDto, PageRequestDto, TaskDto, TaskListDto, TaskQueryDto, TaskSummaryDto,
            ViewDto,
        },
        CoreApi,
    },
    domain::Timestamp,
    ports::{Clock, RemoteStore, TaskRepository},
    testing::{InMemoryKeyStore, InMemoryTaskRepository},
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
                ..SyncConfig::default()
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

    pub fn titles(&self) -> Vec<String> {
        let mut titles: Vec<String> = self.all().into_iter().map(|t| t.title).collect();
        titles.sort();
        titles
    }

    pub fn pending(&self) -> u64 {
        block_on(self.engine.pending()).expect("pending")
    }
}
