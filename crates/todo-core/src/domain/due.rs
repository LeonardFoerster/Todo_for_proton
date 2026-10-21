//! Fälligkeit einer Task: reines Datum oder Zeitpunkt mit Zeitzone.
//!
//! Der Core rechnet in v1 nicht mit Zeitzonen: `tz` wird nur gespeichert. Alles, was den
//! lokalen Kalender braucht (heutiges Datum, Tagesgrenzen), liefert der Aufrufer.

use std::ops::Range;

use chrono::Datelike;

pub use chrono::NaiveDate;

use super::{text::normalize_name, Timestamp};
use crate::{CoreError, CoreResult};

/// Maximale Länge eines Zeitzonen-Namens (IANA-Namen sind deutlich kürzer).
pub const DUE_TZ_MAX_CHARS: usize = 64;

/// Tageszahl (seit 1.1.0001) des 1.1.1970 – Bezug von [`Due::rough_instant`].
pub const UNIX_EPOCH_DAYS_FROM_CE: i64 = 719_163;

/// Ein Kalendertag in der lokalen Zone des Nutzers, vom Aufrufer berechnet (der Core rechnet
/// keine Zonen): Datum plus Tagesgrenzen `start..end` (`end` = Beginn des Folgetags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalDay {
    date: NaiveDate,
    start: Timestamp,
    end: Timestamp,
}

impl LocalDay {
    /// # Errors
    /// `CoreError::Validation`, wenn nicht `start < end`.
    pub fn new(date: NaiveDate, start: Timestamp, end: Timestamp) -> CoreResult<Self> {
        if start >= end {
            return Err(CoreError::Validation(format!(
                "local day {date}: start must be before end"
            )));
        }
        Ok(Self { date, start, end })
    }

    #[must_use]
    pub fn date(&self) -> NaiveDate {
        self.date
    }

    #[must_use]
    pub fn span(&self) -> Range<Timestamp> {
        self.start..self.end
    }

    /// Ungefährer Beginn des lokalen Tages `date`: Beginn dieses Tages plus volle 24 Stunden je
    /// Tag Abstand. Ohne Zonendaten weicht das an Tagen mit anderer UTC-Abweichung
    /// (Zeitumstellung dazwischen) um deren Differenz ab – genug für die Sortierung (ADR-026).
    #[must_use]
    pub fn approx_start_of(&self, date: NaiveDate) -> Timestamp {
        let days = (date - self.date).num_days();
        self.start.plus_millis(days.saturating_mul(MILLIS_PER_DAY))
    }
}

const MILLIS_PER_DAY: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Due {
    /// Ganztägig fällig, z. B. „am 20.10.".
    Date(NaiveDate),
    /// Fällig zu einem Zeitpunkt, z. B. „20.10. 14:00 Europe/Berlin".
    DateTime { at: Timestamp, tz: String },
}

impl Due {
    /// Ist die Fälligkeit verstrichen? Ein Datum ist erst ab dem Folgetag überfällig,
    /// ein Zeitpunkt sobald `now` ihn überschritten hat.
    ///
    /// `today` ist das lokale Datum des Nutzers, `now` die aktuelle Zeit.
    #[must_use]
    pub fn is_overdue(&self, today: NaiveDate, now: Timestamp) -> bool {
        match self {
            Self::Date(date) => *date < today,
            Self::DateTime { at, .. } => *at < now,
        }
    }

    /// Ist die Task am lokalen Tag `day` fällig?
    ///
    /// `day_span` sind die Grenzen dieses Tages in der lokalen Zone (`start..nächster_start`);
    /// sie werden nur für [`Due::DateTime`] gebraucht.
    #[must_use]
    pub fn is_due_on(&self, day: NaiveDate, day_span: &Range<Timestamp>) -> bool {
        match self {
            Self::Date(date) => *date == day,
            Self::DateTime { at, .. } => day_span.contains(at),
        }
    }

    /// Fällig an `day` oder früher (also heute fällig oder überfällig)? Grundlage der
    /// „Heute"-Ansicht; das Gegenteil ist „Demnächst".
    #[must_use]
    pub fn is_due_by(&self, day: &LocalDay) -> bool {
        match self {
            Self::Date(date) => *date <= day.date,
            Self::DateTime { at, .. } => *at < day.end,
        }
    }

    /// Sortierzeitpunkt in „Heute"/„Geplant" (ADR-026): der Zeitpunkt bzw. der (ungefähre)
    /// Beginn des Tages, relativ zu `day`. Ganztägiges steht damit vor Uhrzeiten desselben Tages.
    #[must_use]
    pub fn sort_instant(&self, day: &LocalDay) -> Timestamp {
        match self {
            Self::Date(date) => day.approx_start_of(*date),
            Self::DateTime { at, .. } => *at,
        }
    }

    /// Sortierzeitpunkt ohne Bezugstag (Sortierung „Fälligkeit“ außerhalb von Heute/Geplant,
    /// ADR-037): der Zeitpunkt bzw. Mitternacht UTC des Tages – ganztägig vor Uhrzeiten desselben
    /// UTC-Tages. Gleich in allen Speichern berechenbar (SQLite: Tageszahl · 24 h).
    #[must_use]
    pub fn rough_instant(&self) -> Timestamp {
        match self {
            Self::Date(date) => {
                let days = i64::from(date.num_days_from_ce()) - UNIX_EPOCH_DAYS_FROM_CE;
                Timestamp::from_millis(days * 86_400_000)
            }
            Self::DateTime { at, .. } => *at,
        }
    }

