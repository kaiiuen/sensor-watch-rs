//! Sleep control driver.
//!
//! Port of the C `watch_deepsleep.c`. Provides external wake callbacks, backup
//! data storage, and the Sleep / Deep Sleep / BACKUP modes.

use crate::watch::gpio::{self, Direction, Function, Pin, PullMode};
use crate::watch::rtc;
use atsaml22j::rtc::Mode2;
use atsaml22j::rtc::mode2::tampctrl::{In0actselect, In1actselect, In2actselect};

/// External wake pins.
pub const BTN_ALARM: Pin = Pin(0, 2); // PA02 -> RTC/IN2
pub const A2: Pin = Pin(1, 2); // PB02 -> RTC/IN1
pub const A4: Pin = Pin(1, 0); // PB00 -> RTC/IN0

/// Returns a reference to the RTC MODE2 register block.
fn rtc_mode2() -> &'static Mode2 {
    // SAFETY: the RTC register block lives at a fixed address for the whole
    // program.
    unsafe { (*atsaml22j::Rtc::PTR).mode2() }
}

/// Returns a reference to the RTC MODE0 register block.
fn rtc_mode0() -> &'static atsaml22j::rtc::mode0::Mode0 {
    // SAFETY: the RTC register block lives at a fixed address for the whole
    // program.
    unsafe { (*atsaml22j::Rtc::PTR).mode0() }
}

/// Returns a reference to the SUPC peripheral register block.
fn supc() -> &'static atsaml22j::supc::RegisterBlock {
    // SAFETY: the SUPC register block lives at a fixed address for the whole
    // program.
    unsafe { &*atsaml22j::Supc::PTR }
}

/// Waits for the RTC to finish synchronizing.
fn rtc_sync() {
    while rtc_mode2().syncbusy().read().bits() != 0 {}
}

/// Registers a callback on one of the RTC's external wake pins.
pub fn register_extwake_callback(pin: Pin, callback: rtc::Callback, level: bool) {
    let (in_idx, pinmux) = match pin {
        A4 => (0, 1u8), // RTC/IN0, function G
        A2 => (1, 1u8), // RTC/IN1, function G
        BTN_ALARM => {
            gpio::set_pin_pull_mode(pin, PullMode::Down);
            (2, 1u8) // RTC/IN2, function G
        }
        _ => return,
    };

    // Store the callback in the RTC module's tamper callback slots.
    unsafe {
        match in_idx {
            0 => rtc::A4_CALLBACK = Some(callback),
            1 => rtc::A2_CALLBACK = Some(callback),
            _ => rtc::BTN_ALARM_CALLBACK = Some(callback),
        }
    }

    gpio::set_pin_direction(pin, Direction::In);
    gpio::set_pin_function(pin, Function::Mux(pinmux));

    // Disable the RTC.
    rtc_mode2().ctrla().modify(|_, w| w.enable().clear_bit());
    while !rtc_mode2().syncbusy().read().enable().bit_is_set() {}

    // Update the TAMPCTRL configuration.
    match in_idx {
        0 => {
            rtc_mode2().tampctrl().modify(|_, w| {
                w.in0act().variant(In0actselect::Wake);
                w.tamlvl0().bit(level)
            });
        }
        1 => {
            rtc_mode2().tampctrl().modify(|_, w| {
                w.in1act().variant(In1actselect::Wake);
                w.tamlvl1().bit(level)
            });
        }
        _ => {
            rtc_mode2().tampctrl().modify(|_, w| {
                w.in2act().variant(In2actselect::Wake);
                w.tamlvl2().bit(level)
            });
        }
    }

    // Re-enable the RTC.
    rtc_mode2().ctrla().modify(|_, w| w.enable().set_bit());
    while !rtc_mode2().syncbusy().read().enable().bit_is_set() {}

    cortex_m::peripheral::NVIC::unpend(atsaml22j::Interrupt::RTC);
    // SAFETY: unmasking a valid interrupt is safe.
    unsafe { cortex_m::peripheral::NVIC::unmask(atsaml22j::Interrupt::RTC) };
    // SAFETY: writing a valid interrupt-enable bitmask (TAMPER = bit 14).
    unsafe {
        rtc_mode2()
            .intenset()
            .modify(|r, w| w.bits(r.bits() | (1 << 14)));
    }
}

/// Unregisters the RTC interrupt on one of the EXTWAKE pins.
pub fn disable_extwake_interrupt(pin: Pin) {
    let in_idx = match pin {
        A4 => 0,
        A2 => 1,
        BTN_ALARM => 2,
        _ => return,
    };

    unsafe {
        match in_idx {
            0 => rtc::A4_CALLBACK = None,
            1 => rtc::A2_CALLBACK = None,
            _ => rtc::BTN_ALARM_CALLBACK = None,
        }
    }

    if rtc_mode2().ctrla().read().enable().bit_is_set() {
        rtc_mode2().ctrla().modify(|_, w| w.enable().clear_bit());
        rtc_sync();
    }
    match in_idx {
        0 => {
            rtc_mode2().tampctrl().modify(|_, w| w.in0act().off());
        }
        1 => {
            rtc_mode2().tampctrl().modify(|_, w| w.in1act().off());
        }
        _ => {
            rtc_mode2().tampctrl().modify(|_, w| w.in2act().off());
        }
    }
    rtc_mode2().ctrla().modify(|_, w| w.enable().set_bit());
}

