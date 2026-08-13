//! Minimal serial command shell.
//!
//! Provides a small command interpreter over the debug UART. This is the
//! foundation for the companion app and for real-time clock calibration: a PC
//! can send commands (e.g. set the time) and the watch responds. It is
//! event-driven - it only processes a command when a full line has been
//! received, so it never keeps the CPU awake polling.

use crate::watch::event_log;
use crate::watch::rtc::{self, DateTime};
#[cfg(not(feature = "usb-cdc"))]
use crate::watch::uart;
#[cfg(feature = "usb-cdc")]
use crate::watch::usb;

#[cfg(not(feature = "usb-cdc"))]
fn transport_service() {
    uart::service_rx();
}

#[cfg(feature = "usb-cdc")]
fn transport_service() {}

#[cfg(not(feature = "usb-cdc"))]
fn transport_getc() -> Option<u8> {
    uart::try_getc()
}

#[cfg(feature = "usb-cdc")]
fn transport_getc() -> Option<u8> {
    usb::read().ok().flatten()
}

#[cfg(not(feature = "usb-cdc"))]
fn transport_rx_overflowed() -> bool {
    uart::take_rx_overflow()
}

#[cfg(feature = "usb-cdc")]
fn transport_rx_overflowed() -> bool {
    false
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
    discarding_line: bool,
    invalid_line: bool,
    last_was_terminator: bool,
    /// Mutation commands remain locked until an explicit authorization hook
    /// confirms physical presence or an equivalent development/test condition.
    mutation_authorized: bool,
}

impl Shell {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        Shell {
            line: [0; LINE_MAX],
            len: 0,
            discarding_line: false,
            invalid_line: false,
            last_was_terminator: false,
            // Fail closed in every production configuration. Integrations must
            // explicitly authorize mutations through `set_mutation_authorized`.
            mutation_authorized: false,
        }
    }

    /// Set whether mutating commands may run for the current shell session.
    ///
    /// The movement integration calls this only from its physical-presence
    /// state machine. Tests may use the hook to exercise command authorization;
    /// the shell starts locked and authorization is revoked on the next app
    /// loop when the service window closes.
    pub(crate) fn set_mutation_authorized(&mut self, authorized: bool) {
        self.mutation_authorized = authorized;
    }

    /// Processes any pending input from the UART.
    ///
    /// Call this from a tick or background task. It reads available bytes,
    /// accumulates a line, and executes the command when a newline arrives.
    pub fn poll(&mut self) {
        transport_service();
        if transport_rx_overflowed() {
            self.reset_line();
            transport_puts("ERR rx-overflow\r\n");
        }

        // Consume a bounded amount per wake. The UART ring retains the rest.
        for _ in 0..16 {
            let Some(c) = transport_getc() else { break };
            if c == b'\n' || c == b'\r' {
                if !self.last_was_terminator {
                    if self.invalid_line {
                        transport_puts("ERR invalid\r\n");
                    } else if self.discarding_line {
                        transport_puts("ERR line-too-long\r\n");
                    } else {
                        self.execute();
                    }
                }
                self.reset_line();
                self.last_was_terminator = true;
            } else if c < 0x20 || c == 0x7f {
                self.invalid_line = true;
                self.last_was_terminator = false;
            } else if self.discarding_line {
                self.last_was_terminator = false;
            } else if self.len < LINE_MAX {
                self.line[self.len] = c;
                self.len += 1;
                self.last_was_terminator = false;
            } else {
                self.discarding_line = true;
                self.last_was_terminator = false;
            }
        }
    }

    fn reset_line(&mut self) {
        self.len = 0;
        self.discarding_line = false;
        self.invalid_line = false;
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
                    "CMDS: time, settime YYMMDDHHMMSS, drift [N], panic, events [clear], help\r\n",
                );
            }
            #[cfg(feature = "optical")]
            b"optical" => {
                transport_puts(crate::watch::optical::status_text());
                transport_puts("\r\n");
            }
            b"events" => dump_events(),
            b"drift" => {
                transport_puts("DRIFT ");
                let value = rtc::freqcorr_read();
                let mut buf = [b'0'; 4];
                let magnitude = value.unsigned_abs();
                buf[0] = if value < 0 { b'-' } else { b'+' };
                buf[1] = b'0' + ((magnitude / 100) % 10) as u8;
                buf[2] = b'0' + ((magnitude / 10) % 10) as u8;
                buf[3] = b'0' + (magnitude % 10) as u8;
                transport_puts(core::str::from_utf8(&buf).unwrap_or(""));
                transport_puts("\r\n");
            }
            b"events clear" => {
                if self.mutation_authorized {
                    event_log::clear();
                    transport_puts("OK\r\n");
                } else {
                    transport_puts("ERR locked\r\n");
                }
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
                if line.len() >= 7 && &line[..6] == b"drift " {
                    if let Some(value) = parse_signed(&line[6..]) {
                        if !self.mutation_authorized {
                            transport_puts("ERR locked\r\n");
                        } else {
                            crate::movement::apply_drift_correction(value);
                            transport_puts("OK\r\n");
                        }
                    } else {
                        transport_puts("ERR drift\r\n");
                    }
                // settime YYMMDDHHMMSS
                } else if line.len() == 20 && &line[..7] == b"settime" && line[7] == b' ' {
                    if let Some(dt) = parse_settime(&line[8..]) {
                        if !self.mutation_authorized {
                            transport_puts("ERR locked\r\n");
                        } else if rtc::set_date_time(dt).is_ok() {
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
        transport_puts("\r\n");
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
fn parse_signed(s: &[u8]) -> Option<i16> {
    if s.is_empty() {
        return None;
    }
    let (negative, digits) = match s[0] {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: i16 = 0;
    for digit in digits {
        if !digit.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((digit - b'0') as i16)?;
    }
    if value > 127 {
        return None;
    }
    Some(if negative { -value } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_starts_locked() {
        assert!(!Shell::new_static().mutation_authorized);
    }

    #[test]
    fn authorization_requires_explicit_hook() {
        let mut shell = Shell::new_static();
        shell.set_mutation_authorized(true);
        assert!(shell.mutation_authorized);

        shell.set_mutation_authorized(false);
        assert!(!shell.mutation_authorized);
    }
}

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
