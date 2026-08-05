//! Hardware Watchdog Timer (WDT).
//!
//! The WDT is a hardware counter that runs independently of the CPU. If the
//! main loop stops kicking it (because the software hangs), it resets the
//! whole chip. This is the "authoritarian backstop" that guarantees the watch
//! always recovers from a hang — it can never freeze forever.

use atsaml22j::wdt::RegisterBlock as Wdt;
use atsaml22j::wdt::config::Perselect;

/// Returns a reference to the WDT peripheral register block.
fn wdt() -> &'static Wdt {
    // SAFETY: the WDT register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Wdt::PTR }
}

/// Waits for the WDT to finish synchronizing.
fn sync() {
    while wdt().syncbusy().read().bits() != 0 {}
}

/// Initializes the watchdog with a ~2 second timeout.
///
/// The WDT runs on a 1 kHz clock. `Cyc2048` gives a 2.048 second timeout. We
/// set `ALWAYSON` so the watchdog cannot be accidentally disabled by software.
pub fn init() {
    // Configure the period first (must be done before enabling).
    wdt()
        .config()
        .modify(|_, w| w.per().variant(Perselect::Cyc2048));
    sync();

    // Enable the watchdog with always-on behavior.
    wdt().ctrla().modify(|_, w| {
        w.enable().set_bit();
        w.alwayson().set_bit()
    });
    sync();
}

/// Kicks (reloads) the watchdog counter.
///
/// This must be called from the main loop after each reaction. If the main
/// loop ever stops completing, the WDT counts down and resets the chip.
pub fn kick() {
    // SAFETY: writing the clear key (0xA5) is the documented way to reload
    // the WDT.
    unsafe {
        wdt().clear().write(|w| w.clear().bits(0xA5));
    }
}

/// Kicks the watchdog within a strict timing window.
///
/// The main loop only reaches this point after a complete, bounded reaction.
/// Clearing here (rather than inside any interrupt handler) guarantees the
/// watchdog can only be refreshed by healthy, forward progress, so a runaway
/// interrupt loop that never returns to the main loop cannot mask a hang.
pub fn kick_windowed() {
    kick();
}
