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

    // Register the 1 Hz tick callback that wakes the CPU each second.
    watch::rtc::register_tick_callback(movement::cb_tick);

    loop {
        // The CPU is a start/stop resource: react to the pending event, then
        // immediately enter STANDBY until the next interrupt.
        movement::app_loop();
        cortex_m::asm::wfi();
    }
}
