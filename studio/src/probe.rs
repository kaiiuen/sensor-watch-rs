//! Safe physical connection probe for Advanced users.
//!
//! USB exposes the UF2 mass-storage bootloader only; it does not expose the
//! watch sensors or application runtime. Runtime checks therefore use only the
//! separately wired, user-selected UART jig.

use crate::transport::{PortChoice, SerialTransport};
use std::fmt;
use std::path::{Path, PathBuf};

const MAX_INFO_BYTES: usize = 4096;

const COMMANDS: [&str; 5] = ["help", "time", "events", "panic", "optical"];
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
                "[{}] {} — {}\n",
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

#[derive(Debug)]
struct DriveInfo {
    root: PathBuf,
    info: String,
}

/// Run all safe checks. No command is sent unless `uart` is already connected
/// to the port explicitly selected by the user in the UI.
pub fn run(
    artifact: Option<&Path>,
    ports: &[PortChoice],
    connection_error: Option<&str>,
    mut uart: Option<&mut SerialTransport>,
) -> ProbeReport {
    let mut report = ProbeReport {
        generated_at: unix_timestamp(),
        ..ProbeReport::default()
    };
    report.log("Starting physical probe; USB cannot test sensors or application hardware.");

    let drives = enumerate_drives(&mut report);
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
        for command in COMMANDS {
            let Some(serial) = uart.as_deref_mut() else {
                break;
            };
            report.log(format!("[UART {}] > {command}", serial.port_name()));
            match serial.command(command) {
                Ok(reply) => {
                    report.log(format!("< {reply}"));
                    report.add(
                        format!("UART read-only command: {command}"),
                        TestStatus::Pass,
                        "reply received; no mutation command sent",
                    );
                }
                Err(error) => {
                    report.log(format!("UART error for {command}: {error}"));
                    report.add(
                        format!("UART read-only command: {command}"),
                        TestStatus::Fail,
                        error.to_string(),
                    );
                }
            }
        }
    }
    report
}

fn is_watch_info(info: &str) -> bool {
    let upper = info.to_ascii_uppercase();
    upper.contains("UF2")
        && (upper.contains("SENSOR") || upper.contains("SAML22") || upper.contains("2C29472F"))
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
fn is_removable_drive(root: &Path) -> bool {
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
    #[test]
    fn watch_info_requires_uf2_and_known_identity() {
        assert!(is_watch_info(
            "UF2 Bootloader; Board-ID: Sensor Watch SAML22"
        ));
        assert!(!is_watch_info("UF2 Bootloader; Board-ID: Other"));
    }
}
