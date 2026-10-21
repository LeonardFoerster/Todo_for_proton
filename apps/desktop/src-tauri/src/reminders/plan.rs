//! Reine Planung der Erinnerungen (ADR-033): Wann meldet sich eine Fälligkeit, welche sind jetzt
//! dran, wann ist die nächste? Ohne IO und ohne Uhr – alles kommt herein, damit es testbar ist.

use std::collections::HashSet;

use jiff::{civil, tz::TimeZone, Timestamp};
use todo_core::api::dto::{parse_iso, DueDto};

/// Erinnerungen, die beim Start oder nach dem Aufwachen höchstens so weit zurückliegen, kommen
/// noch (zusammengefasst); ältere gelten als verpasst.
pub const CATCH_UP_MS: i64 = 24 * 60 * 60 * 1000;

/// Eine offene Task mit Erinnerungszeitpunkt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    pub task_id: String,
    pub title: String,
    /// Unix-ms.
    pub at: i64,
}

impl Reminder {
    /// Schlüssel für „schon gemeldet“: dieselbe Task mit verschobener Fälligkeit meldet sich neu.
    #[must_use]
    pub fn key(&self) -> (String, i64) {
        (self.task_id.clone(), self.at)
    }
}

/// Was jetzt zu tun ist.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// Jetzt melden (älteste zuerst).
    pub fire: Vec<Reminder>,
    /// Nächster Zeitpunkt in der Zukunft (Unix-ms), falls es einen gibt.
    pub next: Option<i64>,
}

/// Uhrzeit für ganztägige Fälligkeiten, `"HH:MM"`.
///
/// # Errors
/// Bei anderem Format oder ungültiger Uhrzeit.
pub fn parse_all_day_time(s: &str) -> Result<(i8, i8), String> {
    let (h, m) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid time {s:?}"))?;
    let (h, m): (i8, i8) = (
        h.parse().map_err(|_| format!("invalid time {s:?}"))?,
        m.parse().map_err(|_| format!("invalid time {s:?}"))?,
    );
    if (0..24).contains(&h) && (0..60).contains(&m) && s.len() == 5 {
        Ok((h, m))
    } else {
        Err(format!("invalid time {s:?}"))
    }
}

/// Zeitpunkt der Erinnerung: mit Uhrzeit genau dann, ganztägig um `all_day` Ortszeit in `local`
/// (auch über die Zeitumstellung: Lücke → später, doppelt → früher, wie `jiff` „compatible“).
#[must_use]
pub fn reminder_at(due: &DueDto, all_day: (i8, i8), local: &TimeZone) -> Option<i64> {
    match due {
        DueDto::DateTime { at, .. } => parse_iso(at)
            .ok()
            .map(todo_core::domain::Timestamp::as_millis),
        DueDto::Date { date } => {
            let date: civil::Date = date.parse().ok()?;
            let time = civil::time(all_day.0, all_day.1, 0, 0);
            let zoned = date.to_datetime(time).to_zoned(local.clone()).ok()?;
            Some(zoned.timestamp().as_millisecond())
        }
    }
}

/// Fällig = Zeitpunkt in `(now - CATCH_UP_MS, now]` und noch nicht gemeldet. `next` = frühester
/// Zeitpunkt nach `now`.
#[must_use]
pub fn plan(items: &[Reminder], notified: &HashSet<(String, i64)>, now: i64) -> Plan {
    let mut fire: Vec<Reminder> = items
        .iter()
        .filter(|r| r.at <= now && r.at > now - CATCH_UP_MS && !notified.contains(&r.key()))
        .cloned()
        .collect();
    fire.sort_by(|a, b| (a.at, &a.task_id).cmp(&(b.at, &b.task_id)));
    let next = items.iter().map(|r| r.at).filter(|&at| at > now).min();
    Plan { fire, next }
}

/// Text der Benachrichtigung: eine Task mit Titel, mehrere zusammengefasst.
#[must_use]
pub fn notification_text(fire: &[Reminder]) -> (String, String) {
    match fire {
        [one] => ("Fällig".to_owned(), one.title.clone()),
        many => {
            let titles: Vec<&str> = many.iter().take(3).map(|r| r.title.as_str()).collect();
            let more = many.len().saturating_sub(3);
            let body = if more > 0 {
                format!("{} und {more} weitere", titles.join(", "))
            } else {
                titles.join(", ")
            };
            (format!("{} Tasks fällig", many.len()), body)
        }
    }
}