    /// Prüft die Invarianten und trimmt den Zonennamen.
    pub(crate) fn normalized(self) -> CoreResult<Self> {
        match self {
            Self::Date(_) => Ok(self),
            Self::DateTime { at, tz } => Ok(Self::DateTime {
                at,
                tz: normalize_name(&tz, DUE_TZ_MAX_CHARS, "time zone")?,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
    }

    const NOW: Timestamp = Timestamp::from_millis(1_000_000);

    #[test]
    fn sort_instant_puts_all_day_first_and_follows_the_calendar() {
        let start = Timestamp::from_millis(10 * MILLIS_PER_DAY);
        let today = LocalDay::new(day(17), start, start.plus_millis(MILLIS_PER_DAY)).unwrap();
        assert_eq!(Due::Date(day(17)).sort_instant(&today), start);
        assert_eq!(
            Due::Date(day(15)).sort_instant(&today),
            start.plus_millis(-2 * MILLIS_PER_DAY)
        );
        let morning = Due::DateTime {
            at: start.plus_millis(MILLIS_PER_DAY + 1),
            tz: "UTC".into(),
        };
        assert!(Due::Date(day(18)).sort_instant(&today) < morning.sort_instant(&today));
        assert!(morning.sort_instant(&today) < Due::Date(day(19)).sort_instant(&today));
    }

    #[test]
    fn date_overdue_only_after_the_day() {
        let today = day(17);
        assert!(Due::Date(day(16)).is_overdue(today, NOW), "gestern");
        assert!(!Due::Date(day(17)).is_overdue(today, NOW), "heute");
        assert!(!Due::Date(day(18)).is_overdue(today, NOW), "morgen");
    }

    #[test]
    fn date_time_overdue_once_passed() {
        let due = |ms| Due::DateTime {
            at: Timestamp::from_millis(ms),
            tz: "Europe/Berlin".into(),
        };
        assert!(due(999_999).is_overdue(day(17), NOW));
        assert!(!due(1_000_000).is_overdue(day(17), NOW));
        assert!(!due(1_000_001).is_overdue(day(17), NOW));
    }

    #[test]
    fn date_is_due_on_its_day() {
        let span = Timestamp::EPOCH..Timestamp::EPOCH;
        assert!(Due::Date(day(17)).is_due_on(day(17), &span));
        assert!(!Due::Date(day(17)).is_due_on(day(18), &span));
    }

    #[test]
    fn date_time_is_due_within_caller_day_span() {
        let span = Timestamp::from_millis(100)..Timestamp::from_millis(200);
        let due = |ms| Due::DateTime {
            at: Timestamp::from_millis(ms),
            tz: "UTC".into(),
        };
        assert!(due(100).is_due_on(day(17), &span));
        assert!(due(199).is_due_on(day(17), &span));
        assert!(!due(99).is_due_on(day(17), &span));
        assert!(
            !due(200).is_due_on(day(17), &span),
            "nächster Tag beginnt bei 200"
        );
    }

    #[test]
    fn local_day_requires_start_before_end() {
        let t = Timestamp::from_millis;
        assert!(LocalDay::new(day(17), t(100), t(200)).is_ok());
        assert!(LocalDay::new(day(17), t(200), t(200)).is_err());
        assert!(LocalDay::new(day(17), t(300), t(200)).is_err());
    }

    #[test]
    fn due_by_includes_today_and_overdue() {
        let t = Timestamp::from_millis;
        let today = LocalDay::new(day(17), t(100), t(200)).unwrap();
        assert!(Due::Date(day(16)).is_due_by(&today), "überfällig");
        assert!(Due::Date(day(17)).is_due_by(&today), "heute");
        assert!(!Due::Date(day(18)).is_due_by(&today), "morgen");
        let at = |ms| Due::DateTime {
            at: t(ms),
            tz: "UTC".into(),
        };
        assert!(at(50).is_due_by(&today));
        assert!(at(199).is_due_by(&today));
        assert!(!at(200).is_due_by(&today), "Beginn des Folgetags");
    }

    #[test]
    fn time_zone_is_only_stored() {
        let at = Timestamp::from_millis(1_760_000_000_000);
        let due = Due::DateTime {
            at,
            tz: " Pacific/Chatham ".into(),
        }
        .normalized()
        .unwrap();
        assert_eq!(
            due,
            Due::DateTime {
                at,
                tz: "Pacific/Chatham".into()
            }
        );
    }

    #[test]
    fn rejects_empty_or_overlong_time_zone() {
        let at = Timestamp::EPOCH;
        assert!(Due::DateTime {
            at,
            tz: "  ".into()
        }
        .normalized()
        .is_err());
        let long = "x".repeat(DUE_TZ_MAX_CHARS + 1);
        assert!(Due::DateTime { at, tz: long }.normalized().is_err());
    }
}
