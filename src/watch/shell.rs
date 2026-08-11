//! Minimal serial command shell.
//!
//! Provides a small command interpreter over the debug UART. This is the
//! foundation for the companion app and for real-time clock calibration: a PC
//! can send commands (e.g. set the time) and the watch responds. It is
//! event-driven — it only processes a command when a full line has been
//! received, so it never keeps the CPU awake polling.

use crate::watch::event_log;
use crate::watch::rtc::{self, DateTime};
use crate::watch::uart;
#[cfg(feature = "usb-cdc")]
use crate::watch::usb;

#[cfg(not(feature = "usb-cdc"))]
fn transport_getc() -> u8 {
    uart::getc()
}

#[cfg(feature = "usb-cdc")]
fn transport_getc() -> u8 {
    usb::read().ok().flatten().unwrap_or(0)
}

#[cfg(not(feature = "usb-cdc"))]
fn transport_puts(s: &str) {
    uart::puts(s)
}

#[cfg(feature = "usb-cdc")]
fn transport_puts(s: &str) {
    let _ = usb::write(s.as_bytes());
}

/// The maximum command line length.
const LINE_MAX: usize = 32;

/// The shell state.
pub struct Shell {
    line: [u8; LINE_MAX],
    len: usize,
}

impl Shell {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        Shell {
            line: [0; LINE_MAX],
            len: 0,
        }
    }

    /// Processes any pending input from the UART.
    ///
    /// Call this from a tick or background task. It reads available bytes,
    /// accumulates a line, and executes the command when a newline arrives.
    pub fn poll(&mut self) {
        // Read as many bytes as are available (bounded).
        for _ in 0..16 {
            let c = transport_getc();
            if c == b'\n' || c == b'\r' {
                self.execute();
                self.len = 0;
            } else if c != 0 && self.len < LINE_MAX {
                self.line[self.len] = c;
                self.len += 1;
            }
        }
    }

    /// Executes the current command line.
    fn execute(&mut self) {
        let line = &self.line[..self.len];
        match line {
            b"time" => {
                // Report the current UTC time.
                let dt = rtc::get_date_time();
                transport_puts("TIME ");
                let mut buf = [0u8; 12];
                write_dt(&mut buf, dt);
                transport_puts(core::str::from_utf8(&buf).unwrap_or(""));
                transport_puts("\r\n");
            }
            b"help" => {
                transport_puts(
                    "CMDS: time, settime YYMMDDHHMMSS, drift N, optical, panic, events [clear]\\r\\n",
                );
            }
            b"optical" => {
                transport_puts(crate::watch::optical::status_text());
                transport_puts("\\r\\n");
            }
            b"events" => dump_events(),
            b"events clear" => {
                event_log::clear();
                transport_puts("OK\\r\\n");
            }
            b"panic" => {
                let fp = crate::movement::fault::panic_fingerprint();
                let mut buf = [0u8; 7];
                buf[0] = b'P';
                for i in 0..6 {
                    let nib = ((fp >> (20 - i * 4)) & 0xF) as u8;
                    buf[i + 1] = if nib < 10 {
                        b'0' + nib
                    } else {
                        b'a' + (nib - 10)
                    };
                }
                transport_puts(core::str::from_utf8(&buf).unwrap_or(""));
                transport_puts("\r\n");
            }
            _ => {
                // settime YYMMDDHHMMSS
                if line.len() == 21 && &line[..7] == b"settime" {
                    if let Some(dt) = parse_settime(&line[8..]) {
                        if rtc::set_date_time(dt).is_ok() {
                            transport_puts("OK\r\n");
                        } else {
                            transport_puts("ERR invalid date/time\r\n");
                        }
                    } else {
                        transport_puts("ERR\r\n");
                    }
                } else {
                    transport_puts("?\r\n");
                }
            }
        }
    }
}

/// Dumps the retained structured events in a stable, machine-readable form.
fn dump_events() {
    event_log::for_each(|event| {
        transport_puts("EV ");
        put_hex(event.sequence, 8);
        transport_puts(" ");
        put_hex(event.timestamp, 8);
        transport_puts(" ");
        put_hex(event.code as u32, 2);
        transport_puts(" ");
        put_hex(event.data as u32, 4);
        transport_puts("\\r\\n");
    });
}

/// Writes a value as uppercase hexadecimal without allocating.
fn put_hex(value: u32, digits: usize) {
    let mut buf = [b'0'; 8];
    for i in 0..digits.min(8) {
        let shift = ((digits - 1 - i).min(7) * 4) as u32;
        let nibble = ((value >> shift) & 0xF) as u8;
        buf[i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'A' + nibble - 10
        };
    }
    transport_puts(core::str::from_utf8(&buf[..digits.min(8)]).unwrap_or(""));
}

/// Writes a DateTime as YYMMDDHHMMSS into a 12-byte buffer.
fn write_dt(buf: &mut [u8; 12], dt: DateTime) {
    let mut i = 0;
    for v in [dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second] {
        buf[i] = b'0' + v / 10;
        buf[i + 1] = b'0' + v % 10;
        i += 2;
    }
}

/// Parses a settime command body "YYMMDDHHMMSS" into a DateTime.
fn parse_settime(s: &[u8]) -> Option<DateTime> {
    if s.len() != 12 {
        return None;
    }
    let num = |i: usize| -> Option<u8> {
        let d = |c: u8| -> Option<u8> {
            if c.is_ascii_digit() {
                Some(c - b'0')
            } else {
                None
            }
        };
        Some(d(s[i])? * 10 + d(s[i + 1])?)
    };
    Some(DateTime {
        year: num(0)?,
        month: num(2)?,
        day: num(4)?,
        hour: num(6)?,
        minute: num(8)?,
        second: num(10)?,
    })
}