/// Jetzt als Unix-ms (für die Laufzeit; `plan` bekommt die Zeit hereingereicht).
#[must_use]
pub fn now_ms() -> i64 {
    Timestamp::now().as_millisecond()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn berlin() -> TimeZone {
        TimeZone::get("Europe/Berlin").expect("tzdb")
    }

    fn ms(s: &str) -> i64 {
        s.parse::<Timestamp>().unwrap().as_millisecond()
    }

    fn reminder(id: &str, at: i64) -> Reminder {
        Reminder {
            task_id: id.into(),
            title: format!("Task {id}"),
            at,
        }
    }

    #[test]
    fn all_day_dues_ring_at_the_local_time_also_across_dst() {
        let date = |d: &str| DueDto::Date { date: d.into() };
        // Sommerzeit (UTC+2) und Winterzeit (UTC+1).
        assert_eq!(
            reminder_at(&date("2026-10-21"), (9, 0), &berlin()),
            Some(ms("2026-10-21T07:00:00Z"))
        );
        assert_eq!(
            reminder_at(&date("2026-10-26"), (9, 0), &berlin()),
            Some(ms("2026-10-26T08:00:00Z"))
        );
        // 02:30 am 29.03. gibt es nicht → später (03:30 Sommerzeit = 01:30 UTC).
        assert_eq!(
            reminder_at(&date("2026-03-29"), (2, 30), &berlin()),
            Some(ms("2026-03-29T01:30:00Z"))
        );
        assert_eq!(reminder_at(&date("kaputt"), (9, 0), &berlin()), None);
    }

    #[test]
    fn timed_dues_ring_at_their_instant() {
        let due = DueDto::DateTime {
            at: "2026-10-21T12:00:00.000Z".into(),
            tz: "Europe/Berlin".into(),
        };
        assert_eq!(
            reminder_at(&due, (9, 0), &berlin()),
            Some(ms("2026-10-21T12:00:00Z"))
        );
    }

    #[test]
    fn fires_due_ones_once_within_the_catch_up_window_and_finds_the_next() {
        let now = ms("2026-10-21T12:00:00Z");
        let items = [
            reminder("future", now + 60_000),
            reminder("now", now),
            reminder("hour-ago", now - 3_600_000),
            reminder("too-old", now - CATCH_UP_MS - 1),
            reminder("later", now + 3_600_000),
        ];
        let p = plan(&items, &HashSet::new(), now);
        assert_eq!(
            p.fire
                .iter()
                .map(|r| r.task_id.as_str())
                .collect::<Vec<_>>(),
            ["hour-ago", "now"]
        );
        assert_eq!(p.next, Some(now + 60_000));

        let notified: HashSet<_> = p.fire.iter().map(Reminder::key).collect();
        assert!(plan(&items, &notified, now).fire.is_empty(), "nur einmal");
        // Verschoben → neuer Zeitpunkt → meldet sich wieder.
        let moved = [reminder("now", now + 10)];
        assert_eq!(plan(&moved, &notified, now + 20).fire.len(), 1);
        assert_eq!(plan(&[], &notified, now), Plan::default());
    }

    #[test]
    fn texts_and_time_settings() {
        assert_eq!(
            notification_text(&[reminder("a", 0)]),
            ("Fällig".into(), "Task a".into())
        );
        let many: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| reminder(id, 0))
            .collect();
        assert_eq!(
            notification_text(&many),
            (
                "5 Tasks fällig".into(),
                "Task a, Task b, Task c und 2 weitere".into()
            )
        );
        assert_eq!(parse_all_day_time("09:00"), Ok((9, 0)));
        assert_eq!(parse_all_day_time("23:59"), Ok((23, 59)));
        for bad in ["9:00", "24:00", "12:60", "x", "12-00"] {
            assert!(parse_all_day_time(bad).is_err(), "{bad}");
        }
    }
}
