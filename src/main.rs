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
    // Initialize the RTC (which sets up the clocks it depends on).
    watch::rtc::init();

    // Initialize the Movement framework.
    movement::app_init();
    movement::app_setup();

    // Register the 1 Hz tick callback that wakes the CPU each second.
    watch::rtc::register_tick_callback(movement::cb_tick);

    loop {
        // The CPU is a start/stop resource: react to the pending event, then
        // immediately enter STANDBY until the next interrupt.
        movement::app_loop();
        cortex_m::asm::wfi();
    }
}
