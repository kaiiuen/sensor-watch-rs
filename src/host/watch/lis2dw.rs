//! Host accelerometer stub for Diagnostics.
//!
//! No physical LIS2DW is attached to `MockHw`; host tests therefore model the
//! sensor as absent. This keeps the real face's control flow testable without
//! claiming that an accelerometer self-test passed.

/// A raw three-axis accelerometer reading, matching the firmware API.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

/// Host model: no physical accelerometer is present.
pub fn begin() -> bool {
    false
}

/// Returns the neutral sample used only if a caller requests a reading after
/// the simulated device was reported absent.
pub fn get_raw_reading() -> Reading {
    Reading::default()
}
