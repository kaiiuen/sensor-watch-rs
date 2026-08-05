//! System monitor and fault/error handling.
//!
//! A central "authoritarian watchdog" that tracks the health of every
//! subsystem. Faults are recorded in the RTC backup registers (fixed, no
//! growth) and surfaced to the user as LED flash codes — shown only when the
//! user actually interacts with the watch, never via a polling loop.

use crate::watch::deepsleep;
use crate::watch::led;

/// Backup register for the boot-count throttle.
const REG_BOOT_COUNT: u8 = 7;
/// Backup register for the boot timestamp (in fast ticks, coarse).
const REG_BOOT_TIME: u8 = 6;

/// Maximum number of boots allowed within the throttle window.
const MAX_BOOTS_IN_WINDOW: u32 = 3;
/// The throttle window: if we boot more than this many times in a short
/// period, a brown-out loop is likely and we enter the safe state.
const BOOT_WINDOW_TICKS: u32 = 5;

/// Whether the watch is currently in the brown-out safe state.
static mut SAFE_STATE: bool = false;

/// Returns true if the watch is in the brown-out safe state.
pub fn in_safe_state() -> bool {
    unsafe { SAFE_STATE }
}

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
    /// The firmware image failed its CRC integrity check (bit-rot).
    CorruptImage = 7,
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

/// Detects a brown-out reboot loop and drops the watch into a safe state.
///
/// When a CR2016 battery drops below ~2.0 V its internal resistance spikes,
/// so a high-load peripheral (buzzer, LED) can pull the rail below the CPU
/// threshold, resetting the chip. The load then drops, the battery bounces
/// back, and the watch reboots into the same load — an infinite loop. We count
/// boots in a short window; if we exceed the limit, we enter a safe state that
/// disables the buzzer and LED until the battery is replaced.
pub fn check_boot_throttle() {
    let now = crate::watch::rtc::get_date_time().second as u32;
    let last = deepsleep::get_backup_data(REG_BOOT_TIME);
    let count = deepsleep::get_backup_data(REG_BOOT_COUNT);

    let count = if now.wrapping_sub(last) <= BOOT_WINDOW_TICKS {
        count.wrapping_add(1)
    } else {
        1
    };

    deepsleep::store_backup_data(now, REG_BOOT_TIME);
    deepsleep::store_backup_data(count, REG_BOOT_COUNT);

    if count > MAX_BOOTS_IN_WINDOW {
        unsafe { SAFE_STATE = true };
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
