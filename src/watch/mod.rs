//! Hardware abstraction layer for the Sensor-Watch.
//!
//! This module is a Rust port of the C `watch-library`. Each submodule wraps a
//! peripheral of the SAM L22 (RTC, SLCD, buttons, LED, buzzer, I2C/SPI/UART, ...)
//! behind a safe, idiomatic Rust API.
//!
//! # Testing the real firmware faces on the host (the "seam")
//!
//! The firmware faces (`movement/*_face.rs`) are `#![no_std]` and call the
//! machine-independent free functions in this module (`slcd::display_string`,
//! `rtc::get_date_time`, `gpio::get_pin_level`, `adc::get_vcc_voltage`, ...) that
//! touch SAM L22 MMIO registers directly. Because the backing code is tied to
//! `atsaml22j` MMIO statics, none of it can be compiled or executed on the host,
//! which is why `studio/src/face_sim.rs` reimplements the faces by hand and can
//! silently drift from the firmware.
//!
//! The long-term goal is a seam so the SIMULATOR and FUZZER run the *real* face
//! code instead of `face_sim.rs`. This crate is currently **binary-only**
//! (`#![no_main]`), so it cannot yet be linked into a host binary: introducing a
//! host-run-able lib target that cfg-gates the ARM startup (`cortex-m-rt`) and
//! the ~100 MMIO modules is itself a substantial refactor, and is deliberately
//! staged separately to keep the `--target thumbv6m-none-eabi` firmware build
//! byte-identical.
//!
//! Meanwhile, the *mechanism* is proven and documented in two places:
//!
//! - `core/src/hostsim.rs` - a standalone, dependency-free **host POC** that
//!   copies the `simple_clock` face logic verbatim and compiles/runs it against a
//!   mock `Hw` backend (the exact code a real host seam would drive).
//! - `core/src/mock_hw.rs` - the reusable `Hw` trait + reference mock.
//!
//! When the firmware later gains a lib target, the pattern is: re-export a
//! `Hw`-typed dispatch, route the handful of HAL frees through it (defaulting to
//! the real MMIO impl on-target), and keep `simple_clock`'s `#[cfg(test)]` host
//! module as the template for the other 110 faces. See `core/src/hostsim.rs` for
//! the detailed "extending to all faces" guide.

pub mod adc;
pub mod buzzer;
pub mod clock;
pub mod crc;
pub mod deepsleep;
pub mod ecc;
pub mod event_log;
pub mod extint;
pub mod gpio;
pub mod i2c;
pub mod irq;
pub mod led;
pub mod lis2dw;
pub mod logging;
pub mod memory;
pub mod opt3001;
#[cfg(feature = "optical")]
pub mod optical;
pub mod rtc;
pub mod safety;
pub mod shell;
pub mod slcd;
pub mod spi;
pub mod storage;
pub mod thermistor;
pub mod timeout;
pub mod uart;
#[cfg(feature = "usb-cdc")]
pub mod usb;
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
