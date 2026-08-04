//! Analog-to-digital converter driver.
//!
//! Port of the C `watch_adc.c`. Reads the battery voltage and the five
//! analog-capable pins on the 9-pin connector.

use crate::watch::gpio::{self, Direction, Function, Pin};
use atsaml22j::adc::RegisterBlock as Adc;
use atsaml22j::adc::avgctrl::Samplenumselect;
use atsaml22j::adc::ctrlb::Prescalerselect;
use atsaml22j::adc::inputctrl::Muxposselect;
use atsaml22j::adc::refctrl::Refselselect;

/// The five analog-capable pins on the 9-pin connector.
pub const A0: Pin = Pin(1, 4); // PB04 -> AIN12
pub const A1: Pin = Pin(1, 1); // PB01 -> AIN9
pub const A2: Pin = Pin(1, 2); // PB02 -> AIN10
pub const A3: Pin = Pin(1, 3); // PB03 -> AIN11
pub const A4: Pin = Pin(1, 0); // PB00 -> AIN8

/// The ADC reference voltage selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ReferenceVoltage {
    Intref = 0,
    VccDiv1Point6,
    VccDiv2,
    Vcc,
}

/// OTP5 fuse address and bit positions for ADC calibration.
const OTP5_ADDR: *const u32 = 0x0080_6020 as *const u32;
const BIASCOMP_POS: u32 = 3;
const BIASREFBUF_POS: u32 = 8;

/// Returns a reference to the ADC peripheral register block.
fn adc() -> &'static Adc {
    // SAFETY: the ADC register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Adc::PTR }
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

/// Returns a reference to the SUPC peripheral register block.
fn supc() -> &'static atsaml22j::supc::RegisterBlock {
    // SAFETY: the SUPC register block lives at a fixed address for the whole
    // program.
    unsafe { &*atsaml22j::Supc::PTR }
}

/// Waits for the ADC to finish synchronizing.
fn sync() {
    while adc().syncbusy().read().bits() != 0 {}
}

/// Reads an analog value from the given MUXPOS channel.
fn get_analog_value(channel: Muxposselect) -> u16 {
    if adc().inputctrl().read().muxpos().variant() != Some(channel) {
        adc().inputctrl().modify(|_, w| w.muxpos().variant(channel));
        sync();
    }

    adc().swtrig().modify(|_, w| w.start().set_bit());
    while !adc().intflag().read().resrdy().bit_is_set() {}

    adc().result().read().bits() as u16
}

