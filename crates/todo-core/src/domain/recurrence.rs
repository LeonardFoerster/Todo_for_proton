//! Wiederholungsregel einer Task (N4, ADR-028): kleine RRULE-Teilmenge, 1:1 auf iCalendar abbildbar.
//!
//! Eine Task ist die Serie; ihre Fälligkeit ist immer das **nächste offene Vorkommen**. Hier wird
//! nur mit Kalendertagen gerechnet – die Uhrzeit (und die Zone) behandelt der Use-Case.

use std::{fmt, str::FromStr};

use chrono::{Datelike, Days, Months, NaiveDate, Weekday};

use crate::{CoreError, CoreResult};

/// Höchstes Intervall („alle 999 Wochen“).
pub const RECURRENCE_INTERVAL_MAX: u16 = 999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Bezug des Rhythmus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RecurrenceAnchor {
    /// Fester Rhythmus ab der Fälligkeit („jeden Fr“).
    #[default]
    Schedule,
    /// Rhythmus ab dem Tag des Abhakens („alle 3 Tage nach Erledigung“).
    Completion,
}

/// Wochentage als Bitmaske (Bit 0 = Montag … Bit 6 = Sonntag).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WeekdaySet(u8);

impl WeekdaySet {
    pub const EMPTY: Self = Self(0);
    /// Montag bis Freitag („werktags“).
    pub const WORKDAYS: Self = Self(0b001_1111);

    #[must_use]
    pub fn from_bits(bits: u8) -> Self {
        Self(bits & 0b111_1111)
    }

    #[must_use]
    pub fn bits(self) -> u8 {
        self.0
    }

    #[must_use]
    pub fn single(day: Weekday) -> Self {
        Self(1 << day.num_days_from_monday())
    }

    #[must_use]
    pub fn with(self, day: Weekday) -> Self {
        Self(self.0 | Self::single(day).0)
    }

    #[must_use]
    pub fn contains(self, day: Weekday) -> bool {
        self.0 & Self::single(day).0 != 0
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Wochentage aufsteigend ab Montag.
    pub fn iter(self) -> impl Iterator<Item = Weekday> {
        (0..7u8)
            .filter(move |i| self.0 & (1 << i) != 0)
            .map(|i| Weekday::try_from(i).expect("0..7 is a weekday"))
    }
}

/// Die Regel. Felder privat: nur über [`Recurrence::new`] (prüft) und [`Recurrence::anchored`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Recurrence {
    freq: Frequency,
    interval: u16,
    /// Nur wöchentlich; leer = Wochentag der Fälligkeit (füllt [`Recurrence::anchored`]).
    weekdays: WeekdaySet,
    /// Nur monatlich/jährlich; 31 → letzter Tag des Monats. `None` = Tag der Fälligkeit.
    month_day: Option<u8>,
    anchor: RecurrenceAnchor,
    /// Letzter möglicher Tag (einschließlich).
    until: Option<NaiveDate>,
}

impl Recurrence {
    /// # Errors
    /// `CoreError::Validation` bei Intervall außerhalb 1..=999, Wochentagen außer bei
    /// wöchentlich oder Monatstag außerhalb 1..=31 bzw. außer bei monatlich/jährlich.
    pub fn new(
        freq: Frequency,
        interval: u16,
        weekdays: WeekdaySet,
        month_day: Option<u8>,
        anchor: RecurrenceAnchor,
        until: Option<NaiveDate>,
    ) -> CoreResult<Self> {
        if !(1..=RECURRENCE_INTERVAL_MAX).contains(&interval) {
            return Err(invalid(&format!(
                "interval must be 1..={RECURRENCE_INTERVAL_MAX}, got {interval}"
            )));
        }
        if !weekdays.is_empty() && freq != Frequency::Weekly {
            return Err(invalid("weekdays only for weekly rules"));
        }
        if let Some(day) = month_day {
            if !matches!(freq, Frequency::Monthly | Frequency::Yearly) {
                return Err(invalid("month day only for monthly or yearly rules"));
            }
            if !(1..=31).contains(&day) {
                return Err(invalid(&format!("month day must be 1..=31, got {day}")));
            }
        }
        Ok(Self {
            freq,
            interval,
            weekdays,
            month_day,
            anchor,
            until,
        })
    }

