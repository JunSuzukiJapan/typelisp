//! `std::time::Instant`'s part the runtime uses, for a host whose clock is
//! read through a function of its own (`now`, in the module that `use`s
//! this).

use core::time::Duration;

/// A point on a clock that never goes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(pub(super) Duration);

impl Instant {
    pub fn now() -> Instant {
        Instant(super::now())
    }

    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.0.saturating_sub(earlier.0)
    }

    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.saturating_duration_since(earlier)
    }

    pub fn elapsed(&self) -> Duration {
        Instant::now().saturating_duration_since(*self)
    }
}

impl core::ops::Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, d: Duration) -> Instant {
        Instant(self.0 + d)
    }
}
