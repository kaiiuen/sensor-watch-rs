//! Safe physical connection probe for Advanced users.
//!
//! USB exposes the UF2 mass-storage bootloader only; it does not expose the
//! watch sensors or application runtime. Runtime checks therefore use only the
//! separately wired, user-selected UART jig.

use crate::transport::{PortChoice, SerialTransport};
use std::fmt;
use std::path::{Path, PathBuf};

const MAX_INFO_BYTES: usize = 4096;

const COMMANDS: [&str; 6] = ["help", "time", "identity", "events", "panic", "optical"];
pub(crate) const COMMAND_COUNT: usize = COMMANDS.len();
const MAX_LOG_LINES: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestStatus {
    Pass,
    Fail,
    NotAvailable,
    NotTested,
}

impl TestStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotAvailable => "NOT AVAILABLE",
            Self::NotTested => "NOT TESTED",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub reason: String,
}

#[derive(Clone, Debug, Default)]
pub struct ProbeReport {
    pub tests: Vec<TestResult>,
    pub log: Vec<String>,
    pub generated_at: String,
    pub identity: Option<crate::device_identity::DeviceIdentity>,
}

impl ProbeReport {
    fn add(&mut self, name: impl Into<String>, status: TestStatus, reason: impl Into<String>) {
        self.tests.push(TestResult {
            name: name.into(),
            status,
            reason: reason.into(),
        });
    }

