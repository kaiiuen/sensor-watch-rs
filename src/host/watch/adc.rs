//! Host ADC shim: reports the simulated VCC through the `Hw` seam.
//!
//! The real `src/watch/adc.rs` reads the SAM L22 ADC registers. On host,
//! `get_vcc_voltage()` returns the value seeded on the installed mock, and
//! `enable_adc`/`disable_adc` are no-ops.

use super::seam;

/// No-op on host (the mock is always "enabled").
pub fn enable_adc() {}

/// Returns the simulated VCC in millivolts (e.g. 3000).
pub fn get_vcc_voltage() -> u16 {
    seam::hw().get_vcc_voltage()
}

/// No-op on host.
pub fn disable_adc() {}
