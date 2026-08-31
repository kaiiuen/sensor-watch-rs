//! Host-testable contract for the opt-in Sensor Watch Lite hardware test.
//!
//! This module deliberately models only read-only diagnostics. The storage test
//! uses a reserved scratch range supplied by the hardware implementation; it
//! must never erase or rewrite settings, calibration, logs, or application data.

pub const PROFILE_NAME: &str = "Sensor Watch Lite hardware test";
pub const MAX_RESPONSE: usize = 128;
pub const HEARTBEAT_PERIOD_MS: u32 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    All,
    Identity,
    Rtc,
    Storage,
    Led,
    Status,
    Ping,
    Help,
}

impl Command {
    pub fn parse(line: &[u8]) -> Option<Self> {
        let line = trim_ascii(line);
        match line {
            b"test all" => Some(Self::All),
            b"test identity" => Some(Self::Identity),
            b"test rtc" => Some(Self::Rtc),
            b"test storage" => Some(Self::Storage),
            b"test led" => Some(Self::Led),
            b"status" => Some(Self::Status),
            b"ping" => Some(Self::Ping),
            b"help" => Some(Self::Help),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Identity => "identity",
            Self::Rtc => "rtc",
            Self::Storage => "storage",
            Self::Led => "led",
            Self::Status => "status",
            Self::Ping => "ping",
            Self::Help => "help",
        }
    }
}

fn trim_ascii(mut input: &[u8]) -> &[u8] {
    while let Some((&first, rest)) = input.split_first() {
        if first.is_ascii_whitespace() {
            input = rest;
        } else {
            break;
        }
    }
    while let Some((&last, rest)) = input.split_last() {
        if last.is_ascii_whitespace() {
            input = rest;
        } else {
            break;
        }
    }
    input
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestResult {
    Pass,
    Fail,
    NotRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub identity: TestResult,
    pub rtc: TestResult,
    pub storage: TestResult,
    pub led: TestResult,
    pub usb_bulk_proven: bool,
    pub session_active: bool,
}

impl Status {
    pub const fn fresh() -> Self {
        Self {
            identity: TestResult::NotRun,
            rtc: TestResult::NotRun,
            storage: TestResult::NotRun,
            led: TestResult::NotRun,
            usb_bulk_proven: false,
            session_active: false,
        }
    }
}

/// A minimal hardware seam suitable for both the SAM L22 implementation and
/// deterministic host mocks. No method exposes mutation outside the scratch
/// storage pair used by the storage self-test.
pub trait Hardware {
    fn identity_present(&self) -> bool;
    fn rtc_advances(&mut self) -> bool;
    fn storage_scratch_read(&mut self, out: &mut [u8; 8]) -> bool;
    fn storage_scratch_write(&mut self, value: &[u8; 8]) -> bool;
    fn storage_scratch_restore(&mut self, value: &[u8; 8]) -> bool;
    fn led_cycle(&mut self) -> bool;
}

pub fn run_test<H: Hardware>(hardware: &mut H, status: &mut Status, command: Command) {
    match command {
        Command::All => {
            run_test(hardware, status, Command::Identity);
            run_test(hardware, status, Command::Rtc);
            run_test(hardware, status, Command::Storage);
            run_test(hardware, status, Command::Led);
        }
        Command::Identity => {
            status.identity = if hardware.identity_present() {
                TestResult::Pass
            } else {
                TestResult::Fail
            }
        }
        Command::Rtc => {
            status.rtc = if hardware.rtc_advances() {
                TestResult::Pass
            } else {
                TestResult::Fail
            }
        }
        Command::Storage => {
            let mut original = [0u8; 8];
            let pattern = *b"LITESELF";
            let ok = hardware.storage_scratch_read(&mut original)
                && hardware.storage_scratch_write(&pattern)
                && hardware.storage_scratch_restore(&original);
            status.storage = if ok {
                TestResult::Pass
            } else {
                TestResult::Fail
            };
        }
        Command::Led => {
            status.led = if hardware.led_cycle() {
                TestResult::Pass
            } else {
                TestResult::Fail
            }
        }
        Command::Status | Command::Ping | Command::Help => {}
    }
}

/// Format a response into a caller-owned fixed buffer. Responses are bounded
/// and intentionally say UNKNOWN rather than claiming hardware PASS before a
/// test has run.
pub fn format_response(command: Command, status: Status, out: &mut [u8; MAX_RESPONSE]) -> usize {
    if command == Command::Status {
        return format_status(status, out);
    }
    let text = match command {
        Command::Help => "OK commands: test all|identity|rtc|storage|led, status, ping, help\r\n",
        Command::Ping => "OK pong\r\n",
        Command::All | Command::Identity | Command::Rtc | Command::Storage | Command::Led => {
            test_line(command, status)
        }
        Command::Status => unreachable!(),
    };
    copy_bounded(text.as_bytes(), out)
}

fn result_name(result: TestResult) -> &'static str {
    match result {
        TestResult::Pass => "PASS",
        TestResult::Fail => "FAIL",
        TestResult::NotRun => "UNKNOWN",
    }
}

