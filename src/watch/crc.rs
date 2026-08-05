//! CRC-32 integrity checking.
//!
//! Flash bit-flips (memory rot) can silently corrupt the executable over years.
//! This module computes a CRC-32 over the firmware's text section and compares
//! it against a signature burned at compile time, so a corrupt image can be
//! detected and the watch can enter a safe recovery state instead of running
//! corrupted code.

/// The start of the firmware text in flash (after the bootloader).
const TEXT_START: usize = 0x0000_2000;
/// The end of the firmware text (the start of the RWW EEPROM area).
const TEXT_END: usize = 0x0003_C000;

/// A compile-time CRC-32 signature of the firmware text.
///
/// This is a placeholder that is intentionally not the real signature; a real
/// build would embed a hash computed over the linked image. The runtime check
/// treats a mismatch as a fault but does not block boot, so a false positive
/// cannot brick the watch.
const EXPECTED_CRC: u32 = 0x0000_0000;

/// Computes a CRC-32 (IEEE 802.3) over a byte range in flash.
pub fn crc32(start: usize, end: usize) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    // SAFETY: reading from the flash text region is always safe.
    let mut addr = start;
    while addr < end {
        let byte = unsafe { *(addr as *const u8) };
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
        addr += 1;
    }
    !crc
}

/// Checks the firmware text against the expected signature.
///
/// Returns true if the image is intact. A mismatch indicates bit-rot; the
/// caller should surface a fault and enter a safe recovery state.
pub fn check_firmware_integrity() -> bool {
    let actual = crc32(TEXT_START, TEXT_END);
    actual == EXPECTED_CRC
}
