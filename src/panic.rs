//! Panic and fault handling.
//!
//! A bare-metal wearable must recover from faults on its own. On panic or a
//! hard fault, we record the fault, blink the LED as a visible indicator, and
//! then reset the device so it returns to normal operation instead of freezing
//! forever.
//!
//! The last fault is stored in the RTC backup registers (via
//! [`crate::movement::fault`]) so that after a reset the diagnostics face can
//! show the user an error code for troubleshooting.

use core::panic::PanicInfo;

/// The panic handler: record the fault, blink the LED, then reset.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Record the fault code so the diagnostics face can show it after reset.
    crate::movement::fault::record_fault(crate::movement::fault::Fault::Panic);
    crate::movement::fault::record_reset_reason(crate::movement::fault::ResetReason::Panic);

    // Blink the red LED a few times as a visible fault indicator. The number
    // of blinks (here 2 for a panic) maps to the documented error codes.
    crate::watch::led::enable_leds();
    for _ in 0..2 {
        crate::watch::led::set_led_red();
        delay();
        crate::watch::led::set_led_off();
        delay();
    }

    // Include the panic location in the debug symbol path; the location is
    // captured by the compiler even though we can't print it in no_std.
    let _ = info;

    // Reset the device so it recovers on its own.
    cortex_m::peripheral::SCB::sys_reset()
}

/// A crude blocking delay for the panic blink.
fn delay() {
    for _ in 0..1_000_000 {
        cortex_m::asm::nop();
    }
}