/// Stores data in one of the RTC's backup registers (0-7).
pub fn store_backup_data(data: u32, reg: u8) {
    if reg < 8 {
        // SAFETY: writing a valid backup register value.
        unsafe {
            rtc_mode0().bkup(reg as usize).write(|w| w.bits(data));
        }
    }
}

/// Gets 32 bits of data from the RTC's BACKUP register (0-7).
pub fn get_backup_data(reg: u8) -> u32 {
    if reg < 8 {
        rtc_mode0().bkup(reg as usize).read().bits()
    } else {
        0
    }
}

/// Enters Sleep Mode by disabling all pins and peripherals except the RTC and LCD.
pub fn enter_sleep_mode() {
    // Disable tick interrupt.
    rtc::disable_all_periodic_callbacks();

    // Disable the brownout detector interrupt.
    supc().intenclr().modify(|_, w| w.bod33det().set_bit());

    // Enter standby mode (4).
    cortex_m::asm::wfi();

    // Re-enable the brownout detector interrupt.
    supc().intenset().modify(|_, w| w.bod33det().set_bit());
}

/// Enters STANDBY from the main loop.
///
/// The SysTick interrupt is disabled immediately before WFI and re-enabled
/// right after waking. If SysTick fires at the exact microsecond the CPU
/// enters standby while back-bias is enabled (STDBYCFG.BBIAS = 1), the SAM L22
/// throws a Hard Fault. Disabling it around the sleep instruction avoids that
/// race. This is the primary sleep path used by the main loop.
pub fn enter_standby() {
    // Disable the SysTick interrupt to avoid the standby-entry hard fault.
    // SAFETY: disabling SysTick is always safe.
    unsafe {
        let syst = &*cortex_m::peripheral::SYST::PTR;
        syst.csr.modify(|csr| csr & !(1 << 1)); // clear TICKINT
    }

    // Enter standby.
    cortex_m::asm::wfi();

    // Re-enable the SysTick interrupt now that we are awake.
    // SAFETY: re-enabling SysTick is always safe.
    unsafe {
        let syst = &*cortex_m::peripheral::SYST::PTR;
        syst.csr.modify(|csr| csr | (1 << 1)); // set TICKINT
    }
}

/// Configures the BOD33 brown-out detector.
///
/// BOD33 monitors VDD and triggers an interrupt before the rail falls below
/// the CPU's operating threshold. This lets us react to a low battery instead
/// of silently resetting. Configured to sample once per second in active mode
/// and to run in standby, generating an interrupt at ~2.6 V.
pub fn init_bod33() {
    // BOD33 must be disabled to change its configuration.
    supc().bod33().modify(|_, w| w.enable().clear_bit());
    while !supc().status().read().b33srdy().bit_is_set() {}

    supc().bod33().modify(|_, w| {
        w.vmon().set_bit(); // monitor VDD
        w.actcfg().set_bit(); // sample in active mode
        w.runstdby().set_bit(); // sample in standby
        w.stdbycfg().set_bit(); // run in standby
        w.runbkup().clear_bit(); // don't run in backup
        // SAFETY: 0x9 is a valid PSEL value (check every second).
        unsafe { w.psel().bits(0x9) };
        // SAFETY: 34 is a valid LEVEL value (detect at ~2.6 V).
        unsafe { w.level().bits(34) };
        w.action()
            .variant(atsaml22j::supc::bod33::Actionselect::Int); // generate an interrupt on detect
        w.hyst().clear_bit(); // no hysteresis
        w.enable().set_bit()
    });
    while !supc().status().read().b33srdy().bit_is_set() {}

    // Enable the BOD33 detect interrupt.
    supc().intenset().modify(|_, w| w.bod33det().set_bit());
}

/// Enters Deep Sleep Mode (sleep mode with the LCD disabled).
pub fn enter_deep_sleep_mode() {
    // TODO: disable the LCD (requires the SLCD deinit).
    enter_sleep_mode();
}

/// Enters the SAM L22's lowest-power mode, BACKUP.
///
/// In BACKUP mode, RAM and the CPU are powered off; only the RTC keeps time.
/// Waking requires an RTC alarm or an external wake on A2/A4, and does a full
/// reset (RAM is lost). Callers should save critical state to flash first.
pub fn enter_backup_mode() {
    rtc::disable_all_periodic_callbacks();

    // Set the SLEEPDEEP bit so the next WFI enters BACKUP mode.
    // SAFETY: setting the SLEEPDEEP bit in the SCR is safe.
    unsafe {
        let scb = &*cortex_m::peripheral::SCB::PTR;
        scb.scr.modify(|scr| scr | (1 << 2));
    }

    // Enter BACKUP mode. This does not return until a wake event resets the
    // device.
    cortex_m::asm::wfi();

    // Clear SLEEPDEEP in case we somehow return (e.g. a spurious wake).
    // SAFETY: clearing the SLEEPDEEP bit in the SCR is safe.
    unsafe {
        let scb = &*cortex_m::peripheral::SCB::PTR;
        scb.scr.modify(|scr| scr & !(1 << 2));
    }
}
