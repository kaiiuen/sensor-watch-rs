//! Real-Time Clock driver.
//!
//! Port of the C `watch_rtc.c` from the Sensor-Watch reference. The RTC is the
//! only peripheral enabled by the boot code; it drives the 1 Hz tick, the alarm,
//! and wake-from-sleep behavior.

use crate::watch::timeout::wait_until;
use atsaml22j::rtc::Mode2;
use atsaml22j::rtc::mode2::ctrla::{Modeselect, Prescalerselect};

/// Returns a reference to the RTC peripheral's MODE2 register block.
///
/// The RTC peripheral is memory-mapped at a fixed address (0x4000_2400) and
/// never moves, so we can safely dereference its pointer for the lifetime of
/// the program. This mirrors the C code's global access to `RTC`.
fn rtc() -> &'static Mode2 {
    // SAFETY: the RTC register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { (*atsaml22j::Rtc::PTR).mode2() }
}

/// Reference year for the 6-bit year field (2020 is a leap year, giving us
/// valid dates through 2083).
pub const WATCH_RTC_REFERENCE_YEAR: u16 = 2020;

/// A packed date/time value matching the RTC peripheral's CLOCK register.
///
/// The bit layout mirrors the hardware register:
/// - second: 6 bits (0-59)
/// - minute: 6 bits (0-59)
/// - hour:   5 bits (0-23)
/// - day:    5 bits (1-31)
/// - month:  4 bits (1-12)
/// - year:   6 bits (0-63, representing 2020-2083)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u8,
}

impl DateTime {
    /// Packs the fields into the 32-bit hardware register value.
    pub fn is_valid(self) -> bool {
        crate::watch::safety::valid_datetime(
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
        )
    }

    pub fn to_reg(self) -> u32 {
        (self.second as u32 & 0x3F)
            | ((self.minute as u32 & 0x3F) << 6)
            | ((self.hour as u32 & 0x1F) << 12)
            | ((self.day as u32 & 0x1F) << 17)
            | ((self.month as u32 & 0x0F) << 22)
            | ((self.year as u32 & 0x3F) << 26)
    }

    /// Unpacks a raw register value into a [`DateTime`].
    pub fn from_reg(reg: u32) -> Self {
        DateTime {
            second: (reg & 0x3F) as u8,
            minute: ((reg >> 6) & 0x3F) as u8,
            hour: ((reg >> 12) & 0x1F) as u8,
            day: ((reg >> 17) & 0x1F) as u8,
            month: ((reg >> 22) & 0x0F) as u8,
            year: ((reg >> 26) & 0x3F) as u8,
        }
    }
}

/// Which components of the alarm time to match against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AlarmMatch {
    Disabled = 0,
    Ss = 1,
    MmSs = 2,
    HhMmSs = 3,
}

/// A callback invoked from an RTC interrupt.
pub type Callback = fn();

// Callback slots. The 8 periodic callbacks map to PER0..PER7 (128 Hz..1 Hz).
static mut TICK_CALLBACKS: [Option<Callback>; 8] = [None; 8];
static mut ALARM_CALLBACK: Option<Callback> = None;

// External wake (tamper) callbacks, keyed by tamper input.
// IN0 = A4, IN1 = A2, IN2 = BTN_ALARM. These are set by the deep-sleep driver.
pub(crate) static mut BTN_ALARM_CALLBACK: Option<Callback> = None;
pub(crate) static mut A2_CALLBACK: Option<Callback> = None;
pub(crate) static mut A4_CALLBACK: Option<Callback> = None;

/// Returns true if the RTC peripheral is currently enabled.
pub fn is_enabled() -> bool {
    rtc().ctrla().read().enable().bit_is_set()
}

/// Waits for the RTC to finish synchronizing its registers.
fn sync() -> bool {
    wait_until(|| rtc().syncbusy().read().bits() == 0).is_ok()
}

/// Initializes the RTC in clock/calendar (MODE2) mode.
///
/// This is called from the boot code. It is a no-op if the RTC is already
/// enabled (so we don't reset the clock on a warm reboot).
pub fn init() {
    // Set up the clocks the RTC depends on (XOSC32K + APB clock).
    super::clock::init();

    if is_enabled() {
        return;
    }

    // Disable and reset.
    rtc().ctrla().modify(|_, w| w.enable().clear_bit());
    if !sync() {
        crate::movement::fault::record_fault(crate::movement::fault::Fault::RtcLostTime);
    }
    rtc().ctrla().modify(|_, w| w.swrst().set_bit());
    if !sync() {
        crate::movement::fault::record_fault(crate::movement::fault::Fault::RtcLostTime);
    }

    // Configure: clock mode, DIV1024 prescaler, clock sync enabled.
    rtc().ctrla().modify(|_, w| {
        w.mode().variant(Modeselect::Clock);
        w.prescaler().variant(Prescalerselect::Div1024);
        w.clocksync().set_bit();
        w.enable().set_bit()
    });
    if !sync() {
        crate::movement::fault::record_fault(crate::movement::fault::Fault::RtcLostTime);
    }
}

