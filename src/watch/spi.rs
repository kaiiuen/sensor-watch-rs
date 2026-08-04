//! SPI controller driver.
//!
//! Port of the C `watch_spi.c`. Uses SERCOM3 in SPI master mode. The SPI bus
//! uses A1 (SCK), A2 (MOSI), A4 (MISO), and A3 (CS, not managed here).

use crate::watch::gpio::{self, Direction, Function, Pin};
use crate::watch::timeout::wait_until;
use atsaml22j::sercom0::spi::Spi;

/// SPI pins (A1=SCK, A2=MOSI, A4=MISO).
const A1: Pin = Pin(1, 1);
const A2: Pin = Pin(1, 2);
const A4: Pin = Pin(1, 0);

/// Returns a reference to the SERCOM3 SPI register block.
fn spi() -> &'static Spi {
    // SAFETY: the SERCOM3 register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { (*atsaml22j::Sercom3::PTR).spi() }
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
    wait_until(|| spi().syncbusy().read().bits() == 0);
}

/// Enables the SPI peripheral.
pub fn enable_spi() {
    // Enable clocks: SERCOM3 core (GCLK0) and slow (GCLK3), plus the APBC clock.
    gclk()
        .pchctrl(19)
        .write(|w| w.r#gen().gclk0().chen().set_bit());
    gclk()
        .pchctrl(15)
        .write(|w| w.r#gen().gclk3().chen().set_bit());
    mclk().apbcmask().modify(|_, w| w.sercom3_().set_bit());

    // Configure the SPI pins for SERCOM function C (PMUX value 2).
    gpio::set_pin_direction(A1, Direction::Out);
    gpio::set_pin_function(A1, Function::Mux(2));
    gpio::set_pin_direction(A2, Direction::Out);
    gpio::set_pin_function(A2, Function::Mux(2));
    gpio::set_pin_direction(A4, Direction::In);
    gpio::set_pin_function(A4, Function::Mux(2));

    // Software reset.
    if !spi().syncbusy().read().swrst().bit_is_set() {
        if spi().ctrla().read().enable().bit_is_set() {
            spi().ctrla().modify(|_, w| w.enable().clear_bit());
            sync();
        }
        spi().ctrla().write(|w| w.swrst().set_bit());
    }
    sync();

    // Configure: SPI master mode (0x2), 8-bit chars, master slave select.
    // SAFETY: writing valid CTRLA/CTRLB/BAUD values.
    unsafe {
        spi().ctrla().write(|w| w.mode().bits(0x2));
        spi().ctrlb().modify(|_, w| w.chsize().bits(0));
        spi().ctrlb().modify(|_, w| w.mssen().set_bit());
        spi().baud().write(|w| w.bits(19)); // ~100 kHz at 4 MHz
    }

    // Enable the peripheral.
    spi().ctrla().modify(|_, w| w.enable().set_bit());
    sync();
}

/// Disables the SPI peripheral.
pub fn disable_spi() {
    spi().ctrla().modify(|_, w| w.enable().clear_bit());
    mclk().apbcmask().modify(|_, w| w.sercom3_().clear_bit());
}

/// Writes a series of bytes to a device on the SPI bus.
pub fn write(buf: &[u8]) -> bool {
    for &byte in buf {
        // Wait for the data register to be empty (bounded).
        if !wait_until(|| spi().intflag().read().dre().bit_is_set()) {
            return false;
        }
        // SAFETY: writing a valid DATA value.
        unsafe {
            spi().data().write(|w| w.bits(byte as u32));
        }
    }
    // Wait for transmission to complete (bounded).
    wait_until(|| spi().intflag().read().txc().bit_is_set())
}

/// Reads a series of bytes from a device on the SPI bus.
pub fn read(buf: &mut [u8]) -> bool {
    for byte in buf.iter_mut() {
        // Send a dummy byte to clock in data.
        // SAFETY: writing a valid DATA value.
        unsafe {
            spi().data().write(|w| w.bits(0xFF));
        }
        // Wait for the receive buffer to be full (bounded).
        if !wait_until(|| spi().intflag().read().rxc().bit_is_set()) {
            return false;
        }
        *byte = spi().data().read().bits() as u8;
    }
    true
}

/// Reads and writes a series of bytes on the SPI bus.
pub fn transfer(data_out: &[u8], data_in: &mut [u8]) -> bool {
    let len = data_out.len().min(data_in.len());
    for i in 0..len {
        // Wait for the data register to be empty (bounded).
        if !wait_until(|| spi().intflag().read().dre().bit_is_set()) {
            return false;
        }
        // SAFETY: writing a valid DATA value.
        unsafe {
            spi().data().write(|w| w.bits(data_out[i] as u32));
        }
        // Wait for the receive buffer to be full (bounded).
        if !wait_until(|| spi().intflag().read().rxc().bit_is_set()) {
            return false;
        }
        data_in[i] = spi().data().read().bits() as u8;
    }
    true
}
