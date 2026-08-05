//! Memory usage reporting.
//!
//! Provides RAM usage readouts for the diagnostics face. RAM usage is derived
//! from the linker symbols that mark the end of the static data (`.data` +
//! `.bss`) and the top of the stack. This is a rough estimate of how much RAM
//! the firmware's static state occupies.

/// Returns the total RAM available in bytes (32 KB).
pub fn total_ram() -> u32 {
    32 * 1024
}

/// Returns the amount of RAM used by static data (`.data` + `.bss`).
///
/// This reads the `__sbss` / `__ebss` linker symbols. The stack grows down
/// from the top of RAM, so the static data usage is the low-water mark.
pub fn static_ram_used() -> u32 {
    unsafe extern "C" {
        static __sbss: u8;
        static __ebss: u8;
        static __sdata: u8;
        static __edata: u8;
    }
    let bss = (&raw const __ebss as usize) - (&raw const __sbss as usize);
    let data = (&raw const __edata as usize) - (&raw const __sdata as usize);
    (bss + data) as u32
}

/// Returns the amount of RAM used, as a percentage of total RAM.
pub fn ram_used_percent() -> u8 {
    let used = static_ram_used();
    let total = total_ram();
    if total == 0 {
        return 0;
    }
    ((used as u64 * 100) / total as u64) as u8
}