    fn log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > MAX_LOG_LINES {
            let excess = self.log.len() - MAX_LOG_LINES;
            self.log.drain(..excess);
        }
    }

    pub fn text(&self) -> String {
        let mut out = format!(
            "Sensor Watch physical probe\nGenerated: {}\n\n",
            self.generated_at
        );
        for test in &self.tests {
            out.push_str(&format!(
                "[{}] {} - {}\n",
                test.status.label(),
                test.name,
                test.reason
            ));
        }
        out.push_str("\nLog\n");
        out.push_str(&self.log.join("\n"));
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeProgress {
    pub completed: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Disconnected,
}

pub struct ProbeResult<T> {
    pub report: ProbeReport,
    pub transport: Option<T>,
    pub connection: ConnectionState,
}

pub trait ProbeTransport: Send {
    fn port_name(&self) -> &str;
    fn command(&mut self, command: &str) -> Result<String, crate::transport::TransportError>;
}

impl ProbeTransport for SerialTransport {
    fn port_name(&self) -> &str {
        SerialTransport::port_name(self)
    }

    fn command(&mut self, command: &str) -> Result<String, crate::transport::TransportError> {
        SerialTransport::command(self, command)
    }
}

#[derive(Debug)]
struct DriveInfo {
    root: PathBuf,
    info: String,
}

/// Run all safe checks on an owned transport. No command is sent unless
/// `uart` was already connected explicitly by the user in the UI.
pub fn run(
    artifact: Option<&Path>,
    ports: &[PortChoice],
    connection_error: Option<&str>,
    uart: Option<SerialTransport>,
    progress: impl FnMut(ProbeProgress),
) -> ProbeResult<SerialTransport> {
    run_with_transport(artifact, ports, connection_error, uart, progress)
}

pub fn run_with_transport<T: ProbeTransport>(
    artifact: Option<&Path>,
    ports: &[PortChoice],
    connection_error: Option<&str>,
    mut uart: Option<T>,
    mut progress: impl FnMut(ProbeProgress),
) -> ProbeResult<T> {
    let mut report = ProbeReport {
        generated_at: unix_timestamp(),
        ..ProbeReport::default()
    };
    report.log("Starting physical probe; USB cannot test sensors or application hardware.");

    let drives = enumerate_drives(&mut report);
    let total = progress_total(drives.len());
    progress(ProbeProgress {
        completed: 0,
        total,
        message: "Physical probe checks planned".into(),
    });
    for (index, drive) in drives.iter().enumerate() {
        progress(ProbeProgress {
            completed: index + 1,
            total,
            message: format!("Inspected removable drive {}", drive.root.display()),
        });
    }
    if drives.is_empty() {
        report.add(
            "USB UF2 bootloader drive",
            TestStatus::NotAvailable,
            "USB drive not detected",
        );
        report.add(
            "Unknown removable drive",
            TestStatus::NotAvailable,
            "no removable drive was enumerated",
        );
    } else {
        let watch_drive = drives.iter().find(|drive| is_watch_info(&drive.info));
        if let Some(drive) = watch_drive {
            report.add(
                "USB UF2 bootloader drive",
                TestStatus::Pass,
                format!("watch-like UF2 drive detected at {}", drive.root.display()),
            );
        } else {
            report.add(
                "USB UF2 bootloader drive",
                TestStatus::Fail,
                "removable drive(s) detected, but none identified as Sensor Watch UF2",
            );
            report.add(
                "Unknown removable drive",
                TestStatus::Fail,
                format!(
                    "{} removable drive(s) did not contain Sensor Watch UF2 metadata",
                    drives.len()
                ),
            );
        }
    }

    match artifact {
        Some(path) => match std::fs::read(path) {
            Ok(bytes) => match sensor_watch_core::uf2::validate(&bytes) {
                Ok(valid) => report.add(
                    "UF2 artifact metadata/family/size",
                    TestStatus::Pass,
                    format!(
                        "{} blocks; {} bytes, Sensor Watch family verified",
                        valid.block_count,
                        bytes.len()
                    ),
                ),
                Err(error) => report.add(
                    "UF2 artifact metadata/family/size",
                    TestStatus::Fail,
                    format!("{}: {error}", path.display()),
                ),
            },
            Err(error) => report.add(
                "UF2 artifact metadata/family/size",
                TestStatus::Fail,
                format!("could not read {}: {error}", path.display()),
            ),
        },
        None => report.add(
            "UF2 artifact metadata/family/size",
            TestStatus::NotTested,
            "no built UF2 artifact is available",
        ),
    }

    if ports.is_empty() {
        report.add(
            "UART jig COM port",
            TestStatus::NotAvailable,
            "no serial transport available",
        );
    } else if let Some(error) = connection_error {
        report.add(
            "UART jig COM port",
            TestStatus::Fail,
            format!("port present but connection failed: {error}"),
        );
    } else if uart.is_none() {
        report.add(
            "UART jig COM port",
            TestStatus::NotTested,
            format!(
                "{} port(s) discovered; select one and connect explicitly",
                ports.len()
            ),
        );
    } else {
        let port = uart
            .as_ref()
            .map(|serial| serial.port_name())
            .unwrap_or("selected port");
        report.add(
            "UART jig COM port",
            TestStatus::Pass,
            format!("connected to {port}"),
        );
        for (index, command) in COMMANDS.into_iter().enumerate() {
            let Some(serial) = uart.as_mut() else {
                break;
            };
            progress(ProbeProgress {
                completed: drives.len() + index + 1,
                total,
                message: format!("Running UART check: {command}"),
            });
            report.log(format!("[UART {}] > {command}", serial.port_name()));
            match serial.command(command) {
                Ok(reply) => {
                    report.log(format!("< {reply}"));
                    if command == "identity" {
                        match crate::device_identity::parse_reply(&reply) {
                            Ok(identity) => {
                                report.identity = Some(identity);
                                report.add(
                                    "UART device identity",
                                    TestStatus::Pass,
                                    "masked fingerprint received; not authentication",
                                );
                            }
                            Err(error) => report.add(
                                "UART device identity",
                                TestStatus::Fail,
                                format!("identity reply rejected: {error:?}"),
                            ),
                        }
                    }
                    let (status, reason) = classify_command_reply(command, &reply);
                    report.add(format!("UART read-only command: {command}"), status, reason);
                }
                Err(error) => {
                    let disconnected = error.is_connection_lost();
                    report.log(format!("UART error for {command}: {error}"));
                    report.add(
                        format!("UART read-only command: {command}"),
                        TestStatus::Fail,
                        error.to_string(),
                    );
                    if disconnected {
                        uart = None;
                        break;
                    }
                }
            }
        }
    }
    progress(ProbeProgress {
        completed: total,
        total,
        message: "Physical probe checks complete".into(),
    });
    let connection = if uart.is_some() {
        ConnectionState::Connected
    } else {
        ConnectionState::Disconnected
    };
    ProbeResult {
        report,
        transport: uart,
        connection,
    }
}

fn classify_command_reply(command: &str, reply: &str) -> (TestStatus, &'static str) {
    if command == "identity" {
        return (
            TestStatus::Pass,
            "reply parsed separately; masked fingerprint only",
        );
    }
    if command == "optical" && reply.trim() == "?" {
        return (
            TestStatus::NotAvailable,
            "optical capability is not available on this firmware",
        );
    }
    (TestStatus::Pass, "reply received; no mutation command sent")
}

pub(crate) fn is_watch_info(info: &str) -> bool {
    let lines = info
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let has_uf2_bootloader = lines.iter().any(|line| {
        let upper = line.to_ascii_uppercase();
        upper.starts_with("UF2 BOOTLOADER")
    });
    let board_id = lines.iter().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case("board-id")
            .then_some(value.trim())
    });
    let family_id = lines.iter().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim().eq_ignore_ascii_case("family-id")
            || key.trim().eq_ignore_ascii_case("family id"))
        .then_some(value.trim())
    });
    let known_board = board_id.is_some_and(|value| {
        let upper = value.to_ascii_uppercase();
        upper.starts_with("SENSOR WATCH") || upper.starts_with("SAML22")
    });
    let known_family = family_id.is_some_and(|value| {
        value
            .trim_start_matches("0x")
            .trim_start_matches("0X")
            .eq_ignore_ascii_case("2C29472F")
    });
    (has_uf2_bootloader && (known_board || known_family))
        || lines
            .iter()
            .any(|line| line.eq_ignore_ascii_case("UF2 Sensor Watch"))
}

