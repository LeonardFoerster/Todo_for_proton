//! Hybrid Logical Clock (HLC): kausal korrekte, totale Ordnung von Ops über Geräte hinweg.
//!
//! Ein [`Hlc`] ist `(wall_ms, counter, device)` und wird in genau dieser Reihenfolge verglichen;
//! bei gleicher Zeit und gleichem Zähler entscheidet die höhere Geräte-ID (Tie-Break für LWW).
//! [`HlcState`] vergibt pro Gerät streng monotone Werte – auch wenn die Systemuhr rückwärts
//! springt – und rückt beim Empfang fremder Ops nach (siehe `sync-strategy.md` §2 und §5).

use std::fmt;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    domain::{DeviceId, Timestamp},
    CoreError, CoreResult,
};

/// Fremde Zeitstempel, die so weit vor der eigenen Uhr liegen, werden zwar akzeptiert, rücken
/// die eigene HLC aber nicht vor (sonst „vergiftet" eine falsche Uhr alle Geräte).
pub const MAX_CLOCK_DRIFT_MS: u64 = 24 * 60 * 60 * 1000;

/// Ein HLC-Zeitstempel. Eindeutig pro Op: jedes Gerät vergibt jeden Wert höchstens einmal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hlc {
    /// Physikalischer Anteil: Unix-Millisekunden (UTC).
    pub wall_ms: u64,
    /// Logischer Anteil: zählt Ereignisse mit gleichem `wall_ms`.
    pub counter: u32,
    /// Gerät, das den Wert vergeben hat.
    pub device: DeviceId,
}

impl Hlc {
    /// Länge von [`Hlc::to_bytes`].
    pub const BYTES: usize = 28;
    /// Länge der String-Form ([`fmt::Display`], Hex der Bytes).
    pub const STRING_LEN: usize = 2 * Self::BYTES;

    /// Kompakte Binärform: `wall_ms` (u64 BE) ‖ `counter` (u32 BE) ‖ Geräte-ID (16 Bytes).
    /// Der Byte-Vergleich entspricht der Ordnung von [`Hlc`].
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::BYTES] {
        let mut out = [0; Self::BYTES];
        out[..8].copy_from_slice(&self.wall_ms.to_be_bytes());
        out[8..12].copy_from_slice(&self.counter.to_be_bytes());
        out[12..].copy_from_slice(&self.device.to_bytes());
        out
    }

    /// Gegenstück zu [`Hlc::to_bytes`].
    ///
    /// # Errors
    /// `CoreError::Validation` bei falscher Länge.
    pub fn from_bytes(bytes: &[u8]) -> CoreResult<Self> {
        let bytes: &[u8; Self::BYTES] = bytes.try_into().map_err(|_| {
            CoreError::Validation(format!(
                "hlc must be {} bytes, got {}",
                Self::BYTES,
                bytes.len()
            ))
        })?;
        let (wall, rest) = bytes.split_at(8);
        let (counter, device) = rest.split_at(4);
        Ok(Self {
            wall_ms: u64::from_be_bytes(wall.try_into().expect("8 bytes")),
            counter: u32::from_be_bytes(counter.try_into().expect("4 bytes")),
            device: DeviceId::from_bytes(device.try_into().expect("16 bytes")),
        })
    }

    /// Liest die String-Form (56 Hex-Zeichen, klein) aus [`fmt::Display`].
    ///
    /// # Errors
    /// `CoreError::Validation` bei falscher Länge oder Nicht-Hex-Zeichen.
    pub fn parse(s: &str) -> CoreResult<Self> {
        let invalid = || CoreError::Validation(format!("invalid hlc: {s:?}"));
        if s.len() != Self::STRING_LEN {
            return Err(invalid());
        }
        let mut bytes = [0; Self::BYTES];
        for (byte, pair) in bytes.iter_mut().zip(s.as_bytes().chunks_exact(2)) {
            *byte = (hex_digit(pair[0]).ok_or_else(invalid)? << 4)
                | hex_digit(pair[1]).ok_or_else(invalid)?;
        }
        Self::from_bytes(&bytes)
    }
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Sortierbare String-Form: Hex (klein) von [`Hlc::to_bytes`]. Der String-Vergleich entspricht
/// der Ordnung von [`Hlc`] (z. B. für Dateinamen von Snapshots).
impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.to_bytes() {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Menschenlesbare Formate (JSON) bekommen die String-Form, Binärformate die 28 Bytes.
impl Serialize for Hlc {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(self)
        } else {
            serializer.serialize_bytes(&self.to_bytes())
        }
    }
}

