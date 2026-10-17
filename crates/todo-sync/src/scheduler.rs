//! Wann gesynct wird – als reiner Zustandsautomat ohne Laufzeit, Timer oder Uhr
//! (`sync-strategy.md` §4–§6).
//!
//! Die Shell meldet Ereignisse ([`SchedulerEvent`]) samt aktueller Zeit und setzt danach genau
//! das um, was [`Actions`] sagt: ggf. sofort `SyncEngine::sync_once` starten (und das Ergebnis
//! als [`SchedulerEvent::SyncFinished`] zurückmelden), einen einzigen Timer auf
//! `timer_in_ms` stellen (vorherigen ersetzen; `None` = keinen) und den Status anzeigen.
//!
//! Regeln: Debounce nach lokalen Änderungen; exponentieller Backoff mit Jitter bei Fehlern;
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
    pub debounce_ms: u64,
    pub backoff_min_ms: u64,
    pub backoff_max_ms: u64,
    /// Anteil zufälliger Streuung des Backoffs (0.2 = ±20 %).
    pub jitter: f64,
    /// Periodischer Pull bei sichtbarem Fenster: Startabstand; `None` = nie periodisch.
    pub poll_min_ms: Option<u64>,
    /// Größter Abstand des periodischen Pulls (wächst, solange nichts passiert).
    pub poll_max_ms: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 3_000,
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
    failures: u32,
    /// Abstand bis zum nächsten periodischen Pull.
    poll_wait_ms: u64,
    /// Abstand für den Pull danach (wächst, solange nichts passiert).
    poll_next_ms: u64,
    rate_limited_until: Option<Timestamp>,
    newer_format: bool,
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
            failures: 0,
            rate_limited_until: None,
            newer_format: false,
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
                self.deadline = None;
                self.failures = 0;
                sync_now = on;
            }
            SchedulerEvent::AppStarted | SchedulerEvent::ManualSync => sync_now = true,
            SchedulerEvent::LocalChange { pending_ops } => {
                self.pending_ops = pending_ops;
                if self.running {
                    self.rerun = true;
                } else if self.failures == 0 {
                    self.deadline = Some(at(now, self.config.debounce_ms));
                }
            }
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
                self.deadline = (self.pending_ops > 0).then(|| at(now, self.config.debounce_ms));
                self.status = SyncStatus::Idle {
                    last_success: self.last_success,
                };
                rerun
            }
            // Ein anderer Lauf ist noch aktiv: nichts ändern.
            Err(CoreError::Conflict(_)) => false,
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

    /// Setzt die gewünschte Aktion unter den aktuellen Bedingungen um.
    fn decide(&mut self, mut sync_now: bool, now: Timestamp) -> Actions {
        let blocked_until = self.rate_limited_until.filter(|until| now < *until);
        if !self.enabled || !self.online || self.newer_format || blocked_until.is_some() {
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
            self.status = SyncStatus::Syncing;
        } else if !self.running && self.deadline.is_none() && self.wants_poll() {
            self.deadline = Some(at(now, self.poll_wait_ms));
        }
        if !self.running {
            self.status = self.idle_status(blocked_until.is_some());
        }
        let timer_in_ms = self
            .deadline
            .filter(|_| self.enabled && self.online && self.visible && !self.newer_format)
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
        assert_eq!(first.timer_in_ms, Some(3_000));
        let second = s.handle(SchedulerEvent::LocalChange { pending_ops: 2 }, ts(2_000));
        assert_eq!(second.timer_in_ms, Some(3_000), "Debounce neu gestartet");
        assert!(
            !s.handle(SchedulerEvent::TimerFired, ts(4_000)).sync_now,
            "zu früh"
        );
        assert!(s.handle(SchedulerEvent::TimerFired, ts(5_000)).sync_now);
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
        feed(&mut s, SchedulerEvent::TimerFired); // +1 s: zu früh
        feed(&mut s, SchedulerEvent::TimerFired); // +2 s: zu früh
        let fire = feed(&mut s, SchedulerEvent::TimerFired); // +3 s
        assert!(fire.sync_now);
        let done = feed(&mut s, ok(0, quiet()));
        assert!(matches!(done.status, SyncStatus::Idle { .. }));
        assert_eq!(syncs, 2);
    }
}
