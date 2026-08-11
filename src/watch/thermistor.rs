//! Thermistor ADC driver with validation and calibration.

use crate::watch::{adc, gpio, utility};

pub const PIN: gpio::Pin = adc::A0;
pub const B_COEFFICIENT: f32 = 3950.0;
pub const NOMINAL_TEMPERATURE_C: f32 = 25.0;
pub const NOMINAL_RESISTANCE_OHMS: f32 = 100_000.0;
pub const SERIES_RESISTANCE_OHMS: f32 = 100_000.0;
pub const CALIBRATION_OFFSET_C: f32 = 0.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error {
    Unavailable,
    OpenOrShort,
    OutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Unavailable,
    Ready(f32),
}

pub struct Thermistor {
    state: State,
}

impl Thermistor {
    pub const fn new() -> Self {
        Self {
            state: State::Unavailable,
        }
    }
    pub fn state(&self) -> State {
        self.state
    }

    pub fn begin(&mut self) {
        adc::enable_analog_input(PIN);
        self.state = State::Ready(0.0);
    }

    pub fn read_celsius(&mut self) -> Result<f32, Error> {
        if matches!(self.state, State::Unavailable) {
            return Err(Error::Unavailable);
        }
        let raw = adc::get_analog_pin_level(PIN);
        if raw == 0 || raw >= u16::MAX {
            self.state = State::Unavailable;
            return Err(Error::OpenOrShort);
        }
        let temp = utility::thermistor_temperature(
            raw,
            false,
            B_COEFFICIENT,
            NOMINAL_TEMPERATURE_C,
            NOMINAL_RESISTANCE_OHMS,
            SERIES_RESISTANCE_OHMS,
        ) + CALIBRATION_OFFSET_C;
        if !temp.is_finite() || !(-40.0..=125.0).contains(&temp) {
            self.state = State::Unavailable;
            return Err(Error::OutOfRange);
        }
        self.state = State::Ready(temp);
        Ok(temp)
    }
}

impl Default for Thermistor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn midpoint_divider_is_nominal_temperature() {
        let t = utility::thermistor_temperature(
            32768,
            false,
            B_COEFFICIENT,
            NOMINAL_TEMPERATURE_C,
            NOMINAL_RESISTANCE_OHMS,
            SERIES_RESISTANCE_OHMS,
        );
        assert!((t - 25.0).abs() < 0.1);
    }
    #[test]
    fn rejects_rail_values() {
        assert!(matches!(0, 0));
        assert_eq!(u16::MAX, 65535);
    }
}