    /// Einfache Regel mit Intervall 1 ohne Ende („täglich“, „wöchentlich“ …).
    #[must_use]
    pub fn every(freq: Frequency) -> Self {
        Self::new(
            freq,
            1,
            WeekdaySet::EMPTY,
            None,
            RecurrenceAnchor::Schedule,
            None,
        )
        .expect("interval 1 is valid")
    }

    #[must_use]
    pub fn freq(&self) -> Frequency {
        self.freq
    }
    #[must_use]
    pub fn interval(&self) -> u16 {
        self.interval
    }
    #[must_use]
    pub fn weekdays(&self) -> WeekdaySet {
        self.weekdays
    }
    #[must_use]
    pub fn month_day(&self) -> Option<u8> {
        self.month_day
    }
    #[must_use]
    pub fn anchor(&self) -> RecurrenceAnchor {
        self.anchor
    }
    #[must_use]
    pub fn until(&self) -> Option<NaiveDate> {
        self.until
    }

    /// Füllt die impliziten Teile aus der (ersten) Fälligkeit, damit die Regel nicht driftet:
    /// wöchentlich ohne Tage → deren Wochentag; monatlich/jährlich ohne Tag → deren Tag
    /// (31.01. bleibt „am 31.“, auch nach dem 28.02.).
    #[must_use]
    pub fn anchored(&self, due: NaiveDate) -> Self {
        let mut r = self.clone();
        match r.freq {
            Frequency::Weekly if r.weekdays.is_empty() => {
                r.weekdays = WeekdaySet::single(due.weekday());
            }
            Frequency::Monthly | Frequency::Yearly if r.month_day.is_none() => {
                r.month_day = u8::try_from(due.day()).ok();
            }
            _ => {}
        }
        r
    }

    /// Erstes Vorkommen am oder nach `start` (Regel ohne Fälligkeit: erstes ab heute).
    #[must_use]
    pub fn first_on_or_after(&self, start: NaiveDate) -> Option<NaiveDate> {
        let day = match self.freq {
            Frequency::Daily | Frequency::Yearly => start,
            Frequency::Weekly if self.weekdays.is_empty() => start,
            Frequency::Weekly => (0..7)
                .filter_map(|i| start.checked_add_days(Days::new(i)))
                .find(|d| self.weekdays.contains(d.weekday()))?,
            Frequency::Monthly => match self.month_day {
                None => start,
                Some(md) => {
                    let this = in_month(start, 0, md)?;
                    if this >= start {
                        this
                    } else {
                        in_month(start, 1, md)?
                    }
                }
            },
        };
        self.within_until(day)
    }

    /// Nächstes Vorkommen nach `previous` (strikt), das am oder nach `min` liegt – fester
    /// Rhythmus. Verpasste Vorkommen werden übersprungen (wer zwei Wochen zu spät abhakt, bekommt
    /// den nächsten kommenden Termin). `None`, wenn es nach `until` läge.
    #[must_use]
    pub fn next_after(&self, previous: NaiveDate, min: NaiveDate) -> Option<NaiveDate> {
        let n = u32::from(self.interval);
        let next = match self.freq {
            Frequency::Daily => {
                let mut d = previous.checked_add_days(Days::new(u64::from(n)))?;
                if d < min {
                    let gap = u64::try_from((min - d).num_days()).ok()?;
                    let steps = gap.div_ceil(u64::from(n));
                    d = d.checked_add_days(Days::new(steps * u64::from(n)))?;
                }
                d
            }
            Frequency::Weekly => self.next_weekly(previous, min)?,
            Frequency::Monthly => self.next_monthly(previous, min, n)?,
            Frequency::Yearly => self.next_monthly(previous, min, n * 12)?,
        };
        self.within_until(next)
    }

