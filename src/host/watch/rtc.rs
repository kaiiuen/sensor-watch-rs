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
    seam::with_current_hw(|hw| hw.get_date_time())
}

/// Host: sets the RTC date/time by forwarding to the `Hw::set_date_time` hook
/// (the mock records it as `now`).
pub fn set_date_time(date_time: DateTime) {
    seam::with_current_hw(|hw| hw.set_date_time(date_time));
}

/// Host: writes the frequency-correction register via the `Hw` seam (no-op).
pub fn freqcorr_write(value: i16, sign: i16) {
    seam::with_current_hw(|hw| hw.freqcorr_write(value, sign));
}

/// Host: reads the frequency-correction register via the `Hw` seam (0).
pub fn freqcorr_read() -> i16 {
    seam::with_current_hw(|hw| hw.freqcorr_read())
}