fn progress_total(drive_count: usize) -> usize {
    drive_count + COMMAND_COUNT
}

fn enumerate_drives(report: &mut ProbeReport) -> Vec<DriveInfo> {
    #[cfg(windows)]
    {
        let mut drives = Vec::new();
        for letter in b'A'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", letter as char));
            if !root.exists() || !is_removable_drive(&root) {
                continue;
            }
            let path = root.join("INFO_UF2.TXT");
            let info = match std::fs::File::open(&path) {
                Ok(file) => {
                    let mut bytes = Vec::new();
                    use std::io::Read;
                    let _ = file.take(MAX_INFO_BYTES as u64).read_to_end(&mut bytes);
                    String::from_utf8_lossy(&bytes).into_owned()
                }
                Err(error) => format!("<INFO_UF2.TXT unavailable: {error}>"),
            };
            report.log(format!(
                "Inspected {} ({} bytes bounded)",
                path.display(),
                MAX_INFO_BYTES
            ));
            drives.push(DriveInfo { root, info });
        }
        drives
    }
    #[cfg(not(windows))]
    {
        report.log("Windows removable-drive enumeration is unavailable on this host.");
        Vec::new()
    }
}

#[cfg(windows)]
pub(crate) fn is_removable_drive(root: &Path) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDriveTypeW(root_path_name: *const u16) -> u32;
    }
    let mut wide: Vec<u16> = root.to_string_lossy().encode_utf16().collect();
    wide.push(0);
    unsafe { GetDriveTypeW(wide.as_ptr()) == 2 }
}

fn unix_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

