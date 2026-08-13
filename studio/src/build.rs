//! Firmware build logic.
//!
//! Invokes the firmware build (cargo + rust-objcopy) and converts the raw
//! binary to a `.uf2` file using the `sensor-watch-core` UF2 encoder. This is
//! the "assembler" part of Firmware Studio.
//!
//! The Studio UI currently does not pass its selected preset, faces, board, or
//! component profile into this module. Builds therefore fail closed rather than
//! publishing a stock artifact that could be mistaken for a configured one.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

struct BuildLock {
    path: PathBuf,
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn acquire_build_lock(root: &Path) -> Result<BuildLock, String> {
    let target = root.join("target");
    std::fs::create_dir_all(&target).map_err(|e| format!("cannot create build directory: {e}"))?;
    let path = target.join(".sensor-watch-build.lock");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map(|_| BuildLock { path })
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                "another firmware build is already running".into()
            } else {
                format!("cannot acquire build lock: {e}")
            }
        })
}

fn is_workspace_root(path: &Path) -> bool {
    let manifest = path.join("Cargo.toml");
    let Ok(metadata) = std::fs::symlink_metadata(&manifest) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    let Ok(contents) = std::fs::read_to_string(&manifest) else {
        return false;
    };
    contents.contains("[workspace]")
        && contents.contains("[package]")
        && contents
            .lines()
            .any(|line| line.trim() == "name = \"sensor-watch\"")
}

fn canonical_workspace_root(path: &Path) -> Option<PathBuf> {
    let root = path.canonicalize().ok()?;
    is_workspace_root(&root).then_some(root)
}

fn compiled_workspace_root() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    canonical_workspace_root(manifest_dir.parent().unwrap_or(manifest_dir))
}

fn trusted_runtime_root(candidate: &Path, trusted: &Path) -> Option<PathBuf> {
    let root = canonical_workspace_root(candidate)?;
    (root == trusted).then_some(root)
}

/// Resolves the firmware workspace without trusting an arbitrary ancestor.
///
/// The app can be run from anywhere (e.g. double-clicking an exe copied out of
/// `target/release/`). Runtime candidates are accepted only when they resolve
/// to the workspace this binary was compiled from. This prevents an unrelated
/// ancestor `Cargo.toml` from redirecting firmware builds or source discovery.
pub fn firmware_dir() -> PathBuf {
    let Some(trusted) = compiled_workspace_root() else {
        return PathBuf::from(".");
    };

    if let Ok(executable) = std::env::current_exe() {
        if let Some(mut dir) = executable.parent().map(Path::to_path_buf) {
            loop {
                if let Some(root) = trusted_runtime_root(&dir, &trusted) {
                    return root;
                }
                if !dir.pop() {
                    break;
                }
            }
        }
    }

    // A copied executable must not select a similarly named project found at
    // runtime. Use only the workspace this binary was compiled from.
    trusted
}

/// The embedded target triple.
pub const TARGET: &str = "thumbv6m-none-eabi";

/// Rejects output paths that could accidentally target a file instead of a
/// directory. The build worker still reports every filesystem/tool failure.
pub fn validate_output_dir(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.to_string_lossy().len() > 240 {
        return Err("output directory is empty or excessively long".into());
    }
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("output path must be a real directory, not a symlink or file".into());
        }
        path.canonicalize()
            .map_err(|e| format!("cannot resolve output directory: {e}"))?;
    }
    Ok(())
}

/// A build result.
pub struct BuildResult {
    pub success: bool,
    pub message: String,
    pub uf2_path: Option<PathBuf>,
}

/// The build cannot truthfully produce a configured artifact until Studio's
/// selections are part of the firmware build inputs.
pub const CONFIGURATION_BUILD_BLOCKED: &str =
    "firmware build refused: selected presets, faces, board, and component profile are not wired into firmware build inputs; no configured UF2 was generated";

/// Returns the fail-closed build validation error.
///
/// Keep this as a separate, side-effect-free check so callers and tests can
/// surface the same limitation without touching the filesystem or toolchain.
pub fn validate_configuration_inputs() -> Result<(), &'static str> {
    Err(CONFIGURATION_BUILD_BLOCKED)
}