impl<'de> Deserialize<'de> for Hlc {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
            Self::parse(&s).map_err(de::Error::custom)
        } else {
            deserializer.deserialize_bytes(HlcBytesVisitor)
        }
    }
}

struct HlcBytesVisitor;

impl<'de> de::Visitor<'de> for HlcBytesVisitor {
    type Value = Hlc;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} hlc bytes", Hlc::BYTES)
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Hlc, E> {
        Hlc::from_bytes(v).map_err(E::custom)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Hlc, A::Error> {
        let mut bytes = Vec::with_capacity(Hlc::BYTES);
        while let Some(byte) = seq.next_element::<u8>()? {
            bytes.push(byte);
        }
        Hlc::from_bytes(&bytes).map_err(de::Error::custom)
    }
}

/// Ein fremder Zeitstempel liegt mehr als [`MAX_CLOCK_DRIFT_MS`] vor der eigenen Uhr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSkew {
    pub remote: Hlc,
    pub now: Timestamp,
}

impl fmt::Display for ClockSkew {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "remote clock of device {} is {} ms ahead",
            self.remote.device,
            self.remote.wall_ms.saturating_sub(wall(self.now))
        )
    }
}

impl std::error::Error for ClockSkew {}

/// Zustand der HLC eines Geräts: der zuletzt vergebene oder beobachtete Wert.
///
/// Wird persistiert (`sync_meta`), damit auch nach einem Neustart nie ein Wert doppelt
/// vergeben wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HlcState {
    device: DeviceId,
    wall_ms: u64,
    counter: u32,
}

impl HlcState {
    /// Frischer Zustand eines Geräts (noch nichts vergeben).
    #[must_use]
    pub fn new(device: DeviceId) -> Self {
        Self {
            device,
            wall_ms: 0,
            counter: 0,
        }
    }

    /// Stellt einen persistierten Zustand wieder her ([`HlcState::last`]).
    #[must_use]
    pub fn restore(last: Hlc) -> Self {
        Self {
            device: last.device,
            wall_ms: last.wall_ms,
            counter: last.counter,
        }
    }

    #[must_use]
    pub fn device(&self) -> DeviceId {
        self.device
    }

    /// Der zuletzt vergebene bzw. beobachtete Wert (zum Persistieren).
    #[must_use]
    pub fn last(&self) -> Hlc {
        Hlc {
            wall_ms: self.wall_ms,
            counter: self.counter,
            device: self.device,
        }
    }

    /// Vergibt einen neuen Wert für ein lokales Ereignis. Streng größer als alle bisher
    /// vergebenen und beobachteten Werte, auch wenn `now` rückwärts springt.
    pub fn tick(&mut self, now: Timestamp) -> Hlc {
        let physical = wall(now);
        if physical > self.wall_ms {
            self.wall_ms = physical;
            self.counter = 0;
        } else {
            self.bump();
        }
        self.last()
    }

    /// Verarbeitet einen empfangenen Wert: Folgende [`HlcState::tick`]s liegen danach über
    /// `remote`.
    ///
    /// # Errors
    /// [`ClockSkew`], wenn `remote` mehr als [`MAX_CLOCK_DRIFT_MS`] vor `now` liegt. Der
    /// Zustand bleibt dann unverändert; die Op selbst darf trotzdem gemergt werden.
    pub fn observe(&mut self, remote: &Hlc, now: Timestamp) -> Result<(), ClockSkew> {
        if remote.wall_ms > wall(now).saturating_add(MAX_CLOCK_DRIFT_MS) {
            return Err(ClockSkew {
                remote: *remote,
                now,
            });
        }
        if (remote.wall_ms, remote.counter) > (self.wall_ms, self.counter) {
            self.wall_ms = remote.wall_ms;
            self.counter = remote.counter;
        }
        Ok(())
    }

