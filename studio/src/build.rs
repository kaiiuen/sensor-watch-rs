//! Firmware build logic.
//!
//! Invokes the firmware build (cargo + rust-objcopy) and converts the raw
//! binary to a `.uf2` file using the `sensor-watch-core` UF2 encoder. This is
//! the "assembler" part of Firmware Studio.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The path to the firmware project (relative to the app's workspace root).
pub const FIRMWARE_DIR: &str = ".";

/// The embedded target triple.
pub const TARGET: &str = "thumbv6m-none-eabi";

/// A build result.
pub struct BuildResult {
    pub success: bool,
    pub message: String,
    pub uf2_path: Option<PathBuf>,
}

/// Runs the full firmware build: cargo build, extract the raw binary, and
/// convert it to a `.uf2` file.
pub fn build_firmware() -> BuildResult {
    let firmware_dir = Path::new(FIRMWARE_DIR);

    // 1. Build the firmware in release mode.
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg(TARGET)
        .current_dir(firmware_dir)
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            return BuildResult {
                success: false,
                message: format!("cargo build failed with exit code {:?}", s.code()),
                uf2_path: None,
            };
        }
        Err(e) => {
            return BuildResult {
                success: false,
                message: format!("failed to run cargo: {e}"),
                uf2_path: None,
            };
        }
    }

    // 2. Locate the ELF and the raw binary.
    let elf = firmware_dir.join(format!("target/{TARGET}/release/sensor-watch"));
    let bin = firmware_dir.join(format!("target/{TARGET}/release/sensor-watch.bin"));
    let uf2 = firmware_dir.join(format!("target/{TARGET}/release/sensor-watch.uf2"));

    // 3. Extract the raw binary with rust-objcopy.
    let objcopy = find_objcopy();
    let objcopy = match objcopy {
        Some(p) => p,
        None => {
            return BuildResult {
                success: false,
                message: "rust-objcopy not found".to_string(),
                uf2_path: None,
            };
        }
    };
    let status = Command::new(&objcopy)
        .arg("-O")
        .arg("binary")
        .arg(&elf)
        .arg(&bin)
        .status();
    match status {
        Ok(s) if s.success() => {}
        _ => {
            return BuildResult {
                success: false,
                message: "rust-objcopy failed".to_string(),
                uf2_path: None,
            };
        }
    }

    // 4. Read the raw binary and convert it to UF2.
    let image = match std::fs::read(&bin) {
        Ok(d) => d,
        Err(e) => {
            return BuildResult {
                success: false,
                message: format!("failed to read binary: {e}"),
                uf2_path: None,
            };
        }
    };
    let uf2_data = sensor_watch_core::uf2::convert_to_uf2(&image);
    if let Err(e) = std::fs::write(&uf2, &uf2_data) {
        return BuildResult {
            success: false,
            message: format!("failed to write uf2: {e}"),
            uf2_path: None,
        };
    }

    BuildResult {
        success: true,
        message: format!(
            "Built {} bytes of firmware -> {} bytes of UF2",
            image.len(),
            uf2_data.len()
        ),
        uf2_path: Some(uf2),
    }
}

/// Locates `rust-objcopy` under the Rust toolchain.
fn find_objcopy() -> Option<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let toolchains = Path::new(&home).join(".rustup/toolchains");
    let entries = std::fs::read_dir(&toolchains).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let bin = path.join("lib/rustlib/x86_64-pc-windows-msvc/bin/rust-objcopy.exe");
        if bin.exists() {
            return Some(bin);
        }
        let bin = path.join("lib/rustlib/x86_64-pc-windows-msvc/bin/rust-objcopy");
        if bin.exists() {
            return Some(bin);
        }
    }
    None
}

/// Returns the path to the last-built `.uf2` file, if it exists.
pub fn last_uf2() -> Option<PathBuf> {
    let p = Path::new(FIRMWARE_DIR).join(format!("target/{TARGET}/release/sensor-watch.uf2"));
    if p.exists() { Some(p) } else { None }
}
