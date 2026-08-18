//! Host memory reporting for Diagnostics.
//!
//! Linker symbols used by the firmware do not exist in a host process. These
//! values describe the host simulation's fixed model, not physical watch RAM.

/// Simulated firmware RAM capacity used by the diagnostics display.
pub fn total_ram() -> u32 {
    32 * 1024
}

/// Static RAM used by the host model. It is intentionally deterministic.
pub fn static_ram_used() -> u32 {
    0
}

/// Percentage of the simulated RAM model in use.
pub fn ram_used_percent() -> u8 {
    0
}
