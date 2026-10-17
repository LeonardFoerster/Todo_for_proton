//! `InMemoryRemoteStore`: Sync-Backend im Speicher für Tests – mit Änderungs-Feed,
//! Fehler-Injektion (Timeout, 429, Ablehnung, verlorene Antwort) und künstlicher Latenz.
//!
//! Mehrere simulierte Geräte teilen sich eine Instanz über `Arc`.

use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Mutex, MutexGuard},
    task::{Context, Poll},
};

use crate::{
    domain::Timestamp,
    ports::{
        RemoteChanges, RemoteCursor, RemoteDir, RemoteEntry, RemoteListing, RemotePath,
        RemoteStore, Revision,
    },
    CoreError, CoreResult,
};

/// Art eines Aufrufs (für Fehler-Injektion und Zählung).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteCall {
    OpenAppRoot,
    List,
    Read,
    Create,
    Replace,
    Delete,
    ChangesSince,
}

/// Ein injizierter Fehler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Zeitüberschreitung, nichts passiert (`CoreError::Network`).
    Timeout,
    /// HTTP 429 (`CoreError::RateLimited`).
    RateLimited { retry_after_s: u32 },
    /// Endgültige Ablehnung (`CoreError::RemoteRejected`).
    Rejected,
    /// Die Operation wird **ausgeführt**, die Antwort geht aber verloren
    /// (`CoreError::Network`) – z. B. Absturz nach dem Upload.
    LostResponse,
}

impl Fault {
    fn error(self, call: RemoteCall) -> CoreError {
        match self {
            Self::Timeout => CoreError::Network(format!("{call:?}: injected timeout")),
            Self::RateLimited { retry_after_s } => CoreError::RateLimited { retry_after_s },
            Self::Rejected => CoreError::RemoteRejected(format!("{call:?}: injected rejection")),
            Self::LostResponse => CoreError::Network(format!("{call:?}: injected lost response")),
        }
    }
}

#[derive(Debug, Clone)]
struct FaultRule {
    call: Option<RemoteCall>,
    path_prefix: Option<String>,
    fault: Fault,
    remaining: u32,
}

#[derive(Debug, Clone)]
struct File {
    bytes: Vec<u8>,
    revision: u64,
    modified: i64,
}

#[derive(Debug, Clone)]
enum Event {
    Changed(RemotePath),
    Deleted(RemotePath),
}

#[derive(Debug, Default)]
struct Inner {
    files: BTreeMap<RemotePath, File>,
    next_revision: u64,
    /// Änderungs-Feed; Index + `feed_offset` = Cursor.
    events: Vec<Event>,
    feed_offset: u64,
    feed_disabled: bool,
    faults: Vec<FaultRule>,
    calls: BTreeMap<String, usize>,
    latency_polls: u32,
}

/// Siehe Moduldoku.
#[derive(Debug, Default)]
pub struct InMemoryRemoteStore {
    inner: Mutex<Inner>,
}

