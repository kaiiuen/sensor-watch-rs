//! Host reuse of the firmware ECC implementation.
//!
//! ECC is pure arithmetic, so Diagnostics can exercise the same implementation
//! on the host without a hardware-specific shim.

#[path = "../../watch/ecc.rs"]
pub mod real;
pub use real::{decode, encode};