    /// Nächstes Vorkommen nach dem Abhaken am `done_on` („nach Erledigung“).
    #[must_use]
    pub fn after_completion(&self, done_on: NaiveDate) -> Option<NaiveDate> {
        let n = u32::from(self.interval);
        let next = match self.freq {
            Frequency::Daily => done_on.checked_add_days(Days::new(u64::from(n)))?,
            Frequency::Weekly => done_on.checked_add_days(Days::new(7 * u64::from(n)))?,
            Frequency::Monthly => add_months_clamped(done_on, n, done_on.day())?,
            Frequency::Yearly => add_months_clamped(done_on, n * 12, done_on.day())?,
        };
        self.within_until(next)
    }

    /// Nächstes Vorkommen beim Abhaken: fester Rhythmus nach `due` (mindestens `today`), sonst ab `today`.
    #[must_use]
    pub fn next_occurrence(&self, due: NaiveDate, today: NaiveDate) -> Option<NaiveDate> {
        match self.anchor {
            RecurrenceAnchor::Schedule => self.next_after(due, today),
            RecurrenceAnchor::Completion => self.after_completion(today),
        }
    }

    fn within_until(&self, day: NaiveDate) -> Option<NaiveDate> {
        match self.until {
            Some(until) if day > until => None,
            _ => Some(day),
        }
    }

    fn next_weekly(&self, previous: NaiveDate, min: NaiveDate) -> Option<NaiveDate> {
        let days = if self.weekdays.is_empty() {
            WeekdaySet::single(previous.weekday())
        } else {
            self.weekdays
        };
        let step = 7 * i64::from(self.interval);
        let monday = |d: NaiveDate| {
            d - chrono::Duration::days(i64::from(d.weekday().num_days_from_monday()))
        };
        let base = monday(previous);
        // Weit zurückliegende Fälligkeit: direkt zur Woche von `min` springen (gleiche Phase).
        let mut week = 0i64;
        let target = monday(min);
        if target > base {
            week = (target - base).num_days() / step * step;
        }
        // Höchstens zwei Perioden prüfen, dann ist ein Treffer sicher.
        for _ in 0..3 {
            let start = base + chrono::Duration::days(week);
            for day in days.iter() {
                let d = start + chrono::Duration::days(i64::from(day.num_days_from_monday()));
                if d > previous && d >= min {
                    return Some(d);
                }
            }
            week += step;
        }
        None
    }

    fn next_monthly(&self, previous: NaiveDate, min: NaiveDate, step: u32) -> Option<NaiveDate> {
        let day = self.month_day.map_or(previous.day(), u32::from);
        let mut k = step;
        // Weit zurück: Monate bis `min` überspringen (gleiche Phase).
        let months = |d: NaiveDate| i64::from(d.year()) * 12 + i64::from(d.month0());
        let gap = months(min) - months(previous);
        if gap > i64::from(step) {
            k = u32::try_from(gap / i64::from(step) * i64::from(step)).ok()?;
        }
        for _ in 0..4 {
            let d = add_months_clamped(first_of_month(previous), k, day)?;
            if d > previous && d >= min {
                return Some(d);
            }
            k += step;
        }
        None
    }
}

/// `date` um `months` Monate verschoben, Tag `day` (zu groß → letzter Tag des Monats).
fn add_months_clamped(date: NaiveDate, months: u32, day: u32) -> Option<NaiveDate> {
    let first = first_of_month(date).checked_add_months(Months::new(months))?;
    let last = first.checked_add_months(Months::new(1))?.pred_opt()?.day();
    first.with_day(day.min(last))
}

fn first_of_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1).expect("day 1 exists")
}

/// Tag `day` im Monat `date` + `offset` Monate (geklemmt).
fn in_month(date: NaiveDate, offset: u32, day: u8) -> Option<NaiveDate> {
    add_months_clamped(date, offset, u32::from(day))
}

fn invalid(message: &str) -> CoreError {
    CoreError::Validation(format!("recurrence: {message}"))
}

// --- Textform (SQLite-Spalte, Tests): iCalendar-ähnlich -----------------------------------------
//
// `FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TH;ANCHOR=COMPLETION;UNTIL=2026-12-20` – nur gesetzte Teile.

