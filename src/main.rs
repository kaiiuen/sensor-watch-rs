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
use panic_halt as _;

mod movement;
mod watch;

/// Reset handler: the entry point invoked by the Cortex-M0+ after reset.
#[entry]
fn main() -> ! {
    // Initialize the RTC (which sets up the clocks it depends on).
    watch::rtc::init();

    // Initialize the Movement framework.
    movement::app_init();
    movement::app_setup();

    loop {
        // Run the app loop; if it returns true, enter standby until the next tick.
        if movement::app_loop() {
            cortex_m::asm::wfi();
        }
    }
}
