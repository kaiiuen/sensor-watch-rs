//! Bounded-wait helpers.
//!
//! Every hardware polling loop must be bounded so a hung peripheral can never
//! hang the CPU. These helpers wait for a condition with a timeout and return
//! whether the condition was met.

/// The default number of iterations before a wait times out.
///
/// This is a generous bound: at ~4 MHz, a few thousand iterations is still
/// well under a millisecond, while a genuinely hung bus will time out quickly.
pub const WAIT_TIMEOUT: u32 = 100_000;

/// A standard error type for hardware operations.
///
/// Every driver returns this so failures are reported uniformly and can be
/// handled (or recorded as a fault) instead of being silently ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The operation timed out (a peripheral did not respond).
    Timeout,
    /// The peripheral reported a bus or protocol error.
    Bus,
    /// An invalid argument was provided.
    InvalidArgument,
}

/// Waits until `cond` returns true, or until the timeout elapses.
///
/// Returns `Ok(())` if the condition was met, `Err(Error::Timeout)` otherwise.
#[inline]
pub fn wait_until(mut cond: impl FnMut() -> bool) -> Result<(), Error> {
    for _ in 0..WAIT_TIMEOUT {
        if cond() {
            return Ok(());
        }
    }
    Err(Error::Timeout)
}
