//! Host SLCD shim: routes LCD writes through the `Hw` seam.
//!
//! Mirrors the subset of `src/watch/slcd.rs` that faces call, forwarding to the
//! installed [`MockHw`]. Indicator segments are the shared
//! `sensor_watch_core::mock_hw::Indicator` type.

use super::seam;
// Re-exported so real faces (`use crate::watch::slcd::Indicator;`) resolve on host.
pub use sensor_watch_core::mock_hw::Indicator;

/// Displays a string at digit position 0-9. A space clears that digit.
pub fn display_string(string: &str, position: u8) {
    seam::hw().display_string(string, position);
}

/// Displays a single character (via the string path, so the mock records it).
pub fn display_character(character: u8, position: u8) {
    let buf = [character];
    let s = core::str::from_utf8(&buf).unwrap_or(" ");
    seam::hw().display_string(s, position);
}

/// Turns the colon on.
pub fn set_colon() {
    seam::hw().set_colon();
}

/// Turns the colon off.
pub fn clear_colon() {
    seam::hw().clear_colon();
}

/// Sets an indicator segment.
pub fn set_indicator(indicator: Indicator) {
    seam::hw().set_indicator(indicator);
}

/// Clears an indicator segment.
pub fn clear_indicator(indicator: Indicator) {
    seam::hw().clear_indicator(indicator);
}

/// Sets a raw (com, seg) pixel.
pub fn set_pixel(com: u8, seg: u8) {
    seam::hw().set_pixel(com, seg);
}

/// Clears a raw (com, seg) pixel.
pub fn clear_pixel(com: u8, seg: u8) {
    seam::hw().clear_pixel(com, seg);
}

/// True while the tick animation is running.
pub fn tick_animation_is_running() -> bool {
    seam::hw().tick_animation_is_running()
}

/// Stops the tick animation.
pub fn stop_tick_animation() {
    seam::hw().stop_tick_animation();
}
