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

fn range(row: u32, offset: u32, len: usize) -> Option<(usize, usize)> {
    let addr = row.checked_mul(ROW_SIZE)?.checked_add(offset)?;
    let len = u32::try_from(len).ok()?;
    let end = addr.checked_add(len)?;
    (end <= total_size()).then_some((addr as usize, end as usize))
}

/// Reads `buffer.len()` bytes from `row` (:ROW_SIZE-aligned base) + `offset`.
///
/// Returns false if the range falls outside the emulated area.
pub fn read(row: u32, offset: u32, buffer: &mut [u8]) -> bool {
    let Some((start, end)) = range(row, offset, buffer.len()) else {
        return false;
    };
    buffer.copy_from_slice(&unsafe { AREA }[start..end]);
    true
}

/// Writes `buffer` to `row` + `offset`. Returns false on an out-of-range write.
pub fn write(row: u32, offset: u32, buffer: &[u8]) -> bool {
    let Some((start, end)) = range(row, offset, buffer.len()) else {
        return false;
    };
    let area = unsafe { &mut AREA };
    // Flash programming can only change a 1 bit to 0 until the row is erased.
    // Mirroring that rule keeps host faces from passing tests that would fail on
    // the SAM L22 NVM controller.
    for (stored, &requested) in area[start..end].iter_mut().zip(buffer) {
        *stored &= requested;
    }
    true
}

/// Erases a row, setting all its bytes to `0xFF`.
pub fn erase(row: u32) -> bool {
    let Some((start, end)) = range(row, 0, ROW_SIZE as usize) else {
        return false;
    };
    let area = unsafe { &mut AREA };
    area[start..end].fill(0xFF);
    true
}

/// Waits for any pending writes. Host: always ready.
pub fn sync() -> bool {
    SYNCED.store(true, Ordering::SeqCst);
    true
}

/// The host model uses the same bounded log as the firmware. Keeping the
/// records in rows (rather than a map) is important: tests can then exercise
/// the same erase-before-reuse and interrupted-write behavior as the device.
const WEAR_ROWS: u32 = 8;
const WEAR_MAGIC: u32 = 0x574C_0001;
const WEAR_HEADER_SIZE: u32 = 20;
const LEGACY_WEAR_HEADER_SIZE: u32 = 12;
const NAMESPACE_ROWS: u32 = WEAR_ROWS / 2;
const CRC_NAMESPACE: u32 = 0x4352_4301;
const SETTINGS_NAMESPACE: u32 = 0x5357_0001;

static mut WEAR_ROW: u32 = 0;

#[derive(Clone, Copy)]
struct WearEntry {
    row: u32,
    generation: u32,
    data_offset: u32,
}

fn generation_is_newer(candidate: u32, current: u32) -> bool {
    candidate != current && candidate.wrapping_sub(current) < 0x8000_0000
}

/// Validate a record before erasing its destination. A record must fit in one
/// row so a partial/corrupt write cannot spill into another record's row.
fn valid_wear_entry(offset: u32, len: usize, header_size: u32) -> bool {
    let Ok(len) = u32::try_from(len) else {
        return false;
    };
    offset
        .checked_add(header_size)
        .and_then(|offset| offset.checked_add(len))
        .is_some_and(|end| end <= ROW_SIZE)
}

fn find_last_entry(namespace: Option<u32>, row_start: u32, row_count: u32) -> Option<WearEntry> {
    let mut newest = None;
    for row in row_start..row_start + row_count {
        let mut header = [0u8; WEAR_HEADER_SIZE as usize];
        if !read(row, 0, &mut header[..4])
            || u32::from_le_bytes(header[..4].try_into().unwrap()) != WEAR_MAGIC
        {
            continue;
        }
        if !read(row, 4, &mut header[4..12]) {
            continue;
        }
        let generation = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let generation_complement = u32::from_le_bytes(header[8..12].try_into().unwrap());
        let (generation, data_offset) = if generation ^ generation_complement == u32::MAX {
            if let Some(namespace) = namespace {
                if !read(row, 12, &mut header[12..])
                    || u32::from_le_bytes(header[12..16].try_into().unwrap()) != namespace
                    || u32::from_le_bytes(header[16..20].try_into().unwrap()) != !namespace
                {
                    continue;
                }
                (generation, WEAR_HEADER_SIZE)
            } else {
                // A complete namespaced header is not an unnamespaced record.
                if read(row, 12, &mut header[12..])
                    && u32::from_le_bytes(header[12..16].try_into().unwrap())
                        ^ u32::from_le_bytes(header[16..20].try_into().unwrap())
                        == u32::MAX
                {
                    continue;
                }
                (generation, LEGACY_WEAR_HEADER_SIZE)
            }
        } else if namespace.is_none() {
            // The oldest format had only a magic word. It remains readable.
            (0, 4)
        } else {
            continue;
        };
        if newest.is_none_or(|entry: WearEntry| generation_is_newer(generation, entry.generation)) {
            newest = Some(WearEntry {
                row,
                generation,
                data_offset,
            });
        }
    }
    newest
}