impl InMemoryRemoteStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Die nächsten `times` passenden Aufrufe scheitern mit `fault` (`call = None` = jeder).
    pub fn inject(&self, call: Option<RemoteCall>, fault: Fault, times: u32) {
        self.inject_rule(call, None, fault, times);
    }

    /// Wie [`Self::inject`], aber nur für Pfade mit diesem Präfix.
    pub fn inject_for_path(&self, call: RemoteCall, prefix: &str, fault: Fault, times: u32) {
        self.inject_rule(Some(call), Some(prefix.to_owned()), fault, times);
    }

    fn inject_rule(&self, call: Option<RemoteCall>, prefix: Option<String>, fault: Fault, n: u32) {
        self.inner().faults.push(FaultRule {
            call,
            path_prefix: prefix,
            fault,
            remaining: n,
        });
    }

    pub fn clear_faults(&self) {
        self.inner().faults.clear();
    }

    /// Jeder Aufruf gibt vorher `polls`-mal die Kontrolle ab (künstliche Latenz ohne Uhr).
    pub fn set_latency(&self, polls: u32) {
        self.inner().latency_polls = polls;
    }

    /// Simuliert ein Backend ohne Änderungs-Feed (`changes_since` → `None`).
    pub fn disable_feed(&self) {
        self.inner().feed_disabled = true;
    }

    /// Verwirft den bisherigen Feed: alte Cursor bekommen `refresh = true`.
    pub fn truncate_feed(&self) {
        let mut inner = self.inner();
        inner.feed_offset += inner.events.len() as u64;
        inner.events.clear();
    }

    /// Alle Pfade (sortiert).
    #[must_use]
    pub fn paths(&self) -> Vec<RemotePath> {
        self.inner().files.keys().cloned().collect()
    }

    /// Wie oft `call` aufgerufen wurde.
    #[must_use]
    pub fn calls(&self, call: RemoteCall) -> usize {
        self.inner()
            .calls
            .get(&format!("{call:?}"))
            .copied()
            .unwrap_or(0)
    }

    /// Manipuliert den Inhalt einer Datei direkt (z. B. beschädigte Segmente).
    ///
    /// # Panics
    /// Wenn die Datei nicht existiert.
    pub fn tamper(&self, path: &RemotePath, f: impl FnOnce(&mut Vec<u8>)) {
        let mut inner = self.inner();
        f(&mut inner.files.get_mut(path).expect("file exists").bytes);
    }

    /// Zählt den Aufruf, wartet die Latenz ab und liefert einen ggf. injizierten Fehler.
    async fn enter(&self, call: RemoteCall, path: Option<&RemotePath>) -> Option<Fault> {
        let polls = {
            let mut inner = self.inner();
            *inner.calls.entry(format!("{call:?}")).or_default() += 1;
            inner.latency_polls
        };
        for _ in 0..polls {
            YieldOnce(false).await;
        }
        let mut inner = self.inner();
        let rule = inner.faults.iter_mut().find(|rule| {
            rule.remaining > 0
                && rule.call.is_none_or(|c| c == call)
                && rule
                    .path_prefix
                    .as_deref()
                    .is_none_or(|prefix| path.is_some_and(|p| p.as_str().starts_with(prefix)))
        })?;
        rule.remaining -= 1;
        Some(rule.fault)
    }

    fn entry(path: &RemotePath, file: &File) -> RemoteEntry {
        RemoteEntry {
            path: path.clone(),
            size: file.bytes.len() as u64,
            revision: Revision(file.revision.to_string()),
            modified: Timestamp::from_millis(file.modified),
        }
    }

    fn write(inner: &mut Inner, path: &RemotePath, bytes: Vec<u8>) -> RemoteEntry {
        inner.next_revision += 1;
        let file = File {
            bytes,
            revision: inner.next_revision,
            modified: i64::try_from(inner.next_revision).unwrap_or(i64::MAX),
        };
        let entry = Self::entry(path, &file);
        inner.files.insert(path.clone(), file);
        inner.events.push(Event::Changed(path.clone()));
        entry
    }
}

/// Fehler vor der Ausführung; bei [`Fault::LostResponse`] erst danach.
fn fail_early(fault: Option<Fault>, call: RemoteCall) -> CoreResult<bool> {
    match fault {
        Some(Fault::LostResponse) => Ok(true),
        Some(fault) => Err(fault.error(call)),
        None => Ok(false),
    }
}

