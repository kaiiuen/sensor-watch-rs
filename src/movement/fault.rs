//! System monitor and fault/error handling.
//!
//! A central "authoritarian watchdog" that tracks the health of every
//! subsystem. Faults are recorded in the RTC backup registers (fixed, no
//! growth) and surfaced to the user as LED flash codes — shown only when the
//! user actually interacts with the watch, never via a polling loop.

use crate::watch::deepsleep;
use crate::watch::led;

/// Fault codes. Each maps to a distinct LED flash pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Fault {
    /// The hardware watchdog reset the watch (a hang occurred).
    WatchdogReset = 1,
    /// A panic occurred (software bug).
    Panic = 2,
    /// A wake event took too long to process.
    WakeTooLong = 3,
    /// An invalid event or state was encountered.
    InvalidState = 4,
    /// The battery is critically low.
    BatteryLow = 5,
    /// The RTC lost time (crystal issue).
    RtcLostTime = 6,
}

/// Backup register indices for fault storage (registers 4-7 are free).
const REG_LAST_FAULT: u8 = 4;
const REG_FAULT_COUNT: u8 = 5;
const REG_RESET_REASON: u8 = 6;

/// The reason the device last reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ResetReason {
    PowerOn = 0,
    Watchdog = 1,
    Panic = 2,
    Software = 3,
}

/// Records a fault in the backup registers (fixed, no growth).
pub fn record_fault(fault: Fault) {
    deepsleep::store_backup_data(fault as u32, REG_LAST_FAULT);
    let count = deepsleep::get_backup_data(REG_FAULT_COUNT).wrapping_add(1);
    deepsleep::store_backup_data(count, REG_FAULT_COUNT);
}

/// Returns the last recorded fault code, or 0 if none.
pub fn last_fault() -> u8 {
    deepsleep::get_backup_data(REG_LAST_FAULT) as u8
}

/// Returns the number of faults recorded since the last clear.
pub fn fault_count() -> u32 {
    deepsleep::get_backup_data(REG_FAULT_COUNT)
}

/// Clears the recorded fault state.
pub fn clear_faults() {
    deepsleep::store_backup_data(0, REG_LAST_FAULT);
    deepsleep::store_backup_data(0, REG_FAULT_COUNT);
}

/// Records the reset reason.
pub fn record_reset_reason(reason: ResetReason) {
    deepsleep::store_backup_data(reason as u32, REG_RESET_REASON);
}

/// Checks the hardware reset cause and records a fault if the watchdog fired.
///
/// Called once at boot. If the device reset because the watchdog timed out
/// (a hang), we record a `WatchdogReset` fault so the user is informed.
pub fn check_reset_reason() {
    // SAFETY: reading the reset cause register is always safe.
    let rcause = unsafe { &*atsaml22j::Rstc::PTR }.rcause().read();
    if rcause.wdt().bit_is_set() {
        record_fault(Fault::WatchdogReset);
        record_reset_reason(ResetReason::Watchdog);
    } else if rcause.por().bit_is_set() {
        record_reset_reason(ResetReason::PowerOn);
    }
}

/// Returns the reason the device last reset.
pub fn reset_reason() -> ResetReason {
    match deepsleep::get_backup_data(REG_RESET_REASON) {
        1 => ResetReason::Watchdog,
        2 => ResetReason::Panic,
        3 => ResetReason::Software,
        _ => ResetReason::PowerOn,
    }
}

/// Signals a fault to the user via LED flashes.
///
/// This is called only when the user interacts with the watch (e.g. on a
/// button press or face switch), so it never runs in a polling loop and never
/// keeps the CPU awake waiting.
pub fn signal_fault(fault: Fault) {
    let n = fault as u8;
    led::enable_leds();
    for _ in 0..n {
        led::set_led_red();
        spin();
        led::set_led_off();
        spin();
    }
    led::disable_leds();
}

/// A short blocking spin used only for the fault flash pattern.
fn spin() {
    for _ in 0..200_000 {
        cortex_m::asm::nop();
    }
}
