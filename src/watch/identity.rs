//! SAM L22 device identity HAL.
//!
//! Address and ordering are from Microchip SAM L22 Family Data Sheet
//! DS60001479, "Serial Number": `0x0080_A00C..=0x0080_A01B`. This is a
//! read-only signature row. UID/INFO_UF2 must never be used as authentication.

use sensor_watch_core::identity::{self, IdentityConfidence, IdentitySource, UID_LEN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub uid: [u8; UID_LEN],
    pub source: IdentitySource,
    pub board: Option<&'static str>,
    pub revision: Option<&'static str>,
    pub confidence: IdentityConfidence,
}

/// Read the SAM L22 serial number without changing clocks or peripheral state.
pub fn read() -> DeviceIdentity {
    let base = identity::UID_BASE_ADDRESS as *const u32;
    let words = unsafe {
        [
            core::ptr::read_volatile(base),
            core::ptr::read_volatile(base.add(1)),
            core::ptr::read_volatile(base.add(2)),
            core::ptr::read_volatile(base.add(3)),
        ]
    };
    DeviceIdentity {
        uid: identity::decode_uid(words),
        source: IdentitySource::SamL22SignatureRow,
        board: None,
        revision: None,
        confidence: IdentityConfidence::Unknown,
    }
}
