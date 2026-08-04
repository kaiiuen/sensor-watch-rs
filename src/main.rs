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
    // Check why we reset (e.g. a watchdog timeout from a previous hang).
    movement::fault::check_reset_reason();

    // Configure interrupt priorities before any interrupt is enabled.
    watch::irq::init();

    // Initialize the RTC (which sets up the clocks it depends on).
    watch::rtc::init();

    // Initialize the Movement framework.
    movement::app_init();
    movement::app_setup();

    // Register the 1 Hz tick callback that wakes the CPU each second.
    watch::rtc::register_tick_callback(movement::cb_tick);

    // Start the hardware watchdog. If the main loop ever hangs, the WDT
    // resets the chip so the watch always recovers.
    watch::wdt::init();

    loop {
        // The CPU is a start/stop resource: react to the pending event, then
        // immediately enter STANDBY until the next interrupt.
        movement::app_loop();

        // Kick the watchdog: the main loop completed, so we are alive.
        watch::wdt::kick();

        cortex_m::asm::wfi();
    }
}