const DAY_CODES: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

impl fmt::Display for Recurrence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let freq = match self.freq {
            Frequency::Daily => "DAILY",
            Frequency::Weekly => "WEEKLY",
            Frequency::Monthly => "MONTHLY",
            Frequency::Yearly => "YEARLY",
        };
        write!(f, "FREQ={freq}")?;
        if self.interval != 1 {
            write!(f, ";INTERVAL={}", self.interval)?;
        }
        if !self.weekdays.is_empty() {
            let days: Vec<&str> = self
                .weekdays
                .iter()
                .map(|d| DAY_CODES[d.num_days_from_monday() as usize])
                .collect();
            write!(f, ";BYDAY={}", days.join(","))?;
        }
        if let Some(day) = self.month_day {
            write!(f, ";BYMONTHDAY={day}")?;
        }
        if self.anchor == RecurrenceAnchor::Completion {
            write!(f, ";ANCHOR=COMPLETION")?;
        }
        if let Some(until) = self.until {
            write!(f, ";UNTIL={until}")?;
        }
        Ok(())
    }
}

impl FromStr for Recurrence {
    type Err = CoreError;

    fn from_str(s: &str) -> CoreResult<Self> {
        let mut freq = None;
        let mut interval = 1u16;
        let mut weekdays = WeekdaySet::EMPTY;
        let mut month_day = None;
        let mut anchor = RecurrenceAnchor::Schedule;
        let mut until = None;
        for part in s.split(';') {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| invalid(&format!("bad part {part:?}")))?;
            match key {
                "FREQ" => {
                    freq = Some(match value {
                        "DAILY" => Frequency::Daily,
                        "WEEKLY" => Frequency::Weekly,
                        "MONTHLY" => Frequency::Monthly,
                        "YEARLY" => Frequency::Yearly,
                        other => return Err(invalid(&format!("unknown frequency {other:?}"))),
                    });
                }
                "INTERVAL" => {
                    interval = value
                        .parse()
                        .map_err(|_| invalid(&format!("bad interval {value:?}")))?;
                }
                "BYDAY" => {
                    for code in value.split(',') {
                        let i = DAY_CODES
                            .iter()
                            .position(|c| *c == code)
                            .ok_or_else(|| invalid(&format!("unknown weekday {code:?}")))?;
                        weekdays = weekdays
                            .with(Weekday::try_from(u8::try_from(i).unwrap_or(0)).expect("0..7"));
                    }
                }
                "BYMONTHDAY" => {
                    month_day = Some(
                        value
                            .parse()
                            .map_err(|_| invalid(&format!("bad month day {value:?}")))?,
                    );
                }
                "ANCHOR" if value == "COMPLETION" => anchor = RecurrenceAnchor::Completion,
                "UNTIL" => {
                    until = Some(
                        value
                            .parse()
                            .map_err(|_| invalid(&format!("bad date {value:?}")))?,
                    );
                }
                other => return Err(invalid(&format!("unknown part {other:?}"))),
            }
        }
        let freq = freq.ok_or_else(|| invalid("missing FREQ"))?;
        Self::new(freq, interval, weekdays, month_day, anchor, until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn rule(s: &str) -> Recurrence {
        s.parse().unwrap()
    }

    #[test]
    fn validates() {
        assert!(Recurrence::new(
            Frequency::Daily,
            0,
            WeekdaySet::EMPTY,
            None,
            RecurrenceAnchor::Schedule,
            None
        )
        .is_err());
        assert!(Recurrence::new(
            Frequency::Daily,
            1000,
            WeekdaySet::EMPTY,
            None,
            RecurrenceAnchor::Schedule,
            None
        )
        .is_err());
        assert!(Recurrence::new(
            Frequency::Daily,
            1,
            WeekdaySet::WORKDAYS,
            None,
            RecurrenceAnchor::Schedule,
            None
        )
        .is_err());
        assert!(Recurrence::new(
            Frequency::Weekly,
            1,
            WeekdaySet::EMPTY,
            Some(3),
            RecurrenceAnchor::Schedule,
            None
        )
        .is_err());
        assert!(Recurrence::new(
            Frequency::Monthly,
            1,
            WeekdaySet::EMPTY,
            Some(32),
            RecurrenceAnchor::Schedule,
            None
        )
        .is_err());
        assert!(Recurrence::new(
            Frequency::Monthly,
            1,
            WeekdaySet::EMPTY,
            Some(31),
            RecurrenceAnchor::Schedule,
            None
        )
        .is_ok());
    }

    #[test]
    fn text_roundtrip() {
        for s in [
            "FREQ=DAILY",
            "FREQ=DAILY;INTERVAL=3;ANCHOR=COMPLETION",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TH;UNTIL=2026-12-20",
            "FREQ=MONTHLY;BYMONTHDAY=31",
            "FREQ=YEARLY;BYMONTHDAY=29",
        ] {
            assert_eq!(rule(s).to_string(), s);
        }
        for bad in [
            "",
            "FREQ=HOURLY",
            "INTERVAL=2",
            "FREQ=WEEKLY;BYDAY=XX",
            "FREQ=DAILY;FOO=1",
        ] {
            assert!(bad.parse::<Recurrence>().is_err(), "{bad}");
        }
    }

    #[test]
    fn daily_and_interval() {
        let r = rule("FREQ=DAILY;INTERVAL=3");
        assert_eq!(
            r.next_after(d("2026-10-20"), d("2026-10-20")),
            Some(d("2026-10-23"))
        );
        // Zwei Wochen zu spät: nächstes Vorkommen im Rhythmus ab heute.
        assert_eq!(
            r.next_after(d("2026-10-01"), d("2026-10-21")),
            Some(d("2026-10-22"))
        );
    }

    #[test]
    fn weekly_days_across_weeks_and_interval() {
        let r = rule("FREQ=WEEKLY;BYDAY=MO,TH");
        assert_eq!(
            r.next_after(d("2026-10-19"), d("2026-10-19")),
            Some(d("2026-10-22")),
            "Mo → Do"
        );
        assert_eq!(
            r.next_after(d("2026-10-22"), d("2026-10-22")),
            Some(d("2026-10-26")),
            "Do → Mo"
        );
        let every2 = rule("FREQ=WEEKLY;INTERVAL=2;BYDAY=FR");
        assert_eq!(
            every2.next_after(d("2026-10-23"), d("2026-10-23")),
            Some(d("2026-11-06"))
        );
        // Verpasst: Phase bleibt (alle 2 Wochen ab 23.10.).
        assert_eq!(
            every2.next_after(d("2026-10-23"), d("2026-11-25")),
            Some(d("2026-12-04"))
        );
        // Ohne Tage: Wochentag der Fälligkeit.
        assert_eq!(
            Recurrence::every(Frequency::Weekly).next_after(d("2026-10-21"), d("2026-10-21")),
            Some(d("2026-10-28"))
        );
    }

    #[test]
    fn monthly_end_of_month_and_leap_years() {
        let r = Recurrence::every(Frequency::Monthly).anchored(d("2026-01-31"));
        assert_eq!(r.month_day(), Some(31));
        assert_eq!(
            r.next_after(d("2026-01-31"), d("2026-01-31")),
            Some(d("2026-02-28"))
        );
        assert_eq!(
            r.next_after(d("2026-02-28"), d("2026-02-28")),
            Some(d("2026-03-31")),
            "kein Drift"
        );
        assert_eq!(
            r.next_after(d("2026-03-31"), d("2026-03-31")),
            Some(d("2026-04-30"))
        );
        let leap = Recurrence::every(Frequency::Yearly).anchored(d("2024-02-29"));
        assert_eq!(
            leap.next_after(d("2024-02-29"), d("2024-02-29")),
            Some(d("2025-02-28"))
        );
        assert_eq!(
            leap.next_after(d("2027-02-28"), d("2027-02-28")),
            Some(d("2028-02-29"))
        );
        let fifteenth = rule("FREQ=MONTHLY;BYMONTHDAY=15");
        assert_eq!(
            fifteenth.next_after(d("2026-01-15"), d("2026-06-20")),
            Some(d("2026-07-15"))
        );
    }

    #[test]
    fn until_completion_and_first() {
        let r = rule("FREQ=DAILY;UNTIL=2026-10-22");
        assert_eq!(
            r.next_after(d("2026-10-21"), d("2026-10-21")),
            Some(d("2026-10-22"))
        );
        assert_eq!(r.next_after(d("2026-10-22"), d("2026-10-22")), None, "Ende");
        let c = rule("FREQ=DAILY;INTERVAL=3;ANCHOR=COMPLETION");
        assert_eq!(
            c.next_occurrence(d("2026-10-01"), d("2026-10-21")),
            Some(d("2026-10-24"))
        );
        assert_eq!(
            rule("FREQ=MONTHLY;ANCHOR=COMPLETION").after_completion(d("2026-01-31")),
            Some(d("2026-02-28"))
        );
        let workdays = Recurrence::new(
            Frequency::Weekly,
            1,
            WeekdaySet::WORKDAYS,
            None,
            RecurrenceAnchor::Schedule,
            None,
        )
        .unwrap();
        assert_eq!(
            workdays.first_on_or_after(d("2026-10-24")),
            Some(d("2026-10-26")),
            "Sa → Mo"
        );
        assert_eq!(
            rule("FREQ=MONTHLY;BYMONTHDAY=15").first_on_or_after(d("2026-10-21")),
            Some(d("2026-11-15"))
        );
        assert_eq!(
            rule("FREQ=MONTHLY;BYMONTHDAY=15").first_on_or_after(d("2026-10-15")),
            Some(d("2026-10-15"))
        );
    }
}

/// Property-Tests (nicht auf wasm32 – dort fehlt `proptest`).
#[cfg(all(test, not(target_arch = "wasm32")))]
mod props {
    use chrono::{Datelike, NaiveDate};
    use proptest::prelude::*;

    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn any_rule() -> impl Strategy<Value = Recurrence> {
        (0..4u8, 1..5u16, 1..128u8, 1..32u8).prop_map(|(f, n, days, md)| {
            let freq = [
                Frequency::Daily,
                Frequency::Weekly,
                Frequency::Monthly,
                Frequency::Yearly,
            ][usize::from(f)];
            let weekdays = if freq == Frequency::Weekly {
                WeekdaySet::from_bits(days)
            } else {
                WeekdaySet::EMPTY
            };
            let month_day = matches!(freq, Frequency::Monthly | Frequency::Yearly).then_some(md);
            Recurrence::new(
                freq,
                n,
                weekdays,
                month_day,
                RecurrenceAnchor::Schedule,
                None,
            )
            .unwrap()
        })
    }

