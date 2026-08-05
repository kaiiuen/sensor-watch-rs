//! Hardware abstraction layer for the Sensor-Watch.
//!
//! This module is a Rust port of the C `watch-library`. Each submodule wraps a
//! peripheral of the SAM L22 (RTC, SLCD, buttons, LED, buzzer, I2C/SPI/UART, ...)
//! behind a safe, idiomatic Rust API.

pub mod adc;
pub mod buzzer;
pub mod clock;
pub mod crc;
pub mod deepsleep;
pub mod extint;
pub mod gpio;
pub mod i2c;
pub mod irq;
pub mod led;
pub mod rtc;
pub mod slcd;
pub mod spi;
pub mod storage;
pub mod timeout;
pub mod uart;
pub mod utility;
pub mod utz;
pub mod wdt;
pub mod zones;

/// Initializes the hardware in dependency order.
///
/// The order matters: each peripheral must be ready before anything that
/// depends on it is used.
///   1. Interrupt priorities (before any interrupt is enabled)
///   2. Clocks (the 32 kHz crystal and GCLK routing that everything needs)
///   3. RTC (depends on the 32 kHz clock)
///   4. Watchdog (the hang backstop)
pub fn init() {
    irq::init();
    clock::init();
    rtc::init();
    wdt::init();
}
