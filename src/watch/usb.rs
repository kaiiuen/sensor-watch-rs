//! Developer-only SAM L22 USB full-speed enumeration feasibility layer.
//!
//! This module deliberately stops at the USB device/control boundary. The PAC
//! exposes the USB device registers but omits the descriptor-bank and packet
//! SRAM types needed to safely service EP0 and CDC bulk transfers. The raw
//! layout below is therefore an audited address/shape contract only; it is not
//! used to invent packet I/O. There is no CDC shell, read path, or write path.
//!
//! The descriptor values mirror the reference TinyUSB application:
//! 0x1209:0x2151, full-speed, 64-byte EP0, and CDC endpoint addresses 0x81,
//! 0x02, and 0x82.

#![cfg(any(feature = "usb-enum", feature = "usb-cdc"))]

pub const MAX_PACKET_SIZE: usize = 64;
pub const NOTIFICATION_ENDPOINT: u8 = 0x81;
pub const RX_ENDPOINT: u8 = 0x02;
pub const TX_ENDPOINT: u8 = 0x82;
pub const USB_DPRAM_ORIGIN: usize = 0x2000_0000;
pub const USB_DPRAM_SIZE: usize = 512;
pub const USB_DESCRIPTOR_BANK_SIZE: usize = 12;

/// SAM L22 USB device register offsets from the reference component header.
pub const USB_REGISTER_BASE: usize = 0x4100_0000;
pub const USB_CTRLA_OFFSET: usize = 0x000;
pub const USB_DESCADD_OFFSET: usize = 0x024;
pub const USB_PADCAL_OFFSET: usize = 0x028;
pub const USB_ENDPOINTS_OFFSET: usize = 0x100;
pub const USB_DESCRIPTOR_BANKS: usize = 16;

/// The first request a real implementation must handle after reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlState {
    Default,
    Addressed,
    Configured,
}

/// A small, target-independent representation of the standard control state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlMachine {
    state: ControlState,
}

impl ControlMachine {
    pub const fn new() -> Self {
        Self {
            state: ControlState::Default,
        }
    }

    pub const fn state(self) -> ControlState {
        self.state
    }

    pub fn set_address(&mut self, address: u8) -> bool {
        if address <= 127 && self.state != ControlState::Configured {
            self.state = if address == 0 {
                ControlState::Default
            } else {
                ControlState::Addressed
            };
            true
        } else {
            false
        }
    }

    pub fn set_configured(&mut self, configured: bool) -> bool {
        match (self.state, configured) {
            (ControlState::Addressed, true) => {
                self.state = ControlState::Configured;
                true
            }
            (ControlState::Configured, false) => {
                self.state = ControlState::Addressed;
                true
            }
            _ => false,
        }
    }
}

/// USB device descriptor from the reference TinyUSB application.
pub const DEVICE_DESCRIPTOR: [u8; 18] = [
    18,
    1,
    0x00,
    0x02,
    0xef,
    0x02,
    0x01,
    MAX_PACKET_SIZE as u8,
    0x09,
    0x12,
    0x51,
    0x21,
    0x00,
    0x01,
    1,
    2,
    3,
    1,
];

/// Full-speed configuration descriptor from the reference TinyUSB layout.
///
/// It is retained for review and host validation. It is not installed into
/// the controller until the missing SRAM transfer contract is implemented.
pub const CONFIGURATION_DESCRIPTOR: [u8; 75] = [
    9,
    2,
    75,
    0,
    2,
    1,
    0,
    0xA0,
    50,
    8,
    0x0B,
    0,
    2,
    0x02,
    0x02,
    0x01,
    0,
    9,
    4,
    0,
    0,
    1,
    0x02,
    0x02,
    0x01,
    0,
    5,
    0x24,
    0x00,
    0x10,
    0x01,
    5,
    0x24,
    0x01,
    0x00,
    0x01,
    4,
    0x24,
    0x02,
    0x02,
    5,
    0x24,
    0x06,
    0,
    1,
    7,
    5,
    NOTIFICATION_ENDPOINT,
    0x03,
    8,
    0,
    16,
    9,
    4,
    1,
    0,
    2,
    0x0A,
    0,
    0,
    0,
    7,
    5,
    RX_ENDPOINT,
    0x02,
    64,
    0,
    0,
    7,
    5,
    TX_ENDPOINT,
    0x02,
    64,
    0,
    0,
];

/// A raw descriptor-bank shape matching the SAM L22 reference header.
///
/// This is kept private because the PAC does not expose a safe packet-buffer
/// API and this layer must not expose an unsafe transfer surface by accident.
#[repr(C)]
struct RawDescriptorBank {
    addr: u32,
    pcksize: u32,
    extreg: u16,
    status: u8,
    reserved: u8,
}

const _: () = assert!(core::mem::size_of::<RawDescriptorBank>() == USB_DESCRIPTOR_BANK_SIZE);
const _: () = assert!(MAX_PACKET_SIZE <= 64);
const _: () = assert!(NOTIFICATION_ENDPOINT & 0x80 != 0);
const _: () = assert!(RX_ENDPOINT & 0x80 == 0);
const _: () = assert!(TX_ENDPOINT & 0x80 != 0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbError {
    /// Descriptor/packet SRAM transfer support is not present in this PAC layer.
    MissingPacketSram,
}

/// Fail closed until EP0 packet memory and USB clock/power sequencing are wired.
pub fn init() -> Result<(), UsbError> {
    Err(UsbError::MissingPacketSram)
}

pub fn poll() -> Result<(), UsbError> {
    Err(UsbError::MissingPacketSram)
}

pub fn write(_bytes: &[u8]) -> Result<usize, UsbError> {
    Err(UsbError::MissingPacketSram)
}

pub fn read() -> Result<Option<u8>, UsbError> {
    Err(UsbError::MissingPacketSram)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_descriptors_and_endpoints_are_consistent() {
        assert_eq!(&DEVICE_DESCRIPTOR[8..12], &[0x09, 0x12, 0x51, 0x21]);
        assert_eq!(
            u16::from_le_bytes([CONFIGURATION_DESCRIPTOR[2], CONFIGURATION_DESCRIPTOR[3]]),
            75
        );
        assert_eq!(CONFIGURATION_DESCRIPTOR.len(), 75);
        assert!(
            CONFIGURATION_DESCRIPTOR
                .windows(2)
                .any(|w| w == [5, RX_ENDPOINT])
        );
        assert!(
            CONFIGURATION_DESCRIPTOR
                .windows(2)
                .any(|w| w == [5, TX_ENDPOINT])
        );
    }

    #[test]
    fn standard_control_state_is_fail_closed() {
        let mut machine = ControlMachine::new();
        assert_eq!(machine.state(), ControlState::Default);
        assert!(!machine.set_configured(true));
        assert!(machine.set_address(7));
        assert_eq!(machine.state(), ControlState::Addressed);
        assert!(machine.set_configured(true));
        assert_eq!(machine.state(), ControlState::Configured);
        assert!(!machine.set_address(8));
        assert!(machine.set_configured(false));
        assert_eq!(machine.state(), ControlState::Addressed);
    }
}