fn finish<T>(result: T, lost: bool, call: RemoteCall) -> CoreResult<T> {
    if lost {
        Err(Fault::LostResponse.error(call))
    } else {
        Ok(result)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl RemoteStore for InMemoryRemoteStore {
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir> {
        let path = RemotePath::new(name)?;
        let lost = fail_early(
            self.enter(RemoteCall::OpenAppRoot, Some(&path)).await,
            RemoteCall::OpenAppRoot,
        )?;
        finish(RemoteDir { path }, lost, RemoteCall::OpenAppRoot)
    }

    async fn list(
        &self,
        dir: &RemotePath,
        _since: Option<&RemoteCursor>,
    ) -> CoreResult<RemoteListing> {
        let call = RemoteCall::List;
        let lost = fail_early(self.enter(call, Some(dir)).await, call)?;
        let inner = self.inner();
        let entries = inner
            .files
            .iter()
            .filter(|(path, _)| path.starts_with(dir) && *path != dir)
            .map(|(path, file)| Self::entry(path, file))
            .collect();
        let cursor = RemoteCursor((inner.feed_offset + inner.events.len() as u64).to_string());
        finish(RemoteListing { entries, cursor }, lost, call)
    }

    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>> {
        let call = RemoteCall::Read;
        let lost = fail_early(self.enter(call, Some(path)).await, call)?;
        let bytes = self
            .inner()
            .files
            .get(path)
            .map(|file| file.bytes.clone())
            .ok_or_else(|| CoreError::NotFound(format!("remote file {path}")))?;
        finish(bytes, lost, call)
    }

    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry> {
        let call = RemoteCall::Create;
        let lost = fail_early(self.enter(call, Some(path)).await, call)?;
        let mut inner = self.inner();
        if inner.files.contains_key(path) {
            return Err(CoreError::Conflict(format!("remote file {path} exists")));
        }
        let entry = Self::write(&mut inner, path, bytes);
        finish(entry, lost, call)
    }

    async fn replace(
        &self,
        path: &RemotePath,
        bytes: Vec<u8>,
        expected: Option<&Revision>,
    ) -> CoreResult<RemoteEntry> {
        let call = RemoteCall::Replace;
        let lost = fail_early(self.enter(call, Some(path)).await, call)?;
        let mut inner = self.inner();
        if let Some(expected) = expected {
            let current = inner.files.get(path).map(|f| f.revision.to_string());
            if current.as_deref() != Some(expected.0.as_str()) {
                return Err(CoreError::Conflict(format!(
                    "remote file {path}: revision changed"
                )));
            }
        }
        let entry = Self::write(&mut inner, path, bytes);
        finish(entry, lost, call)
    }

    async fn delete(&self, path: &RemotePath) -> CoreResult<()> {
        let call = RemoteCall::Delete;
        let lost = fail_early(self.enter(call, Some(path)).await, call)?;
        let mut inner = self.inner();
        if inner.files.remove(path).is_some() {
            inner.events.push(Event::Deleted(path.clone()));
        }
        finish((), lost, call)
    }

    async fn changes_since(
        &self,
        cursor: Option<&RemoteCursor>,
    ) -> CoreResult<Option<RemoteChanges>> {
        let call = RemoteCall::ChangesSince;
        let lost = fail_early(self.enter(call, None).await, call)?;
        let inner = self.inner();
        if inner.feed_disabled {
            return finish(None, lost, call);
        }
        let end = inner.feed_offset + inner.events.len() as u64;
        let next = RemoteCursor(end.to_string());
        let Some(cursor) = cursor else {
            let start = RemoteChanges {
                changed: Vec::new(),
                deleted: Vec::new(),
                cursor: next,
                refresh: false,
            };
            return finish(Some(start), lost, call);
        };
        let from: u64 = cursor
            .0
            .parse()
            .map_err(|_| CoreError::Validation(format!("invalid feed cursor {:?}", cursor.0)))?;
        if from < inner.feed_offset || from > end {
            let refresh = RemoteChanges {
                changed: Vec::new(),
                deleted: Vec::new(),
                cursor: next,
                refresh: true,
            };
            return finish(Some(refresh), lost, call);
        }
        // Pro Pfad zählt das letzte Ereignis.
        let skip = usize::try_from(from - inner.feed_offset).unwrap_or(usize::MAX);
        let mut last: BTreeMap<RemotePath, bool> = BTreeMap::new();
        for event in inner.events.iter().skip(skip) {
            match event {
                Event::Changed(path) => last.insert(path.clone(), true),
                Event::Deleted(path) => last.insert(path.clone(), false),
            };
        }
        let mut changes = RemoteChanges {
            changed: Vec::new(),
            deleted: Vec::new(),
            cursor: next,
            refresh: false,
        };
        for (path, exists) in last {
            match inner.files.get(&path) {
                Some(file) if exists => changes.changed.push(Self::entry(&path, file)),
                _ => changes.deleted.push(path),
            }
        }
        finish(Some(changes), lost, call)
    }
}

/// Gibt einmal die Kontrolle an den Executor ab.
struct YieldOnce(bool);

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;

    mod contract {
        crate::remote_store_contract_tests!(super::InMemoryRemoteStore::new());
    }

    fn path(s: &str) -> RemotePath {
        RemotePath::new(s).unwrap()
    }

    #[test]
    fn injected_faults_map_to_errors_and_run_out() {
        let store = InMemoryRemoteStore::new();
        store.inject(Some(RemoteCall::Read), Fault::Timeout, 1);
        store.inject(None, Fault::RateLimited { retry_after_s: 7 }, 1);
        assert!(matches!(
            block_on(store.read(&path("x"))),
            Err(CoreError::Network(_))
        ));
        assert!(matches!(
            block_on(store.create(&path("x"), vec![1])),
            Err(CoreError::RateLimited { retry_after_s: 7 })
        ));
        block_on(store.create(&path("x"), vec![1])).unwrap();
        assert_eq!(block_on(store.read(&path("x"))).unwrap(), [1]);
        store.inject(Some(RemoteCall::Delete), Fault::Rejected, 1);
        assert!(matches!(
            block_on(store.delete(&path("x"))),
            Err(CoreError::RemoteRejected(_))
        ));
        assert_eq!(store.calls(RemoteCall::Create), 2);
    }

    #[test]
    fn lost_response_still_performs_the_write() {
        let store = InMemoryRemoteStore::new();
        store.inject_for_path(RemoteCall::Create, "ops/", Fault::LostResponse, 1);
        block_on(store.create(&path("other"), vec![0])).unwrap();
        let err = block_on(store.create(&path("ops/a"), vec![1])).unwrap_err();
        assert!(err.is_retryable());
        assert_eq!(block_on(store.read(&path("ops/a"))).unwrap(), [1]);
    }

    #[test]
    fn latency_yields_without_blocking() {
        let store = InMemoryRemoteStore::new();
        store.set_latency(3);
        block_on(store.create(&path("x"), vec![1])).unwrap();
        assert_eq!(block_on(store.read(&path("x"))).unwrap(), [1]);
    }

    #[test]
    fn feed_reports_changes_and_refresh() {
        let store = InMemoryRemoteStore::new();
        let start = block_on(store.changes_since(None)).unwrap().unwrap();
        block_on(store.create(&path("a"), vec![1])).unwrap();
        block_on(store.create(&path("b"), vec![2])).unwrap();
        block_on(store.delete(&path("a"))).unwrap();
        let changes = block_on(store.changes_since(Some(&start.cursor)))
            .unwrap()
            .unwrap();
        assert_eq!(
            changes
                .changed
                .iter()
                .map(|e| e.path.clone())
                .collect::<Vec<_>>(),
            [path("b")]
        );
        assert_eq!(changes.deleted, [path("a")]);
        store.truncate_feed();
        let stale = block_on(store.changes_since(Some(&start.cursor)))
            .unwrap()
            .unwrap();
        assert!(stale.refresh);
        store.disable_feed();
        assert_eq!(block_on(store.changes_since(None)).unwrap(), None);
    }

    #[test]
    fn tamper_changes_content() {
        let store = InMemoryRemoteStore::new();
        block_on(store.create(&path("x"), vec![1, 2])).unwrap();
        store.tamper(&path("x"), |bytes| bytes[0] ^= 0xff);
        assert_eq!(block_on(store.read(&path("x"))).unwrap(), [0xfe, 2]);
        assert_eq!(store.paths(), [path("x")]);
    }
}
