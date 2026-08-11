//! I2C controller driver.
//!
//! Port of the C `watch_i2c.c`. Uses SERCOM1 in I2C master mode to talk to
//! devices on the 9-pin connector's I2C bus (SDA=PB30, SCL=PB31).

use crate::watch::gpio::{self, Direction, Function, Pin};
use crate::watch::timeout::wait_until;
use atsaml22j::sercom0::i2cm::I2cm;

/// Errors reported by the SERCOM I2C master.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I2cError {
    InvalidAddress,
    Timeout,
    Nack,
    Bus,
}

/// The I2C pins (SDA=PB30, SCL=PB31).
const SDA: Pin = Pin(1, 30);
const SCL: Pin = Pin(1, 31);

/// Returns a reference to the SERCOM1 I2C master register block.
fn i2cm() -> &'static I2cm {
    // SAFETY: the SERCOM1 register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { (*atsaml22j::Sercom1::PTR).i2cm() }
}

/// Returns a reference to the MCLK peripheral register block.
fn mclk() -> &'static atsaml22j::mclk::RegisterBlock {
    // SAFETY: the MCLK register block lives at a fixed address for the whole
    // program.
    unsafe { &*atsaml22j::Mclk::PTR }
}

/// Returns a reference to the GCLK peripheral register block.
fn gclk() -> &'static atsaml22j::gclk::RegisterBlock {
    // SAFETY: the GCLK register block lives at a fixed address for the whole
    // program.
    unsafe { &*atsaml22j::Gclk::PTR }
}

/// Waits for the SERCOM to finish synchronizing.
fn sync() {
    let _ = wait_until(|| i2cm().syncbusy().read().bits() == 0);
}