    fn bump(&mut self) {
        // Praktisch unerreichbar (4 Mrd. Ereignisse in einer Millisekunde); dann eben die
        // Wanduhr künstlich vorrücken statt die Monotonie zu brechen.
        match self.counter.checked_add(1) {
            Some(counter) => self.counter = counter,
            None => {
                self.wall_ms = self.wall_ms.saturating_add(1);
                self.counter = 0;
            }
        }
    }
}

/// Zeitpunkte vor 1970 gibt es für die HLC nicht.
fn wall(now: Timestamp) -> u64 {
    u64::try_from(now.as_millis()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(ms: i64) -> Timestamp {
        Timestamp::from_millis(ms)
    }

    fn hlc(wall_ms: u64, counter: u32, device: DeviceId) -> Hlc {
        Hlc {
            wall_ms,
            counter,
            device,
        }
    }

    #[test]
    fn orders_by_wall_then_counter_then_device() {
        let (a, b) = {
            let x = DeviceId::new();
            let y = DeviceId::new();
            if x < y {
                (x, y)
            } else {
                (y, x)
            }
        };
        assert!(hlc(1, 9, b) < hlc(2, 0, a));
        assert!(hlc(2, 0, b) < hlc(2, 1, a));
        assert!(hlc(2, 1, a) < hlc(2, 1, b), "höhere Geräte-ID gewinnt");
    }

    #[test]
    fn tick_follows_the_wall_clock() {
        let mut state = HlcState::new(DeviceId::new());
        assert_eq!(state.tick(ts(100)).wall_ms, 100);
        let second = state.tick(ts(100));
        assert_eq!((second.wall_ms, second.counter), (100, 1));
        let third = state.tick(ts(250));
        assert_eq!((third.wall_ms, third.counter), (250, 0));
    }

    #[test]
    fn tick_stays_monotonic_when_clock_goes_backwards() {
        let mut state = HlcState::new(DeviceId::new());
        let before = state.tick(ts(1_000));
        let after = state.tick(ts(10));
        assert!(after > before);
        assert_eq!((after.wall_ms, after.counter), (1_000, 1));
    }

    #[test]
    fn negative_time_counts_as_epoch() {
        let mut state = HlcState::new(DeviceId::new());
        let first = state.tick(ts(-5));
        assert_eq!((first.wall_ms, first.counter), (0, 1));
    }

    #[test]
    fn counter_overflow_advances_wall() {
        let device = DeviceId::new();
        let mut state = HlcState::restore(hlc(7, u32::MAX, device));
        let next = state.tick(ts(0));
        assert_eq!((next.wall_ms, next.counter), (8, 0));
    }

    #[test]
    fn observe_moves_past_remote_values() {
        let mut state = HlcState::new(DeviceId::new());
        state.tick(ts(100));
        let remote = hlc(500, 3, DeviceId::new());
        state.observe(&remote, ts(120)).unwrap();
        let next = state.tick(ts(130));
        assert!(next > remote);
        assert_eq!((next.wall_ms, next.counter), (500, 4));
    }

    #[test]
    fn observe_ignores_older_remote_values() {
        let mut state = HlcState::new(DeviceId::new());
        let own = state.tick(ts(1_000));
        state
            .observe(&hlc(10, 0, DeviceId::new()), ts(1_000))
            .unwrap();
        assert_eq!(state.last(), own);
    }

    #[test]
    fn observe_rejects_values_far_in_the_future() {
        let mut state = HlcState::new(DeviceId::new());
        let own = state.tick(ts(1_000));
        let skewed = hlc(1_001 + MAX_CLOCK_DRIFT_MS, 0, DeviceId::new());
        let err = state.observe(&skewed, ts(1_000)).unwrap_err();
        assert_eq!(err.remote, skewed);
        assert!(err.to_string().contains("ahead"));
        assert_eq!(state.last(), own, "Zustand unverändert");
        let ok = hlc(1_000 + MAX_CLOCK_DRIFT_MS, 0, DeviceId::new());
        assert!(state.observe(&ok, ts(1_000)).is_ok(), "Grenze inklusive");
    }

    #[test]
    fn restore_continues_after_persisted_value() {
        let mut state = HlcState::new(DeviceId::new());
        state.tick(ts(5));
        let last = state.tick(ts(5));
        let mut restored = HlcState::restore(state.last());
        assert_eq!(restored.device(), state.device());
        assert!(restored.tick(ts(0)) > last);
    }

    #[test]
    fn bytes_and_string_roundtrip() {
        let value = hlc(1_760_000_000_123, 42, DeviceId::new());
        assert_eq!(Hlc::from_bytes(&value.to_bytes()).unwrap(), value);
        let s = value.to_string();
        assert_eq!(s.len(), Hlc::STRING_LEN);
        assert!(s.starts_with("00000199c82cc07b0000002a"), "{s}");
        assert_eq!(Hlc::parse(&s).unwrap(), value);
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(Hlc::from_bytes(&[0; 27]).is_err());
        assert!(Hlc::parse("").is_err());
        let valid = hlc(1, 2, DeviceId::new()).to_string();
        assert!(
            Hlc::parse(&valid.to_uppercase()).is_err(),
            "nur Kleinbuchstaben"
        );
        assert!(Hlc::parse(&valid.replacen('0', "g", 1)).is_err());
        assert!(Hlc::parse(&valid[1..]).is_err());
    }

    #[test]
    fn serde_uses_string_for_json() {
        let value = hlc(3, 4, DeviceId::new());
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, format!("\"{value}\""));
        assert_eq!(serde_json::from_str::<Hlc>(&json).unwrap(), value);
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod properties {
        use proptest::prelude::*;

        use super::*;

        #[derive(Debug, Clone)]
        enum Event {
            Tick(i64),
            Observe(u64, u32, i64),
        }

        fn event() -> impl Strategy<Value = Event> {
            prop_oneof![
                3 => (-1_000i64..10_000).prop_map(Event::Tick),
                1 => (0u64..12_000, 0u32..5, -1_000i64..10_000)
                    .prop_map(|(w, c, now)| Event::Observe(w, c, now)),
            ]
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(1_000))]

            /// Jeder `tick` liegt über allen vorher vergebenen und beobachteten Werten – egal,
            /// wie die Uhr springt.
            #[test]
            fn tick_is_strictly_monotonic(events in prop::collection::vec(event(), 1..100)) {
                let mut state = HlcState::new(DeviceId::new());
                let mut highest: Option<(u64, u32)> = None;
                for event in events {
                    match event {
                        Event::Tick(now) => {
                            let value = state.tick(Timestamp::from_millis(now));
                            let key = (value.wall_ms, value.counter);
                            prop_assert!(highest.is_none_or(|h| key > h), "{key:?} <= {highest:?}");
                            highest = Some(key);
                        }
                        Event::Observe(wall_ms, counter, now) => {
                            let remote = Hlc { wall_ms, counter, device: DeviceId::new() };
                            if state.observe(&remote, Timestamp::from_millis(now)).is_ok() {
                                highest = highest.max(Some((wall_ms, counter)));
                            }
                        }
                    }
                }
            }

            #[test]
            fn byte_and_string_order_match_hlc_order(
                a in (any::<u64>(), any::<u32>()),
                b in (any::<u64>(), any::<u32>()),
            ) {
                let device = DeviceId::new();
                let x = Hlc { wall_ms: a.0, counter: a.1, device };
                let y = Hlc { wall_ms: b.0, counter: b.1, device };
                prop_assert_eq!(x.cmp(&y), x.to_bytes().cmp(&y.to_bytes()));
                prop_assert_eq!(x.cmp(&y), x.to_string().cmp(&y.to_string()));
                prop_assert_eq!(Hlc::parse(&x.to_string()).unwrap(), x);
            }
        }
    }
}
