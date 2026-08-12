//! Flash storage driver.
//!
//! Port of the C `watch_storage.c`. Provides read/write/erase access to the
//! SAM L22's 8 kilobyte EEPROM emulation area (RWW EEPROM).

use crate::watch::timeout::wait_until;
use atsaml22j::nvmctrl::RegisterBlock as Nvmctrl;
use atsaml22j::nvmctrl::ctrla::Cmdselect;

/// RWW EEPROM area constants.
const RWWEE_ADDR_START: u32 = 0x0040_0000;
const RWWEE_ADDR_END: u32 = RWWEE_ADDR_START + PAGE_SIZE * RWWEE_PAGES;
const ROW_SIZE: u32 = 256;
const PAGE_SIZE: u32 = 64;
const RWWEE_PAGES: u32 = 128;

/// Returns the total size of the RWW EEPROM area in bytes.
pub fn total_size() -> u32 {
    RWWEE_ADDR_END - RWWEE_ADDR_START
}

/// Returns the number of bytes currently used in the RWW EEPROM area.
///
/// A byte is "used" if it is not 0xFF (erased flash). This scans the whole
/// area, so it is only suitable for a diagnostics readout.
pub fn used_size() -> u32 {
    let mut used = 0u32;
    let mut buf = [0u8; 256];
    let mut row = 0u32;
    while row < total_size() / ROW_SIZE
        && row
            .checked_mul(ROW_SIZE)
            .is_some_and(|offset| offset < total_size())
    {
        if read(row, 0, &mut buf) {
            for &b in buf.iter() {
                if b != 0xFF {
                    used += 1;
                }
            }
        }
        row += 1;
    }
    used
}

/// The NVM memory array (Flash), accessed as 16-bit words.
const NVM_MEMORY: *mut u16 = 0x0000_0000 as *mut u16;

/// Returns a reference to the NVMCTRL peripheral register block.
fn nvmctrl() -> &'static Nvmctrl {
    // SAFETY: the NVMCTRL register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Nvmctrl::PTR }
}

/// Checks that the given address range is within the RWW EEPROM area.
fn is_valid_address(addr: u32, size: u32) -> bool {
    addr >= RWWEE_ADDR_START && addr <= RWWEE_ADDR_END && size <= RWWEE_ADDR_END - addr
}

fn address_for(row: u32, offset: u32, size: u32) -> Option<u32> {
    let row_offset = row.checked_mul(ROW_SIZE)?;
    let address = RWWEE_ADDR_START
        .checked_add(row_offset)?
        .checked_add(offset)?;
    is_valid_address(address, size).then_some(address)
}

fn valid_page_write(address: u32, size: u32) -> bool {
    size != 0
        && size.is_multiple_of(2)
        && address.is_multiple_of(2)
        && size <= PAGE_SIZE
        && (address % PAGE_SIZE)
            .checked_add(size)
            .is_some_and(|end| end <= PAGE_SIZE)
}

/// Reads a range of bytes from the storage area.
pub fn read(row: u32, offset: u32, buffer: &mut [u8]) -> bool {
    let size = match u32::try_from(buffer.len()) {
        Ok(size) => size,
        Err(_) => return false,
    };
    let address = match address_for(row, offset, size) {
        Some(address) => address,
        None => return false,
    };
    if size == 0 {
        return true;
    }
    if !sync() {
        return false;
    }

    let mut nvm_address = (address / 2) as usize;
    let mut i: usize;

    // SAFETY: reading from the NVM memory array is always safe.
    unsafe {
        if !address.is_multiple_of(2) {
            let data = *NVM_MEMORY.add(nvm_address);
            nvm_address += 1;
            buffer[0] = (data >> 8) as u8;
            i = 1;
        } else {
            i = 0;
        }

        while (i as u32) < size {
            let data = *NVM_MEMORY.add(nvm_address);
            nvm_address += 1;
            buffer[i] = (data & 0xFF) as u8;
            if (i as u32) < size - 1 {
                buffer[i + 1] = (data >> 8) as u8;
            }
            i += 2;
        }
    }
    true
}

