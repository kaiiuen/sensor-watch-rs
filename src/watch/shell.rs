//! Minimal serial command shell.
//!
//! Provides a small command interpreter over the debug UART. This is the
//! foundation for the companion app and for real-time clock calibration: a PC
//! can send commands (e.g. set the time) and the watch responds. It is
//! event-driven — it only processes a command when a full line has been
//! received, so it never keeps the CPU awake polling.

use crate::watch::rtc::{self, DateTime};
use crate::watch::uart;

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
            let c = uart::getc();
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
                uart::puts("TIME ");
                let mut buf = [0u8; 12];
                write_dt(&mut buf, dt);
                uart::puts(core::str::from_utf8(&buf).unwrap_or(""));
                uart::puts("\r\n");
            }
            b"help" => {
                uart::puts("CMDS: time, settime YYMMDDHHMMSS, drift N\r\n");
            }
            _ => {
                // settime YYMMDDHHMMSS
                if line.len() == 21 && &line[..7] == b"settime" {
                    if let Some(dt) = parse_settime(&line[8..]) {
                        rtc::set_date_time(dt);
                        uart::puts("OK\r\n");
                    } else {
                        uart::puts("ERR\r\n");
                    }
                } else {
                    uart::puts("?\r\n");
                }
            }
        }
    }
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
