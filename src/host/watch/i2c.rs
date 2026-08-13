//! Host I2C mock dispatch. The default mock has no sensor attached.

use super::seam;
use sensor_watch_core::safety::valid_i2c_address;

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
pub fn write16_checked(addr: i16, reg: u8, data: u16) -> Result<(), I2cError> {
    if !valid_i2c_address(addr) {
        return Err(I2cError::InvalidAddress);
    }
    seam::with_current_hw(|hw| hw.i2c_write16(addr, reg, data))
        .map_err(|_| I2cError::Nack)
}

pub fn read16_checked(addr: i16, reg: u8) -> Result<u16, I2cError> {
    if !valid_i2c_address(addr) {
        return Err(I2cError::InvalidAddress);
    }
    seam::with_current_hw(|hw| hw.i2c_read16(addr, reg)).map_err(|_| I2cError::Nack)
}
pub fn write8(_addr: i16, _reg: u8, _data: u8) {}
pub fn read8(_addr: i16, _reg: u8) -> u8 {
    0
}
pub fn send(_addr: i16, _buf: &[u8]) {}
pub fn receive(_addr: i16, buf: &mut [u8]) {
    buf.fill(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_operations_reject_reserved_addresses_before_using_the_seam() {
        assert_eq!(write16_checked(0x07, 0, 0), Err(I2cError::InvalidAddress));
        assert_eq!(read16_checked(0x78, 0), Err(I2cError::InvalidAddress));
    }
}