/// Runs the full firmware build: cargo build, extract the raw binary, and
/// convert it to a `.uf2` file in the given output directory.
pub fn build_firmware(output_dir: &Path) -> BuildResult {
    if let Err(error) = validate_configuration_inputs() {
        return BuildResult {
            success: false,
            message: error.to_string(),
            uf2_path: None,
        };
    }
    if let Err(error) = validate_output_dir(output_dir) {
        return BuildResult {
            success: false,
            message: error,
            uf2_path: None,
        };
    }
    let fw_dir = firmware_dir();
    let _build_lock = match acquire_build_lock(&fw_dir) {
        Ok(lock) => lock,
        Err(error) => {
            return BuildResult {
                success: false,
                message: error,
                uf2_path: None,
            };
        }
    };

    // Ensure the output directory exists (it may not on a fresh standalone exe),
    // then validate it again. The second check closes the gap where a path could
    // be replaced by a symlink while it was being created.
    if let Err(e) = std::fs::create_dir_all(output_dir) {
        return BuildResult {
            success: false,
            message: format!("failed to create output dir: {e}"),
            uf2_path: None,
        };
    }
    if let Err(error) = validate_output_dir(output_dir) {
        return BuildResult {
            success: false,
            message: format!("output directory changed during setup: {error}"),
            uf2_path: None,
        };
    }

    // 1. Build the firmware in release mode.
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--bin")
        .arg("sensor-watch")
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

    // Keep the ELF, source tree, and panic resolver tied to this exact build.
    // The manifest is host-side only and does not change firmware behavior.
    if let Err(error) = crate::panic_map::write_manifest(&elf, &fw_dir) {
        return BuildResult {
            success: false,
            message: format!("failed to write panic map manifest: {error}"),
            uf2_path: None,
        };
    }

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
    for path in [&uf2, &tmp] {
        if std::fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return BuildResult {
                success: false,
                message: format!("refusing symlinked build output: {}", path.display()),
                uf2_path: None,
            };
        }
    }
    if let Err(e) = std::fs::remove_file(&tmp) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return BuildResult {
                success: false,
                message: format!("failed to remove stale UF2 temp file: {e}"),
                uf2_path: None,
            };
        }
    }
    let write_temp = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(&uf2_data)
        });
    if let Err(e) = write_temp {
        return BuildResult {
            success: false,
            message: format!("failed to write UF2 temp file: {e}"),
            uf2_path: None,
        };
    }
    let staged = match std::fs::read(&tmp) {
        Ok(data) => data,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return BuildResult {
                success: false,
                message: format!("failed to read staged UF2: {e}"),
                uf2_path: None,
            };
        }
    };
    if staged != uf2_data || sensor_watch_core::uf2::validate(&staged).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return BuildResult {
            success: false,
            message: "staged UF2 failed content validation".into(),
            uf2_path: None,
        };
    }
    let backup = uf2.with_extension("uf2.previous");
    if let Err(error) =
        ensure_regular_or_absent(&uf2).and_then(|_| ensure_regular_or_absent(&backup))
    {
        let _ = std::fs::remove_file(&tmp);
        return BuildResult {
            success: false,
            message: format!("refusing unsafe UF2 output path: {error}"),
            uf2_path: None,
        };
    }
    let had_old = uf2.is_file();
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
    let published = match std::fs::read(&uf2) {
        Ok(data) => data,
        Err(e) => {
            return BuildResult {
                success: false,
                message: format!("UF2 published but could not be re-read: {e}"),
                uf2_path: Some(uf2),
            };
        }
    };
    if published != uf2_data || sensor_watch_core::uf2::validate(&published).is_err() {
        return BuildResult {
            success: false,
            message: "published UF2 failed content validation".into(),
            uf2_path: Some(uf2),
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

/// Writes the same signed, offline-verifiable manifest used by sensor-watch-tools.
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
    write_manifest_file(&manifest_path, manifest.as_bytes())?;
    write_manifest_file(
        &manifest_path.with_extension("json.sig"),
        format!("{signature}\n").as_bytes(),
    )
}

fn write_manifest_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("refusing non-regular manifest path: {}", path.display()),
            ));
        }
    }
    let temp = path.with_extension("json.tmp");
    if let Ok(metadata) = std::fs::symlink_metadata(&temp) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("refusing manifest temp path: {}", temp.display()),
            ));
        }
        std::fs::remove_file(&temp)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    use std::io::Write;
    file.write_all(contents)?;
    file.sync_all()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(temp, path)
}

/// Locates `rust-objcopy` under the Rust toolchain.
fn ensure_regular_or_absent(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("refusing symlinked path: {}", path.display()))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(format!("path is not a regular file: {}", path.display()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect path: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sensor-watch-studio-workspace-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn workspace_manifest_is_identified_by_package_and_workspace() {
        let root = temp_root("valid");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\".\"]\n\n[package]\nname = \"sensor-watch\"\n",
        )
        .unwrap();
        assert_eq!(
            canonical_workspace_root(&root),
            Some(root.canonicalize().unwrap())
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unrelated_ancestor_manifest_is_not_accepted() {
        let root = temp_root("unrelated");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\".\"]\n\n[package]\nname = \"other-project\"\n",
        )
        .unwrap();
        assert!(canonical_workspace_root(&root).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn different_valid_workspace_is_not_trusted() {
        let root = temp_root("untrusted");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\".\"]\n\n[package]\nname = \"sensor-watch\"\n",
        )
        .unwrap();
        let trusted = compiled_workspace_root().unwrap();
        assert!(trusted_runtime_root(&root, &trusted).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn firmware_dir_resolves_the_compiled_workspace() {
        assert_eq!(firmware_dir(), compiled_workspace_root().unwrap());
    }

    #[test]
    fn configured_builds_are_rejected_before_side_effects() {
        assert_eq!(
            validate_configuration_inputs(),
            Err(CONFIGURATION_BUILD_BLOCKED)
        );

        let result = build_firmware(Path::new("target/studio-test-output"));
        assert!(!result.success);
        assert_eq!(result.message, CONFIGURATION_BUILD_BLOCKED);
        assert!(result.uf2_path.is_none());
    }
}

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
