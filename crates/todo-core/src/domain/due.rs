//! Fälligkeit einer Task: reines Datum oder Zeitpunkt mit Zeitzone.
//!
//! Der Core rechnet in v1 nicht mit Zeitzonen: `tz` wird nur gespeichert. Alles, was den
//! lokalen Kalender braucht (heutiges Datum, Tagesgrenzen), liefert der Aufrufer.

use std::ops::Range;

pub use chrono::NaiveDate;

use super::{text::normalize_name, Timestamp};
use crate::CoreResult;

/// Maximale Länge eines Zeitzonen-Namens (IANA-Namen sind deutlich kürzer).
pub const DUE_TZ_MAX_CHARS: usize = 64;

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
