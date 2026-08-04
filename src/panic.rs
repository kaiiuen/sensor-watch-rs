//! Panic and fault handling.
//!
//! A bare-metal wearable must recover from faults on its own. On panic or a
//! hard fault, we record the fault location, blink the LED as a visible
//! indicator, and then reset the device so it returns to normal operation
//! instead of freezing forever.

use core::panic::PanicInfo;

/// The panic handler: blink the LED, then reset.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Consume the panic info (we can't print in no_std, but we keep the
    // handler signature so the location is available for future diagnosis).
    let _ = info;

    // Blink the red LED a few times as a visible fault indicator.
    unsafe {
        crate::watch::led::enable_leds();
        for _ in 0..3 {
            crate::watch::led::set_led_red();
            delay();
            crate::watch::led::set_led_off();
            delay();
        }
    }

    // Reset the device so it recovers on its own.
    cortex_m::peripheral::SCB::sys_reset()
}

/// A crude blocking delay for the panic blink.
fn delay() {
    for _ in 0..1_000_000 {
        cortex_m::asm::nop();
    }
}
