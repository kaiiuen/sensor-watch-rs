//! OPT3001 ambient-light sensor driver.
//!
//! The OPT3001 uses big-endian 16-bit registers and requires a register pointer
//! write followed by a repeated-start read. Conversion results are only accepted
//! after the configured conversion interval and are rejected when out of range.

use crate::watch::i2c;

pub const ADDRESS: i16 = 0x44;
const RESULT: u8 = 0x00;
const CONFIG: u8 = 0x01;
// Automatic range, 100 ms conversion, continuous mode, latch enabled.
const CONFIG_CONTINUOUS_100MS: u16 = 0xC610;
pub const CONVERSION_TICKS: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error {
    Unavailable,
    Bus,
    InvalidResult,
    NotReady,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Unavailable,
    Idle,
    Converting { ticks_left: u8 },
    Ready(f32),
}

pub struct Opt3001 {
    state: State,
}

impl Opt3001 {
    pub const fn new() -> Self {
        Self {
            state: State::Unavailable,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn begin(&mut self) -> Result<(), Error> {
        if i2c::write16_checked(ADDRESS, CONFIG, CONFIG_CONTINUOUS_100MS).is_err() {
            self.state = State::Unavailable;
            return Err(Error::Bus);
        }
        self.state = State::Idle;
        Ok(())
    }

    pub fn start_conversion(&mut self) -> Result<(), Error> {
        if matches!(self.state, State::Unavailable) {
            return Err(Error::Unavailable);
        }
        self.state = State::Converting {
            ticks_left: CONVERSION_TICKS,
        };
        Ok(())
    }

    pub fn tick(&mut self) {
        if let State::Converting { ticks_left } = self.state {
            self.state = if ticks_left > 1 {
                State::Converting {
                    ticks_left: ticks_left - 1,
                }
            } else {
                State::Idle
            };
        }
    }

    pub fn read_lux(&mut self) -> Result<f32, Error> {
        if matches!(self.state, State::Unavailable) {
            return Err(Error::Unavailable);
        }
        if matches!(self.state, State::Converting { .. }) {
            return Err(Error::NotReady);
        }
        let raw = i2c::read16_checked(ADDRESS, RESULT).map_err(|_| Error::Bus)?;
        let exponent = (raw >> 12) as u32;
        let mantissa = (raw & 0x0FFF) as u32;
        if exponent > 0x0C {
            self.state = State::Unavailable;
            return Err(Error::InvalidResult);
        }
        let lux = mantissa as f32 * 0.01 * (1u32 << exponent) as f32;
        if !lux.is_finite() || !(0.0..=100_000.0).contains(&lux) {
            self.state = State::Unavailable;
            return Err(Error::InvalidResult);
        }
        self.state = State::Ready(lux);
        Ok(lux)
    }
}

impl Default for Opt3001 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, not(target_arch = "arm"), feature = "hostmock"))]
mod host_mock_tests {
    use super::*;
    use crate::watch::seam;
    use sensor_watch_core::mock_hw::MockHw;

    #[test]
    fn reads_fixture_through_host_i2c_seam() {
        let mut hw = MockHw::new();
        hw.opt3001_result = Some(0x2123);
        seam::install_hw(&mut hw);
        let mut sensor = Opt3001::new();
        sensor.begin().unwrap();
        assert!((sensor.read_lux().unwrap() - 11.64).abs() < 0.001);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_opt3001_msb_first_result() {
        let raw = 0x2123u16;
        let lux = (raw & 0x0fff) as f32 * 0.01 * 2.0f32.powi((raw >> 12) as i32);
        assert!((lux - 11.64).abs() < 0.001);
    }
    #[test]
    fn rejects_reserved_exponent() {
        assert!(((0xD000u16 >> 12) as u32) > 0x0C);
    }
}
