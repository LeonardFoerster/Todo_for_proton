//! Zeitzonen als Port (N4, ADR-028): Der Core rechnet sonst nicht mit Zonen. Für wiederkehrende
//! Tasks mit Uhrzeit braucht er „gleiche lokale Uhrzeit am nächsten Tag“ – auch über die
//! Zeitumstellung. Nativ liefert die Shell die Implementierung (System-tzdb); ohne Port gehen nur
//! ganztägige Serien.

use chrono::NaiveDateTime;

use super::{MaybeSend, MaybeSync};
use crate::domain::Timestamp;

pub trait TimeZones: MaybeSend + MaybeSync {
    /// Wandzeit von `at` in der IANA-Zone `tz`; `None` bei unbekannter Zone.
    fn to_local(&self, at: Timestamp, tz: &str) -> Option<NaiveDateTime>;
    /// Zeitpunkt der Wandzeit `local` in `tz`. Lücke (Sommerzeit) → später, doppelt → früher.
    fn to_utc(&self, local: NaiveDateTime, tz: &str) -> Option<Timestamp>;
}

#[cfg(any(test, feature = "test-support"))]
pub use test_support::FixedOffsetZones;

#[cfg(any(test, feature = "test-support"))]
mod test_support {
    use chrono::{NaiveDateTime, TimeZone as _};

    use super::TimeZones;
    use crate::domain::Timestamp;

    /// Jede Zone mit fester Abweichung (Tests): `"UTC"` = 0, sonst `offset_minutes`.
    #[derive(Debug, Clone, Copy)]
    pub struct FixedOffsetZones {
        pub offset_minutes: i32,
    }

    impl FixedOffsetZones {
        fn offset(&self, tz: &str) -> chrono::FixedOffset {
            let minutes = if tz == "UTC" { 0 } else { self.offset_minutes };
            chrono::FixedOffset::east_opt(minutes * 60).expect("valid offset")
        }
    }

    impl TimeZones for FixedOffsetZones {
        fn to_local(&self, at: Timestamp, tz: &str) -> Option<NaiveDateTime> {
            let utc = chrono::DateTime::from_timestamp_millis(at.as_millis())?;
            Some(utc.with_timezone(&self.offset(tz)).naive_local())
        }

        fn to_utc(&self, local: NaiveDateTime, tz: &str) -> Option<Timestamp> {
            let t = self.offset(tz).from_local_datetime(&local).single()?;
            Some(Timestamp::from_millis(t.timestamp_millis()))
        }
    }
}