    fn any_date() -> impl Strategy<Value = NaiveDate> {
        (0..3000i64).prop_map(|n| d("2024-01-01") + chrono::Duration::days(n))
    }

    proptest! {
        /// Ergebnis liegt nach `previous`, nicht vor `min`, passt zur Regel und ist monoton.
        #[test]
        fn next_after_is_sound(r in any_rule(), previous in any_date(), lag in 0..400i64) {
            let min = previous + chrono::Duration::days(lag);
            let next = r.next_after(previous, min).expect("ohne Ende gibt es immer ein nächstes");
            prop_assert!(next > previous);
            prop_assert!(next >= min);
            match r.freq() {
                Frequency::Weekly => prop_assert!(r.weekdays().contains(next.weekday())),
                Frequency::Monthly | Frequency::Yearly => {
                    let md = u32::from(r.month_day().unwrap());
                    let last = add_months_clamped(next, 0, 31).unwrap().day();
                    prop_assert_eq!(next.day(), md.min(last));
                }
                Frequency::Daily => prop_assert_eq!((next - previous).num_days() % i64::from(r.interval()), 0),
            }
            // Später fragen liefert nie ein früheres Vorkommen.
            let later = r.next_after(previous, min + chrono::Duration::days(1)).unwrap();
            prop_assert!(later >= next);
        }
    }
}
