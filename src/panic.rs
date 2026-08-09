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
//!
//! Because we are `#no_std` and have no RTT/defmt, we cannot print the panic
//! location directly. Instead we compute a small "fingerprint" of the panic
//! `file:line` and persist it (24 bits) into the fault backup register before
//! resetting. After the reset, a developer queries it via the `panic` shell
//! command and correlates the 6-digit hex value against a build-time map of
//! fingerprints to `file:line` (a future/RTT/symbol table).

use core::panic::PanicInfo;

/// The panic handler: record the fault, blink the LED, then reset.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Record the fault code so the diagnostics face can show it after reset.
    crate::movement::fault::record_fault(crate::movement::fault::Fault::Panic);
    crate::movement::fault::record_reset_reason(crate::movement::fault::ResetReason::Panic);

    // Persist a fingerprint of the panic location so it survives the reset.
    // We can't format in no_std, so roll the file string through a small hash
    // (FNV-1a) and fold in the line number. The result is stored in the fault
    // register's spare 24 bits; see `record_panic_fingerprint`.
    crate::movement::fault::record_panic_fingerprint(panic_fingerprint(info));

    // Blink the red LED a few times as a visible fault indicator. The number
    // of blinks (here 2 for a panic) maps to the documented error codes.
    crate::watch::led::enable_leds();
    for _ in 0..2 {
        crate::watch::led::set_led_red();
        delay();
        crate::watch::led::set_led_off();
        delay();
    }

    // Reset the device so it recovers on its own.
    cortex_m::peripheral::SCB::sys_reset()
}

/// Computes a deterministic u32 fingerprint of the panic location.
///
/// Roll the source file bytes through FNV-1a, fold in the line number (bit-
/// reversed so low-order line bits don't collide with the file hash's low
/// bytes), and spread the column across the top. Column is deliberately
/// down-weighted: line is the primary discriminator and column rarely varies
/// meaningfully.
fn panic_fingerprint(info: &PanicInfo) -> u32 {
    let loc = info.location();
    let mut h = 0x811c9dc5u32; // FNV offset basis
    for b in loc.map(|l| l.file().as_bytes()).unwrap_or(b"?") {
        h = (h ^ (*b as u32)).wrapping_mul(0x01000193); // FNV prime
    }
    let line = loc.map(|l| l.line()).unwrap_or(0) as u32;
    let col = loc.map(|l| l.column()).unwrap_or(0) as u32;
    h ^= line.reverse_bits();
    h ^= col.wrapping_mul(2654435761u32);
    h
}

/// A crude blocking delay for the panic blink.
fn delay() {
    for _ in 0..1_000_000 {
        cortex_m::asm::nop();
    }
}
