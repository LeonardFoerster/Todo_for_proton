//! Port `TimeZones` (N4, ADR-028) über `jiff` mit der System-tzdb: gleiche lokale Uhrzeit für das
//! nächste Vorkommen einer Serie, auch über die Zeitumstellung (Lücke → später, doppelt → früher).

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use jiff::{civil, tz::TimeZone};
use todo_core::{domain::Timestamp, ports::TimeZones};

#[derive(Debug, Default)]
pub struct JiffZones;

impl TimeZones for JiffZones {
    fn to_local(&self, at: Timestamp, tz: &str) -> Option<NaiveDateTime> {
        let zone = TimeZone::get(tz).ok()?;
        let dt = zone.to_datetime(jiff::Timestamp::from_millisecond(at.as_millis()).ok()?);
        NaiveDate::from_ymd_opt(
            i32::from(dt.year()),
            u32::try_from(dt.month()).ok()?,
            u32::try_from(dt.day()).ok()?,
        )?
        .and_hms_nano_opt(
            u32::try_from(dt.hour()).ok()?,
            u32::try_from(dt.minute()).ok()?,
            u32::try_from(dt.second()).ok()?,
            u32::try_from(dt.subsec_nanosecond()).ok()?,
        )
    }

    fn to_utc(&self, local: NaiveDateTime, tz: &str) -> Option<Timestamp> {
        let zone = TimeZone::get(tz).ok()?;
        let dt = civil::DateTime::new(
            i16::try_from(local.year()).ok()?,
            i8::try_from(local.month()).ok()?,
            i8::try_from(local.day()).ok()?,
            i8::try_from(local.hour()).ok()?,
            i8::try_from(local.minute()).ok()?,
            i8::try_from(local.second()).ok()?,
            i32::try_from(local.nanosecond()).ok()?,
        )
        .ok()?;
        let zoned = zone.to_ambiguous_zoned(dt).compatible().ok()?;
        Some(Timestamp::from_millis(zoned.timestamp().as_millisecond()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn ms(s: &str) -> i64 {
        s.parse::<jiff::Timestamp>().unwrap().as_millisecond()
    }

    #[test]
    fn keeps_local_time_across_dst_and_resolves_gaps_later() {
        let z = JiffZones;
        // 14:00 Berlin im Sommer = 12:00 UTC, im Winter = 13:00 UTC.
        assert_eq!(
            z.to_utc(local("2026-10-24 14:00"), "Europe/Berlin"),
            Some(Timestamp::from_millis(ms("2026-10-24T12:00:00Z")))
        );
        assert_eq!(
            z.to_utc(local("2026-10-26 14:00"), "Europe/Berlin"),
            Some(Timestamp::from_millis(ms("2026-10-26T13:00:00Z")))
        );
        assert_eq!(
            z.to_local(
                Timestamp::from_millis(ms("2026-10-26T13:00:00Z")),
                "Europe/Berlin"
            ),
            Some(local("2026-10-26 14:00"))
        );
        // 02:30 am 29.03. gibt es nicht → 03:30 Sommerzeit.
        assert_eq!(
            z.to_utc(local("2026-03-29 02:30"), "Europe/Berlin"),
            Some(Timestamp::from_millis(ms("2026-03-29T01:30:00Z")))
        );
        assert_eq!(z.to_utc(local("2026-03-29 02:30"), "Nirgendwo/Stadt"), None);
    }
}
