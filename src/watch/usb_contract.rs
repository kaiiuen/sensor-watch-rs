//! Compile-time and host-testable contract for the opt-in USB CDC feature.
//!
//! These values mirror the reference TinyUSB descriptors. They intentionally
//! live outside the controller-facing module so host tests can verify the
//! reviewed contract without pretending that the PAC can service USB transfers.

/// Full-speed CDC max packet size used by the reference implementation.
pub const MAX_PACKET_SIZE: usize = 64;
/// CDC notification endpoint address.
pub const NOTIFICATION_ENDPOINT: u8 = 0x81;
/// CDC bulk OUT endpoint address.
pub const RX_ENDPOINT: u8 = 0x02;
/// CDC bulk IN endpoint address.
pub const TX_ENDPOINT: u8 = 0x82;

// Keep the descriptor contract checked by the compiler, not only by tests.
const _: () = assert!(MAX_PACKET_SIZE.is_power_of_two());
const _: () = assert!(MAX_PACKET_SIZE <= 64);
const _: () = assert!(NOTIFICATION_ENDPOINT & 0x80 != 0);
const _: () = assert!(RX_ENDPOINT & 0x80 == 0);
const _: () = assert!(TX_ENDPOINT & 0x80 != 0);
const _: () = assert!(NOTIFICATION_ENDPOINT != TX_ENDPOINT);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_tinyusb_endpoints() {
        assert_eq!(MAX_PACKET_SIZE, 64);
        assert_eq!(NOTIFICATION_ENDPOINT, 0x81);
        assert_eq!(RX_ENDPOINT, 0x02);
        assert_eq!(TX_ENDPOINT, 0x82);
    }
}
