//! Sensor-Watch firmware rewrite in Rust.
//!
//! Target: Microchip SAM L22J18A (ARM Cortex-M0+), the board replacement for
//! the classic Casio F-91W. This is the entry point for the bare-metal
//! firmware; hardware abstraction lives in the `watch` module, and the
//! watchface framework lives in the `movement` module.

#![no_std]
#![no_main]
#![allow(static_mut_refs)]

use cortex_m_rt::entry;

mod movement;
mod panic;
mod watch;

/// Reset handler: the entry point invoked by the Cortex-M0+ after reset.
#[entry]
fn main() -> ! {
    // Copy any `.ramfunc` routines from flash to RAM before they are called.
    // Flash-write routines must run from RAM to avoid the read-while-write
    // bus stall when writing the RWW EEPROM area.
    copy_ramfunc();

    // Check why we reset (e.g. a watchdog timeout from a previous hang).
    movement::fault::check_reset_reason();

    // Detect a brown-out reboot loop and drop into the safe state if needed.
    movement::fault::check_boot_throttle();

    // Check the firmware image for bit-rot. If it fails, record a fault so the
    // user is informed; the watch still boots (a false positive must not brick
    // the watch).
    if !watch::crc::check_firmware_integrity() {
        movement::fault::record_fault(movement::fault::Fault::CorruptImage);
    }

    // Initialize the hardware in dependency order: interrupt priorities,
    // clocks, RTC, then the watchdog backstop.
    watch::init();

    // Configure the brown-out detector (low-battery interrupt).
    watch::deepsleep::init_bod33();

    // Initialize the Movement framework.
    movement::app_init();

    // Apply the board config (LED polarity, buzzer voltage).
    movement::board::apply();

    movement::app_setup();

    // Register the 1 Hz tick callback that wakes the CPU each second.
    watch::rtc::register_tick_callback(movement::cb_tick);

    loop {
        // The CPU is a start/stop resource: react to the pending event, then
        // immediately enter STANDBY until the next interrupt.
        movement::app_loop();

        // Kick the watchdog: the main loop completed, so we are alive.
        watch::wdt::kick();

        // Enter STANDBY. The SysTick interrupt is disabled just before WFI and
        // re-enabled after waking: if SysTick happened to fire at the exact
        // microsecond we enter standby (with back-bias enabled), the SAM L22
        // throws a Hard Fault. Disabling it around WFI avoids that race.
        watch::deepsleep::enter_standby();
    }
}

/// Copies the `.ramfunc` section from flash to RAM.
///
/// The linker places `.ramfunc` code in RAM (VMA) with its contents in flash
/// (LMA). The cortex-m-rt startup only copies `.data`, so we copy `.ramfunc`
/// here before any flash-write routine is called.
fn copy_ramfunc() {
    unsafe extern "C" {
        static __ramfunc_start: u8;
        static __ramfunc_end: u8;
        static __sramfunc_lma: u8;
    }
    unsafe {
        let src = &raw const __sramfunc_lma as *const u8;
        let dst = &raw const __ramfunc_start as *mut u8;
        let len = (&raw const __ramfunc_end as usize) - (&raw const __ramfunc_start as usize);
        core::ptr::copy_nonoverlapping(src, dst, len);
    }
}