/// Sets the date and time.
pub fn set_date_time(date_time: DateTime) -> Result<(), ()> {
    if !date_time.is_valid() {
        return Err(());
    }
    // Double sync: without it, setting the time at high tick rates is unreliable.
    if !sync() {
        crate::movement::fault::record_fault(crate::movement::fault::Fault::RtcLostTime);
        return Err(());
    }
    // SAFETY: writing the full CLOCK register with a valid packed value.
    unsafe { rtc().clock().write(|w| w.bits(date_time.to_reg())) };
    if !sync() {
        crate::movement::fault::record_fault(crate::movement::fault::Fault::RtcLostTime);
        return Err(());
    }
    Ok(())
}

/// Returns the current date and time.
pub fn get_date_time() -> DateTime {
    if !sync() {
        crate::movement::fault::record_fault(crate::movement::fault::Fault::RtcLostTime);
    }
    let date_time = DateTime::from_reg(rtc().clock().read().bits());
    if date_time.is_valid() {
        date_time
    } else {
        DateTime {
            year: 0,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }
}

/// Registers a callback invoked once per second (1 Hz tick).
pub fn register_tick_callback(callback: Callback) {
    register_periodic_callback(callback, 1);
}

/// Disables the 1 Hz tick callback.
pub fn disable_tick_callback() {
    disable_periodic_callback(1);
}

/// Registers a callback invoked at the given frequency.
///
/// `frequency` must be a power of two from 1 to 128 (inclusive).
pub fn register_periodic_callback(callback: Callback, frequency: u8) {
    if !(1..=128).contains(&frequency) || !frequency.is_power_of_two() {
        return;
    }
    // Left-justify the period in a 32-bit int, then count leading zeros.
    // 1 Hz -> 7 leading zeros (PER7); 128 Hz -> 0 leading zeros (PER0).
    let tmp = (frequency as u32 & 0xFF) << 24;
    let per_n = tmp.leading_zeros() as usize;

    critical_section::with(|_| unsafe {
        TICK_CALLBACKS[per_n] = Some(callback);
    });

    // Enable the RTC interrupt in the NVIC and set the periodic interrupt bit.
    // SAFETY: unmasking a valid interrupt and writing a valid enable bitmask.
    unsafe {
        cortex_m::peripheral::NVIC::unmask(atsaml22j::Interrupt::RTC);
        rtc().intenset().write(|w| w.bits(1 << per_n));
    }
}

/// Disables the periodic callback at the given frequency.
pub fn disable_periodic_callback(frequency: u8) {
    if !(1..=128).contains(&frequency) || !frequency.is_power_of_two() {
        return;
    }
    let tmp = (frequency as u32 & 0xFF) << 24;
    let per_n = tmp.leading_zeros() as usize;
    // SAFETY: writing a valid interrupt-disable bitmask.
    unsafe { rtc().intenclr().write(|w| w.bits(1 << per_n)) };
}

/// Disables periodic callbacks matching the given bitmask.
pub fn disable_matching_periodic_callbacks(mask: u8) {
    // SAFETY: writing a valid interrupt-disable bitmask.
    unsafe { rtc().intenclr().write(|w| w.bits(mask as u16)) };
}

/// Disables all periodic callbacks, including the 1 Hz tick.
pub fn disable_all_periodic_callbacks() {
    disable_matching_periodic_callbacks(0xFF);
}

/// Registers an alarm callback that fires when the RTC time matches `alarm_time`
/// as masked by `mask`.
pub fn register_alarm_callback(callback: Callback, alarm_time: DateTime, mask: AlarmMatch) {
    if !alarm_time.is_valid() {
        return;
    }
    // SAFETY: writing valid alarm register values, storing the callback, and
    // unmasking the RTC interrupt.
    unsafe {
        rtc().alarm(0).write(|w| w.bits(alarm_time.to_reg()));
        rtc().mask(0).write(|w| w.bits(mask as u8));
    }
    critical_section::with(|_| unsafe {
        ALARM_CALLBACK = Some(callback);
    });
    unsafe {
        cortex_m::peripheral::NVIC::unmask(atsaml22j::Interrupt::RTC);
        rtc().intenset().write(|w| w.bits(1 << 8)); // ALARM0
    }
}

/// Disables the alarm callback.
pub fn disable_alarm_callback() {
    // SAFETY: writing a valid interrupt-disable bitmask.
    unsafe { rtc().intenclr().write(|w| w.bits(1 << 8)) }; // ALARM0
}

// --- Compare-callback queue ---
//
// A software analog of the Second Movement hardware compare-callback queue.
// Second Movement runs the RTC in counter (MODE0) mode with hardware compare
// registers; this firmware runs it in calendar (MODE2) mode. We provide the
// same indexed-compare-callback API, implemented in software: up to
// `N_COMP_CB` indexed slots hold a target time (packed DateTime) and callback,
// and the earliest pending slot is armed via the existing one-shot alarm.

/// The number of compare-callback slots.
pub const N_COMP_CB: usize = 8;

/// The compare-callback slot reserved for the minute-wake timer, so faces and
/// other scheduled work (slots 0-6) never collide with it.
pub const MINUTE_WAKE_INDEX: usize = N_COMP_CB - 1;

/// A single compare-callback slot.
struct CompCallback {
    target: u32,
    callback: Option<Callback>,
    enabled: bool,
}

/// The compare-callback slots.
static mut COMP_CALLBACKS: [CompCallback; N_COMP_CB] = [const {
    CompCallback {
        target: 0,
        callback: None,
        enabled: false,
    }
}; N_COMP_CB];

/// Arms the earliest pending compare callback via the one-shot alarm.
fn schedule_next_compare() {
    unsafe {
        let now = get_date_time();
        let now_timestamp = crate::watch::utility::date_time_to_unix_time(now, 0);
        let mut earliest: Option<(u32, u32)> = None;
        for slot in COMP_CALLBACKS.iter() {
            if !slot.enabled {
                continue;
            }
            let target = DateTime::from_reg(slot.target);
            if !target.is_valid() {
                continue;
            }
            let target_timestamp = crate::watch::utility::date_time_to_unix_time(target, 0);
            if target_timestamp >= now_timestamp
                && (earliest.is_none() || target_timestamp < earliest.unwrap().0)
            {
                earliest = Some((target_timestamp, slot.target));
            }
        }
        if let Some((_, target)) = earliest {
            schedule_wakeup(compare_tick, DateTime::from_reg(target));
        }
    }
}

/// Registers a compare callback at the given target time (packed DateTime).
pub fn register_comp_callback(callback: Callback, target: u32, index: usize) {
    if index >= N_COMP_CB || !DateTime::from_reg(target).is_valid() {
        return;
    }
    unsafe {
        COMP_CALLBACKS[index].target = target;
        COMP_CALLBACKS[index].callback = Some(callback);
        COMP_CALLBACKS[index].enabled = true;
    }
    schedule_next_compare();
}

/// Registers a compare callback without re-arming the alarm.
pub fn register_comp_callback_no_schedule(callback: Callback, target: u32, index: usize) {
    if index >= N_COMP_CB || !DateTime::from_reg(target).is_valid() {
        return;
    }
    unsafe {
        COMP_CALLBACKS[index].target = target;
        COMP_CALLBACKS[index].callback = Some(callback);
        COMP_CALLBACKS[index].enabled = true;
    }
}

/// Disables a compare callback.
pub fn disable_comp_callback(index: usize) {
    if index >= N_COMP_CB {
        return;
    }
    unsafe {
        COMP_CALLBACKS[index].enabled = false;
    }
    schedule_next_compare();
}

/// Disables a compare callback without re-arming the alarm.
pub fn disable_comp_callback_no_schedule(index: usize) {
    if index >= N_COMP_CB {
        return;
    }
    unsafe {
        COMP_CALLBACKS[index].enabled = false;
    }
}

/// Fires any compare callbacks whose target time has been reached.
///
/// Called from the one-shot alarm that armed the earliest slot.
fn compare_tick() {
    unsafe {
        let now = get_date_time();
        let now_timestamp = crate::watch::utility::date_time_to_unix_time(now, 0);
        for slot in COMP_CALLBACKS.iter_mut() {
            let target = DateTime::from_reg(slot.target);
            let target_timestamp = if target.is_valid() {
                crate::watch::utility::date_time_to_unix_time(target, 0)
            } else {
                u32::MAX
            };
            if slot.enabled && target_timestamp <= now_timestamp {
                slot.enabled = false;
                if let Some(cb) = slot.callback {
                    cb();
                }
            }
        }
        // Re-arm the next pending slot.
        schedule_next_compare();
    }
}

/// Schedules a one-shot wakeup at the given time.
///
/// The alarm fires once when the RTC clock reaches `alarm_time` (matching
/// seconds, minutes, hours, and day). This is how faces request a future
/// wakeup without keeping the CPU awake.
pub fn schedule_wakeup(callback: Callback, alarm_time: DateTime) {
    register_alarm_callback(callback, alarm_time, AlarmMatch::HhMmSs);
}

/// Schedules a wakeup a given number of seconds in the future.
pub fn schedule_wakeup_in(callback: Callback, seconds: u32) {
    let now = get_date_time();
    if !now.is_valid() {
        return;
    }
    let timestamp = match crate::watch::utility::date_time_to_unix_time(now, 0).checked_add(seconds)
    {
        Some(timestamp) => timestamp,
        None => return,
    };
    let target = crate::watch::utility::date_time_from_unix_time(timestamp, 0);
    if target.is_valid() {
        schedule_wakeup(callback, target);
    }
}

/// The RTC interrupt handler.
///
/// The PAC's `rt` feature declares `extern "C" { fn RTC(); }` and places it in
/// the vector table, so we provide the matching `#[no_mangle]` symbol here.
/// It dispatches to the tick, tamper, and alarm callbacks.
#[unsafe(no_mangle)]
pub extern "C" fn RTC() {
    let interrupt_status = rtc().intflag().read().bits();
    let interrupt_enabled = rtc().intenset().read().bits();

    let pending = interrupt_status & interrupt_enabled;

    // These sources are independent. Handle and clear all pending sources in
    // one entry; an else-if chain can strand an alarm or tamper flag when it
    // arrives in the same RTC cycle as the periodic tick.
    if pending & 0x00FF != 0 {
        // Handle the periodic (tick) callbacks, starting from the 1 Hz tick (PER7).
        for i in (0..8).rev() {
            if pending & (1 << i) != 0 {
                let callback = critical_section::with(|_| unsafe { TICK_CALLBACKS[i] });
                if let Some(cb) = callback {
                    cb();
                }
                // SAFETY: writing a valid interrupt-flag clear bitmask.
                unsafe { rtc().intflag().write(|w| w.bits(1 << i)) };
            }
        }
    }

    if pending & 0x4000 != 0 {
        // Tamper (external wake) interrupts.
        let reason = rtc().tampid().read().bits();
        if reason & 0x04 != 0 {
            // TAMPID2 = BTN_ALARM
            let callback = critical_section::with(|_| unsafe { BTN_ALARM_CALLBACK });
            if let Some(cb) = callback {
                cb();
            }
        }
        if reason & 0x02 != 0 {
            // TAMPID1 = A2
            let callback = critical_section::with(|_| unsafe { A2_CALLBACK });
            if let Some(cb) = callback {
                cb();
            }
        }
        if reason & 0x01 != 0 {
            // TAMPID0 = A4
            let callback = critical_section::with(|_| unsafe { A4_CALLBACK });
            if let Some(cb) = callback {
                cb();
            }
        }
        // Clear the tamper ID and the interrupt flag.
        // SAFETY: writing valid clear values.
        unsafe {
            rtc().tampid().write(|w| w.bits(reason));
            rtc().intflag().write(|w| w.bits(0x4000));
        }
    }

    if pending & 0x0100 != 0 {
        // Alarm0 is a one-shot source. Disable it before invoking the callback;
        // callbacks may schedule a replacement alarm and must not have that
        // replacement immediately disabled on return.
        // SAFETY: writing a valid interrupt-disable bitmask.
        unsafe { rtc().intenclr().write(|w| w.bits(1 << 8)) };
        let callback = critical_section::with(|_| unsafe { ALARM_CALLBACK });
        if let Some(cb) = callback {
            cb();
        }
        // SAFETY: writing a valid interrupt-flag clear bitmask.
        unsafe { rtc().intflag().write(|w| w.bits(0x0100)) };
    }
}

/// Enables or disables the RTC while in flight.
///
/// This is a dangerous operation, so the enable bit is written twice, waiting
/// for synchronization in between.
pub fn enable(en: bool) {
    if !sync() {
        crate::movement::fault::record_fault(crate::movement::fault::Fault::RtcLostTime);
        return;
    }
    rtc().ctrla().modify(|_, w| w.enable().bit(en));
    if !sync() {
        crate::movement::fault::record_fault(crate::movement::fault::Fault::RtcLostTime);
        return;
    }
    rtc().ctrla().modify(|_, w| w.enable().bit(en));
    if !sync() {
        crate::movement::fault::record_fault(crate::movement::fault::Fault::RtcLostTime);
    }
}

/// Writes a frequency-correction value in a single register write.
pub fn freqcorr_write(value: i16, sign: i16) {
    let mut data = (value as u32) & 0x7F;
    if sign != 0 {
        data |= 1 << 7;
    }
    // SAFETY: writing a valid frequency-correction value.
    unsafe { rtc().freqcorr().write(|w| w.bits(data as u8)) };
}

/// Reads the current frequency-correction value (signed).
pub fn freqcorr_read() -> i16 {
    let data = rtc().freqcorr().read().bits() as i16;
    let value = data & 0x7F;
    if data & 0x80 != 0 { -value } else { value }
}
