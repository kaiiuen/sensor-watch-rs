//! Hardware abstraction layer for the Sensor-Watch.
//!
//! This module is a Rust port of the C `watch-library`. Each submodule wraps a
//! peripheral of the SAM L22 (RTC, SLCD, buttons, LED, buzzer, I2C/SPI/UART, ...)
//! behind a safe, idiomatic Rust API.

pub mod rtc;
