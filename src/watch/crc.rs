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

/// The storage row and offset where the firmware signature lives.
const CRC_ROW: u32 = 1;
const CRC_OFFSET: u32 = 0;
const CRC_NAMESPACE: u32 = 0x4352_4301;

/// A magic value written alongside the signature to detect valid stored data.
const CRC_MAGIC: u32 = 0x4352_4301; // "CRC" + version

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

/// Computes the CRC-32 of the firmware text region.
pub fn firmware_crc() -> u32 {
    crc32(TEXT_START, TEXT_END)
}

/// Loads the stored firmware signature from flash.
///
/// Returns `Some(crc)` if a valid signature is present, or `None` if none has
/// been stored yet (first boot after flashing).
fn load_stored() -> Option<u32> {
    let mut buf = [0u8; 8];
    if !crate::watch::storage::wear_leveled_read_namespaced(CRC_NAMESPACE, CRC_OFFSET, &mut buf) {
        return None;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != CRC_MAGIC {
        return None;
    }
    Some(u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]))
}

/// Stores the firmware signature in flash.
fn store(crc: u32) {
    let mut buf = [0u8; 8];
    buf[0..4].copy_from_slice(&CRC_MAGIC.to_le_bytes());
    buf[4..8].copy_from_slice(&crc.to_le_bytes());
    crate::watch::storage::wear_leveled_write_namespaced(CRC_NAMESPACE, CRC_OFFSET, &buf);
}

/// Checks the firmware text against the stored signature.
///
/// On the first boot after a flash, no signature is stored yet, so we compute
/// and store the current CRC. On subsequent boots we compare the live text
/// against the stored signature. A mismatch indicates bit-rot; the caller
/// should surface a fault and enter a safe recovery state.
///
/// Returns `true` if the image is intact (or on first boot), `false` if a
/// mismatch was detected.
pub fn check_firmware_integrity() -> bool {
    let actual = firmware_crc();

    match load_stored() {
        // First boot after flashing: store the signature and report intact.
        None => {
            store(actual);
            true
        }
        Some(stored) => stored == actual,
    }
}
