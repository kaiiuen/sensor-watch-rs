//! Pure logic for the Sensor-Watch firmware.
//!
//! This crate contains the parts of the firmware that are pure computation and
//! have no hardware dependency: the packed date/time type, the settings
//! bit-packing, and the date/time utility functions. Because it has no
//! hardware dependency, it can be unit-tested on the host, giving us proof
//! that the foundation logic is correct.

#![no_std]

#[cfg(not(target_arch = "arm"))]
extern crate alloc;

pub mod background_tasks;
pub mod board;
pub mod datetime;
pub mod ecc;
pub mod event_log;
pub mod identity;
// The hardware seam: the `Hw` trait, reference mock, and the reusable
// `Event`/`Button`/`Indicator` types used by face logic.
#[cfg(not(target_arch = "arm"))]
pub mod mock_hw;
pub mod optical;
pub mod rtc_calibration;
pub mod safety;
pub mod settings;
pub mod transfer;
#[cfg(not(target_arch = "arm"))]
pub mod uf2;
pub mod utility;

// Proof-of-concept: runs a verbatim copy of the firmware `simple_clock` face
// against the mock `Hw` on the host (see the module docs for the extension
// plan).
#[cfg(not(target_arch = "arm"))]
pub mod hostsim;