fn test_line(command: Command, status: Status) -> &'static str {
    match command {
        Command::All => "OK test all complete\r\n",
        Command::Identity => {
            if status.identity == TestResult::Pass {
                "OK identity PASS\r\n"
            } else {
                "ERR identity FAIL\r\n"
            }
        }
        Command::Rtc => {
            if status.rtc == TestResult::Pass {
                "OK rtc PASS\r\n"
            } else {
                "ERR rtc FAIL\r\n"
            }
        }
        Command::Storage => {
            if status.storage == TestResult::Pass {
                "OK storage PASS\r\n"
            } else {
                "ERR storage FAIL\r\n"
            }
        }
        Command::Led => {
            if status.led == TestResult::Pass {
                "OK led PASS\r\n"
            } else {
                "ERR led FAIL\r\n"
            }
        }
        _ => "ERR unsupported\r\n",
    }
}

fn copy_bounded(bytes: &[u8], out: &mut [u8; MAX_RESPONSE]) -> usize {
    let len = bytes.len().min(out.len());
    out[..len].copy_from_slice(&bytes[..len]);
    len
}

fn format_status(status: Status, out: &mut [u8; MAX_RESPONSE]) -> usize {
    let mut text = [0u8; MAX_RESPONSE];
    let mut len = 0;
    for part in [
        b"OK status session=".as_slice(),
        if status.session_active {
            b"ACTIVE".as_slice()
        } else {
            b"INACTIVE".as_slice()
        },
        b" identity=".as_slice(),
        result_name(status.identity).as_bytes(),
        b" rtc=".as_slice(),
        result_name(status.rtc).as_bytes(),
        b" storage=".as_slice(),
        result_name(status.storage).as_bytes(),
        b" led=".as_slice(),
        result_name(status.led).as_bytes(),
        b" bulk=".as_slice(),
        if status.usb_bulk_proven {
            b"PROVEN".as_slice()
        } else {
            b"FAIL-CLOSED".as_slice()
        },
        b"\r\n".as_slice(),
    ] {
        let count = part.len().min(text.len() - len);
        text[len..len + count].copy_from_slice(&part[..count]);
        len += count;
    }
    copy_bounded(&text[..len], out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mock {
        scratch: [u8; 8],
        identity: bool,
        rtc: bool,
        led: bool,
        writes: u8,
        restored: bool,
    }

    impl Hardware for Mock {
        fn identity_present(&self) -> bool {
            self.identity
        }
        fn rtc_advances(&mut self) -> bool {
            self.rtc
        }
        fn storage_scratch_read(&mut self, out: &mut [u8; 8]) -> bool {
            *out = self.scratch;
            true
        }
        fn storage_scratch_write(&mut self, value: &[u8; 8]) -> bool {
            self.scratch = *value;
            self.writes += 1;
            true
        }
        fn storage_scratch_restore(&mut self, value: &[u8; 8]) -> bool {
            self.scratch = *value;
            self.restored = true;
            true
        }
        fn led_cycle(&mut self) -> bool {
            self.led
        }
    }

    #[test]
    fn parses_only_read_only_commands() {
        assert_eq!(Command::parse(b" test all\r\n"), Some(Command::All));
        assert_eq!(Command::parse(b"erase 0"), None);
        assert_eq!(Command::parse(b"write settings"), None);
    }

    #[test]
    fn storage_mock_restores_original_bytes() {
        let mut mock = Mock {
            scratch: *b"ORIGINAL",
            identity: true,
            rtc: true,
            led: true,
            writes: 0,
            restored: false,
        };
        let original = mock.scratch;
        let mut status = Status::fresh();
        run_test(&mut mock, &mut status, Command::Storage);
        assert_eq!(status.storage, TestResult::Pass);
        assert_eq!(mock.scratch, original);
        assert_eq!(mock.writes, 1);
        assert!(mock.restored);
    }

    #[test]
    fn host_status_never_fabricates_pass() {
        let mut out = [0; MAX_RESPONSE];
        let len = format_response(Command::Status, Status::fresh(), &mut out);
        assert!(
            core::str::from_utf8(&out[..len])
                .unwrap()
                .contains("bulk=FAIL-CLOSED")
        );
        assert!(!core::str::from_utf8(&out[..len]).unwrap().contains("PASS"));
    }
}
