//! Wann gesynct wird – als reiner Zustandsautomat ohne Laufzeit, Timer oder Uhr
//! (`sync-strategy.md` §4–§6).
//!
//! Die Shell meldet Ereignisse ([`SchedulerEvent`]) samt aktueller Zeit und setzt danach genau
//! das um, was [`Actions`] sagt: ggf. sofort `SyncEngine::sync_once` starten (und das Ergebnis
//! als [`SchedulerEvent::SyncFinished`] zurückmelden), einen einzigen Timer auf
//! `timer_in_ms` stellen (vorherigen ersetzen; `None` = keinen) und den Status anzeigen.
//!
//! Regeln: Debounce nach lokalen Änderungen mit Höchstwartezeit (eine Serie wird spätestens
//! `max_wait_ms` nach ihrer ersten Änderung hochgeladen, ADR-022); exponentieller Backoff mit Jitter bei Fehlern;
//! `Retry-After` bei Drosselung; im Hintergrund oder offline **kein Timer** (keine
//! Leerlauf-CPU); periodischer Pull nur bei sichtbarem Fenster und mit wachsendem Abstand,
//! solange nichts passiert.

use todo_core::{domain::Timestamp, CoreError};

use crate::SyncReport;

/// Status für die UI (dezentes Icon in der Sidebar-Fußzeile).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    /// Nicht angemeldet / Sync aus.
    Disabled,
    /// Alles synchron; letzter erfolgreicher Lauf. `None` vor dem ersten Lauf.
    Idle {
        last_success: Option<Timestamp>,
    },
    Syncing,
    /// Kein Netz; so viele lokale Ops warten.
    Offline {
        pending_ops: u64,
    },
    Paused(SyncPauseReason),
    /// Letzter Lauf scheiterte endgültig (wird trotzdem mit Backoff erneut versucht).
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPauseReason {
    AuthRequired,
    RateLimited,
    NewerFormat,
}

/// Ergebnis eines Laufs, wie die Shell es zurückmeldet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    pub result: Result<SyncReport, CoreError>,
    /// Ops in der Outbox nach dem Lauf.
    pub pending_ops: u64,
}

/// Eingänge des Automaten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerEvent {
    /// Sync an (nach Login) oder aus.
    SetEnabled(bool),
    /// App gestartet und erste Liste gerendert.
    AppStarted,
    /// Lokale Änderung committet.
    LocalChange {
        pending_ops: u64,
    },
    /// Nur Notiz-Inhalt gespeichert (Autosave): längerer Debounce und längere Höchstwartezeit,
    /// sonst lädt durchgehendes Tippen alle paar Sekunden den ganzen Text hoch (ADR-029).
    ContentChange {
        pending_ops: u64,
    },
    /// Jetzt hochladen, falls etwas wartet (Notiz- oder Moduswechsel).
    Flush,
    AppFocused,
    /// Fenster versteckt/minimiert oder App wird geschlossen.
    AppHidden,
    NetworkUp,
    NetworkDown,
    /// Der zuletzt gestellte Timer ist abgelaufen.
    TimerFired,
    /// Nutzer will jetzt syncen.
    ManualSync,
    SyncFinished(SyncOutcome),
}

/// Was die Shell jetzt tun soll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actions {
    /// Sofort einen Sync-Lauf starten.
    pub sync_now: bool,
    /// Einzigen Timer auf diese Dauer stellen (vorherigen ersetzen); `None` = keinen Timer.
    pub timer_in_ms: Option<u64>,
    pub status: SyncStatus,
}

/// Zeiten des Automaten.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SchedulerConfig {
    /// Ruhe nach einer lokalen Änderung, bevor hochgeladen wird.
    pub debounce_ms: u64,
    /// Spätestens so lange nach der ersten Änderung einer Serie wird hochgeladen, auch wenn
    /// weiter geändert wird (sonst verschöbe jede Änderung den Upload erneut).
    pub max_wait_ms: u64,
    pub backoff_min_ms: u64,
    pub backoff_max_ms: u64,
    /// Anteil zufälliger Streuung des Backoffs (0.2 = ±20 %).
    pub jitter: f64,
    /// Wie `debounce_ms`/`max_wait_ms`, wenn die Serie nur aus Notiz-Inhalten besteht.
    pub content_debounce_ms: u64,
    pub content_max_wait_ms: u64,
    /// Periodischer Pull bei sichtbarem Fenster: Startabstand; `None` = nie periodisch.
    pub poll_min_ms: Option<u64>,
    /// Größter Abstand des periodischen Pulls (wächst, solange nichts passiert).
    pub poll_max_ms: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 1_500,
            max_wait_ms: 5_000,
            content_debounce_ms: 3_000,
            content_max_wait_ms: 30_000,
            backoff_min_ms: 5_000,
            backoff_max_ms: 10 * 60 * 1000,
            jitter: 0.2,
            poll_min_ms: Some(60_000),
            poll_max_ms: 10 * 60 * 1000,
        }
    }
}

