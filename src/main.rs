//! Sensor-Watch firmware rewrite in Rust.
//!
//! Target: Microchip SAM L22J18A (ARM Cortex-M0+), the board replacement for
//! the classic Casio F-91W. This is the entry point for the bare-metal
//! firmware; hardware abstraction lives in the `watch` module.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_halt as _;

mod watch;

/// Reset handler: the entry point invoked by the Cortex-M0+ after reset.
#[entry]
fn main() -> ! {
    // TODO: initialize clocks, RTC, LCD, buttons, and the watchface framework.
    watch::rtc::init();
    loop {
        cortex_m::asm::nop();
    }
}
