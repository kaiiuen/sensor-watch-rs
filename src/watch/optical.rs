//! Optical receiver boundary for the current board revision.
//!
//! The main board has no proven light sensor: `LIGHT` is the user button. An
//! optical receiver would require an external accessory ADC, so this module is
//! intentionally disabled and performs no GPIO or ADC access.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Disabled,
    SensorUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    SensorUnavailable,
    Disabled,
}

pub const fn state() -> State {
    State::SensorUnavailable
}

/// Polling is an explicit no-op until a board-specific external receiver exists.
pub fn poll() -> Result<(), Error> {
    Err(Error::SensorUnavailable)
}

pub const fn status_text() -> &'static str {
    "OPTICAL disabled: SensorUnavailable (LIGHT is a button; external ADC required)"
}