/// Siehe Moduldoku.
#[derive(Debug, Clone)]
pub struct Scheduler {
    config: SchedulerConfig,
    enabled: bool,
    visible: bool,
    online: bool,
    running: bool,
    /// Während eines Laufs kam ein Auslöser – danach gleich nochmal.
    rerun: bool,
    /// Wann der nächste Lauf fällig ist (Debounce, Retry oder periodischer Pull).
    deadline: Option<Timestamp>,
    /// Erste noch nicht hochgeladene lokale Änderung der laufenden Serie (für `max_wait_ms`).
    burst_since: Option<Timestamp>,
    /// Die Serie besteht nur aus Notiz-Inhalten (`ContentChange`).
    burst_content_only: bool,
    failures: u32,
    /// Abstand bis zum nächsten periodischen Pull.
    poll_wait_ms: u64,
    /// Abstand für den Pull danach (wächst, solange nichts passiert).
    poll_next_ms: u64,
    rate_limited_until: Option<Timestamp>,
    newer_format: bool,
    /// Backend verlangt neuen Login: pausiert bis `SetEnabled(true)` (nach dem Login).
    auth_required: bool,
    last_success: Option<Timestamp>,
    pending_ops: u64,
    status: SyncStatus,
    rng: u64,
}

impl Scheduler {
    /// `seed` für den Jitter (z. B. aus der Geräte-ID); gleiche Seeds → gleiche Abfolge.
    #[must_use]
    pub fn new(config: SchedulerConfig, seed: u64) -> Self {
        let poll_min = config.poll_min_ms.unwrap_or(0);
        Self {
            poll_wait_ms: poll_min,
            poll_next_ms: poll_min,
            config,
            enabled: false,
            visible: true,
            online: true,
            running: false,
            rerun: false,
            deadline: None,
            burst_since: None,
            burst_content_only: true,
            failures: 0,
            rate_limited_until: None,
            newer_format: false,
            auth_required: false,
            last_success: None,
            pending_ops: 0,
            status: SyncStatus::Disabled,
            rng: seed | 1,
        }
    }

    #[must_use]
    pub fn status(&self) -> &SyncStatus {
        &self.status
    }

    /// Verarbeitet ein Ereignis zum Zeitpunkt `now`.
    pub fn handle(&mut self, event: SchedulerEvent, now: Timestamp) -> Actions {
        let mut sync_now = false;
        match event {
            SchedulerEvent::SetEnabled(on) => {
                self.enabled = on;
                self.auth_required = false;
                self.deadline = None;
                self.burst_since = None;
                self.burst_content_only = true;
                self.failures = 0;
                sync_now = on;
            }
            SchedulerEvent::AppStarted | SchedulerEvent::ManualSync => sync_now = true,
            SchedulerEvent::LocalChange { pending_ops } => {
                self.pending_ops = pending_ops;
                self.burst_content_only = false;
                if self.running {
                    self.rerun = true;
                } else if self.failures == 0 {
                    let since = *self.burst_since.get_or_insert(now);
                    self.deadline = Some(self.burst_due(now, since));
                }
            }
            SchedulerEvent::ContentChange { pending_ops } => {
                self.pending_ops = pending_ops;
                // Während eines Laufs kein sofortiger Folgelauf: Der Debounce danach genügt.
                let since = *self.burst_since.get_or_insert(now);
                if !self.running && self.failures == 0 {
                    self.deadline = Some(self.burst_due(now, since));
                }
            }
            SchedulerEvent::Flush => sync_now = self.pending_ops > 0,
            SchedulerEvent::AppFocused => {
                self.visible = true;
                self.poll_next_ms = self.config.poll_min_ms.unwrap_or(0);
                sync_now = true;
            }
            SchedulerEvent::AppHidden => {
                self.visible = false;
                // Vor dem Verschwinden noch hochladen; danach keine Timer mehr.
                sync_now = self.pending_ops > 0;
            }
            SchedulerEvent::NetworkUp => {
                self.online = true;
                self.failures = 0;
                sync_now = true;
            }
            SchedulerEvent::NetworkDown => {
                self.online = false;
                self.deadline = None;
                self.burst_since = None;
                self.burst_content_only = true;
            }
            SchedulerEvent::TimerFired => {
                sync_now = self.deadline.is_some_and(|due| now >= due);
            }
            SchedulerEvent::SyncFinished(outcome) => sync_now = self.finished(outcome, now),
        }
        self.decide(sync_now, now)
    }

