//! Host integrity-check model for Diagnostics.
//!
//! A host process has no firmware flash image at the SAM L22 text addresses.
//! The host model therefore reports the integrity check as unavailable/simulated
//! rather than reading arbitrary process memory or claiming a physical PASS.

/// Host model: no physical firmware image is checked.
pub fn check_firmware_integrity() -> bool {
    false
}
