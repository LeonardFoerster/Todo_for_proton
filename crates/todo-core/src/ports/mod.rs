//! Ports (Traits), über die der Core mit der Außenwelt spricht.
//!
//! Implementierungen liegen ausschließlich in Adapter-Crates.

mod clock;

pub use clock::Clock;
#[cfg(any(test, feature = "test-support"))]
pub use clock::{FixedClock, SteppingClock};
