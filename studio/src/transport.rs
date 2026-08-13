//! Host transport for the Studio shell.
//!
//! The physical path is the documented SERCOM3 UART jig: 9600 baud, 8-N-1,
//! watch A4 (TX) to adapter RX, A2 (RX) to adapter TX, and common ground.
//! This deliberately does not identify the watch's UF2 USB mass-storage device
//! as a serial port or imply that USB CDC is available.

use std::fmt;
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

pub const UART_BAUD: u32 = 9_600;
pub const MAX_COMMAND_BYTES: usize = 32;
pub const MAX_RESPONSE_BYTES: usize = 256;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportMode {
    Simulated,
    UartJig,
}

impl TransportMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Simulated => "Simulated",
            Self::UartJig => "UART Jig",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortChoice {
    pub name: String,
    pub description: String,
}

#[derive(Debug)]
pub enum TransportError {
    InvalidCommand(&'static str),
    NoPortSelected,
    PortEnumeration(String),
    Open {
        port: String,
        source: String,
    },
    Io(String),
    Timeout {
        operation: &'static str,
        timeout: Duration,
    },
    FrameTooLong,
    Disconnected,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCommand(reason) => write!(f, "invalid command: {reason}"),
            Self::NoPortSelected => f.write_str("no serial port selected"),
            Self::PortEnumeration(error) => write!(f, "serial port discovery failed: {error}"),
            Self::Open { port, source } => write!(f, "could not open {port}: {source}"),
            Self::Io(error) => write!(f, "serial I/O failed: {error}"),
            Self::Timeout { operation, timeout } => {
                write!(f, "{operation} timed out after {} ms", timeout.as_millis())
            }
            Self::FrameTooLong => f.write_str("serial response exceeded the 256-byte limit"),
            Self::Disconnected => f.write_str("UART jig is disconnected"),
        }
    }
}

impl std::error::Error for TransportError {}

pub fn discover_ports() -> Result<Vec<PortChoice>, TransportError> {
    serialport::available_ports()
        .map(|ports| {
            ports
                .into_iter()
                .map(|port| PortChoice {
                    name: port.port_name,
                    description: match port.port_type {
                        serialport::SerialPortType::UsbPort(info) => {
                            let product = info.product.unwrap_or_else(|| "USB serial".into());
                            format!("{product} ({:04x}:{:04x})", info.vid, info.pid)
                        }
                        _ => "Serial port".into(),
                    },
                })
                .collect()
        })
        .map_err(|error| TransportError::PortEnumeration(error.to_string()))
}

pub struct SerialTransport {
    port_name: String,
    port: Box<dyn serialport::SerialPort>,
    timeout: Duration,
}

impl SerialTransport {
    pub fn connect(port_name: &str, timeout: Duration) -> Result<Self, TransportError> {
        if port_name.trim().is_empty() {
            return Err(TransportError::NoPortSelected);
        }
        let port = serialport::new(port_name, UART_BAUD)
            .timeout(timeout)
            .open()
            .map_err(|error| TransportError::Open {
                port: port_name.to_string(),
                source: error.to_string(),
            })?;
        Ok(Self {
            port_name: port_name.to_string(),
            port,
            timeout,
        })
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn command(&mut self, command: &str) -> Result<String, TransportError> {
        let frame = encode_command(command)?;
        let timeout = self.timeout();
        write_all_with_timeout(&mut self.port, &frame, timeout)?;
        read_frame(&mut self.port, timeout)
    }
}

/// Encode one shell command as the firmware's line-delimited frame.
pub fn encode_command(command: &str) -> Result<Vec<u8>, TransportError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(TransportError::InvalidCommand("command is empty"));
    }
    if command.len() > MAX_COMMAND_BYTES {
        return Err(TransportError::InvalidCommand("command is too long"));
    }
    if command.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return Err(TransportError::InvalidCommand(
            "control characters are not allowed",
        ));
    }
    if !is_allowed_uart_command(command) {
        return Err(TransportError::InvalidCommand(
            "command is not allowed over unauthenticated UART",
        ));
    }
    let mut frame = command.as_bytes().to_vec();
    frame.extend_from_slice(b"\r\n");
    Ok(frame)
}

fn is_allowed_uart_command(command: &str) -> bool {
    match command {
        "help" | "status" | "time" | "events" | "events clear" | "panic" | "optical" => true,
        value if value.starts_with("drift ") => value[6..]
            .parse::<i32>()
            .map(|ppm| (-127..=127).contains(&ppm))
            .unwrap_or(false),
        value if value.starts_with("settime ") => {
            let timestamp = &value[8..];
            timestamp.len() == 12 && timestamp.bytes().all(|byte| byte.is_ascii_digit())
        }
        _ => false,
    }
}

