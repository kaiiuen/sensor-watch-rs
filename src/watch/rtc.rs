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
fn sync() {
    wait_until(|| rtc().syncbusy().read().bits() == 0);
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
    sync();
    rtc().ctrla().modify(|_, w| w.swrst().set_bit());
    sync();

    // Configure: clock mode, DIV1024 prescaler, clock sync enabled.
    rtc().ctrla().modify(|_, w| {
        w.mode().variant(Modeselect::Clock);
        w.prescaler().variant(Prescalerselect::Div1024);
        w.clocksync().set_bit();
        w.enable().set_bit()
    });
    sync();
}

/// Sets the date and time.
pub fn set_date_time(date_time: DateTime) {
    // Double sync: without it, setting the time at high tick rates is unreliable.
    sync();
    // SAFETY: writing the full CLOCK register with a valid packed value.
    unsafe { rtc().clock().write(|w| w.bits(date_time.to_reg())) };
    sync();
}

/// Returns the current date and time.
pub fn get_date_time() -> DateTime {
    sync();
    DateTime::from_reg(rtc().clock().read().bits())
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
    if !frequency.is_power_of_two() {
        return;
    }
    // Left-justify the period in a 32-bit int, then count leading zeros.
    // 1 Hz -> 7 leading zeros (PER7); 128 Hz -> 0 leading zeros (PER0).
    let tmp = (frequency as u32 & 0xFF) << 24;
    let per_n = tmp.leading_zeros() as usize;

    unsafe {
        TICK_CALLBACKS[per_n] = Some(callback);
    }

    // Enable the RTC interrupt in the NVIC and set the periodic interrupt bit.
    // SAFETY: unmasking a valid interrupt and writing a valid enable bitmask.
    unsafe {
        cortex_m::peripheral::NVIC::unmask(atsaml22j::Interrupt::RTC);
        rtc().intenset().write(|w| w.bits(1 << per_n));
    }
}

/// Disables the periodic callback at the given frequency.
pub fn disable_periodic_callback(frequency: u8) {
    if !frequency.is_power_of_two() {
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
    // SAFETY: writing valid alarm register values, storing the callback, and
    // unmasking the RTC interrupt.
    unsafe {
        rtc().alarm(0).write(|w| w.bits(alarm_time.to_reg()));
        rtc().mask(0).write(|w| w.bits(mask as u8));
        ALARM_CALLBACK = Some(callback);
        cortex_m::peripheral::NVIC::unmask(atsaml22j::Interrupt::RTC);
        rtc().intenset().write(|w| w.bits(1 << 8)); // ALARM0
    }
}

/// Disables the alarm callback.
pub fn disable_alarm_callback() {
    // SAFETY: writing a valid interrupt-disable bitmask.
    unsafe { rtc().intenclr().write(|w| w.bits(1 << 8)) }; // ALARM0
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
    let mut target = now;
    let mut s = now.second as u32 + seconds;
    target.second = (s % 60) as u8;
    s /= 60;
    target.minute = (target.minute as u32 + s % 60) as u8;
    s /= 60;
    target.hour = (target.hour as u32 + s % 24) as u8;
    schedule_wakeup(callback, target);
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

    if (interrupt_status & interrupt_enabled) & 0x00FF != 0 {
        // Handle the periodic (tick) callbacks, starting from the 1 Hz tick (PER7).
        for i in (0..8).rev() {
            if (interrupt_status & interrupt_enabled) & (1 << i) != 0 {
                if let Some(cb) = unsafe { TICK_CALLBACKS[i] } {
                    cb();
                }
                // SAFETY: writing a valid interrupt-flag clear bitmask.
                unsafe { rtc().intflag().write(|w| w.bits(1 << i)) };
            }
        }
    } else if (interrupt_status & interrupt_enabled) & 0x4000 != 0 {
        // Tamper (external wake) interrupts.
        let reason = rtc().tampid().read().bits();
        if reason & 0x04 != 0 {
            // TAMPID2 = BTN_ALARM
            if let Some(cb) = unsafe { BTN_ALARM_CALLBACK } {
                cb();
            }
        } else if reason & 0x02 != 0 {
            // TAMPID1 = A2
            if let Some(cb) = unsafe { A2_CALLBACK } {
                cb();
            }
        } else if reason & 0x01 != 0 {
            // TAMPID0 = A4
            if let Some(cb) = unsafe { A4_CALLBACK } {
                cb();
            }
        }
        // Clear the tamper ID and the interrupt flag.
        // SAFETY: writing valid clear values.
        unsafe {
            rtc().tampid().write(|w| w.bits(reason));
            rtc().intflag().write(|w| w.bits(0x4000));
        }
    } else if (interrupt_status & interrupt_enabled) & 0x0100 != 0 {
        // Alarm0.
        if let Some(cb) = unsafe { ALARM_CALLBACK } {
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
    sync();
    rtc().ctrla().modify(|_, w| w.enable().bit(en));
    sync();
    rtc().ctrla().modify(|_, w| w.enable().bit(en));
    sync();
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