    /// Wertet einen Lauf aus; `true`, wenn sofort ein weiterer folgen soll.
    fn finished(&mut self, outcome: SyncOutcome, now: Timestamp) -> bool {
        self.running = false;
        self.pending_ops = outcome.pending_ops;
        let rerun = std::mem::take(&mut self.rerun);
        match outcome.result {
            Ok(report) => {
                self.failures = 0;
                self.rate_limited_until = None;
                self.last_success = Some(now);
                self.newer_format = report.newer_format;
                if let Some(min) = self.config.poll_min_ms {
                    if report.pushed > 0 || report.received > 0 {
                        self.poll_next_ms = min;
                    }
                    self.poll_wait_ms = self.poll_next_ms;
                    self.poll_next_ms = (self.poll_next_ms * 2).min(self.config.poll_max_ms);
                }
                self.deadline = (self.pending_ops > 0).then(|| match self.burst_since {
                    Some(since) => self.burst_due(now, since),
                    None => at(now, self.config.debounce_ms),
                });
                self.status = SyncStatus::Idle {
                    last_success: self.last_success,
                };
                rerun
            }
            // Ein anderer Lauf ist noch aktiv: nichts ändern.
            Err(CoreError::Conflict(_)) => false,
            // Ohne Login hilft kein Wiederholen: pausieren, bis die Shell nach dem Login
            // `SetEnabled(true)` meldet.
            Err(CoreError::AuthRequired | CoreError::Auth(_)) => {
                self.auth_required = true;
                self.deadline = None;
                false
            }
            Err(CoreError::RateLimited { retry_after_s }) => {
                self.failures += 1;
                let wait = (u64::from(retry_after_s) * 1000).max(self.backoff_ms());
                self.rate_limited_until = Some(at(now, wait));
                self.deadline = Some(at(now, wait));
                false
            }
            Err(err) => {
                self.failures += 1;
                self.deadline = Some(at(now, self.backoff_ms()));
                self.status = if err.is_retryable() {
                    SyncStatus::Offline {
                        pending_ops: self.pending_ops,
                    }
                } else {
                    SyncStatus::Error {
                        message: err.to_string(),
                    }
                };
                false
            }
        }
    }

    /// Fälligkeit einer Serie lokaler Änderungen, die bei `since` begann.
    fn burst_due(&self, now: Timestamp, since: Timestamp) -> Timestamp {
        let (debounce, max_wait) = if self.burst_content_only {
            (
                self.config.content_debounce_ms,
                self.config.content_max_wait_ms,
            )
        } else {
            (self.config.debounce_ms, self.config.max_wait_ms)
        };
        at(now, debounce).min(at(since, max_wait))
    }

    /// Setzt die gewünschte Aktion unter den aktuellen Bedingungen um.
    fn decide(&mut self, mut sync_now: bool, now: Timestamp) -> Actions {
        let blocked_until = self.rate_limited_until.filter(|until| now < *until);
        if !self.enabled
            || !self.online
            || self.newer_format
            || self.auth_required
            || blocked_until.is_some()
        {
            sync_now = false;
        }
        if self.running {
            // Läuft schon: Auslöser merken statt parallel zu starten.
            self.rerun |= sync_now;
            sync_now = false;
        }
        if sync_now {
            self.running = true;
            self.deadline = None;
            self.burst_since = None;
            self.burst_content_only = true;
            self.status = SyncStatus::Syncing;
        } else if !self.running && self.deadline.is_none() && self.wants_poll() {
            self.deadline = Some(at(now, self.poll_wait_ms));
        }
        if !self.running {
            self.status = self.idle_status(blocked_until.is_some());
        }
        let timer_in_ms = self
            .deadline
            .filter(|_| {
                self.enabled
                    && self.online
                    && self.visible
                    && !self.newer_format
                    && !self.auth_required
            })
            .map(|due| ms_until(now, due));
        Actions {
            sync_now,
            timer_in_ms,
            status: self.status.clone(),
        }
    }

