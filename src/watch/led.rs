//! Bi-color LED driver.
//!
//! Port of the C `watch_led.c` and the TCC enable/disable code from
//! `watch_private.c`. The LEDs are driven by TCC0 in normal PWM mode.

use crate::watch::gpio::{self, Direction, Function, Pin};
use atsaml22j::tcc0::RegisterBlock as Tcc0;
use atsaml22j::tcc0::ctrla::Prescalerselect;
use atsaml22j::tcc0::wave::Wavegenselect;

/// LED pins and TCC channels (OSO-SWAT-A1-05 board, red/green edition).
const RED: Pin = Pin(0, 20); // PA20
const GREEN: Pin = Pin(0, 21); // PA21
const BUZZER: Pin = Pin(0, 27); // PA27

/// TCC channels for the LEDs and buzzer.
const RED_TCC_CHANNEL: usize = 2;
const GREEN_TCC_CHANNEL: usize = 3;
const BUZZER_TCC_CHANNEL: usize = 1;

/// PMUX function value for the TCC0 outputs (function F = 5).
const TCC_PINMUX: u8 = 5;

/// Whether the LED polarity is inverted (common-anode, for Red/Pro boards).
static mut INVERT_POLARITY: bool = false;

/// Sets whether the LED polarity is inverted.
///
/// Red dev boards and Pro use a common-anode LED, so the polarity must be
/// inverted relative to the common-cathode green/blue boards.
pub fn set_invert_polarity(invert: bool) {
    unsafe {
        INVERT_POLARITY = invert;
    }
}

/// Returns a reference to the TCC0 peripheral register block.
fn tcc0() -> &'static Tcc0 {
    // SAFETY: the TCC0 register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Tcc0::PTR }
}

/// Returns a reference to the GCLK peripheral register block.
fn gclk() -> &'static atsaml22j::gclk::RegisterBlock {
    // SAFETY: the GCLK register block lives at a fixed address for the whole
    // program.
    unsafe { &*atsaml22j::Gclk::PTR }
}

/// Returns a reference to the MCLK peripheral register block.
fn mclk() -> &'static atsaml22j::mclk::RegisterBlock {
    // SAFETY: the MCLK register block lives at a fixed address for the whole
    // program.
    unsafe { &*atsaml22j::Mclk::PTR }
}

/// Waits for the TCC to finish synchronizing.
fn sync() {
    while tcc0().syncbusy().read().bits() != 0 {}
}

/// Returns true if the TCC0 peripheral is enabled.
pub fn is_enabled() -> bool {
    tcc0().ctrla().read().enable().bit_is_set()
}

/// Enables the bi-color LED.
pub fn enable_leds() {
    if !is_enabled() {
        enable_tcc();
    }
}

/// Disables the LEDs (and the buzzer, which shares the TCC peripheral).
pub fn disable_leds() {
    disable_tcc();
}

/// Sets the LED to a custom color by modulating each output's duty cycle.
pub fn set_led_color(red: u8, green: u8) {
    set_led_color_rgb(red, green, green);
}

/// Sets the LED to a custom RGB color by modulating each output's duty cycle.
pub fn set_led_color_rgb(red: u8, green: u8, _blue: u8) {
    if is_enabled() {
        let period = tcc0().per().read().bits();
        // SAFETY: writing valid compare-buffer values.
        unsafe {
            tcc0()
                .ccbuf(RED_TCC_CHANNEL)
                .write(|w| w.bits((period * red as u32 * 1000) / 255000));
            tcc0()
                .ccbuf(GREEN_TCC_CHANNEL)
                .write(|w| w.bits((period * green as u32 * 1000) / 255000));
        }
    }
}

/// Sets the red LED to full brightness, and turns the green LED off.
pub fn set_led_red() {
    set_led_color(255, 0);
}

/// Sets the green LED to full brightness, and turns the red LED off.
pub fn set_led_green() {
    set_led_color(0, 255);
}

/// Sets both red and green LEDs to full brightness.
pub fn set_led_yellow() {
    set_led_color(255, 255);
}

/// Turns both LEDs off.
pub fn set_led_off() {
    set_led_color(0, 0);
}

/// Enables TCC0 for PWM (port of `_watch_enable_tcc`).
fn enable_tcc() {
    // Clock TCC0 with the main clock (GCLK0) and enable the peripheral clock.
    gclk()
        .pchctrl(22)
        .write(|w| w.r#gen().gclk0().chen().set_bit());
    mclk().apbcmask().modify(|_, w| w.tcc0_().set_bit());

    // Disable and reset TCC0.
    tcc0().ctrla().modify(|_, w| w.enable().clear_bit());
    sync();
    tcc0().ctrla().write(|w| w.swrst().set_bit());
    sync();

    // Divide the clock down to 1 MHz. Without USB, the main clock is 4 MHz, so
    // use DIV4. (USB support is not yet ported; assume no USB.)
    tcc0()
        .ctrla()
        .modify(|_, w| w.prescaler().variant(Prescalerselect::Div4));

    // Normal PWM mode: period controlled by PER, duty cycle by each CC channel.
    tcc0()
        .wave()
        .modify(|_, w| w.wavegen().variant(Wavegenselect::Npwm));

    // Invert the output polarity for common-anode (Red/Pro) boards.
    if unsafe { INVERT_POLARITY } {
        tcc0().wave().modify(|_, w| {
            w.pol2().set_bit();
            w.pol3().set_bit()
        });
    }

    // Set a period for the LEDs (below 20000 to avoid flickering).
    // SAFETY: writing a valid period value.
    unsafe {
        tcc0().per().write(|w| w.bits(1024));
    }

    // Set the duty cycle of all pins to 0 (LEDs off, buzzer silent).
    // SAFETY: writing valid compare values.
    unsafe {
        tcc0().cc(BUZZER_TCC_CHANNEL).write(|w| w.bits(0));
        tcc0().cc(RED_TCC_CHANNEL).write(|w| w.bits(0));
        tcc0().cc(GREEN_TCC_CHANNEL).write(|w| w.bits(0));
    }

    // Enable the TCC.
    tcc0().ctrla().modify(|_, w| w.enable().set_bit());
    sync();

    // Enable the LED PWM pins.
    gpio::set_pin_direction(RED, Direction::Out);
    gpio::set_pin_function(RED, Function::Mux(TCC_PINMUX));
    gpio::set_pin_direction(GREEN, Direction::Out);
    gpio::set_pin_function(GREEN, Function::Mux(TCC_PINMUX));
}

/// Disables TCC0 and its PWM pins (port of `_watch_disable_tcc`).
fn disable_tcc() {
    // Disable all PWM pins.
    gpio::set_pin_direction(BUZZER, Direction::Off);
    gpio::set_pin_function(BUZZER, Function::Off);
    gpio::set_pin_direction(RED, Direction::Off);
    gpio::set_pin_function(RED, Function::Off);
    gpio::set_pin_direction(GREEN, Direction::Off);
    gpio::set_pin_function(GREEN, Function::Off);

    // Disable the TCC.
    tcc0().ctrla().modify(|_, w| w.enable().clear_bit());
    mclk().apbcmask().modify(|_, w| w.tcc0_().clear_bit());
}
