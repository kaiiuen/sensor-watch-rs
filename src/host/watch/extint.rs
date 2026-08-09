//! Host button-pin constants, matching `src/watch/extint.rs`.
//!
//! Faces / framework refer to the button pins as `crate::watch::extint::BTN_*`.
//! Keep the numeric values in lockstep with the real ARM module; on host they are
//! only used to map a `Pin` back to a
//! [`Button`](sensor_watch_core::mock_hw::Button) when reading button levels.

pub use super::gpio::Pin;

/// The pins used by the three buttons (OSO-SWAT-A1-05 board).
pub const BTN_ALARM: Pin = Pin(0, 2); // PA02
pub const BTN_LIGHT: Pin = Pin(0, 22); // PA22
pub const BTN_MODE: Pin = Pin(0, 23); // PA23