/// Writes bytes to a page in the storage area (the row should already be erased).
///
/// Runs from RAM (`.ramfunc`) so the CPU does not stall on the read-while-write
/// bus when writing the RWW EEPROM area.
#[unsafe(link_section = ".ramfunc")]
pub fn write(row: u32, offset: u32, buffer: &[u8]) -> bool {
    let size = match u32::try_from(buffer.len()) {
        Ok(size) => size,
        Err(_) => return false,
    };
    let address = match address_for(row, offset, size) {
        Some(address) => address,
        None => return false,
    };
    if !valid_page_write(address, size) {
        return false;
    }
    if !sync() {
        return false;
    }

    // Issue a page buffer clear command.
    // SAFETY: writing valid CTRLA command values.
    unsafe {
        nvmctrl().ctrla().modify(|_, w| {
            w.cmd().variant(Cmdselect::Pbc);
            w.cmdex().bits(0xA5)
        });
    }
    if !sync() {
        return false;
    }

    let mut nvm_address = (address / 2) as usize;
    // SAFETY: writing to the NVM memory array and CTRLA is safe.
    unsafe {
        let mut i: u32 = 0;
        while i < size {
            let mut data = buffer[i as usize] as u16;
            if i < PAGE_SIZE - 1 {
                data |= (buffer[i as usize + 1] as u16) << 8;
            }
            *NVM_MEMORY.add(nvm_address) = data;
            nvm_address += 1;
            i += 2;
        }
        nvmctrl().addr().write(|w| w.bits(address / 2));
        nvmctrl().ctrla().modify(|_, w| {
            w.cmd().variant(Cmdselect::Rwweewp);
            w.cmdex().bits(0xA5)
        });
    }
    if !sync() {
        return false;
    }

    // Write-verify: read back the data and confirm it matches. If it does
    // not, the write failed (e.g. the row was not erased) and we report it.
    let mut verify = [0u8; 256];
    if !read(row, offset, &mut verify[..size as usize]) {
        return false;
    }
    verify[..size as usize] == *buffer
}

/// Erases a row in the storage area, setting all its bytes to 0xFF.
///
/// Runs from RAM (`.ramfunc`) so the CPU does not stall on the read-while-write
/// bus when erasing the RWW EEPROM area.
#[unsafe(link_section = ".ramfunc")]
pub fn erase(row: u32) -> bool {
    let address = match address_for(row, 0, ROW_SIZE) {
        Some(address) => address,
        None => return false,
    };
    if address % ROW_SIZE != 0 {
        return false;
    }

    if !sync() {
        return false;
    }

    // SAFETY: writing valid ADDR/CTRLA command values.
    unsafe {
        nvmctrl().addr().write(|w| w.bits(address / 2));
        nvmctrl().ctrla().modify(|_, w| {
            w.cmd().variant(Cmdselect::Rwweeer);
            w.cmdex().bits(0xA5)
        });
    }

    sync()
}

/// Waits for any pending writes to complete.
pub fn sync() -> bool {
    if wait_until(|| nvmctrl().intflag().read().ready().bit_is_set()).is_err() {
        return false;
    }
    // SAFETY: clearing the status register is safe.
    unsafe {
        nvmctrl().status().write(|w| w.bits(0xFFFF));
    }
    true
}

/// The number of rows used for wear-leveled writes.
///
/// We rotate writes across these rows so no single row wears out first,
/// extending the life of the EEPROM emulation area.
const WEAR_ROWS: u32 = 8;

/// A magic value written at the start of each wear-leveled row to identify a
/// committed entry.
const WEAR_MAGIC: u32 = 0x574C_0001; // "WL" + version
/// New entries store a generation and its complement after the magic. The
/// complement distinguishes the new header from legacy row data at offset 4.
const WEAR_HEADER_SIZE: u32 = 12;

/// The current wear-leveling row index, kept in RAM.
///
/// This is intentionally NOT stored in an RTC backup register: the backup
/// registers are a scarce shared resource (only 8 exist) reserved for settings,
/// board config, fault codes, and battery type. The cursor is only a hint - on
/// boot it is recovered by scanning the rows for the valid magic header.
static mut WEAR_ROW: u32 = 0;

#[derive(Clone, Copy)]
struct WearEntry {
    row: u32,
    generation: u32,
    data_offset: u32,
}

/// Returns whether `candidate` is newer than `current` in serial-number space.
/// This keeps the newest entry correct when the 32-bit generation wraps.
fn generation_is_newer(candidate: u32, current: u32) -> bool {
    candidate != current && candidate.wrapping_sub(current) < 0x8000_0000
}

/// Validates the complete entry before any destructive erase occurs.
fn valid_wear_entry(offset: u32, len: usize) -> bool {
    let data_offset = match offset.checked_add(WEAR_HEADER_SIZE) {
        Some(offset) => offset,
        None => return false,
    };
    let data_size = match u32::try_from(len) {
        Ok(size) => size,
        Err(_) => return false,
    };
    let base = RWWEE_ADDR_START;
    let header_address = base;
    let Some(data_address) = base.checked_add(data_offset) else {
        return false;
    };
    valid_page_write(header_address, WEAR_HEADER_SIZE) && valid_page_write(data_address, data_size)
}

