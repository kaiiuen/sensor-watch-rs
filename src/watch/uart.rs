//! UART driver.
//!
//! Port of the C `watch_uart.c`. Uses SERCOM3 in USART mode. TX can be A2 or
//! A4; RX can be A1, A2, A3, or A4.

use crate::watch::gpio::{self, Direction, Function, Pin};
use crate::watch::timeout::wait_until;
use atsaml22j::sercom0::usart::Usart;

const RX_CAPACITY: usize = 64;

struct RxRing {
    bytes: [u8; RX_CAPACITY],
    read: usize,
    write: usize,
    len: usize,
    overflowed: bool,
}

impl RxRing {
    const fn new() -> Self {
        Self {
            bytes: [0; RX_CAPACITY],
            read: 0,
            write: 0,
            len: 0,
            overflowed: false,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.len == RX_CAPACITY {
            self.overflowed = true;
            return;
        }
        self.bytes[self.write] = byte;
        self.write = (self.write + 1) % RX_CAPACITY;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let byte = self.bytes[self.read];
        self.read = (self.read + 1) % RX_CAPACITY;
        self.len -= 1;
        Some(byte)
    }
}

static mut RX_RING: RxRing = RxRing::new();

/// The UART-capable pins.
const A1: Pin = Pin(1, 1);
const A2: Pin = Pin(1, 2);
const A3: Pin = Pin(1, 3);
const A4: Pin = Pin(1, 0);

fn valid_uart_pin(pin: Pin, tx: bool) -> bool {
    pin.0 == 1
        && if tx {
            pin.1 == 0 || pin.1 == 2
        } else {
            pin.1 <= 3
        }
}

fn valid_baud(baud: u32) -> bool {
    (300..=250_000).contains(&baud)
}

/// Returns a reference to the SERCOM3 USART register block.
fn usart() -> &'static Usart {
    // SAFETY: the SERCOM3 register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { (*atsaml22j::Sercom3::PTR).usart() }
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
    let _ = wait_until(|| usart().syncbusy().read().bits() == 0);
}

/// Initializes the debug UART.
///
/// `tx_pin` is A2 or A4 (or 0 for receive-only); `rx_pin` is A1/A2/A3/A4
/// (or 0 for transmit-only).
pub fn enable_uart(tx_pin: Option<Pin>, rx_pin: Option<Pin>, baud: u32) {
    let tx_valid = tx_pin.map_or(true, |pin| valid_uart_pin(pin, true));
    let rx_valid = rx_pin.map_or(true, |pin| valid_uart_pin(pin, false));
    if !tx_valid || !rx_valid || !valid_baud(baud) {
        disable_uart();
        return;
    }
    // Enable clocks: SERCOM3 core (GCLK0) plus the APBC clock.
    gclk()
        .pchctrl(19)
        .write(|w| w.r#gen().gclk0().chen().set_bit());
    mclk().apbcmask().modify(|_, w| w.sercom3_().set_bit());

    // Software reset.
    if !usart().syncbusy().read().swrst().bit_is_set() {
        if usart().ctrla().read().enable().bit_is_set() {
            usart().ctrla().modify(|_, w| w.enable().clear_bit());
            sync();
        }
        usart().ctrla().write(|w| w.swrst().set_bit());
    }
    sync();

    // Configure: USART mode (1), LSB-first (DORD), 8-bit chars.
    // SAFETY: writing valid CTRLA/CTRLB values.
    unsafe {
        usart()
            .ctrla()
            .write(|w| w.mode().bits(0x1).dord().set_bit());
        usart().ctrlb().modify(|_, w| w.chsize().bits(0));
    }

    // Configure TX pin.
    if let Some(pin) = tx_pin {
        gpio::set_pin_direction(pin, Direction::Out);
        gpio::set_pin_function(pin, Function::Mux(2)); // function C
        // SAFETY: writing valid TXPO values.
        unsafe {
            if pin == A2 {
                usart()
                    .ctrla()
                    .modify(|r, w| w.bits(r.bits() & !(0x3 << 20)));
            } else if pin == A4 {
                usart()
                    .ctrla()
                    .modify(|r, w| w.bits((r.bits() & !(0x3 << 20)) | (1 << 20)));
            }
        }
        usart().ctrlb().modify(|_, w| w.txen().set_bit());
    }

    // Configure RX pin.
    if let Some(pin) = rx_pin {
        gpio::set_pin_direction(pin, Direction::In);
        gpio::set_pin_function(pin, Function::Mux(2)); // function C
        // SAFETY: writing valid RXPO values.
        unsafe {
            let rxpo = match pin {
                A1 => 3,
                A2 => 0,
                A3 => 1,
                A4 => 2,
                _ => 0,
            };
            usart()
                .ctrla()
                .modify(|r, w| w.bits((r.bits() & !(0x3 << 16)) | (rxpo << 16)));
        }
        usart().ctrlb().modify(|_, w| w.rxen().set_bit());
    }

    // Set the baud rate (4 MHz clock, no USB).
    // SAFETY: writing a valid BAUD value.
    unsafe {
        let br = 65536u32 - ((65536u32 * 16 * baud) / 4000000);
        usart().baud().write(|w| w.bits(br as u16));
    }

    // Enable the peripheral.
    usart().ctrla().modify(|_, w| w.enable().set_bit());
    sync();
}

/// Transmits a string of bytes on the UART's TX pin.
pub fn disable_uart() {
    usart().ctrla().modify(|_, w| w.enable().clear_bit());
    mclk().apbcmask().modify(|_, w| w.sercom3_().clear_bit());
}

pub fn puts(s: &str) {
    for &byte in s.as_bytes() {
        // Wait for the data register to be empty (bounded).
        if wait_until(|| usart().intflag().read().dre().bit_is_set()).is_err() {
            return;
        }
        // SAFETY: writing a valid DATA value.
        unsafe {
            usart().data().write(|w| w.bits(byte as u16));
        }
    }
    // Wait for transmission to complete (bounded).
    let _ = wait_until(|| usart().intflag().read().txc().bit_is_set());
}

/// Moves currently received hardware bytes into the bounded software ring.
///
/// This is deliberately nonblocking and bounded so a noisy or disconnected jig
/// cannot keep the main loop awake indefinitely. Call it from the shell poller.
pub fn service_rx() {
    for _ in 0..RX_CAPACITY {
        if !usart().intflag().read().rxc().bit_is_set() {
            break;
        }
        let byte = usart().data().read().bits() as u8;
        critical_section::with(|_| unsafe { RX_RING.push(byte) });
    }
}

/// Returns and clears the receive-overflow indication.
pub fn take_rx_overflow() -> bool {
    critical_section::with(|_| unsafe {
        let overflowed = RX_RING.overflowed;
        RX_RING.overflowed = false;
        overflowed
    })
}

/// Receives a byte from the software ring without waiting.
pub fn try_getc() -> Option<u8> {
    critical_section::with(|_| unsafe { RX_RING.pop() })
}

/// Receives a single byte from the UART (bounded blocking).
pub fn getc() -> u8 {
    if wait_until(|| usart().intflag().read().rxc().bit_is_set()).is_err() {
        return 0;
    }
    usart().data().read().bits() as u8
}
