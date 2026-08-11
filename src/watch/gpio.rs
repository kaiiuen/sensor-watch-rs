//! GPIO driver.
//!
//! Port of the C `watch_gpio.c` and the GPIO HAL (`hpl_gpio_base.h`). Provides
//! pin direction, pull, function, and level control for the SAM L22's PORT
//! peripheral.

use atsaml22j::port::RegisterBlock as Port;

/// A pin, encoded as (port, pin number).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin(pub u8, pub u8);

/// Returns a reference to the PORT register block.
fn port() -> &'static Port {
    // SAFETY: the PORT register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Port::PTR }
}

/// Returns a reference to the PORT_IOBUS register block (used for DIR/OUT).
fn port_iobus() -> &'static Port {
    // SAFETY: the PORT_IOBUS register block lives at a fixed address for the
    // whole program.
    unsafe { &*atsaml22j::PortIobus::PTR }
}

/// Returns the PINCFG register for the given port and pin.
fn pincfg(port_idx: usize, pin_idx: usize) -> &'static atsaml22j::port::Pincfg0_ {
    if port_idx == 0 {
        port().pincfg0_(pin_idx)
    } else {
        // SAFETY: port 1 uses the pincfg1_0 array with the same layout.
        unsafe { &*(port().pincfg1_0(pin_idx) as *const _ as *const _) }
    }
}

/// Returns the PMUX register for the given port and pin-pair index.
fn pmux(port_idx: usize, pair_idx: usize) -> &'static atsaml22j::port::Pmux0_ {
    if port_idx == 0 {
        port().pmux0_(pair_idx)
    } else {
        // SAFETY: port 1 uses the pmux1_0 array with the same layout.
        unsafe { &*(port().pmux1_0(pair_idx) as *const _ as *const _) }
    }
}

/// Sets the direction of a pin.
pub fn set_pin_direction(pin: Pin, direction: Direction) {
    if !crate::watch::safety::valid_pin(pin.0, pin.1) {
        return;
    }
    let (port_idx, pin_idx) = (pin.0 as usize, pin.1 as usize);
    let mask = 1u32 << pin.1;
    // SAFETY: writing valid direction bitmasks.
    unsafe {
        match direction {
            Direction::Off => {
                port_iobus()
                    .dir(port_idx)
                    .modify(|r, w| w.bits(r.bits() & !mask));
                set_pin_function(pin, Function::Off);
            }
            Direction::In => {
                port_iobus()
                    .dir(port_idx)
                    .modify(|r, w| w.bits(r.bits() & !mask));
                // SAFETY: writing a valid PINCFG value.
                pincfg(port_idx, pin_idx).modify(|_, w| w.inen().set_bit());
            }
            Direction::Out => {
                port_iobus()
                    .dir(port_idx)
                    .modify(|r, w| w.bits(r.bits() | mask));
            }
        }
    }
}

/// Sets the pull mode of a pin.
pub fn set_pin_pull_mode(pin: Pin, pull: PullMode) {
    if !crate::watch::safety::valid_pin(pin.0, pin.1) {
        return;
    }
    let (port_idx, pin_idx) = (pin.0 as usize, pin.1 as usize);
    let mask = 1u32 << pin.1;
    match pull {
        PullMode::Off => {
            pincfg(port_idx, pin_idx).modify(|_, w| w.pullen().clear_bit());
        }
        PullMode::Up => {
            // SAFETY: writing valid DIR/PINCFG/OUT values.
            unsafe {
                port_iobus()
                    .dir(port_idx)
                    .modify(|r, w| w.bits(r.bits() & !mask));
                pincfg(port_idx, pin_idx).modify(|_, w| w.pullen().set_bit());
                port_iobus()
                    .out(port_idx)
                    .modify(|r, w| w.bits(r.bits() | mask));
            }
        }
        PullMode::Down => {
            // SAFETY: writing valid DIR/PINCFG/OUT values.
            unsafe {
                port_iobus()
                    .dir(port_idx)
                    .modify(|r, w| w.bits(r.bits() & !mask));
                pincfg(port_idx, pin_idx).modify(|_, w| w.pullen().set_bit());
                port_iobus()
                    .out(port_idx)
                    .modify(|r, w| w.bits(r.bits() & !mask));
            }
        }
    }
}

/// Sets the peripheral function of a pin.
pub fn set_pin_function(pin: Pin, function: Function) {
    if !crate::watch::safety::valid_pin(pin.0, pin.1)
        || matches!(function, Function::Mux(v) if !crate::watch::safety::valid_pmux(v))
    {
        return;
    }
    let (port_idx, pin_idx) = (pin.0 as usize, pin.1 as usize);
    match function {
        Function::Off => {
            pincfg(port_idx, pin_idx).modify(|_, w| w.pmuxen().clear_bit());
        }
        Function::A => set_pmux(port_idx, pin_idx, 0),
        Function::Mux(v) => set_pmux(port_idx, pin_idx, v),
    }
}

/// Enables the peripheral multiplexer and sets the PMUX value for a pin.
fn set_pmux(port_idx: usize, pin_idx: usize, value: u8) {
    if port_idx >= 2 || pin_idx >= 32 || !crate::watch::safety::valid_pmux(value) {
        return;
    }
    // SAFETY: writing valid PINCFG/PMUX values.
    unsafe {
        pincfg(port_idx, pin_idx).modify(|_, w| w.pmuxen().set_bit());
        let pmux = pmux(port_idx, pin_idx / 2);
        if pin_idx & 1 == 0 {
            pmux.modify(|_, w| w.pmuxe().bits(value));
        } else {
            pmux.modify(|_, w| w.pmuxo().bits(value));
        }
    }
}

/// Gets the input level of a pin.
pub fn get_pin_level(pin: Pin) -> bool {
    if !crate::watch::safety::valid_pin(pin.0, pin.1) {
        return false;
    }
    let port_idx = pin.0 as usize;
    let mask = 1u32 << pin.1;
    let dir = port_iobus().dir(port_idx).read().bits();
    let level = if dir & mask != 0 {
        port_iobus().out(port_idx).read().bits() & mask
    } else {
        port().in_(port_idx).read().bits() & mask
    };
    level != 0
}

/// Sets the output level of a pin.
pub fn set_pin_level(pin: Pin, level: bool) {
    if !crate::watch::safety::valid_pin(pin.0, pin.1) {
        return;
    }
    let port_idx = pin.0 as usize;
    let mask = 1u32 << pin.1;
    // SAFETY: writing a valid output bitmask.
    unsafe {
        if level {
            port_iobus()
                .out(port_idx)
                .modify(|r, w| w.bits(r.bits() | mask));
        } else {
            port_iobus()
                .out(port_idx)
                .modify(|r, w| w.bits(r.bits() & !mask));
        }
    }
}

/// Pin direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Off,
    In,
    Out,
}

/// Pin pull mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullMode {
    Off,
    Up,
    Down,
}

/// Pin peripheral function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    /// No peripheral function (plain GPIO).
    Off,
    /// Peripheral function A (PMUX value 0).
    A,
    /// A specific PMUX function value (0-15).
    Mux(u8),
}