    fn wants_poll(&self) -> bool {
        self.enabled
            && self.online
            && self.visible
            && self.failures == 0
            && self.config.poll_min_ms.is_some()
            && self.last_success.is_some()
    }

    fn idle_status(&self, rate_limited: bool) -> SyncStatus {
        if !self.enabled {
            SyncStatus::Disabled
        } else if self.auth_required {
            SyncStatus::Paused(SyncPauseReason::AuthRequired)
        } else if self.newer_format {
            SyncStatus::Paused(SyncPauseReason::NewerFormat)
        } else if rate_limited {
            SyncStatus::Paused(SyncPauseReason::RateLimited)
        } else if !self.online {
            SyncStatus::Offline {
                pending_ops: self.pending_ops,
            }
        } else if self.failures > 0 {
            // Fehlerstatus aus `finished` beibehalten (Offline oder Error).
            match &self.status {
                SyncStatus::Syncing | SyncStatus::Disabled | SyncStatus::Idle { .. } => {
                    SyncStatus::Offline {
                        pending_ops: self.pending_ops,
                    }
                }
                other => other.clone(),
            }
        } else {
            SyncStatus::Idle {
                last_success: self.last_success,
            }
        }
    }

    /// Exponentieller Backoff (`min · 2^(n-1)`, gedeckelt) mit ±`jitter`.
    fn backoff_ms(&mut self) -> u64 {
        let exp = self.failures.saturating_sub(1).min(32);
        let base = self
            .config
            .backoff_min_ms
            .saturating_mul(1u64 << exp)
            .min(self.config.backoff_max_ms);
        let factor = 1.0 + self.config.jitter * (2.0 * self.next_unit() - 1.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )] // Millisekunden weit unter 2^52, Faktor positiv
        let jittered = (base as f64 * factor) as u64;
        jittered.min(self.config.backoff_max_ms)
    }

    /// Gleichverteilt in [0, 1) (xorshift64*; deterministisch für Tests).
    fn next_unit(&mut self) -> f64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let x = self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d);
        #[allow(clippy::cast_precision_loss)] // 53 Bit genügen
        let unit = (x >> 11) as f64 / (1u64 << 53) as f64;
        unit
    }
}

fn at(now: Timestamp, ms: u64) -> Timestamp {
    now.plus_millis(i64::try_from(ms).unwrap_or(i64::MAX))
}

