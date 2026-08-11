//! Host I2C mock dispatch. The default mock has no sensor attached.

use super::seam;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I2cError {
    InvalidAddress,
    Timeout,
    Nack,
    Bus,
}

pub fn enable_i2c() {}
pub fn disable_i2c() {}
pub fn pins_to_floating_before_sleep() {}
pub fn write16_checked(_addr: i16, _reg: u8, _data: u16) -> Result<(), I2cError> {
    seam::hw()
        .i2c_write16(_addr, _reg, _data)
        .map_err(|_| I2cError::Nack)
}
pub fn read16_checked(_addr: i16, _reg: u8) -> Result<u16, I2cError> {
    seam::hw()
        .i2c_read16(_addr, _reg)
        .map_err(|_| I2cError::Nack)
}
pub fn write8(_addr: i16, _reg: u8, _data: u8) {}
pub fn read8(_addr: i16, _reg: u8) -> u8 {
    0
}
pub fn send(_addr: i16, _buf: &[u8]) {}
pub fn receive(_addr: i16, buf: &mut [u8]) {
    buf.fill(0);
}
