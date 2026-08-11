//! System monitor and fault/error handling.
//!
//! A central "authoritarian watchdog" that tracks the health of every
//! subsystem. Faults are recorded in the RTC backup registers (fixed, no
//! growth) and surfaced to the user as LED flash codes — shown only when the
//! user actually interacts with the watch, never via a polling loop.

use crate::watch::deepsleep;
use crate::watch::led;

/// Backup register indices for fault storage.
///
/// Registers 4-6 are reserved for the fault system; register 7 is reserved for
/// the board config. Values are packed to fit:
///   - reg 4: last fault code
///   - reg 5: fault count
///   - reg 6: reset reason (byte 0) + boot time (bytes 1-2) + boot count (byte 3)
const REG_LAST_FAULT: u8 = 4;
const REG_FAULT_COUNT: u8 = 5;
const REG_RESET_REASON: u8 = 6;

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
    /// The 32 kHz crystal failed; the RTC fell back to the internal oscillator.
    ClockFailure = 8,
}

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
///
/// Only the low byte of reg 4 is written so an existing panic-location
/// fingerprint (stored in its upper 24 bits) is preserved.
pub fn record_fault(fault: Fault) {
    // Keep a RAM breadcrumb as well as the reset-surviving backup-register
    // summary. This remains safe during early boot and panic handling.
    crate::watch::event_log::record_untimed(
        crate::watch::event_log::EventCode::Fault,
        fault as u16,
    );
    let reg = deepsleep::get_backup_data(REG_LAST_FAULT);
    let packed = (reg & !0xFF) | (fault as u32 & 0xFF);
    deepsleep::store_backup_data(packed, REG_LAST_FAULT);
    let count = deepsleep::get_backup_data(REG_FAULT_COUNT).wrapping_add(1);
    deepsleep::store_backup_data(count, REG_FAULT_COUNT);
}

/// Stores a panic-location fingerprint in the upper 24 bits of reg 4.
///
/// Reg 4 packs the last fault code in its low byte and a 24-bit fingerprint of
/// the panic `file:line` (see `crate::panic`) in its upper bits. The fingerprint
/// survives the reset that follows a panic, so a developer can later recover the
/// panic location instead of seeing only the generic `Panic` code.
pub fn record_panic_fingerprint(fp: u32) {
    let reg = deepsleep::get_backup_data(REG_LAST_FAULT);
    let packed = (reg & 0xFF) | ((fp & 0xFFFFFF) << 8);
    deepsleep::store_backup_data(packed, REG_LAST_FAULT);
}

/// Returns the stored panic-location fingerprint (24 bits), or 0 if none.
///
/// Decoded as a 6-digit hex value (e.g. via the `panic` shell command) and
/// correlated against a build-time mapping of fingerprints to `file:line`.
pub fn panic_fingerprint() -> u32 {
    (deepsleep::get_backup_data(REG_LAST_FAULT) >> 8) & 0xFFFFFF
}

/// Returns the last recorded fault code, or 0 if none.
pub fn last_fault() -> u8 {
    deepsleep::get_backup_data(REG_LAST_FAULT) as u8
}

impl Fault {
    /// Maps a stored fault code back to its `Fault` variant.
    ///
    /// Codes outside the known set map to `InvalidState`, so a corrupted or
    /// foreign backup-register value never produces an out-of-range LED pattern.
    pub fn from_code(code: u8) -> Fault {
        match code {
            1 => Fault::WatchdogReset,
            2 => Fault::Panic,
            3 => Fault::WakeTooLong,
            4 => Fault::InvalidState,
            5 => Fault::BatteryLow,
            6 => Fault::RtcLostTime,
            7 => Fault::CorruptImage,
            8 => Fault::ClockFailure,
            _ => Fault::InvalidState,
        }
    }
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

/// Records the reset reason (byte 0 of reg 6, preserving boot data).
pub fn record_reset_reason(reason: ResetReason) {
    let reg = deepsleep::get_backup_data(REG_RESET_REASON);
    let packed = (reg & !0xFF) | (reason as u32 & 0xFF);
    deepsleep::store_backup_data(packed, REG_RESET_REASON);
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
    // reg 6 layout: byte 0 = reset reason, bytes 1-2 = boot time, byte 3 = boot count.
    let reg = deepsleep::get_backup_data(REG_RESET_REASON);
    let last = (reg >> 8) & 0xFFFF;
    let count = (reg >> 24) & 0xFF;

    let count = if now.wrapping_sub(last) <= BOOT_WINDOW_TICKS {
        count.wrapping_add(1)
    } else {
        1
    };

    let packed = (reg & 0xFF) | ((now & 0xFFFF) << 8) | ((count & 0xFF) << 24);
    deepsleep::store_backup_data(packed, REG_RESET_REASON);

    if count > MAX_BOOTS_IN_WINDOW {
        unsafe { SAFE_STATE = true };
    }
}

/// Checks the clock failure detector and records a fault if the crystal failed.
///
/// Called once at boot. If the CFD fired, the 32 kHz crystal is broken and the
/// RTC is running on the internal oscillator; we record a `ClockFailure` fault
/// so the user is informed the watch is keeping less-accurate time.
pub fn check_clock_failure() {
    if crate::watch::clock::cfd_fired() {
        record_fault(Fault::ClockFailure);
    }
}

/// Backup register for the last heartbeat timestamp.
///
/// Uses register 2, which is not used by any other always-on subsystem
/// (reg 0 = settings, reg 1 = solar location, reg 3 = battery type).
const REG_HEARTBEAT: u8 = 2;

/// Monitors the RTC heartbeat (the ticking seconds).
///
/// Called on each tick. If the RTC seconds stop advancing (a hang or a frozen
/// RTC), we record an `RtcLostTime` fault. The hardware watchdog still resets
/// on a full hang; this adds a second layer that detects a frozen clock even
/// when the CPU is alive.
pub fn check_heartbeat() {
    let now = crate::watch::rtc::get_date_time().to_reg();
    let last = deepsleep::get_backup_data(REG_HEARTBEAT);
    // If the last heartbeat is more than a few seconds behind, the RTC is not
    // advancing (or was reset). Record a fault.
    if last != 0 && now.wrapping_sub(last) > 5 {
        record_fault(Fault::RtcLostTime);
    }
    deepsleep::store_backup_data(now, REG_HEARTBEAT);
}

/// Returns the reason the device last reset.
pub fn reset_reason() -> ResetReason {
    match deepsleep::get_backup_data(REG_RESET_REASON) & 0xFF {
        1 => ResetReason::Watchdog,
        2 => ResetReason::Panic,
        3 => ResetReason::Software,
        _ => ResetReason::PowerOn,
    }
}

/// Reveals the last recorded fault code via the LED, on demand.
///
/// Safe to call from an interrupt or the fast-tick sampling path. Reads the
/// last stored fault code and, if one is present, flashes it via
/// [`signal_fault`]. If no fault has been recorded it does nothing, so a
/// healthy watch shows no attention-grabbing flash when the light button is
/// pressed. Pairs with `Fault::from_code` so the backup-register byte maps back
/// to a valid variant.
pub fn ping_fault_on_light() {
    let last = last_fault();
    if last != 0 {
        signal_fault(Fault::from_code(last));
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
