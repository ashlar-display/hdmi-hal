//! A flag that asks a procedure to stop: [`Cancel`].
//!
//! A procedure that drives the PHY and SCDC over many calls (link training, a TMDS bring-up)
//! reads the flag between its calls and, once it is set, runs its own recovery and ends with
//! a typed result saying so. Stopping a procedure by dropping its future instead leaves the
//! PHY and the sink in a state nothing records: a call may have been half done. So an
//! interrupt handler or another task sets the flag, and the procedure stops at its next
//! check.
//!
//! [`AtomicBool`] implements it, so a plain load and store on any target the stack builds
//! for; references to a flag implement it too, so a driver can hold `&flag` whatever owns
//! the flag (a `static`, an `Arc`). [`NoCancel`] never cancels.

use core::sync::atomic::{AtomicBool, Ordering};

/// A flag the caller sets to ask a procedure to stop.
///
/// A procedure reads it between its calls, never in the middle of one, and never clears
/// it: the caller resets it before the next procedure, or that one stops at its first
/// check.
pub trait Cancel {
    /// Whether the caller has asked the procedure to stop.
    fn is_cancelled(&self) -> bool;
}

/// Never cancels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct NoCancel;

impl Cancel for NoCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// A flag another task or an interrupt handler sets.
impl Cancel for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Relaxed)
    }
}

/// The flag behind a reference, so a driver can hold `&flag` and the caller keep the flag.
impl<T: Cancel + ?Sized> Cancel for &T {
    fn is_cancelled(&self) -> bool {
        (**self).is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stopped(cancel: impl Cancel) -> bool {
        cancel.is_cancelled()
    }

    #[test]
    fn a_flag_cancels_once_set_and_no_cancel_never_does() {
        let flag = AtomicBool::new(false);
        assert!(!stopped(&flag));
        flag.store(true, Ordering::Relaxed);
        assert!(stopped(&flag));
        let dynamic: &dyn Cancel = &flag;
        assert!(stopped(dynamic));
        assert!(!stopped(NoCancel));
    }
}
