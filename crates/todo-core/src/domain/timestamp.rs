//! Zeitpunkt als Unix-Millisekunden (UTC), bewusst ohne Kalender-Abhängigkeit.

use serde::{Deserialize, Serialize};

/// Zeitpunkt in Millisekunden seit 1970-01-01T00:00:00Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Unix-Epoche (`1970-01-01T00:00:00Z`).
    pub const EPOCH: Self = Self(0);

    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// Dieser Zeitpunkt plus `millis` (sättigend, kein Überlauf).
    #[must_use]
    pub const fn plus_millis(self, millis: i64) -> Self {
        Self(self.0.saturating_add(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_chronologically() {
        let earlier = Timestamp::from_millis(-1);
        let later = Timestamp::from_millis(1_700_000_000_000);
        assert!(earlier < Timestamp::EPOCH);
        assert!(Timestamp::EPOCH < later);
        assert_eq!(later.max(earlier), later);
    }

    #[test]
    fn plus_millis_saturates() {
        assert_eq!(Timestamp::from_millis(5).plus_millis(10).as_millis(), 15);
        assert_eq!(
            Timestamp::from_millis(i64::MAX).plus_millis(1).as_millis(),
            i64::MAX
        );
    }

    #[test]
    fn serializes_as_number() {
        let ts = Timestamp::from_millis(1_700_000_000_123);
        assert_eq!(serde_json::to_string(&ts).unwrap(), "1700000000123");
        assert_eq!(
            serde_json::from_str::<Timestamp>("1700000000123").unwrap(),
            ts
        );
    }
}
