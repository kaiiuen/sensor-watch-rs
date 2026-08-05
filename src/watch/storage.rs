//! Flash storage driver.
//!
//! Port of the C `watch_storage.c`. Provides read/write/erase access to the
//! SAM L22's 8 kilobyte EEPROM emulation area (RWW EEPROM).

use atsaml22j::nvmctrl::RegisterBlock as Nvmctrl;
use atsaml22j::nvmctrl::ctrla::Cmdselect;

/// RWW EEPROM area constants.
const RWWEE_ADDR_START: u32 = 0x0040_0000;
const RWWEE_ADDR_END: u32 = RWWEE_ADDR_START + PAGE_SIZE * RWWEE_PAGES;
const ROW_SIZE: u32 = 256;
const PAGE_SIZE: u32 = 64;
const RWWEE_PAGES: u32 = 128;

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
    (RWWEE_ADDR_START..=RWWEE_ADDR_END).contains(&addr) && addr + size <= RWWEE_ADDR_END
}

/// Reads a range of bytes from the storage area.
pub fn read(row: u32, offset: u32, buffer: &mut [u8]) -> bool {
    let address = RWWEE_ADDR_START + row * ROW_SIZE + offset;
    let size = buffer.len() as u32;
    if !is_valid_address(address, size) {
        return false;
    }

    sync();

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
    let address = RWWEE_ADDR_START + row * ROW_SIZE + offset;
    let size = buffer.len() as u32;
    if !is_valid_address(address, size) {
        return false;
    }

    sync();

    // Issue a page buffer clear command.
    // SAFETY: writing valid CTRLA command values.
    unsafe {
        nvmctrl().ctrla().modify(|_, w| {
            w.cmd().variant(Cmdselect::Pbc);
            w.cmdex().bits(0xA5)
        });
    }
    sync();

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
    sync();

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
    let address = RWWEE_ADDR_START + row * ROW_SIZE;
    if !is_valid_address(address, ROW_SIZE) {
        return false;
    }

    sync();

    // SAFETY: writing valid ADDR/CTRLA command values.
    unsafe {
        nvmctrl().addr().write(|w| w.bits(address / 2));
        nvmctrl().ctrla().modify(|_, w| {
            w.cmd().variant(Cmdselect::Rwweeer);
            w.cmdex().bits(0xA5)
        });
    }

    true
}

/// Waits for any pending writes to complete.
pub fn sync() -> bool {
    while !nvmctrl().intflag().read().ready().bit_is_set() {}
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

/// The current wear-leveling row index, stored in a backup register.
const WEAR_ROW_REG: u8 = 7;

/// Writes data with simple wear leveling.
///
/// The data is written to a rotating row (0..WEAR_ROWS). Each write moves to
/// the next row, so writes are spread across the area instead of hammering
/// one row. The current row index is persisted in an RTC backup register so
/// it survives resets.
pub fn wear_leveled_write(offset: u32, buffer: &[u8]) -> bool {
    let row = crate::watch::deepsleep::get_backup_data(WEAR_ROW_REG) % WEAR_ROWS;

    // Erase the target row, then write to it.
    if !erase(row) {
        return false;
    }
    if !write(row, offset, buffer) {
        return false;
    }

    // Advance to the next row for the next write.
    let next = (row + 1) % WEAR_ROWS;
    crate::watch::deepsleep::store_backup_data(next, WEAR_ROW_REG);
    true
}
