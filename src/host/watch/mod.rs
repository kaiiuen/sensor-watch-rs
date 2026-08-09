//! Host implementation of the `watch` HAL that dispatches through the `Hw` seam.
//!
//! This is the host build's replacement for the ARM `src/watch/mod.rs`. The face
//! code (`movement/*.rs`) calls `crate::watch::*` free functions with the SAME
//! names/signatures as the real HAL, but here they forward to the installed
//! [`Hw`] backend (a [`MockHw`] in tests) via `crate::watch::seam::hw()`.
//!
//! Only the subset of the real HAL that the migrated faces use is provided here,
//! growing one method at a time as faces are ported (keep it minimal, mirroring
//! the `core::mock_hw::Hw` trait growth). It reuses the `core` types (`DateTime`,
//! button pins) rather than duplicating them.

pub mod adc;
pub mod buzzer;
pub mod extint;
pub mod gpio;
pub mod rtc;
pub mod slcd;
pub mod utility;

/// The `Hw`-seam plumbing: [`install_hw`](seam::install_hw) + [`hw`](seam::hw).
pub mod seam;

// Re-export the shared date/time type so callers (and real face code) can use it
// uniformly across target and host.
pub use sensor_watch_core::datetime::DateTime;

