//! Optional native USB CDC application transport for the SAM L22.
//!
//! The reference firmware uses TinyUSB in device/full-speed mode with CDC
//! notification endpoint `0x81`, bulk OUT `0x02`, bulk IN `0x82`, 64-byte
//! packets, and the descriptors below. The `atsaml22j` 0.1.0 PAC exposes the
//! USB device control/status registers, but does not expose the USB descriptor
//!/endpoint transfer SRAM (and this workspace does not depend on TinyUSB or a
//! Rust USB device stack). Consequently this module is an honest compile-safe
//! integration point, not a claimed CDC implementation.
//!
//! Enable with `--features usb-cdc`. Initialization then returns
//! [`UsbError::Unsupported`]. The firmware entry point turns that into an
//! explicit boot failure, so an accidentally enabled feature cannot silently
//! ship a battery-draining or nonfunctional USB mode.

#![cfg(feature = "usb-cdc")]

/// USB CDC packet size used by the reference TinyUSB configuration.
pub const MAX_PACKET_SIZE: usize = 64;
/// CDC notification endpoint from the reference descriptors.
pub const NOTIFICATION_ENDPOINT: u8 = 0x81;
/// CDC bulk OUT endpoint from the reference descriptors.
pub const RX_ENDPOINT: u8 = 0x02;
/// CDC bulk IN endpoint from the reference descriptors.
pub const TX_ENDPOINT: u8 = 0x82;

/// Device descriptor values copied from the reference application.
///
/// This is kept as data so the eventual stack integration has a reviewed,
/// stable source of truth. It is not installed into hardware by this module.
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

/// Error returned because the selected PAC/API cannot support transfers yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbError {
    /// Endpoint descriptor/data SRAM access is absent from the current PAC, and no
    /// USB device-stack implementation is available in this workspace.
    Unsupported,
}

/// Initializes native USB CDC.
///
/// This deliberately does not touch USB clocks, pins, or registers. A partial
/// initialization would be worse than an error: it could change the clock from
/// the battery-safe 4 MHz mode while still leaving the application invisible
/// to the host.
pub fn init() -> Result<(), UsbError> {
    Err(UsbError::Unsupported)
}

/// Services USB and moves bytes between the CDC FIFOs and the shell.
///
/// Present to define the integration boundary for the eventual stack. There
/// is no valid implementation until the PAC/device-stack gap is resolved.
pub fn poll() -> Result<(), UsbError> {
    Err(UsbError::Unsupported)
}

/// Queues bytes for CDC transmission.
pub fn write(_bytes: &[u8]) -> Result<usize, UsbError> {
    Err(UsbError::Unsupported)
}

/// Reads one byte from the CDC receive FIFO, if available.
pub fn read() -> Result<Option<u8>, UsbError> {
    Err(UsbError::Unsupported)
}
