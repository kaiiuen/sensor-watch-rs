//! Sensor-Watch firmware rewrite in Rust.
//!
//! Target: Microchip SAM L22J18A (ARM Cortex-M0+), the board replacement for
//! the classic Casio F-91W. This is the entry point for the bare-metal
//! firmware; hardware abstraction lives in the `watch` module, and the
//! watchface framework lives in the `movement` module.
//!
//! # Target vs. host
//!
//! This binary is the real on-device firmware entry and is only compiled for
//! `thumbv6m-none-eabi` (see `build.sh` / CI). The `sensor-watch` package also
//! hosts a testable `lib` target (`src/lib.rs`), and building this package on a
//! host (dev) target (e.g. `cargo build --features hostmock -p sensor-watch`)
//! must not try to link the ARM-only firmware. So the ARM modules/entry are
//! gated behind `target_arch = "arm"`, and on host the binary compiles to a
//! trivial no-op stub. The ARM-gated content below is byte-for-byte the
//! original firmware, so the on-target binary is unchanged (verified by hash).

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]
#![allow(static_mut_refs)]
// The HAL intentionally exposes the full C-reference API surface, and every
// face provides both `new_static()` and `new()`. Not all of it is referenced
// from the binary, so silence dead-code warnings at the crate level.
#![allow(dead_code)]

#[cfg(all(feature = "defmt-log", not(target_arch = "arm")))]
compile_error!("the `defmt-log` feature is only supported for the ARM firmware target");

#[cfg(target_arch = "arm")]
use cortex_m_rt::entry;

#[cfg(target_arch = "arm")]
mod movement;
#[cfg(target_arch = "arm")]
mod panic;
#[cfg(target_arch = "arm")]
mod watch;

/// Reset handler: the entry point invoked by the Cortex-M0+ after reset.
#[cfg(target_arch = "arm")]
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

    // Check the firmware image for bit-rot. If it fails, record a fault and
    // keep booting best-effort rather than bricking: the watch still tries to
    // run, and the fault code is only revealed (as an LED flash) when the user
    // presses a button, so a corrupt image never draws attention on its own.
    if !watch::crc::check_firmware_integrity() {
        movement::fault::record_fault(movement::fault::Fault::CorruptImage);
    }

    // Initialize the hardware in dependency order: interrupt priorities,
    // clocks, RTC, then the watchdog backstop.
    watch::init();

    // USB CDC is an opt-in application mode. The current SAM L22 PAC does not
    // expose the transfer SRAM required by a real device stack, so fail
    // explicitly instead of pretending that CDC is available.
    #[cfg(feature = "usb-cdc")]
    if let Err(error) = watch::usb::init() {
        panic!("USB CDC unavailable: {:?}", error);
    }

    // Check the clock failure detector: if the 32 kHz crystal failed, the RTC
    // is running on the internal oscillator (less accurate). Record a fault.
    movement::fault::check_clock_failure();

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

        // Kick the watchdog: the main loop completed, so we are alive. This
        // is only reached after a complete, bounded reaction, so a runaway
        // interrupt loop cannot mask a hang.
        watch::wdt::kick_windowed();

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
#[cfg(target_arch = "arm")]
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

/// Host-only stub entry so this package's binary still links if someone builds
/// it for a dev (non-ARM) target. The real firmware lives behind
/// `target_arch = "arm"` above.
#[cfg(not(target_arch = "arm"))]
fn main() {}
