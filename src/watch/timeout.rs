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

/// Waits until `cond` returns true, or until the timeout elapses.
///
/// Returns `true` if the condition was met, `false` if it timed out.
#[inline]
pub fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..WAIT_TIMEOUT {
        if cond() {
            return true;
        }
    }
    false
}
