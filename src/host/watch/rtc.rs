//! Host RTC shim: reads the simulated clock through the `Hw` seam.
//!
//! Faces use `crate::watch::rtc::{self, DateTime}` and `rtc::get_date_time()`.
//! `DateTime` is the shared `core` type (field-for-field identical to
//! `src/watch/rtc.rs`'s), and `get_date_time()` forwards to the installed mock's
//! simulated RTC.

pub use sensor_watch_core::datetime::{DateTime, WATCH_RTC_REFERENCE_YEAR};

use super::seam;

/// Reads the current date/time from the installed mock.
pub fn get_date_time() -> DateTime {
    seam::hw().get_date_time()
}

/// Host: setting the RTC is not (yet) represented on the `Hw` seam; calls are
/// accepted and ignored on host. Add `set_date_time` to the seam when a face
/// needs it (currently none in the host-compilable subset does).
pub fn set_date_time(_date_time: DateTime) {}
