//! Host calibration deliberately has no physical thermistor backend.
//! Returning `Unavailable` prevents tests or host tools from inventing an
//! environment and mirrors a missing sensor on hardware. Studio may explicitly
//! provide a simulated Celsius value through the host `Hw` seam.

use crate::watch::seam;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error {
    Unavailable,
}

#[derive(Default)]
pub struct Thermistor;

impl Thermistor {
    pub const fn new() -> Self {
        Self
    }

    pub fn begin(&mut self) {}

    pub fn read_celsius(&mut self) -> Result<f32, Error> {
        seam::with_current_hw(|hw| {
            hw.get_thermistor_temperature_celsius()
                .ok_or(Error::Unavailable)
        })
    }
}