/// Enables the ADC peripheral.
pub fn enable_adc() {
    mclk().apbcmask().modify(|_, w| w.adc_().set_bit());
    gclk()
        .pchctrl(25)
        .write(|w| w.r#gen().gclk0().chen().set_bit());

    // Read the calibration fuses from OTP5.
    // SAFETY: OTP5 is a valid memory-mapped fuse address.
    let otp = unsafe { *OTP5_ADDR };
    let biasrefbuf = ((otp >> BIASREFBUF_POS) & 0x7) as u8;
    let biascomp = ((otp >> BIASCOMP_POS) & 0x7) as u8;

    if !adc().syncbusy().read().swrst().bit_is_set() {
        if adc().ctrla().read().enable().bit_is_set() {
            adc().ctrla().modify(|_, w| w.enable().clear_bit());
            sync();
        }
        adc().ctrla().modify(|_, w| w.swrst().set_bit());
    }
    sync();

    // Without USB, the main clock is 4 MHz; divide by 8 for a 500 kHz ADC clock.
    adc()
        .ctrlb()
        .modify(|_, w| w.prescaler().variant(Prescalerselect::Div8));

    // SAFETY: writing valid calibration values.
    unsafe {
        adc().calib().modify(|_, w| {
            w.biascomp().bits(biascomp);
            w.biasrefbuf().bits(biasrefbuf)
        });
    }
    adc()
        .refctrl()
        .modify(|_, w| w.refsel().variant(Refselselect::Intvcc2));
    // MUXNEG = GND (0x18); the PAC enum only covers AIN0-7, so use raw bits.
    // SAFETY: 0x18 is the valid MUXNEG GND value.
    unsafe {
        adc()
            .inputctrl()
            .modify(|r, w| w.bits((r.bits() & !(0x1F << 8)) | (0x18 << 8)));
    }
    adc().ctrlc().modify(|_, w| {
        w.ressel()
            .variant(atsaml22j::adc::ctrlc::Resselselect::_16bit)
    });
    adc()
        .avgctrl()
        .modify(|_, w| w.samplenum().variant(Samplenumselect::_16));
    // SAFETY: 0 is a valid SAMPLEN value.
    unsafe { adc().sampctrl().modify(|_, w| w.samplen().bits(0)) };
    adc().intenset().modify(|_, w| w.resrdy().set_bit());
    adc().ctrla().modify(|_, w| w.enable().set_bit());
    sync();

    // Throw away one measurement after the reference change.
    get_analog_value(Muxposselect::Scaledcorevcc);
}

/// Configures the selected pin for analog input.
pub fn enable_analog_input(pin: Pin) {
    gpio::set_pin_direction(pin, Direction::Off);
    // The ADC pins use PMUX function B (value 1).
    gpio::set_pin_function(pin, Function::Mux(1));
}

/// Reads an analog value from one of the pins.
pub fn get_analog_pin_level(pin: Pin) -> u16 {
    match pin {
        A0 => get_analog_value(Muxposselect::Ain12),
        A1 => get_analog_value(Muxposselect::Ain9),
        A2 => get_analog_value(Muxposselect::Ain10),
        A3 => get_analog_value(Muxposselect::Ain11),
        A4 => get_analog_value(Muxposselect::Ain8),
        _ => 0,
    }
}

/// Sets the number of samples to accumulate when measuring a pin level.
///
/// Must be a power of 2 from 1 to 1024.
pub fn set_analog_num_samples(samples: u16) {
    if !samples.is_power_of_two() {
        return;
    }
    let sample_val = samples.trailing_zeros() as u8;
    if sample_val <= Samplenumselect::_1024 as u8 {
        // SAFETY: sample_val is a valid SAMPLENUM value.
        unsafe {
            adc()
                .avgctrl()
                .modify(|_, w| w.samplenum().bits(sample_val))
        };
        sync();
    }
}

/// Sets the length of time spent sampling (1-64 cycles).
pub fn set_analog_sampling_length(cycles: u8) {
    // The ADC always needs at least one cycle; subtract one and clamp.
    // SAFETY: the clamped value is a valid SAMPLEN value.
    unsafe {
        adc()
            .sampctrl()
            .modify(|_, w| w.samplen().bits((cycles - 1) & 0x3F))
    };
    sync();
}

/// Maps a `ReferenceVoltage` to the REFCTRL REFSEL value.
fn reference_to_refsel(reference: ReferenceVoltage) -> Refselselect {
    match reference {
        ReferenceVoltage::Intref => Refselselect::Intref,
        ReferenceVoltage::VccDiv1Point6 => Refselselect::Intvcc0,
        ReferenceVoltage::VccDiv2 => Refselselect::Intvcc1,
        ReferenceVoltage::Vcc => Refselselect::Intvcc2,
    }
}

/// Selects the reference voltage to use for analog readings.
pub fn set_analog_reference_voltage(reference: ReferenceVoltage) {
    adc().ctrla().modify(|_, w| w.enable().clear_bit());

    if reference == ReferenceVoltage::Intref {
        supc().vref().modify(|_, w| w.vrefoe().set_bit());
    } else {
        supc().vref().modify(|_, w| w.vrefoe().clear_bit());
    }

    adc()
        .refctrl()
        .modify(|_, w| w.refsel().variant(reference_to_refsel(reference)));
    adc().ctrla().modify(|_, w| w.enable().set_bit());
    sync();

    // Throw away one measurement after the reference change.
    get_analog_value(Muxposselect::Scaledcorevcc);
}

/// Returns the voltage of the VCC supply in millivolts.
pub fn get_vcc_voltage() -> u16 {
    let oldref = adc().refctrl().read().refsel().variant();

    // If we weren't already using the internal reference, select it now.
    if oldref != Some(Refselselect::Intref) {
        set_analog_reference_voltage(ReferenceVoltage::Intref);
    }

    let raw_val = get_analog_value(Muxposselect::Scalediovcc);

    // Restore the old reference, if needed.
    if oldref != Some(Refselselect::Intref) {
        if let Some(r) = oldref {
            set_analog_reference_voltage(match r {
                Refselselect::Intvcc0 => ReferenceVoltage::VccDiv1Point6,
                Refselselect::Intvcc1 => ReferenceVoltage::VccDiv2,
                Refselselect::Intvcc2 => ReferenceVoltage::Vcc,
                _ => ReferenceVoltage::Vcc,
            });
        }
    }

    let samplenum = adc().avgctrl().read().samplenum().bits();
    ((raw_val as u32 * 1000) / (1024 * (1 << samplenum))) as u16
}

/// Disables the analog circuitry on the selected pin.
pub fn disable_analog_input(pin: Pin) {
    gpio::set_pin_function(pin, Function::Off);
}

/// Disables the ADC peripheral.
pub fn disable_adc() {
    adc().ctrla().modify(|_, w| w.enable().clear_bit());
    sync();
    mclk().apbcmask().modify(|_, w| w.adc_().clear_bit());
}
