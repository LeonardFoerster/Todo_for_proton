//! Treibt den Zustandsautomaten `todo_sync::Scheduler` mit echter Zeit: ein tokio-Task, ein
//! Kanal für Ereignisse, höchstens ein Timer. Sync-Läufe laufen in eigenen Tasks und melden ihr
//! Ergebnis über denselben Kanal zurück – nie auf dem UI-Thread, nie parallel.
//!
//! Ohne Anmeldung gibt es keinen [`SyncRunner`]: Der Scheduler bleibt `Disabled`, lokale
//! Änderungen sammeln sich in der Outbox.

use std::{
    fmt,
    sync::{
        atomic::{AtomicU64, Ordering as AtomicOrdering},
        Arc, Mutex, PoisonError,
    },
    time::Duration,
};

use todo_core::{
    api::dto::iso,
    domain::Timestamp,
    ports::{Clock, TaskRepository},
    CoreResult,
};
use todo_sync::{Scheduler, SchedulerConfig, SchedulerEvent, SyncOutcome, SyncReport, SyncStatus};
use tokio::{
    runtime::Handle,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    time::{sleep_until, Instant},
};

use crate::dto::SyncStatusDto;

/// Ein Sync-Lauf gegen das angemeldete Backend.
#[async_trait::async_trait]
pub trait SyncRunner: Send + Sync {
    async fn sync_once(&self) -> CoreResult<SyncReport>;
}

/// Empfänger für Statuswechsel (Tauri-Event `core://sync-status`).
pub type StatusSink = Box<dyn Fn(&SyncStatusDto) + Send + Sync>;

enum Msg {
    Event(SchedulerEvent),
    LocalChange,
    /// Nur Notiz-Inhalt gespeichert (längerer Debounce, ADR-029).
    ContentChange,
    Runner(Option<Arc<dyn SyncRunner>>),
    Finished(Arc<dyn SyncRunner>, SyncOutcome),
}

/// Griff auf den Treiber; billig klonbar. Alle Methoden kehren sofort zurück.
#[derive(Clone)]
pub struct SyncHandle {
    tx: UnboundedSender<Msg>,
    status: Arc<Mutex<SyncStatusDto>>,
    /// Anzahl gemeldeter lokaler Änderungen (Nachweis der Auslöser, ADR-022).
    local_changes: Arc<AtomicU64>,
}

impl fmt::Debug for SyncHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncHandle").finish_non_exhaustive()
    }
}

impl SyncHandle {
    /// Meldet ein Ereignis an den Scheduler (App gestartet, Fokus, Netz, manueller Sync …).
    pub fn event(&self, event: SchedulerEvent) {
        let _ = self.tx.send(Msg::Event(event));
    }

    /// Nach jedem lokalen Schreibvorgang: Debounce-Push planen.
    pub fn local_change(&self) {
        self.local_changes.fetch_add(1, AtomicOrdering::Relaxed);
        let _ = self.tx.send(Msg::LocalChange);
    }

    /// Nach dem Speichern eines Notiz-Inhalts (Autosave): Debounce 3 s, höchstens 30 s.
    pub fn content_change(&self) {
        self.local_changes.fetch_add(1, AtomicOrdering::Relaxed);
        let _ = self.tx.send(Msg::ContentChange);
    }

    /// Bisher gemeldete lokale Änderungen (Tests der Sync-Auslöser).
    #[must_use]
    pub fn local_changes(&self) -> u64 {
        self.local_changes.load(AtomicOrdering::Relaxed)
    }

    /// Nach Login (`Some`) bzw. Logout (`None`).
    pub fn set_runner(&self, runner: Option<Arc<dyn SyncRunner>>) {
        let _ = self.tx.send(Msg::Runner(runner));
    }

