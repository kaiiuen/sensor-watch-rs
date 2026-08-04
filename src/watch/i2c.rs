//! I2C controller driver.
//!
//! Port of the C `watch_i2c.c`. Uses SERCOM1 in I2C master mode to talk to
//! devices on the 9-pin connector's I2C bus (SDA=PB30, SCL=PB31).

use crate::watch::gpio::{self, Direction, Function, Pin};
use atsaml22j::sercom0::i2cm::I2cm;

/// The I2C pins (SDA=PB30, SCL=PB31).
const SDA: Pin = Pin(1, 30);
const SCL: Pin = Pin(1, 31);

/// Returns a reference to the SERCOM1 I2C master register block.
fn i2cm() -> &'static I2cm {
    // SAFETY: the SERCOM1 register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &(*atsaml22j::Sercom1::PTR).i2cm() }
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
    while i2cm().syncbusy().read().bits() != 0 {}
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

/// Sends a series of bytes to a device on the I2C bus.
pub fn send(addr: i16, buf: &[u8]) {
    // Set the peripheral address (7-bit) and issue a START condition.
    // SAFETY: writing a valid ADDR value.
    unsafe {
        i2cm().addr().write(|w| w.bits(((addr as u32) & 0x7F) << 1));
    }
    // Wait for the address to be acknowledged.
    while i2cm().status().read().busstate().bits() != 1 {}

    for &byte in buf {
        // SAFETY: writing a valid DATA value.
        unsafe {
            i2cm().data().write(|w| w.bits(byte));
        }
        // Wait for the data to be transmitted (master on bus flag).
        while !i2cm().intflag().read().mb().bit_is_set() {}
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
    // Set the peripheral address (7-bit, read) and issue a START condition.
    // SAFETY: writing a valid ADDR value.
    unsafe {
        i2cm()
            .addr()
            .write(|w| w.bits(((addr as u32) & 0x7F) << 1 | 1));
    }
    // Wait for the address to be acknowledged.
    while i2cm().status().read().busstate().bits() != 1 {}

    for i in 0..buf.len() {
        // Wait for data to be ready (slave on bus flag).
        while !i2cm().intflag().read().sb().bit_is_set() {}
        buf[i] = i2cm().data().read().bits();
    }

    // Issue a STOP condition.
    // SAFETY: writing a valid CTRLB CMD value (STOP = 0x3).
    unsafe {
        i2cm()
            .ctrlb()
            .modify(|r, w| w.bits((r.bits() & !(0x3 << 16)) | (0x3 << 16)));
    }
}

/// Writes a byte to a register in an I2C device.
pub fn write8(addr: i16, reg: u8, data: u8) {
    send(addr, &[reg, data]);
}

/// Reads a byte from a register in an I2C device.
pub fn read8(addr: i16, reg: u8) -> u8 {
    send(addr, &[reg]);
    let mut data = [0u8; 1];
    receive(addr, &mut data);
    data[0]
}

/// Reads an unsigned little-endian word from a register in an I2C device.
pub fn read16(addr: i16, reg: u8) -> u16 {
    send(addr, &[reg]);
    let mut data = [0u8; 2];
    receive(addr, &mut data);
    u16::from_le_bytes(data)
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
