//! Sensor-Watch firmware rewrite in Rust.
//!
//! Target: Microchip SAM L22J18A (ARM Cortex-M0+), the board replacement for
//! the classic Casio F-91W. This is the entry point for the bare-metal
//! firmware; hardware abstraction lives in the `watch` module, and the
//! watchface framework lives in the `movement` module.

#![no_std]
#![no_main]
#![allow(static_mut_refs)]

extern crate alloc;

use cortex_m_rt::entry;
use panic_halt as _;

mod movement;
mod watch;

// Global allocator for the watch faces (which use Box).
#[global_allocator]
static ALLOCATOR: embedded_alloc::TlsfHeap = embedded_alloc::TlsfHeap::empty();

/// Reset handler: the entry point invoked by the Cortex-M0+ after reset.
#[entry]
fn main() -> ! {
    // Initialize the heap for the allocator.
    unsafe {
        ALLOCATOR.init(
            cortex_m::singleton!(: [u8; 8192] = [0; 8192]).unwrap() as *mut _ as usize,
            8192,
        );
    }

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