/// Read one CR/LF-terminated shell response, tolerating either line ending.
pub fn read_frame<R: Read>(reader: &mut R, timeout: Duration) -> Result<String, TransportError> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::with_capacity(32);
    let mut one = [0u8; 1];
    loop {
        match reader.read(&mut one) {
            Ok(0) => return Err(TransportError::Disconnected),
            Ok(_) if one[0] == b'\n' || one[0] == b'\r' => {
                if bytes.is_empty() {
                    continue;
                }
                return String::from_utf8(bytes)
                    .map_err(|_| TransportError::Io("response was not UTF-8".into()));
            }
            Ok(_) => {
                bytes.push(one[0]);
                if bytes.len() > MAX_RESPONSE_BYTES {
                    return Err(TransportError::FrameTooLong);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    return Err(TransportError::Timeout {
                        operation: "read",
                        timeout,
                    });
                }
                std::thread::yield_now();
            }
            Err(error) => return Err(TransportError::Io(error.to_string())),
        }
        if Instant::now() >= deadline {
            return Err(TransportError::Timeout {
                operation: "read",
                timeout,
            });
        }
    }
}

fn write_all_with_timeout<W: Write>(
    writer: &mut W,
    frame: &[u8],
    timeout: Duration,
) -> Result<(), TransportError> {
    let deadline = Instant::now() + timeout;
    let mut written = 0;
    while written < frame.len() {
        match writer.write(&frame[written..]) {
            Ok(0) => return Err(TransportError::Disconnected),
            Ok(count) => written += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    return Err(TransportError::Timeout {
                        operation: "write",
                        timeout,
                    });
                }
                std::thread::yield_now();
            }
            Err(error) => return Err(TransportError::Io(error.to_string())),
        }
        if Instant::now() >= deadline && written < frame.len() {
            return Err(TransportError::Timeout {
                operation: "write",
                timeout,
            });
        }
    }
    writer
        .flush()
        .map_err(|error| TransportError::Io(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct MockReader {
        bytes: VecDeque<u8>,
    }

    impl Read for MockReader {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            match self.bytes.pop_front() {
                Some(byte) => {
                    out[0] = byte;
                    Ok(1)
                }
                None => Err(io::Error::new(io::ErrorKind::WouldBlock, "empty")),
            }
        }
    }

    struct MockWriter {
        bytes: Vec<u8>,
    }

    struct BlockingWriter;

    impl Write for BlockingWriter {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "full"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Write for MockWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn command_is_crlf_framed_and_bounded() {
        assert_eq!(encode_command(" time ").unwrap(), b"time\r\n");
        assert_eq!(encode_command("panic").unwrap(), b"panic\r\n");
        assert_eq!(encode_command("events clear").unwrap(), b"events clear\r\n");
        assert!(encode_command("drift 100").is_ok());
        assert!(encode_command("drift 127").is_ok());
        assert!(encode_command("drift 128").is_err());
        assert!(matches!(
            encode_command(""),
            Err(TransportError::InvalidCommand(_))
        ));
        assert!(matches!(
            encode_command("time\nagain"),
            Err(TransportError::InvalidCommand(_))
        ));
    }

    #[test]
    fn response_parser_accepts_crlf_and_ignores_empty_frames() {
        let mut reader = MockReader {
            bytes: b"\r\nTIME 260101120000\r\n".iter().copied().collect(),
        };
        assert_eq!(
            read_frame(&mut reader, Duration::from_millis(20)).unwrap(),
            "TIME 260101120000"
        );
    }

    #[test]
    fn response_parser_times_out_without_data() {
        let mut reader = MockReader {
            bytes: VecDeque::new(),
        };
        assert!(matches!(
            read_frame(&mut reader, Duration::ZERO),
            Err(TransportError::Timeout {
                operation: "read",
                ..
            })
        ));
    }

    #[test]
    fn write_timeout_and_framing_are_separate_from_parser() {
        let mut writer = MockWriter { bytes: Vec::new() };
        write_all_with_timeout(
            &mut writer,
            &encode_command("help").unwrap(),
            Duration::from_millis(20),
        )
        .unwrap();
        assert_eq!(writer.bytes, b"help\r\n");

        let mut blocked = BlockingWriter;
        assert!(matches!(
            write_all_with_timeout(&mut blocked, b"help\r\n", Duration::ZERO),
            Err(TransportError::Timeout {
                operation: "write",
                ..
            })
        ));
    }
}