fn ms_until(now: Timestamp, due: Timestamp) -> u64 {
    u64::try_from(due.as_millis().saturating_sub(now.as_millis())).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use todo_core::ports::{Clock, SteppingClock};

    use super::*;

    const T0: i64 = 1_792_000_000_000;

    fn ts(ms: i64) -> Timestamp {
        Timestamp::from_millis(T0 + ms)
    }

    /// Ohne Jitter, damit Zeiten exakt prüfbar sind.
    fn scheduler() -> Scheduler {
        Scheduler::new(
            SchedulerConfig {
                jitter: 0.0,
                ..SchedulerConfig::default()
            },
            42,
        )
    }

    fn ok(pending_ops: u64, report: SyncReport) -> SchedulerEvent {
        SchedulerEvent::SyncFinished(SyncOutcome {
            result: Ok(report),
            pending_ops,
        })
    }

    fn quiet() -> SyncReport {
        SyncReport::default()
    }

    fn busy() -> SyncReport {
        SyncReport {
            received: 3,
            ..SyncReport::default()
        }
    }

    fn failed(err: CoreError, pending_ops: u64) -> SchedulerEvent {
        SchedulerEvent::SyncFinished(SyncOutcome {
            result: Err(err),
            pending_ops,
        })
    }

    /// Aktiviert und schließt den Start-Sync erfolgreich ab.
    fn running(s: &mut Scheduler) -> Actions {
        assert!(s.handle(SchedulerEvent::SetEnabled(true), ts(0)).sync_now);
        s.handle(ok(0, quiet()), ts(100))
    }

    #[test]
    fn disabled_does_nothing() {
        let mut s = scheduler();
        for event in [
            SchedulerEvent::AppStarted,
            SchedulerEvent::LocalChange { pending_ops: 1 },
            SchedulerEvent::TimerFired,
        ] {
            let actions = s.handle(event, ts(0));
            assert!(!actions.sync_now);
            assert_eq!(actions.timer_in_ms, None);
            assert_eq!(actions.status, SyncStatus::Disabled);
        }
    }

    #[test]
    fn start_syncs_and_reports_status() {
        let mut s = scheduler();
        let start = s.handle(SchedulerEvent::SetEnabled(true), ts(0));
        assert!(start.sync_now);
        assert_eq!(start.status, SyncStatus::Syncing);
        let done = s.handle(ok(0, quiet()), ts(500));
        assert!(!done.sync_now);
        assert_eq!(
            done.status,
            SyncStatus::Idle {
                last_success: Some(ts(500))
            }
        );
        assert_eq!(
            done.timer_in_ms,
            Some(60_000),
            "periodischer Pull, sichtbar"
        );
    }

    #[test]
    fn local_changes_are_debounced() {
        let mut s = scheduler();
        running(&mut s);
        let first = s.handle(SchedulerEvent::LocalChange { pending_ops: 1 }, ts(1_000));
        assert!(!first.sync_now);
        assert_eq!(first.timer_in_ms, Some(1_500));
        let second = s.handle(SchedulerEvent::LocalChange { pending_ops: 2 }, ts(2_000));
        assert_eq!(second.timer_in_ms, Some(1_500), "Debounce neu gestartet");
        assert!(
            !s.handle(SchedulerEvent::TimerFired, ts(3_000)).sync_now,
            "zu früh"
        );
        assert!(s.handle(SchedulerEvent::TimerFired, ts(3_500)).sync_now);
    }

    /// ADR-022: Änderungen im Sekundentakt verschieben den Upload nicht endlos – spätestens 5 s
    /// nach der ersten Änderung der Serie, danach beginnt eine neue Serie.
    #[test]
    fn a_burst_of_changes_uploads_within_max_wait() {
        let mut s = scheduler();
        running(&mut s);
        let mut started = Vec::new();
        for second in 0..12 {
            let now = ts(10_000 + second * 1_000);
            if s.handle(SchedulerEvent::TimerFired, now).sync_now {
                started.push(second);
                s.handle(ok(0, SyncReport::default()), now);
            }
            let actions = s.handle(SchedulerEvent::LocalChange { pending_ops: 1 }, now);
            let due = actions.timer_in_ms.expect("Timer");
            assert!(due <= 1_500, "nie länger als der Debounce: {due}");
        }
        assert_eq!(
            started,
            [5, 10],
            "je Serie ein Lauf, spätestens 5 s nach ihrem Beginn"
        );
    }

    /// ADR-029: Notiz-Inhalte warten 3 s nach der letzten Eingabe, höchstens 30 s nach der ersten.
    #[test]
    fn content_changes_use_the_longer_debounce_and_max_wait() {
        let mut s = scheduler();
        running(&mut s);
        let first = s.handle(SchedulerEvent::ContentChange { pending_ops: 1 }, ts(1_000));
        assert_eq!(first.timer_in_ms, Some(3_000));
        let mut started = None;
        for tick in 1..=80 {
            let now = ts(1_000 + tick * 400);
            if s.handle(SchedulerEvent::TimerFired, now).sync_now {
                started = Some(tick * 400);
                break;
            }
            s.handle(SchedulerEvent::ContentChange { pending_ops: 1 }, now);
        }
        assert_eq!(started, Some(30_000), "durchgehendes Tippen: nach 30 s");

        // Eine Task-Änderung in der Serie macht sie zur normalen Serie.
        s.handle(ok(0, quiet()), ts(40_000));
        s.handle(SchedulerEvent::ContentChange { pending_ops: 1 }, ts(41_000));
        let mixed = s.handle(SchedulerEvent::LocalChange { pending_ops: 2 }, ts(41_500));
        assert_eq!(mixed.timer_in_ms, Some(1_500));
    }

    #[test]
    fn flush_uploads_now_only_with_pending_ops() {
        let mut s = scheduler();
        running(&mut s);
        assert!(!s.handle(SchedulerEvent::Flush, ts(1_000)).sync_now);
        s.handle(SchedulerEvent::ContentChange { pending_ops: 1 }, ts(1_000));
        assert!(s.handle(SchedulerEvent::Flush, ts(1_200)).sync_now);
        // Während des Laufs getippt: danach wieder der Inhalts-Debounce, kein Sofortlauf.
        s.handle(SchedulerEvent::ContentChange { pending_ops: 1 }, ts(1_300));
        let after = s.handle(ok(1, quiet()), ts(1_500));
        assert!(!after.sync_now);
        assert_eq!(after.timer_in_ms, Some(3_000), "ab Ende des Laufs");
    }

    #[test]
    fn last_change_of_a_burst_is_capped_by_max_wait() {
        let mut s = scheduler();
        running(&mut s);
        s.handle(SchedulerEvent::LocalChange { pending_ops: 1 }, ts(1_000));
        let late = s.handle(SchedulerEvent::LocalChange { pending_ops: 2 }, ts(5_000));
        assert_eq!(
            late.timer_in_ms,
            Some(1_000),
            "1 000 + 5 000 statt 5 000 + 1 500"
        );
        assert!(s.handle(SchedulerEvent::TimerFired, ts(6_000)).sync_now);
    }

    #[test]
    fn change_during_a_run_triggers_exactly_one_more() {
        let mut s = scheduler();
        running(&mut s);
        s.handle(SchedulerEvent::LocalChange { pending_ops: 1 }, ts(1_000));
        assert!(s.handle(SchedulerEvent::TimerFired, ts(2_500)).sync_now);
        let during = s.handle(SchedulerEvent::LocalChange { pending_ops: 1 }, ts(2_600));
        assert!(!during.sync_now);
        assert!(
            s.handle(ok(1, SyncReport::default()), ts(3_000)).sync_now,
            "gleich nochmal"
        );
        assert!(!s.handle(ok(0, SyncReport::default()), ts(3_200)).sync_now);
    }

    #[test]
    fn offline_and_backoff_ignore_the_burst_window() {
        let mut s = scheduler();
        running(&mut s);
        s.handle(SchedulerEvent::NetworkDown, ts(1_000));
        let offline = s.handle(SchedulerEvent::LocalChange { pending_ops: 1 }, ts(1_100));
        assert_eq!(offline.timer_in_ms, None, "offline kein Timer");
        assert!(s.handle(SchedulerEvent::NetworkUp, ts(9_000)).sync_now);
        let failed = s.handle(
            SchedulerEvent::SyncFinished(SyncOutcome {
                result: Err(CoreError::Network("weg".into())),
                pending_ops: 1,
            }),
            ts(9_100),
        );
        let backoff = failed.timer_in_ms.expect("Backoff");
        let change = s.handle(SchedulerEvent::LocalChange { pending_ops: 2 }, ts(9_200));
        assert_eq!(
            change.timer_in_ms,
            Some(backoff - 100),
            "Backoff bleibt, kein schnellerer Upload"
        );
    }

    #[test]
    fn changes_during_a_run_trigger_one_rerun() {
        let mut s = scheduler();
        s.handle(SchedulerEvent::SetEnabled(true), ts(0));
        s.handle(SchedulerEvent::LocalChange { pending_ops: 1 }, ts(10));
        let manual = s.handle(SchedulerEvent::ManualSync, ts(20));
        assert!(!manual.sync_now, "nie parallel");
        assert!(s.handle(ok(1, quiet()), ts(30)).sync_now, "Rerun");
        let after = s.handle(ok(0, quiet()), ts(40));
        assert!(!after.sync_now);
    }

    #[test]
    fn backoff_grows_from_5_s_to_10_min() {
        let mut s = scheduler();
        s.handle(SchedulerEvent::SetEnabled(true), ts(0));
        let mut waits = Vec::new();
        let mut now = 0;
        for _ in 0..10 {
            let actions = s.handle(failed(CoreError::Network("down".into()), 2), ts(now));
            assert_eq!(actions.status, SyncStatus::Offline { pending_ops: 2 });
            let wait = actions.timer_in_ms.unwrap();
            waits.push(wait / 1000);
            now += i64::try_from(wait).unwrap();
            assert!(s.handle(SchedulerEvent::TimerFired, ts(now)).sync_now);
        }
        assert_eq!(waits, [5, 10, 20, 40, 80, 160, 320, 600, 600, 600]);
        let ok = s.handle(ok(0, quiet()), ts(now));
        assert_eq!(ok.timer_in_ms, Some(60_000), "nach Erfolg wieder normal");
    }

    #[test]
    fn local_changes_do_not_shorten_backoff() {
        let mut s = scheduler();
        s.handle(SchedulerEvent::SetEnabled(true), ts(0));
        s.handle(failed(CoreError::Network("down".into()), 1), ts(0));
        s.handle(failed(CoreError::Network("down".into()), 1), ts(0));
        let change = s.handle(SchedulerEvent::LocalChange { pending_ops: 2 }, ts(1_000));
        assert_eq!(change.timer_in_ms, Some(9_000), "Retry nach 10 s bleibt");
    }

    #[test]
    fn jitter_spreads_retries_within_bounds() {
        let mut s = Scheduler::new(SchedulerConfig::default(), 7);
        s.handle(SchedulerEvent::SetEnabled(true), ts(0));
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..20 {
            let mut fresh = s.clone();
            fresh.rng = s.next_unit().to_bits();
            let wait = fresh
                .handle(failed(CoreError::Network("x".into()), 0), ts(0))
                .timer_in_ms
                .unwrap();
            assert!((4_000..=6_000).contains(&wait), "{wait}");
            seen.insert(wait);
        }
        assert!(seen.len() > 5, "streut: {seen:?}");
    }

    #[test]
    fn retry_after_is_respected() {
        let mut s = scheduler();
        s.handle(SchedulerEvent::SetEnabled(true), ts(0));
        let limited = s.handle(
            failed(CoreError::RateLimited { retry_after_s: 120 }, 0),
            ts(0),
        );
        assert_eq!(
            limited.status,
            SyncStatus::Paused(SyncPauseReason::RateLimited)
        );
        assert_eq!(limited.timer_in_ms, Some(120_000));
        for event in [SchedulerEvent::ManualSync, SchedulerEvent::AppFocused] {
            assert!(!s.handle(event, ts(60_000)).sync_now, "gesperrt");
        }
        assert!(s.handle(SchedulerEvent::TimerFired, ts(120_000)).sync_now);
    }

    #[test]
    fn auth_errors_pause_until_enabled_again() {
        for err in [
            CoreError::AuthRequired,
            CoreError::Auth(todo_core::ports::AuthError::SessionRevoked),
        ] {
            let mut s = scheduler();
            s.handle(SchedulerEvent::SetEnabled(true), ts(0));
            let paused = s.handle(failed(err, 3), ts(0));
            assert_eq!(
                paused.status,
                SyncStatus::Paused(SyncPauseReason::AuthRequired)
            );
            assert_eq!(paused.timer_in_ms, None, "kein Retry ohne Login");
            for event in [
                SchedulerEvent::ManualSync,
                SchedulerEvent::AppFocused,
                SchedulerEvent::NetworkUp,
                SchedulerEvent::LocalChange { pending_ops: 4 },
            ] {
                let actions = s.handle(event, ts(10_000));
                assert!(!actions.sync_now);
                assert_eq!(actions.timer_in_ms, None);
            }
            // Nach dem Login meldet die Shell den Sync wieder an.
            assert!(
                s.handle(SchedulerEvent::SetEnabled(true), ts(20_000))
                    .sync_now
            );
        }
    }

    #[test]
    fn hidden_app_has_no_timers_but_pushes_pending_changes() {
        let mut s = scheduler();
        running(&mut s);
        s.handle(SchedulerEvent::LocalChange { pending_ops: 1 }, ts(1_000));
        let hidden = s.handle(SchedulerEvent::AppHidden, ts(1_500));
        assert!(hidden.sync_now, "vor dem Verstecken hochladen");
        let done = s.handle(ok(0, quiet()), ts(2_000));
        assert_eq!(done.timer_in_ms, None, "kein Timer im Hintergrund");
        let failed = s.handle(failed(CoreError::Network("x".into()), 0), ts(2_500));
        assert_eq!(failed.timer_in_ms, None, "auch kein Retry-Timer");
        let shown = s.handle(SchedulerEvent::AppFocused, ts(3_000));
        assert!(shown.sync_now);
    }

    #[test]
    fn offline_waits_for_network() {
        let mut s = scheduler();
        running(&mut s);
        let down = s.handle(SchedulerEvent::NetworkDown, ts(1_000));
        assert_eq!(down.timer_in_ms, None);
        let change = s.handle(SchedulerEvent::LocalChange { pending_ops: 4 }, ts(2_000));
        assert!(!change.sync_now);
        assert_eq!(change.timer_in_ms, None);
        assert_eq!(change.status, SyncStatus::Offline { pending_ops: 4 });
        assert!(s.handle(SchedulerEvent::NetworkUp, ts(3_000)).sync_now);
    }

    #[test]
    fn idle_polling_slows_down_and_resets_on_activity() {
        let mut s = scheduler();
        running(&mut s);
        let mut intervals = Vec::new();
        let mut now = 100;
        for _ in 0..6 {
            now += 60_000;
            assert!(
                s.handle(SchedulerEvent::TimerFired, ts(now + 600_000))
                    .sync_now
            );
            intervals.push(s.handle(ok(0, quiet()), ts(now)).timer_in_ms.unwrap() / 1000);
        }
        assert_eq!(intervals, [120, 240, 480, 600, 600, 600]);
        s.handle(SchedulerEvent::TimerFired, ts(now + 600_000));
        let active = s.handle(ok(0, busy()), ts(now));
        assert_eq!(
            active.timer_in_ms,
            Some(60_000),
            "Aktivität → wieder häufiger"
        );
    }

    #[test]
    fn polling_can_be_disabled() {
        let mut s = Scheduler::new(
            SchedulerConfig {
                poll_min_ms: None,
                ..SchedulerConfig::default()
            },
            1,
        );
        let actions = running(&mut s);
        assert_eq!(actions.timer_in_ms, None, "kein Polling im Leerlauf");
    }

    #[test]
    fn permanent_errors_show_error_and_still_back_off() {
        let mut s = scheduler();
        s.handle(SchedulerEvent::SetEnabled(true), ts(0));
        let actions = s.handle(failed(CoreError::RemoteRejected("403".into()), 1), ts(0));
        assert!(
            matches!(actions.status, SyncStatus::Error { ref message } if message.contains("403"))
        );
        assert_eq!(actions.timer_in_ms, Some(5_000));
    }

    #[test]
    fn newer_format_pauses_until_restart() {
        let mut s = scheduler();
        s.handle(SchedulerEvent::SetEnabled(true), ts(0));
        let report = SyncReport {
            newer_format: true,
            ..SyncReport::default()
        };
        let paused = s.handle(ok(0, report), ts(10));
        assert_eq!(
            paused.status,
            SyncStatus::Paused(SyncPauseReason::NewerFormat)
        );
        assert_eq!(paused.timer_in_ms, None);
        assert!(!s.handle(SchedulerEvent::ManualSync, ts(20)).sync_now);
    }

    #[test]
    fn disabling_stops_everything() {
        let mut s = scheduler();
        running(&mut s);
        let off = s.handle(SchedulerEvent::SetEnabled(false), ts(1_000));
        assert_eq!(off.status, SyncStatus::Disabled);
        assert_eq!(off.timer_in_ms, None);
    }

    /// Ablauf mit einer `SteppingClock` wie in der Shell: keine echten Sleeps, Zeit nur über
    /// die Uhr.
    #[test]
    fn full_cycle_with_stepping_clock() {
        let clock = SteppingClock::new(ts(0), 1_000);
        let mut s = scheduler();
        let mut syncs = 0;
        let mut feed = |s: &mut Scheduler, event| {
            let actions = s.handle(event, clock.now());
            syncs += usize::from(actions.sync_now);
            actions
        };
        feed(&mut s, SchedulerEvent::SetEnabled(true));
        feed(&mut s, ok(0, quiet()));
        feed(&mut s, SchedulerEvent::LocalChange { pending_ops: 1 });
        let early = feed(&mut s, SchedulerEvent::TimerFired); // +1 s: zu früh (Debounce 1,5 s)
        assert!(!early.sync_now);
        let fire = feed(&mut s, SchedulerEvent::TimerFired); // +2 s
        assert!(fire.sync_now);
        let done = feed(&mut s, ok(0, quiet()));
        assert!(matches!(done.status, SyncStatus::Idle { .. }));
        assert_eq!(syncs, 2);
    }
}