    /// Zuletzt gemeldeter Status.
    pub fn status(&self) -> SyncStatusDto {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Startet den Treiber auf `runtime` (in der App: Tauris tokio-Runtime).
pub fn spawn(
    runtime: &Handle,
    repo: Arc<dyn TaskRepository>,
    clock: Arc<dyn Clock>,
    config: SchedulerConfig,
    sink: StatusSink,
) -> SyncHandle {
    let (tx, rx) = unbounded_channel();
    let status = Arc::new(Mutex::new(SyncStatusDto::from_status(
        &SyncStatus::Disabled,
        0,
        None,
    )));
    let seed = u64::try_from(clock.now().as_millis()).unwrap_or(1);
    let driver = Driver {
        scheduler: Scheduler::new(config, seed),
        repo,
        clock,
        runtime: runtime.clone(),
        runner: None,
        pending_ops: 0,
        last_success: None,
        timer: None,
        tx: tx.clone(),
        status: Arc::clone(&status),
        sink,
    };
    runtime.spawn(driver.run(rx));
    SyncHandle {
        tx,
        status,
        local_changes: Arc::default(),
    }
}

struct Driver {
    scheduler: Scheduler,
    repo: Arc<dyn TaskRepository>,
    clock: Arc<dyn Clock>,
    runtime: Handle,
    runner: Option<Arc<dyn SyncRunner>>,
    pending_ops: u64,
    last_success: Option<Timestamp>,
    timer: Option<Instant>,
    tx: UnboundedSender<Msg>,
    status: Arc<Mutex<SyncStatusDto>>,
    sink: StatusSink,
}

impl Driver {
    async fn run(mut self, mut rx: UnboundedReceiver<Msg>) {
        // Ausstehende Ops vom letzten Lauf der App (für die Statusanzeige).
        self.pending_ops = self.repo.pending_op_count().await.unwrap_or(0);
        self.publish(&SyncStatus::Disabled);
        loop {
            let msg = match self.timer {
                Some(at) => tokio::select! {
                    msg = rx.recv() => msg,
                    () = sleep_until(at) => {
                        self.timer = None;
                        Some(Msg::Event(SchedulerEvent::TimerFired))
                    }
                },
                None => rx.recv().await,
            };
            let Some(msg) = msg else { break };
            let event = match msg {
                Msg::Event(event) => event,
                Msg::LocalChange => {
                    self.pending_ops = self.repo.pending_op_count().await.unwrap_or(0);
                    SchedulerEvent::LocalChange {
                        pending_ops: self.pending_ops,
                    }
                }
                Msg::ContentChange => {
                    self.pending_ops = self.repo.pending_op_count().await.unwrap_or(0);
                    SchedulerEvent::ContentChange {
                        pending_ops: self.pending_ops,
                    }
                }
                Msg::Runner(runner) => {
                    let enabled = runner.is_some();
                    self.runner = runner;
                    SchedulerEvent::SetEnabled(enabled)
                }
                Msg::Finished(runner, outcome) => {
                    // Ergebnis eines inzwischen ersetzten Backends (Logout/Kontowechsel): nur den
                    // Lauf als beendet melden, Fehler nicht dem neuen Backend anrechnen.
                    let current = self
                        .runner
                        .as_ref()
                        .is_some_and(|r| Arc::ptr_eq(r, &runner));
                    self.pending_ops = outcome.pending_ops;
                    match &outcome.result {
                        Ok(_) => tracing::debug!(pending_ops = outcome.pending_ops, "sync done"),
                        Err(err) => tracing::warn!(
                            error = %err,
                            pending_ops = outcome.pending_ops,
                            current,
                            "sync failed"
                        ),
                    }
                    if current {
                        SchedulerEvent::SyncFinished(outcome)
                    } else {
                        SchedulerEvent::SyncFinished(SyncOutcome {
                            result: Ok(SyncReport::default()),
                            pending_ops: outcome.pending_ops,
                        })
                    }
                }
            };
            let now = self.clock.now();
            let actions = self.scheduler.handle(event, now);
            if let SyncStatus::Idle {
                last_success: Some(at),
            } = actions.status
            {
                self.last_success = Some(at);
            }
            self.timer = actions
                .timer_in_ms
                .map(|ms| Instant::now() + Duration::from_millis(ms));
            if actions.sync_now {
                self.start_sync();
            }
            self.publish(&actions.status);
        }
    }

    fn start_sync(&self) {
        let Some(runner) = self.runner.clone() else {
            // Kann nur passieren, wenn der Scheduler aktiv ist, aber kein Backend gesetzt wurde.
            let _ = self.tx.send(Msg::Event(SchedulerEvent::SetEnabled(false)));
            return;
        };
        let tx = self.tx.clone();
        let repo = Arc::clone(&self.repo);
        self.runtime.spawn(async move {
            let result = runner.sync_once().await;
            let pending_ops = repo.pending_op_count().await.unwrap_or(0);
            let _ = tx.send(Msg::Finished(
                runner,
                SyncOutcome {
                    result,
                    pending_ops,
                },
            ));
        });
    }

    fn publish(&self, status: &SyncStatus) {
        let dto = SyncStatusDto::from_status(status, self.pending_ops, self.last_success.map(iso));
        let mut current = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        if *current != dto {
            current.clone_from(&dto);
            drop(current);
            (self.sink)(&dto);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use todo_core::{
        api::{dto::CreateTaskDto, CoreApi},
        testing::InMemoryTaskRepository,
        CoreError,
    };

    use super::*;
    use crate::dto::SyncStateDto;

    #[derive(Default)]
    struct FakeRunner {
        runs: AtomicU32,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl SyncRunner for FakeRunner {
        async fn sync_once(&self) -> CoreResult<SyncReport> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(CoreError::AuthRequired)
            } else {
                Ok(SyncReport::default())
            }
        }
    }

    type Seen = Arc<Mutex<Vec<SyncStateDto>>>;

    /// Läuft mit tokios (in Tests pausierbarer) Zeit, damit Scheduler-Fristen ablaufen.
    struct TokioClock(Instant);

    impl Clock for TokioClock {
        fn now(&self) -> Timestamp {
            let ms = i64::try_from(self.0.elapsed().as_millis()).unwrap();
            Timestamp::from_millis(1_000 + ms)
        }
    }

    fn setup() -> (SyncHandle, Seen, CoreApi) {
        let clock: Arc<dyn Clock> = Arc::new(TokioClock(Instant::now()));
        let api = CoreApi::new(Arc::new(InMemoryTaskRepository::new()), clock.clone());
        let seen: Seen = Arc::default();
        let sink_seen = seen.clone();
        let handle = spawn(
            &Handle::current(),
            api.repository(),
            clock,
            SchedulerConfig {
                jitter: 0.0,
                ..SchedulerConfig::default()
            },
            Box::new(move |s| sink_seen.lock().unwrap().push(s.state)),
        );
        (handle, seen, api)
    }

    /// Lässt den Treiber alle anstehenden Nachrichten abarbeiten.
    async fn settle() {
        for _ in 0..20 {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    #[tokio::test]
    async fn without_runner_stays_disabled_and_counts_pending_ops() {
        let (handle, seen, api) = setup();
        api.ensure_inbox().await.unwrap();
        api.create_task(CreateTaskDto {
            title: "a".into(),
            list_id: None,
            ..CreateTaskDto::default()
        })
        .await
        .unwrap();
        handle.local_change();
        handle.event(SchedulerEvent::AppStarted);
        settle().await;
        let status = handle.status();
        assert_eq!(status.state, SyncStateDto::Disabled);
        assert!(status.pending_ops > 0);
        assert_eq!(seen.lock().unwrap()[0], SyncStateDto::Disabled);
    }

    #[tokio::test]
    async fn login_syncs_immediately_and_becomes_idle() {
        let (handle, seen, _api) = setup();
        let runner = Arc::new(FakeRunner::default());
        handle.set_runner(Some(runner.clone()));
        settle().await;
        assert_eq!(runner.runs.load(Ordering::SeqCst), 1);
        assert_eq!(handle.status().state, SyncStateDto::Idle);
        assert!(handle.status().last_success.is_some());
        let states = seen.lock().unwrap().clone();
        assert!(states.contains(&SyncStateDto::Syncing), "{states:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn local_change_is_debounced() {
        let (handle, _seen, _api) = setup();
        let runner = Arc::new(FakeRunner::default());
        handle.set_runner(Some(runner.clone()));
        settle().await;
        handle.local_change();
        settle().await;
        assert_eq!(runner.runs.load(Ordering::SeqCst), 1, "noch im Debounce");
        tokio::time::advance(Duration::from_millis(1_600)).await;
        settle().await;
        assert_eq!(runner.runs.load(Ordering::SeqCst), 2);
    }

    /// Notiz-Inhalte: 3 s Debounce statt 1,5 s; `Flush` lädt sofort hoch (ADR-029).
    #[tokio::test(start_paused = true)]
    async fn content_changes_wait_longer_and_flush_pushes_now() {
        let (handle, _seen, api) = setup();
        let runner = Arc::new(FakeRunner::default());
        handle.set_runner(Some(runner.clone()));
        settle().await;
        let note = api
            .create_note(None, todo_core::api::note_dto::NoteKindDto::Text)
            .await
            .unwrap();
        api.update_note_body(note.id, "x".into(), None)
            .await
            .unwrap();
        handle.content_change();
        settle().await;
        tokio::time::advance(Duration::from_millis(1_600)).await;
        settle().await;
        assert_eq!(runner.runs.load(Ordering::SeqCst), 1, "1,6 s reichen nicht");
        tokio::time::advance(Duration::from_millis(1_500)).await;
        settle().await;
        assert_eq!(runner.runs.load(Ordering::SeqCst), 2, "nach 3 s");

        handle.content_change();
        settle().await;
        handle.event(SchedulerEvent::Flush);
        settle().await;
        assert_eq!(runner.runs.load(Ordering::SeqCst), 3, "Flush sofort");
    }

    #[tokio::test]
    async fn auth_failure_pauses_until_next_login() {
        let (handle, _seen, _api) = setup();
        let runner = Arc::new(FakeRunner {
            fail: true,
            ..FakeRunner::default()
        });
        handle.set_runner(Some(runner.clone()));
        settle().await;
        assert_eq!(handle.status().state, SyncStateDto::AuthRequired);
        handle.event(SchedulerEvent::ManualSync);
        settle().await;
        assert_eq!(runner.runs.load(Ordering::SeqCst), 1, "pausiert");
        handle.set_runner(None);
        settle().await;
        assert_eq!(handle.status().state, SyncStateDto::Disabled);
    }
}
