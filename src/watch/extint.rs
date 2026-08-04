//! Buttons and external interrupts driver.
//!
//! Port of the C `watch_extint.c` and the EIC HAL (`hal_ext_irq.c`,
//! `hpl_eic.c`). Handles the three buttons (Light, Mode, Alarm) and external
//! interrupts from the nine-pin connector.

use crate::watch::gpio::{self, Direction, Function, Pin, PullMode};
use atsaml22j::eic::RegisterBlock as Eic;

/// The pins used by the buttons and the 9-pin connector (OSO-SWAT-A1-05 board).
pub const BTN_ALARM: Pin = Pin(0, 2); // PA02
pub const BTN_LIGHT: Pin = Pin(0, 22); // PA22
pub const BTN_MODE: Pin = Pin(0, 23); // PA23
pub const A0: Pin = Pin(1, 4); // PB04
pub const A1: Pin = Pin(1, 1); // PB01
pub const A2: Pin = Pin(1, 2); // PB02
pub const A3: Pin = Pin(1, 3); // PB03
pub const A4: Pin = Pin(1, 0); // PB00

/// EIC channel for each pin (OSO-SWAT-A1-05 board).
const A0_EIC_CHANNEL: u8 = 4;
const A1_EIC_CHANNEL: u8 = 1;
const A2_EIC_CHANNEL: u8 = 2;
const A3_EIC_CHANNEL: u8 = 3;
const A4_EIC_CHANNEL: u8 = 0;
const BTN_ALARM_EIC_CHANNEL: u8 = 2;
const BTN_LIGHT_EIC_CHANNEL: u8 = 6;
const BTN_MODE_EIC_CHANNEL: u8 = 7;

/// The type of interrupt trigger to scan for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Trigger {
    None = 0,
    Rising,
    Falling,
    Both,
}

/// A callback invoked from an external interrupt.
pub type Callback = fn();

/// The EIC channel -> pin map (sorted by channel, for binary search).
const EIC_MAP: [(u8, Pin); 6] = [
    (0, A4),
    (1, A1),
    (2, BTN_ALARM),
    (3, A3),
    (6, BTN_LIGHT),
    (7, BTN_MODE),
];

/// Callback slots, indexed by EIC channel.
static mut CALLBACKS: [Option<Callback>; 16] = [None; 16];

/// Returns a reference to the EIC peripheral register block.
fn eic() -> &'static Eic {
    // SAFETY: the EIC register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Eic::PTR }
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

/// Waits for the EIC to finish synchronizing.
fn sync() {
    while eic().syncbusy().read().bits() != 0 {}
}

/// Returns the EIC channel for a pin, or None if the pin has no EIC channel.
fn eic_channel(pin: Pin) -> Option<u8> {
    for &(channel, map_pin) in EIC_MAP.iter() {
        if map_pin == pin {
            return Some(channel);
        }
    }
    None
}

/// Enables the external interrupt controller.
pub fn enable_external_interrupts() {
    // Configure EIC to use GCLK3 (the 32.768 kHz crystal).
    gclk()
        .pchctrl(3)
        .write(|w| w.r#gen().gclk3().chen().set_bit());
    // Enable the AHB clock for the EIC.
    mclk().apbamask().modify(|_, w| w.eic_().set_bit());
    // Initialize the EIC.
    init();
}

/// Disables the external interrupt controller.
pub fn disable_external_interrupts() {
    // Disable the EIC interrupt in the NVIC.
    cortex_m::peripheral::NVIC::mask(atsaml22j::Interrupt::EIC);
    eic().ctrla().modify(|_, w| w.enable().clear_bit());
    eic().ctrla().modify(|_, w| w.swrst().set_bit());
    mclk().apbamask().modify(|_, w| w.eic_().clear_bit());
}

/// Initializes the EIC (port of `_ext_irq_init`).
fn init() {
    // Software reset.
    if !eic().syncbusy().read().swrst().bit_is_set() {
        if eic().ctrla().read().enable().bit_is_set() {
            eic().ctrla().modify(|_, w| w.enable().clear_bit());
            sync();
        }
        eic().ctrla().write(|w| w.swrst().set_bit());
    }
    sync();

    // Clock selection: use ULPOSC32K (CONF_EIC_CKSEL = 1).
    eic().ctrla().modify(|_, w| w.cksel().set_bit());

    // Enable the EIC and set up the NVIC interrupt.
    eic().ctrla().modify(|_, w| w.enable().set_bit());
    cortex_m::peripheral::NVIC::mask(atsaml22j::Interrupt::EIC);
    cortex_m::peripheral::NVIC::unpend(atsaml22j::Interrupt::EIC);
    // SAFETY: unmasking a valid interrupt is safe.
    unsafe { cortex_m::peripheral::NVIC::unmask(atsaml22j::Interrupt::EIC) };
}

/// Registers an external interrupt callback on one of the external interrupt pins.
pub fn register_interrupt_callback(pin: Pin, callback: Callback, trigger: Trigger) {
    let Some(channel) = eic_channel(pin) else {
        return;
    };

    // Set the pin as input.
    gpio::set_pin_direction(pin, Direction::In);

    // The EIC config register is enable-protected, so disable it first.
    if eic().ctrla().read().enable().bit_is_set() {
        eic().ctrla().modify(|_, w| w.enable().clear_bit());
        sync();
    }

    // Update the CONFIG register for this channel.
    let config_index = if channel > 7 { 1 } else { 0 };
    let sense_pos = 4 * (channel % 8);
    // SAFETY: writing valid CONFIG values.
    unsafe {
        eic().config(config_index).modify(|r, w| {
            let bits = r.bits() & !(7 << sense_pos);
            w.bits(bits | ((trigger as u32) << sense_pos))
        });
    }

    // Set the pin function to peripheral A (EIC) and enable pull-down for buttons.
    gpio::set_pin_function(pin, Function::A);
    if pin == BTN_ALARM || pin == BTN_LIGHT || pin == BTN_MODE {
        gpio::set_pin_pull_mode(pin, PullMode::Down);
    }

    // Re-enable the EIC.
    eic().ctrla().modify(|_, w| w.enable().set_bit());

    // Store the callback and enable the interrupt.
    unsafe {
        CALLBACKS[channel as usize] = Some(callback);
    }
    // SAFETY: writing a valid interrupt-enable bitmask.
    unsafe {
        eic()
            .intenset()
            .modify(|r, w| w.bits(r.bits() | (1 << channel)));
    }
}

/// The EIC interrupt handler.
///
/// The PAC's `rt` feature declares `extern "C" { fn EIC(); }` and places it in
/// the vector table, so we provide the matching `#[no_mangle]` symbol here.
#[unsafe(no_mangle)]
pub extern "C" fn EIC() {
    let flags = eic().intflag().read().bits();
    // SAFETY: writing a valid interrupt-flag clear bitmask.
    unsafe {
        eic().intflag().write(|w| w.bits(flags));
    }

    let mut remaining = flags;
    while remaining != 0 {
        let pos = remaining.trailing_zeros() as usize;
        if let Some(cb) = unsafe { CALLBACKS[pos] } {
            cb();
        }
        remaining &= !(1 << pos);
    }
}
