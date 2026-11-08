//! Time for the procedures that drive the PHY and SCDC: a monotonic clock, a delay, and the
//! [`Timestamp`] the clock reads.
//!
//! Two traits, not one: the clock is read where nothing waits (stamping an event, checking a
//! deadline), and a platform may take them from different peripherals, a free-running counter
//! and a timer. A procedure that needs both bounds on both; a test double may implement both
//! on one type, advancing its clock by each delay.
//!
//! # Over embedded-hal
//!
//! [`Delay`] is the stack's own trait rather than a bound on embedded-hal's `DelayNs`, so that
//! the async companion can have a `Send` twin. An implementation over a `DelayNs` is a few
//! lines: convert the duration to microseconds, rounding up, and split values beyond `u32`.
//!
//! ```
//! use core::time::Duration;
//! use embedded_hal::delay::DelayNs;
//! use hdmi_hal::time::Delay;
//!
//! struct HalDelay<D>(D);
//!
//! impl<D: DelayNs> Delay for HalDelay<D> {
//!     fn delay(&mut self, duration: Duration) {
//!         let mut us = duration.as_nanos().div_ceil(1_000);
//!         while us > 0 {
//!             let step = us.min(u128::from(u32::MAX));
//!             self.0.delay_us(step as u32);
//!             us -= step;
//!         }
//!     }
//! }
//! ```

use core::time::Duration;

/// A point in time: microseconds from an arbitrary monotonic epoch.
///
/// Only differences between timestamps from one [`Clock`] mean anything. A type of its own
/// rather than a bare `u64`, so microseconds are not mixed with other counts; `u64`
/// microseconds do not wrap in any lifetime a device has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64);

impl Timestamp {
    /// The timestamp `micros` microseconds after the epoch.
    pub const fn from_micros(micros: u64) -> Self {
        Self(micros)
    }

    /// Microseconds since the epoch.
    pub const fn as_micros(self) -> u64 {
        self.0
    }

    /// The time from `earlier` to `self`, or `None` if `earlier` is later.
    pub const fn checked_duration_since(self, earlier: Timestamp) -> Option<Duration> {
        match self.0.checked_sub(earlier.0) {
            Some(micros) => Some(Duration::from_micros(micros)),
            None => None,
        }
    }

    /// The time from `earlier` to `self`, or zero if `earlier` is later.
    pub const fn saturating_duration_since(self, earlier: Timestamp) -> Duration {
        Duration::from_micros(self.0.saturating_sub(earlier.0))
    }

    /// The timestamp `duration` after `self`, or `None` on overflow.
    ///
    /// A duration that is not a whole number of microseconds is rounded up, so a deadline
    /// computed with it is never earlier than asked.
    pub const fn checked_add(self, duration: Duration) -> Option<Timestamp> {
        let micros = duration.as_nanos().div_ceil(1_000);
        if micros > u64::MAX as u128 {
            return None;
        }
        match self.0.checked_add(micros as u64) {
            Some(micros) => Some(Self(micros)),
            None => None,
        }
    }
}

/// A monotonic clock.
///
/// Neither fallible nor `async`: reading a counter does not wait. Takes `&self`, so one clock
/// can be shared; an implementation with state uses interior mutability.
///
/// Resolution is the implementation's, and it documents it. The procedures' shortest times
/// are 1 ms, so a clock of 1 ms or finer keeps their timing as configured; a coarser one is
/// still correct, but the times they report are as coarse as the clock.
pub trait Clock {
    /// The current time. Never earlier than any time this clock returned before.
    fn now(&self) -> Timestamp;
}

/// A delay.
///
/// Infallible: a timer that cannot wait leaves the caller nothing to recover with.
pub trait Delay {
    /// Wait at least `duration`, rounding up to the implementation's own tick; never less,
    /// so a minimum wait holds whatever the timer.
    fn delay(&mut self, duration: Duration);
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    #[test]
    fn timestamp_round_trips_micros() {
        const T: Timestamp = Timestamp::from_micros(1_500);
        assert_eq!(T.as_micros(), 1_500);
        assert_eq!(Timestamp::default().as_micros(), 0);
    }

    #[test]
    fn duration_since_is_checked_or_saturating() {
        let a = Timestamp::from_micros(1_000);
        let b = Timestamp::from_micros(3_500);
        assert_eq!(
            b.checked_duration_since(a),
            Some(Duration::from_micros(2_500))
        );
        assert_eq!(a.checked_duration_since(b), None);
        assert_eq!(a.checked_duration_since(a), Some(Duration::ZERO));
        assert_eq!(b.saturating_duration_since(a), Duration::from_micros(2_500));
        assert_eq!(a.saturating_duration_since(b), Duration::ZERO);
    }

    #[test]
    fn checked_add_rounds_up_to_the_microsecond() {
        let t = Timestamp::from_micros(10);
        assert_eq!(
            t.checked_add(Duration::from_millis(1)),
            Some(Timestamp::from_micros(1_010))
        );
        assert_eq!(
            t.checked_add(Duration::from_nanos(1)),
            Some(Timestamp::from_micros(11))
        );
        assert_eq!(t.checked_add(Duration::ZERO), Some(t));
    }

    #[test]
    fn checked_add_reports_overflow() {
        let t = Timestamp::from_micros(u64::MAX - 1);
        assert_eq!(
            t.checked_add(Duration::from_micros(1)),
            Some(Timestamp::from_micros(u64::MAX))
        );
        assert_eq!(t.checked_add(Duration::from_micros(2)), None);
        assert_eq!(Timestamp::default().checked_add(Duration::MAX), None);
    }

    #[test]
    fn timestamps_order_by_time() {
        assert!(Timestamp::from_micros(1) < Timestamp::from_micros(2));
    }

    /// A clock and delay on one type, the delay advancing the clock, as a test double does.
    struct SimTime {
        now: Cell<u64>,
    }

    impl Clock for SimTime {
        fn now(&self) -> Timestamp {
            Timestamp::from_micros(self.now.get())
        }
    }

    impl Delay for SimTime {
        fn delay(&mut self, duration: Duration) {
            let us = duration.as_nanos().div_ceil(1_000) as u64;
            self.now.set(self.now.get() + us);
        }
    }

    fn wait_until<T: Clock + Delay>(time: &mut T, deadline: Timestamp, poll: Duration) -> u32 {
        let mut polls = 0;
        while time.now() < deadline {
            time.delay(poll);
            polls += 1;
        }
        polls
    }

    #[test]
    fn clock_and_delay_compose_on_one_type() {
        let mut time = SimTime { now: Cell::new(0) };
        let start = time.now();
        let deadline = start.checked_add(Duration::from_millis(5)).unwrap();
        assert_eq!(wait_until(&mut time, deadline, Duration::from_millis(1)), 5);
        assert_eq!(
            time.now().checked_duration_since(start),
            Some(Duration::from_millis(5))
        );
    }
}
