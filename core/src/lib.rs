//! Pure logic for the Sensor-Watch firmware.
//!
//! This crate contains the parts of the firmware that are pure computation and
//! have no hardware dependency: the packed date/time type, the settings
//! bit-packing, and the date/time utility functions. Because it has no
//! hardware dependency, it can be unit-tested on the host, giving us proof
//! that the foundation logic is correct.

#![no_std]

extern crate alloc;

pub mod datetime;
pub mod ecc;
pub mod settings;
pub mod uf2;
pub mod utility;
