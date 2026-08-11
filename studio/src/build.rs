//! Firmware build logic.
//!
//! Invokes the firmware build (cargo + rust-objcopy) and converts the raw
//! binary to a `.uf2` file using the `sensor-watch-core` UF2 encoder. This is
//! the "assembler" part of Firmware Studio.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolves the firmware project directory.
///
/// The app can be run from anywhere (e.g. double-clicking the exe in
/// `target/release/`), so we resolve the firmware project relative to the
/// executable's location rather than the current working directory. We walk up
/// from the exe until we find a directory containing `Cargo.toml` with the
/// `sensor-watch` package (the workspace root).
pub fn firmware_dir() -> PathBuf {
    // Start from the directory containing the executable.
    let mut dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    // Walk up looking for the workspace root (contains Cargo.toml).
    loop {
        let cargo = dir.join("Cargo.toml");
        if cargo.exists() {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    // Fall back to the CWD.
    PathBuf::from(".")
}

/// The embedded target triple.
pub const TARGET: &str = "thumbv6m-none-eabi";

/// Rejects output paths that could accidentally target a file instead of a
/// directory. The build worker still reports every filesystem/tool failure.
pub fn validate_output_dir(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.to_string_lossy().len() > 240 {
        return Err("output directory is empty or excessively long".into());
    }
    if path.exists() && !path.is_dir() {
        return Err("output path is not a directory".into());
    }
    Ok(())
}

/// A build result.
pub struct BuildResult {
    pub success: bool,
    pub message: String,
    pub uf2_path: Option<PathBuf>,
}

/// Runs the full firmware build: cargo build, extract the raw binary, and
/// convert it to a `.uf2` file in the given output directory.
pub fn build_firmware(output_dir: &Path) -> BuildResult {
    if let Err(error) = validate_output_dir(output_dir) {
        return BuildResult {
            success: false,
            message: error,
            uf2_path: None,
        };
    }
    let fw_dir = firmware_dir();

    // Ensure the output directory exists (it may not on a fresh standalone exe).
    if let Err(e) = std::fs::create_dir_all(output_dir) {
        return BuildResult {
            success: false,
            message: format!("failed to create output dir: {e}"),
            uf2_path: None,
        };
    }

    // 1. Build the firmware in release mode.
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg(TARGET)
        .current_dir(&fw_dir)
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
    let elf = fw_dir.join(format!("target/{TARGET}/release/sensor-watch"));
    let bin = fw_dir.join(format!("target/{TARGET}/release/sensor-watch.bin"));
    let uf2 = output_dir.join("sensor-watch.uf2");

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
    if image.is_empty() || image.len() > sensor_watch_core::uf2::MAX_APPLICATION_BYTES {
        return BuildResult {
            success: false,
            message: format!(
                "firmware binary must be non-empty and no larger than {} bytes",
                sensor_watch_core::uf2::MAX_APPLICATION_BYTES
            ),
            uf2_path: None,
        };
    }
    let uf2_data = sensor_watch_core::uf2::convert_to_uf2(&image);
    if let Err(error) = sensor_watch_core::uf2::validate(&uf2_data) {
        return BuildResult {
            success: false,
            message: format!("generated UF2 failed validation: {error}"),
            uf2_path: None,
        };
    }

    // Stage beside the destination, then replace it only after validation. Keep
    // a backup while replacing so a failed Windows rename can be rolled back.
    let tmp = uf2.with_extension("uf2.tmp");
    if let Err(e) = std::fs::remove_file(&tmp) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return BuildResult {
                success: false,
                message: format!("failed to remove stale UF2 temp file: {e}"),
                uf2_path: None,
            };
        }
    }
    if let Err(e) = std::fs::write(&tmp, &uf2_data) {
        return BuildResult {
            success: false,
            message: format!("failed to write UF2 temp file: {e}"),
            uf2_path: None,
        };
    }
    let backup = uf2.with_extension("uf2.previous");
    let had_old = uf2.exists();
    if had_old {
        let _ = std::fs::remove_file(&backup);
        if let Err(e) = std::fs::rename(&uf2, &backup) {
            let _ = std::fs::remove_file(&tmp);
            return BuildResult {
                success: false,
                message: format!("failed to stage existing UF2: {e}"),
                uf2_path: None,
            };
        }
    }
    if let Err(e) = std::fs::rename(&tmp, &uf2) {
        if had_old {
            let _ = std::fs::rename(&backup, &uf2);
        }
        let _ = std::fs::remove_file(&tmp);
        return BuildResult {
            success: false,
            message: format!("failed to replace UF2: {e}"),
            uf2_path: None,
        };
    }
    // Never discard the previous artifact. Preserve it as a uniquely named,
    // validated recovery generation before publishing the new output.
    if had_old {
        let recovery_dir = output_dir.join("recovery").join("generations");
        if let Err(e) = std::fs::create_dir_all(&recovery_dir) {
            return BuildResult {
                success: false,
                message: format!("built UF2, but could not create recovery directory: {e}"),
                uf2_path: Some(uf2),
            };
        }
        let old_data = match std::fs::read(&backup) {
            Ok(data) => data,
            Err(e) => {
                return BuildResult {
                    success: false,
                    message: format!("built UF2, but could not read previous backup: {e}"),
                    uf2_path: Some(uf2),
                };
            }
        };
        if let Err(e) = sensor_watch_core::uf2::validate(&old_data) {
            return BuildResult {
                success: false,
                message: format!("refusing to retain invalid previous UF2: {e}"),
                uf2_path: Some(uf2),
            };
        }
        let old_sha = hex_sha256(&old_data);
        let generation = format!("g{}-{}", unix_nanos(), &old_sha[..12]);
        let old_path = recovery_dir.join(format!("{generation}.uf2"));
        if let Err(e) = std::fs::copy(&backup, &old_path) {
            return BuildResult {
                success: false,
                message: format!("built UF2, but could not preserve previous generation: {e}"),
                uf2_path: Some(uf2),
            };
        }
        if let Err(e) = write_manifest(&old_path, &old_data, &generation) {
            return BuildResult {
                success: false,
                message: format!("built UF2, but could not write recovery manifest: {e}"),
                uf2_path: Some(uf2),
            };
        }
        let _ = std::fs::remove_file(&backup);
    }
    let generation = format!("g{}-{}", unix_nanos(), &hex_sha256(&uf2_data)[..12]);
    if let Err(e) = write_manifest(&uf2, &uf2_data, &generation) {
        return BuildResult {
            success: false,
            message: format!("UF2 published, but manifest write failed: {e}"),
            uf2_path: Some(uf2),
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

fn unix_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn hex_sha256(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Writes the same signed, offline-verifiable manifest used by verify-uf2.py.
fn write_manifest(uf2: &Path, data: &[u8], generation: &str) -> std::io::Result<()> {
    let parsed = sensor_watch_core::uf2::validate(data).map_err(std::io::Error::other)?;
    let mut fields = BTreeMap::new();
    fields.insert(
        "application_start",
        serde_json::json!(format!("0x{:08X}", sensor_watch_core::uf2::APP_START_ADDR)),
    );
    fields.insert("artifact", serde_json::json!(uf2.display().to_string()));
    fields.insert("board", serde_json::json!("ATSAML22J18A"));
    fields.insert(
        "crc32_ieee",
        serde_json::json!(format!(
            "0x{:08X}",
            sensor_watch_core::uf2::crc32(&parsed.image)
        )),
    );
    fields.insert(
        "family_id",
        serde_json::json!(format!(
            "0x{:08X}",
            sensor_watch_core::uf2::SAML22_FAMILY_ID
        )),
    );
    fields.insert(
        "format",
        serde_json::json!("sensor-watch-recovery-manifest-v2"),
    );
    fields.insert("generation_id", serde_json::json!(generation));
    fields.insert(
        "maximum_application_bytes",
        serde_json::json!(sensor_watch_core::uf2::MAX_APPLICATION_BYTES),
    );
    fields.insert("payload_bytes", serde_json::json!(parsed.image.len()));
    fields.insert(
        "payload_sha256",
        serde_json::json!(hex_sha256(&parsed.image)),
    );
    fields.insert("sha256", serde_json::json!(hex_sha256(data)));
    fields.insert("uf2_blocks", serde_json::json!(parsed.block_count));
    fields.insert("uf2_bytes", serde_json::json!(data.len()));
    let canonical = serde_json::to_vec(&fields).map_err(std::io::Error::other)?;
    let signature = format!("sha256:{}", hex_sha256(&canonical));
    fields.insert("signature", serde_json::json!(signature.clone()));
    let manifest = serde_json::to_string_pretty(&fields).map_err(std::io::Error::other)? + "\n";
    let manifest_path = uf2.with_extension("uf2.json");
    std::fs::write(&manifest_path, manifest)?;
    std::fs::write(
        manifest_path.with_extension("json.sig"),
        format!("{signature}\n"),
    )
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

/// Returns the path to the last-built `.uf2` file in the given output dir, if it
/// exists.
pub fn last_uf2(output_dir: &Path) -> Option<PathBuf> {
    let p = output_dir.join("sensor-watch.uf2");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}