impl fmt::Display for TestStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::mpsc;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::Duration;

    struct BlockingTransport {
        started: mpsc::Sender<()>,
        release: Arc<AtomicBool>,
    }

    impl ProbeTransport for BlockingTransport {
        fn port_name(&self) -> &str {
            "fake"
        }

        fn command(&mut self, _command: &str) -> Result<String, crate::transport::TransportError> {
            let _ = self.started.send(());
            while !self.release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            Ok("ok".into())
        }
    }

    #[test]
    fn blocking_probe_worker_is_nonblocking_and_rejoins() {
        let (started_tx, started_rx) = mpsc::channel();
        let release = Arc::new(AtomicBool::new(false));
        let worker_release = Arc::clone(&release);
        let worker = std::thread::spawn(move || {
            run_with_transport(
                None,
                &[PortChoice {
                    name: "fake".into(),
                    description: "test".into(),
                }],
                None,
                Some(BlockingTransport {
                    started: started_tx,
                    release: worker_release,
                }),
                |_| {},
            )
        });
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should reach the first command");
        assert!(
            !worker.is_finished(),
            "blocking transport must remain off the caller"
        );
        release.store(true, Ordering::Release);
        let result = worker.join().expect("probe worker should join");
        assert_eq!(result.connection, ConnectionState::Connected);
        assert_eq!(
            result
                .report
                .tests
                .iter()
                .filter(|test| test.name.starts_with("UART read-only command:"))
                .count(),
            5
        );
    }

    #[test]
    fn progress_total_is_drive_count_plus_six_commands() {
        assert_eq!(progress_total(0), 6);
        assert_eq!(progress_total(1), 7);
        assert_eq!(progress_total(3), 8);
    }

    #[test]
    fn five_commands_report_the_actual_total_without_uart() {
        let mut progress = Vec::new();
        let result = run_with_transport(None, &[], None, None::<BlockingTransport>, |event| {
            progress.push(event);
        });

        assert_eq!(result.connection, ConnectionState::Disconnected);
        let total = progress.first().expect("initial progress").total;
        assert!(total >= COMMAND_COUNT);
        assert_eq!(progress.last().map(|event| event.completed), Some(total));
    }

    #[test]
    fn five_commands_are_reported_with_a_connected_transport() {
        let release = Arc::new(AtomicBool::new(true));
        let (started_tx, _started_rx) = mpsc::channel();
        let mut progress = Vec::new();
        let result = run_with_transport(
            None,
            &[PortChoice {
                name: "fake".into(),
                description: "test".into(),
            }],
            None,
            Some(BlockingTransport {
                started: started_tx,
                release,
            }),
            |event| progress.push(event),
        );

        assert_eq!(result.connection, ConnectionState::Connected);
        assert_eq!(
            result
                .report
                .tests
                .iter()
                .filter(|test| test.name.starts_with("UART read-only command:"))
                .count(),
            COMMAND_COUNT
        );
        let total = progress.first().expect("initial progress").total;
        assert_eq!(progress.last().map(|event| event.completed), Some(total));
        assert!(progress.iter().all(|event| event.total == total));
    }

    #[test]
    fn unsupported_optical_response_is_not_available() {
        let mut response = Cursor::new(b"?\r\n");
        let reply = crate::transport::read_frame(&mut response, Duration::from_millis(20))
            .expect("the unsupported response should be a valid frame");

        assert_eq!(
            classify_command_reply("optical", &reply),
            (
                TestStatus::NotAvailable,
                "optical capability is not available on this firmware"
            )
        );
    }

    #[test]
    fn unrelated_command_replies_remain_pass() {
        assert_eq!(
            classify_command_reply("time", "?"),
            (TestStatus::Pass, "reply received; no mutation command sent")
        );
        assert_eq!(
            classify_command_reply("optical", "OPTICAL disabled"),
            (TestStatus::Pass, "reply received; no mutation command sent")
        );
    }

    #[test]
    fn watch_info_requires_uf2_and_known_identity() {
        assert!(is_watch_info(
            "UF2 Bootloader; Board-ID: Sensor Watch SAML22"
        ));
        assert!(is_watch_info("UF2 Bootloader; Board-ID: Sensor Watch"));
        assert!(is_watch_info("UF2 Bootloader; Family ID: 0x2C29472F"));
        assert!(!is_watch_info("UF2 Bootloader; Board-ID: Generic UF2"));
        assert!(!is_watch_info("UF2 Bootloader; Board-ID: Other"));
        assert!(!is_watch_info("UF2 Bootloader; Board-ID: Arduino Zero"));
        assert!(!is_watch_info(
            "UF2 Sensor Watch telemetry: this is arbitrary spoofed text"
        ));
        assert!(!is_watch_info("UF2; Sensor Watch"));
    }
}