/// Enables the I2C peripheral.
pub fn enable_i2c() {
    // Enable clocks: SERCOM1 core (GCLK0) and slow (GCLK3), plus the APBC clock.
    gclk()
        .pchctrl(13)
        .write(|w| w.r#gen().gclk0().chen().set_bit());
    gclk()
        .pchctrl(14)
        .write(|w| w.r#gen().gclk3().chen().set_bit());
    mclk().apbcmask().modify(|_, w| w.sercom1_().set_bit());

    // Configure the SDA/SCL pins for SERCOM function C (PMUX value 2).
    gpio::set_pin_direction(SDA, Direction::In);
    gpio::set_pin_function(SDA, Function::Mux(2));
    gpio::set_pin_direction(SCL, Direction::In);
    gpio::set_pin_function(SCL, Function::Mux(2));

    // Software reset.
    if !i2cm().syncbusy().read().swrst().bit_is_set() {
        if i2cm().ctrla().read().enable().bit_is_set() {
            i2cm().ctrla().modify(|_, w| w.enable().clear_bit());
            sync();
        }
        i2cm().ctrla().write(|w| w.swrst().set_bit());
    }
    sync();

    // Configure: I2C master mode, 100 kHz (BAUD for 4 MHz clock).
    // SAFETY: writing valid CTRLA/BAUD values.
    unsafe {
        i2cm().ctrla().write(|w| w.mode().bits(0x4));
        i2cm().baud().write(|w| w.bits(19)); // 100 kHz at 4 MHz
    }

    // Enable the peripheral.
    i2cm().ctrla().modify(|_, w| w.enable().set_bit());
    sync();
}

/// Disables the I2C peripheral.
pub fn disable_i2c() {
    i2cm().ctrla().modify(|_, w| w.enable().clear_bit());
    mclk().apbcmask().modify(|_, w| w.sercom1_().clear_bit());
}

/// Reconfigures the I2C pins as floating GPIO inputs before sleep.
///
/// Any sensor board on the 9-pin connector is powered from the same LDO rail
/// as the SAM L22, so it can backward-power itself through the SDA/SCL pull-up
/// lines while the CPU sleeps. Reconfiguring the pins to floating inputs (no
/// pull, no peripheral function) halts that leakage. They are restored to
/// SERCOM function C by `enable_i2c` on the next wake.
pub fn pins_to_floating_before_sleep() {
    gpio::set_pin_function(SDA, Function::Off);
    gpio::set_pin_direction(SDA, Direction::Off);
    gpio::set_pin_function(SCL, Function::Off);
    gpio::set_pin_direction(SCL, Direction::Off);
}

/// Sends a series of bytes to a device on the I2C bus.
pub fn send(addr: i16, buf: &[u8]) {
    if !crate::watch::safety::valid_i2c_address(addr) {
        disable_i2c();
        return;
    }
    // Set the peripheral address (7-bit) and issue a START condition.
    // SAFETY: writing a valid ADDR value.
    unsafe {
        i2cm().addr().write(|w| w.bits(((addr as u32) & 0x7F) << 1));
    }
    // Wait for the address to be acknowledged (bounded).
    if wait_until(|| i2cm().status().read().busstate().bits() == 1).is_err() {
        return;
    }

    for &byte in buf {
        // SAFETY: writing a valid DATA value.
        unsafe {
            i2cm().data().write(|w| w.bits(byte));
        }
        // Wait for the data to be transmitted (master on bus flag), bounded.
        if wait_until(|| i2cm().intflag().read().mb().bit_is_set()).is_err() {
            return;
        }
    }

    // Issue a STOP condition.
    // SAFETY: writing a valid CTRLB CMD value (STOP = 0x3).
    unsafe {
        i2cm()
            .ctrlb()
            .modify(|r, w| w.bits((r.bits() & !(0x3 << 16)) | (0x3 << 16)));
    }
}

/// Receives a series of bytes from a device on the I2C bus.
pub fn receive(addr: i16, buf: &mut [u8]) {
    if !crate::watch::safety::valid_i2c_address(addr) {
        disable_i2c();
        return;
    }
    // Set the peripheral address (7-bit, read) and issue a START condition.
    // SAFETY: writing a valid ADDR value.
    unsafe {
        i2cm()
            .addr()
            .write(|w| w.bits(((addr as u32) & 0x7F) << 1 | 1));
    }
    // Wait for the address to be acknowledged (bounded).
    if wait_until(|| i2cm().status().read().busstate().bits() == 1).is_err() {
        return;
    }

    for byte in buf.iter_mut() {
        // Wait for data to be ready (slave on bus flag), bounded.
        if wait_until(|| i2cm().intflag().read().sb().bit_is_set()).is_err() {
            return;
        }
        *byte = i2cm().data().read().bits();
    }

    // Issue a STOP condition.
    // SAFETY: writing a valid CTRLB CMD value (STOP = 0x3).
    unsafe {
        i2cm()
            .ctrlb()
            .modify(|r, w| w.bits((r.bits() & !(0x3 << 16)) | (0x3 << 16)));
    }
}

/// Writes a byte to a register in an I2C device (legacy best-effort API).
pub fn write8(addr: i16, reg: u8, data: u8) {
    let _ = write8_checked(addr, reg, data);
}

/// Checked register write used by sensor drivers.
pub fn write8_checked(addr: i16, reg: u8, data: u8) -> Result<(), I2cError> {
    if !crate::watch::safety::valid_i2c_address(addr) {
        return Err(I2cError::InvalidAddress);
    }
    unsafe { i2cm().addr().write(|w| w.bits(((addr as u32) & 0x7F) << 1)) };
    for byte in [reg, data] {
        if wait_until(|| i2cm().intflag().read().mb().bit_is_set()).is_err() {
            return Err(I2cError::Timeout);
        }
        unsafe { i2cm().data().write(|w| w.bits(byte)) };
    }
    if wait_until(|| i2cm().intflag().read().mb().bit_is_set()).is_err() {
        return Err(I2cError::Timeout);
    }
    unsafe {
        i2cm()
            .ctrlb()
            .modify(|r, w| w.bits((r.bits() & !(0x3 << 16)) | (0x3 << 16)))
    };
    Ok(())
}

pub fn write16_checked(addr: i16, reg: u8, data: u16) -> Result<(), I2cError> {
    if !crate::watch::safety::valid_i2c_address(addr) {
        return Err(I2cError::InvalidAddress);
    }
    unsafe { i2cm().addr().write(|w| w.bits(((addr as u32) & 0x7F) << 1)) };
    for byte in [reg, (data >> 8) as u8, data as u8] {
        if wait_until(|| i2cm().intflag().read().mb().bit_is_set()).is_err() {
            return Err(I2cError::Timeout);
        }
        unsafe { i2cm().data().write(|w| w.bits(byte)) };
    }
    if wait_until(|| i2cm().intflag().read().mb().bit_is_set()).is_err() {
        return Err(I2cError::Timeout);
    }
    unsafe {
        i2cm()
            .ctrlb()
            .modify(|r, w| w.bits((r.bits() & !(0x3 << 16)) | (0x3 << 16)))
    };
    Ok(())
}

/// Reads a byte from a register in an I2C device.
pub fn read8(addr: i16, reg: u8) -> u8 {
    read8_checked(addr, reg).unwrap_or(0)
}

pub fn read8_checked(addr: i16, reg: u8) -> Result<u8, I2cError> {
    let mut data = [0u8; 1];
    write_read(addr, &[reg], &mut data)?;
    Ok(data[0])
}

pub fn read16_checked(addr: i16, reg: u8) -> Result<u16, I2cError> {
    let mut data = [0u8; 2];
    write_read(addr, &[reg], &mut data)?;
    Ok(u16::from_be_bytes(data))
}

/// Reads an unsigned big-endian word from a register in an I2C device.
///
/// Register-oriented sensors conventionally transmit the high byte first;
/// `read16_le` is intentionally separate for devices such as LIS2DW output
/// registers that document little-endian samples.
pub fn read16(addr: i16, reg: u8) -> u16 {
    let mut data = [0u8; 2];
    if write_read(addr, &[reg], &mut data).is_err() {
        return 0;
    }
    u16::from_be_bytes(data)
}

/// Reads a little-endian word from a register.
pub fn read16_le(addr: i16, reg: u8) -> u16 {
    let mut data = [0u8; 2];
    if write_read(addr, &[reg], &mut data).is_err() {
        return 0;
    }
    u16::from_le_bytes(data)
}

/// Performs a register-select followed by a read without a STOP between them.
/// This is the repeated-start transaction required by register I2C sensors.
pub fn write_read(addr: i16, write: &[u8], read: &mut [u8]) -> Result<(), I2cError> {
    if !crate::watch::safety::valid_i2c_address(addr) {
        disable_i2c();
        return Err(I2cError::InvalidAddress);
    }
    if write.is_empty() || read.is_empty() {
        return Err(I2cError::Bus);
    }
    unsafe { i2cm().addr().write(|w| w.bits(((addr as u32) & 0x7F) << 1)) };
    if wait_until(|| i2cm().intflag().read().mb().bit_is_set()).is_err() {
        return Err(I2cError::Timeout);
    }
    for &byte in write {
        unsafe { i2cm().data().write(|w| w.bits(byte)) };
        if wait_until(|| i2cm().intflag().read().mb().bit_is_set()).is_err() {
            return Err(I2cError::Timeout);
        }
    }
    // Writing ADDR while the bus is active creates a repeated START.
    unsafe {
        i2cm()
            .addr()
            .write(|w| w.bits((((addr as u32) & 0x7F) << 1) | 1))
    };
    if wait_until(|| i2cm().intflag().read().sb().bit_is_set()).is_err() {
        return Err(I2cError::Timeout);
    }
    let read_len = read.len();
    for (index, byte) in read.iter_mut().enumerate() {
        *byte = i2cm().data().read().bits();
        if index + 1 < read_len {
            unsafe { i2cm().ctrlb().modify(|r, w| w.bits(r.bits() & !(1 << 18))) };
        } else {
            unsafe { i2cm().ctrlb().modify(|r, w| w.bits(r.bits() | (1 << 18))) };
        }
        if index + 1 < read_len && wait_until(|| i2cm().intflag().read().sb().bit_is_set()).is_err()
        {
            return Err(I2cError::Timeout);
        }
    }
    unsafe {
        i2cm()
            .ctrlb()
            .modify(|r, w| w.bits((r.bits() & !(0x3 << 16)) | (0x3 << 16)))
    };
    Ok(())
}

/// Reads three bytes as an unsigned little-endian int from a register.
pub fn read24(addr: i16, reg: u8) -> u32 {
    send(addr, &[reg]);
    let mut data = [0u8; 3];
    receive(addr, &mut data);
    u32::from_le_bytes([data[0], data[1], data[2], 0])
}

/// Reads an unsigned little-endian int from a register in an I2C device.
pub fn read32(addr: i16, reg: u8) -> u32 {
    send(addr, &[reg]);
    let mut data = [0u8; 4];
    receive(addr, &mut data);
    u32::from_le_bytes(data)
}