/// Writes data with log-structured wear leveling across eight rows.
pub fn wear_leveled_write(offset: u32, buffer: &[u8]) -> bool {
    if !valid_wear_entry(offset, buffer.len(), LEGACY_WEAR_HEADER_SIZE) {
        return false;
    }
    let row = unsafe {
        if WEAR_ROW == 0 {
            WEAR_ROW =
                find_last_entry(None, 0, WEAR_ROWS).map_or(0, |entry| (entry.row + 1) % WEAR_ROWS);
        }
        WEAR_ROW % WEAR_ROWS
    };
    let generation =
        find_last_entry(None, 0, WEAR_ROWS).map_or(1, |entry| entry.generation.wrapping_add(1));
    if !erase(row) {
        return false;
    }
    let mut header = [0u8; LEGACY_WEAR_HEADER_SIZE as usize];
    header[..4].copy_from_slice(&WEAR_MAGIC.to_le_bytes());
    header[4..8].copy_from_slice(&generation.to_le_bytes());
    header[8..].copy_from_slice(&(!generation).to_le_bytes());
    if !write(row, 0, &header) || !write(row, offset + LEGACY_WEAR_HEADER_SIZE, buffer) {
        return false;
    }
    unsafe { WEAR_ROW = (row + 1) % WEAR_ROWS };
    true
}

/// Reads the newest valid unnamespaced record, ignoring partial/corrupt rows.
pub fn wear_leveled_read(offset: u32, buffer: &mut [u8]) -> bool {
    let Some(entry) = find_last_entry(None, 0, WEAR_ROWS) else {
        return false;
    };
    valid_entry_read(entry, offset, buffer.len())
        && read(entry.row, entry.data_offset + offset, buffer)
}

fn valid_entry_read(entry: WearEntry, offset: u32, len: usize) -> bool {
    let Ok(len) = u32::try_from(len) else {
        return false;
    };
    entry
        .data_offset
        .checked_add(offset)
        .and_then(|start| start.checked_add(len))
        .is_some_and(|end| end <= ROW_SIZE)
}

fn namespace_rows(namespace: u32) -> Option<(u32, u32)> {
    match namespace {
        CRC_NAMESPACE => Some((0, NAMESPACE_ROWS)),
        SETTINGS_NAMESPACE => Some((NAMESPACE_ROWS, NAMESPACE_ROWS)),
        _ => None,
    }
}

/// Legacy rows have no namespace in their header. Their payload magic is the
/// one-time ownership claim used during migration, so unrelated objects cannot
/// interpret the same old bytes.
fn legacy_magic(namespace: u32) -> Option<u32> {
    match namespace {
        CRC_NAMESPACE | SETTINGS_NAMESPACE => Some(namespace),
        _ => None,
    }
}

fn read_owned_legacy(namespace: u32, offset: u32, buffer: &mut [u8], entry: WearEntry) -> bool {
    let Some(magic) = legacy_magic(namespace) else {
        return false;
    };
    if offset != 0 || buffer.len() < core::mem::size_of::<u32>() {
        return false;
    }
    if !valid_entry_read(entry, offset, buffer.len()) {
        return false;
    }
    let data_offset = entry.data_offset + offset;
    let mut stored_magic = [0u8; 4];
    if !read(entry.row, data_offset, &mut stored_magic) || u32::from_le_bytes(stored_magic) != magic
    {
        return false;
    }
    read(entry.row, data_offset, buffer)
}

