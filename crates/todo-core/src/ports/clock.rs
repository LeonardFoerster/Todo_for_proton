//! Zeitquelle als Port, damit Domain und Use-Cases deterministisch testbar bleiben.

use crate::domain::Timestamp;

/// Liefert die aktuelle Zeit. Nativ und im Browser jeweils vom Adapter implementiert.
pub trait Clock {
    fn now(&self) -> Timestamp;
}

#[cfg(any(test, feature = "test-support"))]
pub use test_support::{FixedClock, SteppingClock};

#[cfg(any(test, feature = "test-support"))]
mod test_support {
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::Clock;
    use crate::domain::Timestamp;

    /// Liefert immer denselben Zeitpunkt.
    #[derive(Debug, Clone, Copy)]
    pub struct FixedClock(pub Timestamp);

    impl Clock for FixedClock {
        fn now(&self) -> Timestamp {
            self.0
        }
    }

    /// Startet bei `start` und rückt bei jedem Aufruf von `now()` um `step_millis` vor.
    #[derive(Debug)]
    pub struct SteppingClock {
        next: AtomicI64,
        step_millis: i64,
    }

    impl SteppingClock {
        #[must_use]
        pub fn new(start: Timestamp, step_millis: i64) -> Self {
            Self {
                next: AtomicI64::new(start.as_millis()),
                step_millis,
            }
        }
    }

    impl Clock for SteppingClock {
        fn now(&self) -> Timestamp {
            Timestamp::from_millis(self.next.fetch_add(self.step_millis, Ordering::Relaxed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_constant() {
        let clock = FixedClock(Timestamp::from_millis(42));
        assert_eq!(clock.now(), clock.now());
        assert_eq!(clock.now().as_millis(), 42);
    }

    #[test]
    fn stepping_clock_advances_per_call() {
        let clock = SteppingClock::new(Timestamp::from_millis(1_000), 10);
        let seen: Vec<i64> = (0..3).map(|_| clock.now().as_millis()).collect();
        assert_eq!(seen, [1_000, 1_010, 1_020]);
    }

    #[test]
    fn stepping_clock_is_strictly_increasing() {
        let clock = SteppingClock::new(Timestamp::EPOCH, 1);
        let a = clock.now();
        let b = clock.now();
        assert!(a < b);
    }
}
