//! Host storage shim: an in-memory RWW-EEPROM emulation.
//!
//! The real `src/watch/storage.rs` reads/writes the SAM L22's flash-backed RWW
//! EEPROM area. On host we emulate a small 8 KiB area with a byte slice so a face
//! (e.g. `lander`, which persists its hero/legend counters) can exercise its
//! `storage::read`/`write`/`erase`/`sync` calls deterministically in unit tests.
//!
//! `erase` sets a row's bytes to `0xFF` (matching the flash-erased value). The
//! host rewrite keeps `wear_leveled_*` as thin wrappers for signature parity so
//! faces that use them compile unchanged.

use core::sync::atomic::{AtomicBool, Ordering};

/// The emulated EEPROM area (8 KiB, the RWW EEPROM size).
static mut AREA: [u8; 8192] = [0xFF; 8192];

/// A "row" is 256 bytes (matching the real flash row size).
const ROW_SIZE: u32 = 256;

/// Whether erase/write operations are complete (always true on host).
static SYNCED: AtomicBool = AtomicBool::new(true);

/// Returns the total size of the storage area in bytes.
pub fn total_size() -> u32 {
    8192
}

/// Returns the number of non-`0xFF` bytes (the amount "used").
pub fn used_size() -> u32 {
    unsafe { AREA.iter().filter(|&&b| b != 0xFF).count() as u32 }
}

/// Reads `buffer.len()` bytes from `row` (:ROW_SIZE-aligned base) + `offset`.
///
/// Returns false if the range falls outside the emulated area.
pub fn read(row: u32, offset: u32, buffer: &mut [u8]) -> bool {
    let addr = row * ROW_SIZE + offset;
    let len = buffer.len() as u32;
    if addr + len > unsafe { AREA.len() as u32 } {
        return false;
    }
    buffer.copy_from_slice(&unsafe { AREA }[addr as usize..(addr + len) as usize]);
    true
}

/// Writes `buffer` to `row` + `offset`. Returns false on an out-of-range write.
pub fn write(row: u32, offset: u32, buffer: &[u8]) -> bool {
    let addr = row * ROW_SIZE + offset;
    let len = buffer.len() as u32;
    if addr + len > unsafe { AREA.len() as u32 } {
        return false;
    }
    let area = unsafe { &mut AREA };
    for (i, b) in buffer.iter().enumerate() {
        area[addr as usize + i] = *b;
    }
    true
}

/// Erases a row, setting all its bytes to `0xFF`.
pub fn erase(row: u32) -> bool {
    let addr = row * ROW_SIZE;
    if addr + ROW_SIZE > unsafe { AREA.len() as u32 } {
        return false;
    }
    let area = unsafe { &mut AREA };
    for b in area[addr as usize..(addr + ROW_SIZE) as usize].iter_mut() {
        *b = 0xFF;
    }
    true
}

/// Waits for any pending writes. Host: always ready.
pub fn sync() -> bool {
    SYNCED.store(true, Ordering::SeqCst);
    true
}

/// Writes data with log-structured wear leveling. Host: a plain write at the
/// given offset (the highest row acts as the "most recent" row).
pub fn wear_leveled_write(offset: u32, buffer: &[u8]) -> bool {
    write(ROW_SIZE as u32 * 0, offset + 4, buffer)
}

/// Reads data written with log-structured wear leveling. Host: reads row 0.
pub fn wear_leveled_read(offset: u32, buffer: &mut [u8]) -> bool {
    read(0, offset + 4, buffer)
}
