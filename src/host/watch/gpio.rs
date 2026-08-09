//! Host GPIO shim for the three button pins.
//!
//! The real `src/watch/gpio.rs` reads the SAM L22 PORT registers. On host, the
//! only GPIO the migrated faces/entry currently touch is the three buttons, so
//! this shim maps a button `Pin` back to its logical
//! [`Button`](sensor_watch_core::mock_hw::Button) and reads the level from the
//! installed mock via the `Hw` seam.

use super::seam;
use sensor_watch_core::mock_hw::Button;

/// A pin, encoded as (port, pin number) — same layout as the real HAL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin(pub u8, pub u8);

/// The pin direction enum (kept for signature parity with the real HAL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Off,
    In,
    Out,
}

/// Returns true if the given button pin is logically pressed (high).
///
/// Maps the three known button pins to a [`Button`]; any other pin (e.g. the
/// flashlight face's A2 output) is forwarded to the `Hw::read_pin_level` hook so
/// the mock records its level.
pub fn get_pin_level(pin: Pin) -> bool {
    let button = match pin {
        Pin(0, 2) => Button::Alarm,
        Pin(0, 22) => Button::Light,
        Pin(0, 23) => Button::Mode,
        _ => return seam::hw().read_pin_level((pin.0, pin.1)),
    };
    seam::hw().get_button_level(button)
}

/// Sets a GPIO pin's direction. Host forwards a boolean (`Direction::Out` =
/// `true`) to the `Hw::set_pin_direction` hook; recorded as the direction shadow.
pub fn set_pin_direction(pin: Pin, direction: Direction) {
    seam::hw().set_pin_direction((pin.0, pin.1), direction == Direction::Out);
}

/// Sets a GPIO pin's output level. Host forwards to `Hw::set_pin_level`.
pub fn set_pin_level(pin: Pin, level: bool) {
    seam::hw().set_pin_level((pin.0, pin.1), level);
}