/// Finds the newest valid row and its format. Four-byte headers from older
/// firmware are accepted as generation zero so an upgrade does not lose the
/// last saved settings.
fn find_last_entry() -> Option<WearEntry> {
    let mut newest = None;
    for row in 0..WEAR_ROWS {
        let mut header = [0u8; 12];
        if !read(row, 0, &mut header[..4])
            || u32::from_le_bytes(header[..4].try_into().unwrap()) != WEAR_MAGIC
        {
            continue;
        }
        let (generation, data_offset) = if read(row, 4, &mut header[4..])
            && u32::from_le_bytes(header[4..8].try_into().unwrap())
                ^ u32::from_le_bytes(header[8..12].try_into().unwrap())
                == u32::MAX
        {
            (
                u32::from_le_bytes(header[4..8].try_into().unwrap()),
                WEAR_HEADER_SIZE,
            )
        } else {
            (0, 4)
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

/// Writes data with log-structured wear leveling.
///
/// The data is written to a rotating row (0..WEAR_ROWS) with a version-magic
/// header. Each write moves to the next row, so writes are spread across the
/// area instead of hammering one row. The current row index is kept in RAM and
/// recovered on boot by scanning for the valid magic, giving crash recovery
/// without consuming a backup register.
pub fn wear_leveled_write(offset: u32, buffer: &[u8]) -> bool {
    // Validate both page writes before erasing the destination row. A malformed
    // request must never destroy the previously committed entry.
    if !valid_wear_entry(offset, buffer.len()) {
        return false;
    }

    // Recover the cursor before the first write after reset. Without this, a
    // reboot always erased row zero and could destroy the newest entry.
    let row = unsafe {
        if WEAR_ROW == 0 {
            WEAR_ROW = find_last_entry().map_or(0, |entry| (entry.row + 1) % WEAR_ROWS);
        }
        WEAR_ROW % WEAR_ROWS
    };
    let generation = find_last_entry().map_or(1, |entry| entry.generation.wrapping_add(1));

    // Erase the target row, then write the generation-bearing header and data.
    if !erase(row) {
        return false;
    }
    let mut header = [0u8; 12];
    header[..4].copy_from_slice(&WEAR_MAGIC.to_le_bytes());
    header[4..8].copy_from_slice(&generation.to_le_bytes());
    header[8..].copy_from_slice(&(!generation).to_le_bytes());
    if !write(row, 0, &header) {
        return false;
    }
    let data_offset = offset + WEAR_HEADER_SIZE;
    if !write(row, data_offset, buffer) {
        return false;
    }

    // Advance to the next row for the next write.
    unsafe { WEAR_ROW = (row + 1) % WEAR_ROWS };
    true
}

/// Reads data written with log-structured wear leveling.
///
/// Scans the rows for the most recent valid entry (matching the magic header)
/// and reads the data from it. Returns true if a valid entry was found.
pub fn wear_leveled_read(offset: u32, buffer: &mut [u8]) -> bool {
    let Some(entry) = find_last_entry() else {
        return false;
    };
    let data_offset = match offset.checked_add(entry.data_offset) {
        Some(offset) => offset,
        None => return false,
    };
    read(entry.row, data_offset, buffer)
}

/// Writes a 32-bit word with SECDED ECC protection.
///
/// The data word is encoded with a 7-bit Hamming code and stored as 5 bytes
/// (40 bits). On read, single-bit errors are corrected and double-bit errors
/// are detected.
pub fn ecc_write(row: u32, offset: u32, data: u32) -> bool {
    let code = crate::watch::ecc::encode(data);
    let mut buf = [0u8; 5];
    buf[0] = (code & 0xFF) as u8;
    buf[1] = ((code >> 8) & 0xFF) as u8;
    buf[2] = ((code >> 16) & 0xFF) as u8;
    buf[3] = ((code >> 24) & 0xFF) as u8;
    buf[4] = ((code >> 32) & 0xFF) as u8;
    write(row, offset, &buf)
}

/// Reads a 32-bit word with SECDED ECC protection.
///
/// Returns `(data, corrected)`: the data word and whether a single-bit error
/// was corrected. A double-bit error returns `corrected = false` (corruption).
pub fn ecc_read(row: u32, offset: u32) -> (u32, bool) {
    let mut buf = [0u8; 5];
    if !read(row, offset, &mut buf) {
        return (0, false);
    }
    let code = (buf[0] as u64)
        | ((buf[1] as u64) << 8)
        | ((buf[2] as u64) << 16)
        | ((buf[3] as u64) << 24)
        | ((buf[4] as u64) << 32);
    crate::watch::ecc::decode(code)
}
