//! Host utility shim: re-export the shared, pure date/time helpers from `core`.
//!
//! The real `src/watch/utility.rs` is pure logic with no hardware dependency, but
//! it lives in the ARM HAL tree. On host we reuse the proven `sensor_watch_core`
//! copy (identical, already unit-tested) so faces like `simple_clock` can call
//! `crate::watch::utility::get_weekday` unchanged.

pub use sensor_watch_core::utility::{convert_to_12_hour, get_weekday};
