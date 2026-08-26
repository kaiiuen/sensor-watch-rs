//! Interrupt priority configuration.
//!
//! On the SAM L22 (ARMv6-M), a lower priority value means higher urgency.
//! We assign explicit priorities so a critical interrupt (RTC alarm) can
//! never be blocked by a less critical one (button, tick). This makes the
//! event dispatcher deterministic.

use atsaml22j::Interrupt;

/// Priority values. Lower number = higher urgency.
///
/// - RTC alarm: highest (must always fire to wake the watch)
/// - RTC tick: high (the 1 Hz heartbeat)
/// - Buttons (EIC): medium
/// - TC3 (buzzer sequences): low
const PRIO_RTC_ALARM: u8 = 0;
const PRIO_RTC_TICK: u8 = 1;
const PRIO_EIC: u8 = 2;
const PRIO_TC3: u8 = 3;

/// Software-only UART wake status adapter; no vector is installed yet.
pub fn capture_uart_interrupt_status(status: crate::movement::uart_policy::UartInterruptStatus) {
    crate::watch::uart::capture_uart_wake_status(status);
}

/// Configures the interrupt priorities for all interrupts used by the system.
///
/// Must be called once at boot, before any interrupt is enabled.
pub fn init() {
    // SAFETY: setting NVIC priorities is safe at boot before interrupts are
    // enabled; the NVIC peripheral is taken once here.
    let mut nvic = cortex_m::Peripherals::take().unwrap().NVIC;
    // SAFETY: these are valid priorities for the given interrupts.
    unsafe {
        nvic.set_priority(Interrupt::RTC, PRIO_RTC_ALARM);
        nvic.set_priority(Interrupt::EIC, PRIO_EIC);
        nvic.set_priority(Interrupt::TC3, PRIO_TC3);
    }
}