/// Writes an independently rotating record for a known namespace.
pub fn wear_leveled_write_namespaced(namespace: u32, offset: u32, buffer: &[u8]) -> bool {
    let Some((row_start, row_count)) = namespace_rows(namespace) else {
        return false;
    };
    if !valid_wear_entry(offset, buffer.len(), WEAR_HEADER_SIZE) {
        return false;
    }
    let previous = find_last_entry(Some(namespace), row_start, row_count);
    let row = previous.map_or(row_start, |entry| {
        row_start + (entry.row - row_start + 1) % row_count
    });
    let generation = previous.map_or(1, |entry| entry.generation.wrapping_add(1));
    if !erase(row) {
        return false;
    }
    let mut header = [0u8; WEAR_HEADER_SIZE as usize];
    header[..4].copy_from_slice(&WEAR_MAGIC.to_le_bytes());
    header[4..8].copy_from_slice(&generation.to_le_bytes());
    header[8..12].copy_from_slice(&(!generation).to_le_bytes());
    header[12..16].copy_from_slice(&namespace.to_le_bytes());
    header[16..20].copy_from_slice(&(!namespace).to_le_bytes());
    write(row, 0, &header) && write(row, offset + WEAR_HEADER_SIZE, buffer)
}

/// Reads the newest valid namespaced record, with firmware-compatible legacy
/// fallback for upgrades where an old unnamespaced record is still present.
pub fn wear_leveled_read_namespaced(namespace: u32, offset: u32, buffer: &mut [u8]) -> bool {
    let Some((row_start, row_count)) = namespace_rows(namespace) else {
        return false;
    };
    if let Some(entry) = find_last_entry(Some(namespace), row_start, row_count) {
        return valid_entry_read(entry, offset, buffer.len())
            && read(entry.row, entry.data_offset + offset, buffer);
    }

    // Only expose an old record when its payload explicitly claims this
    // namespace. Otherwise the legacy bytes remain unreadable through every
    // namespaced API.
    find_last_entry(None, 0, WEAR_ROWS)
        .is_some_and(|entry| read_owned_legacy(namespace, offset, buffer, entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: AtomicBool = AtomicBool::new(false);

    struct TestLock;

    impl TestLock {
        fn acquire() -> Self {
            while TEST_LOCK
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                core::hint::spin_loop();
            }
            Self
        }
    }

    impl Drop for TestLock {
        fn drop(&mut self) {
            TEST_LOCK.store(false, Ordering::Release);
        }
    }

    #[test]
    fn rejects_overflowing_and_out_of_range_ranges() {
        let mut readback = [0; 4];
        assert!(!read(u32::MAX, 0, &mut readback));
        assert!(!read(0, u32::MAX, &mut readback));
        assert!(!write(u32::MAX, 0, &[1]));
        assert!(!erase(u32::MAX));
        assert!(!wear_leveled_write(u32::MAX, &[1]));
        assert!(!wear_leveled_read(u32::MAX, &mut readback));
    }

    #[test]
    fn row_boundary_is_checked() {
        let mut readback = [0; 2];
        assert!(write(31, 254, &[1, 2]));
        assert!(read(31, 254, &mut readback));
        assert_eq!(readback, [1, 2]);
        assert!(!write(31, 255, &[1, 2]));
    }

    #[test]
    fn wear_level_read_cannot_cross_the_committed_row() {
        let _lock = TestLock::acquire();
        for row in 0..WEAR_ROWS {
            assert!(erase(row));
        }
        unsafe { WEAR_ROW = 0 };
        assert!(wear_leveled_write(0, &[0xA5]));
        let mut readback = [0; ROW_SIZE as usize];
        assert!(!wear_leveled_read(0, &mut readback));
    }

    #[test]
    fn flash_write_cannot_set_bits_without_erase() {
        assert!(erase(30));
        assert!(write(30, 0, &[0x0F]));
        assert!(write(30, 0, &[0xF0]));
        let mut readback = [0];
        assert!(read(30, 0, &mut readback));
        assert_eq!(readback, [0x00]);
    }

    #[test]
    fn wear_leveling_rotates_and_ignores_corrupt_latest_header() {
        let _lock = TestLock::acquire();
        for row in 0..WEAR_ROWS {
            assert!(erase(row));
        }
        unsafe { WEAR_ROW = 0 };

        for value in 1u8..=9 {
            assert!(wear_leveled_write(0, &[value]));
        }
        let mut latest = [0];
        assert!(wear_leveled_read(0, &mut latest));
        assert_eq!(latest, [9]);

        // The newest record is row 0 after the ninth write. Damage its
        // generation complement as an interrupted header write would do.
        assert!(write(0, 8, &[0]));
        let mut recovered = [0];
        assert!(wear_leveled_read(0, &mut recovered));
        assert_eq!(recovered, [8]);

        // A header written without its complement is not a committed record.
        assert!(erase(2));
        assert!(write(2, 0, &WEAR_MAGIC.to_le_bytes()));
        assert!(wear_leveled_read(0, &mut recovered));
        assert_eq!(recovered, [8]);
    }

    #[test]
    fn namespaced_records_rotate_independently_and_reject_unknown_namespaces() {
        let _lock = TestLock::acquire();
        for row in 0..WEAR_ROWS {
            assert!(erase(row));
        }

        assert!(!wear_leveled_write_namespaced(0xDEAD_BEEF, 0, &[1]));
        assert!(wear_leveled_write_namespaced(
            SETTINGS_NAMESPACE,
            0,
            &[0x11]
        ));
        assert!(wear_leveled_write_namespaced(CRC_NAMESPACE, 0, &[0x22]));
        assert!(wear_leveled_write_namespaced(
            SETTINGS_NAMESPACE,
            0,
            &[0x33]
        ));

        let mut settings = [0];
        let mut crc = [0];
        assert!(wear_leveled_read_namespaced(
            SETTINGS_NAMESPACE,
            0,
            &mut settings
        ));
        assert!(wear_leveled_read_namespaced(CRC_NAMESPACE, 0, &mut crc));
        assert_eq!(settings, [0x33]);
        assert_eq!(crc, [0x22]);
        assert!(!wear_leveled_read_namespaced(0xDEAD_BEEF, 0, &mut settings));
    }

    #[test]
    fn legacy_records_are_read_only_by_their_owned_namespace() {
        let _lock = TestLock::acquire();
        for row in 0..WEAR_ROWS {
            assert!(erase(row));
        }
        unsafe { WEAR_ROW = 0 };

        let mut settings_payload = [0u8; 8];
        settings_payload[..4].copy_from_slice(&SETTINGS_NAMESPACE.to_le_bytes());
        settings_payload[4..].copy_from_slice(&0xA5A5_5A5Au32.to_le_bytes());
        assert!(wear_leveled_write(0, &settings_payload));

        let mut settings = [0u8; 8];
        let mut crc = [0u8; 8];
        assert!(wear_leveled_read_namespaced(
            SETTINGS_NAMESPACE,
            0,
            &mut settings
        ));
        assert_eq!(settings, settings_payload);
        assert!(!wear_leveled_read_namespaced(CRC_NAMESPACE, 0, &mut crc));

        for row in 0..WEAR_ROWS {
            assert!(erase(row));
        }
        unsafe { WEAR_ROW = 0 };
        let mut crc_payload = [0u8; 8];
        crc_payload[..4].copy_from_slice(&CRC_NAMESPACE.to_le_bytes());
        crc_payload[4..].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        assert!(wear_leveled_write(0, &crc_payload));

        assert!(wear_leveled_read_namespaced(CRC_NAMESPACE, 0, &mut crc));
        assert_eq!(crc, crc_payload);
        assert!(!wear_leveled_read_namespaced(
            SETTINGS_NAMESPACE,
            0,
            &mut settings
        ));
    }

    #[test]
    fn malformed_record_sizes_fail_before_erasing_existing_data() {
        let _lock = TestLock::acquire();
        for row in 0..WEAR_ROWS {
            assert!(erase(row));
        }
        unsafe { WEAR_ROW = 0 };
        assert!(wear_leveled_write(0, &[0x5A]));
        assert!(!wear_leveled_write(ROW_SIZE, &[1]));
        let mut value = [0];
        assert!(wear_leveled_read(0, &mut value));
        assert_eq!(value, [0x5A]);
    }
}
