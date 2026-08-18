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
    // Once distribution discovery has run, never reach back into the compiled
    // checkout unless explicit developer mode selected it.
    if crate::distribution::initialized() {
        return crate::distribution::active()
            .firmware_project_dir()
            .unwrap_or_else(|| PathBuf::from("."));
    }
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

/// Metadata produced by the shared local UF2/manifest verification path.
///
/// The digest fields prove only that this artifact and its sidecars are locally
/// consistent. They do not establish cryptographic authenticity or provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactInspection {
    pub path: PathBuf,
    pub generation: String,
    pub family_id: String,
    pub uf2_bytes: String,
    pub uf2_blocks: String,
    pub payload_bytes: String,
    pub sha256: String,
    pub payload_sha256: String,
    pub manifest_digest: String,
}

/// Inspects an explicitly selected UF2 and requires both manifest sidecars.
/// Passing the manifest path explicitly makes a missing `.uf2.json` fail closed;
/// the shared verifier then requires the matching `.json.sig` sidecar as well.
pub fn inspect_artifact(path: &Path) -> Result<ArtifactInspection, String> {
    let manifest_path = path.with_extension("uf2.json");
    let manifest = sensor_watch_tools::verify_uf2(path, Some(&manifest_path), None)?;
    let value = |key: &str| sensor_watch_tools::manifest_value(&manifest, key);
    Ok(ArtifactInspection {
        path: path.to_path_buf(),
        generation: value("generation_id"),
        family_id: value("family_id"),
        uf2_bytes: value("uf2_bytes"),
        uf2_blocks: value("uf2_blocks"),
        payload_bytes: value("payload_bytes"),
        sha256: value("sha256"),
        payload_sha256: value("payload_sha256"),
        manifest_digest: sensor_watch_tools::manifest_value(&manifest, "manifest_digest"),
    })
}

/// The build inputs that must be defined before Studio can publish a configured
/// artifact. Keep this list explicit: a component checkbox is not a pin map.
pub const CONFIGURATION_INPUT_CONTRACT: &[&str] = &[
    "active preset identity and ordered face/source inputs",
    "target board identity, revision, and board-specific runtime settings",
    "component-to-firmware feature/module selections",
    "concrete pin, bus, address, power, and ownership mappings for every selected component",
    "a generated-input provenance/validation record tied to the exact firmware build",
];

/// The build cannot truthfully produce a configured artifact until every item in
/// [`CONFIGURATION_INPUT_CONTRACT`] is supplied to the firmware build.
pub const CONFIGURATION_BUILD_BLOCKED: &str = concat!(
    "firmware build refused: Studio configuration input contract is incomplete; ",
    "no configured UF2 was generated. Complete these inputs before retrying:\n",
    "- active preset identity and ordered face/source inputs\n",
    "- target board identity, revision, and board-specific runtime settings\n",
    "- component-to-firmware feature/module selections\n",
    "- concrete pin, bus, address, power, and ownership mappings for every selected component\n",
    "- a generated-input provenance/validation record tied to the exact firmware build",
);

/// Returns the fail-closed build validation error.
///
/// Keep this as a separate, side-effect-free check so callers and tests can
/// surface the same limitation without touching the filesystem or toolchain.
pub fn validate_configuration_inputs() -> Result<(), &'static str> {
    Err(CONFIGURATION_BUILD_BLOCKED)
}

/// The exact inputs currently missing from the Studio-to-firmware build path.
/// This is also used by the profile UI so the disabled state cannot drift from
/// the build preflight.
pub fn missing_configuration_inputs() -> &'static [&'static str] {
    CONFIGURATION_INPUT_CONTRACT
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

    match publish_uf2(output_dir, &uf2, &uf2_data) {
        Ok(()) => BuildResult {
            success: true,
            message: format!(
                "Built {} bytes of firmware -> {} bytes of UF2",
                image.len(),
                uf2_data.len()
            ),
            uf2_path: Some(uf2),
        },
        Err(message) => BuildResult {
            success: false,
            message,
            uf2_path: None,
        },
    }
}

fn publish_uf2(output_dir: &Path, uf2: &Path, uf2_data: &[u8]) -> Result<(), String> {
    publish_uf2_with_manifest_writer(output_dir, uf2, uf2_data, |path, data, generation| {
        write_manifest(path, data, generation)
    })
}

fn publish_uf2_with_manifest_writer<F>(
    output_dir: &Path,
    uf2: &Path,
    uf2_data: &[u8],
    write_current_manifest: F,
) -> Result<(), String>
where
    F: FnMut(&Path, &[u8], &str) -> std::io::Result<()>,
{
    publish_uf2_with_cleanup_writer(output_dir, uf2, uf2_data, write_current_manifest, |path| {
        std::fs::remove_file(path)
    })
}

fn publish_uf2_with_cleanup_writer<F, C>(
    output_dir: &Path,
    uf2: &Path,
    uf2_data: &[u8],
    write_current_manifest: F,
    remove_recovery_file: C,
) -> Result<(), String>
where
    F: FnMut(&Path, &[u8], &str) -> std::io::Result<()>,
    C: FnMut(&Path) -> std::io::Result<()>,
{
    publish_uf2_with_writers(
        output_dir,
        uf2,
        uf2_data,
        write_current_manifest,
        remove_recovery_file,
        |from, to| std::fs::rename(from, to),
    )
}

fn publish_uf2_with_writers<F, C, R>(
    output_dir: &Path,
    uf2: &Path,
    uf2_data: &[u8],
    mut write_current_manifest: F,
    mut remove_recovery_file: C,
    mut rename_file: R,
) -> Result<(), String>
where
    F: FnMut(&Path, &[u8], &str) -> std::io::Result<()>,
    C: FnMut(&Path) -> std::io::Result<()>,
    R: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    let tmp = uf2.with_extension("uf2.tmp");
    let backup = uf2.with_extension("uf2.previous");
    let current_manifest = uf2.with_extension("uf2.json");
    let current_signature = current_manifest.with_extension("json.sig");
    let backup_manifest = backup.with_extension("previous.json");
    let backup_signature = backup_manifest.with_extension("json.sig");

    for path in [
        uf2,
        tmp.as_path(),
        backup.as_path(),
        current_manifest.as_path(),
        current_signature.as_path(),
        backup_manifest.as_path(),
        backup_signature.as_path(),
    ] {
        ensure_regular_or_absent(path)?;
    }
    // A previous sentinel is durable recovery state, not a replaceable cache.
    // Refuse before removing or renaming anything when it already exists.
    if backup.exists() || backup_manifest.exists() || backup_signature.exists() {
        return Err(format!(
            "refusing to overwrite existing recovery sentinel: {}",
            backup.display()
        ));
    }
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(&tmp, uf2_data).map_err(|e| format!("failed to write UF2 temp file: {e}"))?;
    let staged = std::fs::read(&tmp).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("failed to read staged UF2: {e}")
    })?;
    if staged != uf2_data || sensor_watch_core::uf2::validate(&staged).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err("staged UF2 failed content validation".into());
    }

    let had_old = uf2.is_file();
    if had_old {
        let mut moved = Vec::new();
        for (current, staged) in [
            (current_manifest.as_path(), backup_manifest.as_path()),
            (current_signature.as_path(), backup_signature.as_path()),
            (uf2, backup.as_path()),
        ] {
            if current.exists() {
                if let Err(e) = rename_file(current, staged) {
                    restore_staged_files(&mut moved);
                    let _ = std::fs::remove_file(&tmp);
                    return Err(format!("failed to stage existing UF2: {e}"));
                }
                moved.push((current.to_path_buf(), staged.to_path_buf()));
            }
        }
    }
    let mut retained_generation: Option<PathBuf> = None;
    let rollback = |retained_generation: &mut Option<PathBuf>| {
        if let Some(generation_uf2) = retained_generation.take() {
            let generation_manifest = generation_uf2.with_extension("uf2.json");
            let _ = std::fs::remove_file(&generation_uf2);
            let _ = std::fs::remove_file(&generation_manifest);
            let _ = std::fs::remove_file(generation_manifest.with_extension("json.sig"));
        }
        let _ = std::fs::remove_file(uf2);
        let _ = std::fs::remove_file(&current_manifest);
        let _ = std::fs::remove_file(&current_signature);
        if had_old {
            let _ = std::fs::rename(&backup, uf2);
            let _ = std::fs::rename(&backup_manifest, &current_manifest);
            let _ = std::fs::rename(&backup_signature, &current_signature);
        }
    };
    if let Err(e) = std::fs::rename(&tmp, uf2) {
        rollback(&mut retained_generation);
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("failed to replace UF2: {e}"));
    }
    let published = match std::fs::read(uf2) {
        Ok(data) => data,
        Err(e) => {
            rollback(&mut retained_generation);
            return Err(format!("UF2 published but could not be re-read: {e}"));
        }
    };
    if published != uf2_data || sensor_watch_core::uf2::validate(&published).is_err() {
        rollback(&mut retained_generation);
        return Err("published UF2 failed content validation".into());
    }

    if had_old {
        let recovery_dir = output_dir.join("recovery").join("generations");
        if let Err(e) = std::fs::create_dir_all(&recovery_dir) {
            rollback(&mut retained_generation);
            return Err(format!(
                "built UF2, but could not create recovery directory: {e}"
            ));
        }
        let old_data = match std::fs::read(&backup) {
            Ok(data) => data,
            Err(e) => {
                rollback(&mut retained_generation);
                return Err(format!(
                    "built UF2, but could not read previous backup: {e}"
                ));
            }
        };
        if let Err(e) = sensor_watch_core::uf2::validate(&old_data) {
            rollback(&mut retained_generation);
            return Err(format!("refusing to retain invalid previous UF2: {e}"));
        }
        let old_sha = hex_sha256(&old_data);
        let generation = format!("g{}-{}", unix_nanos(), &old_sha[..12]);
        let old_path = recovery_dir.join(format!("{generation}.uf2"));
        if let Err(e) = std::fs::copy(&backup, &old_path)
            .and_then(|_| write_manifest(&old_path, &old_data, &generation))
        {
            let old_manifest = old_path.with_extension("uf2.json");
            let _ = std::fs::remove_file(&old_path);
            let _ = std::fs::remove_file(&old_manifest);
            let _ = std::fs::remove_file(old_manifest.with_extension("json.sig"));
            rollback(&mut retained_generation);
            return Err(format!(
                "built UF2, but could not preserve previous generation: {e}"
            ));
        }
        retained_generation = Some(old_path);
    }
    let generation = format!("g{}-{}", unix_nanos(), &hex_sha256(uf2_data)[..12]);
    if let Err(e) = write_current_manifest(uf2, uf2_data, &generation) {
        rollback(&mut retained_generation);
        return Err(format!("UF2 published, but manifest write failed: {e}"));
    }
    if had_old {
        cleanup_recovery_files(
            [&backup, &backup_manifest, &backup_signature],
            &mut remove_recovery_file,
        )
        .map_err(|error| {
            format!(
                "UF2 and manifest published, but {error}; newly published artifact was preserved"
            )
        })?;
    }
    Ok(())
}

fn restore_staged_files(moved: &mut Vec<(PathBuf, PathBuf)>) {
    while let Some((original, staged)) = moved.pop() {
        let _ = std::fs::rename(staged, original);
    }
}

fn cleanup_recovery_files<C>(paths: [&Path; 3], remove_file: &mut C) -> Result<(), String>
where
    C: FnMut(&Path) -> std::io::Result<()>,
{
    let mut failures = Vec::new();
    for path in paths {
        if let Err(error) = remove_file(path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                failures.push(format!("{}: {error}", path.display()));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("recovery cleanup failed: {}", failures.join("; ")))
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

/// Writes the same locally consistent manifest used by sensor-watch-tools.
/// The resulting digest is not a cryptographic signature or authenticity claim.
fn write_manifest(uf2: &Path, data: &[u8], generation: &str) -> std::io::Result<()> {
    sensor_watch_core::uf2::validate(data).map_err(std::io::Error::other)?;
    let manifest_path = uf2.with_extension("uf2.json");
    let sidecar_path = manifest_path.with_extension("json.sig");
    for path in [&manifest_path, &sidecar_path] {
        ensure_regular_or_absent(path).map_err(std::io::Error::other)?;
        if path.exists() {
            std::fs::remove_file(path).map_err(std::io::Error::other)?;
        }
    }
    let manifest =
        sensor_watch_tools::create_manifest(uf2, Some(generation.to_string()), Some(uf2))
            .map_err(std::io::Error::other)?;
    sensor_watch_tools::write_manifest(&manifest_path, &manifest).map_err(std::io::Error::other)
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
    fn configuration_contract_names_all_required_input_classes() {
        let contract = missing_configuration_inputs();
        assert_eq!(contract, CONFIGURATION_INPUT_CONTRACT);
        assert!(contract.iter().any(|item| item.contains("preset")));
        assert!(contract.iter().any(|item| item.contains("board")));
        assert!(contract.iter().any(|item| item.contains("feature/module")));
        assert!(contract.iter().any(|item| item.contains("pin")));
        assert!(contract.iter().any(|item| item.contains("provenance")));
    }

    #[test]
    fn blocked_build_message_lists_every_required_preflight_input() {
        let message = validate_configuration_inputs().unwrap_err();

        assert!(message.starts_with("firmware build refused:"));
        for input in CONFIGURATION_INPUT_CONTRACT {
            assert!(message.contains(input), "blocked message omitted: {input}");
        }
    }

    #[test]
    fn configured_builds_are_rejected_before_side_effects() {
        assert_eq!(
            validate_configuration_inputs(),
            Err(CONFIGURATION_BUILD_BLOCKED)
        );

        let output = temp_root("blocked-output");
        let result = build_firmware(&output);
        assert!(!result.success);
        assert_eq!(result.message, CONFIGURATION_BUILD_BLOCKED);
        assert!(result.uf2_path.is_none());
        assert!(!output.exists());
    }

    #[test]
    fn successful_publication_retains_verified_prior_generation_and_cleans_sentinels() {
        let root = temp_root("successful-publication");
        let recovery_dir = root.join("recovery/generations");
        std::fs::create_dir_all(&root).unwrap();
        let uf2 = root.join("sensor-watch.uf2");
        let prior_data = sensor_watch_core::uf2::convert_to_uf2(&[10; 1024]);
        let replacement_data = sensor_watch_core::uf2::convert_to_uf2(&[11; 1024]);
        std::fs::write(&uf2, &prior_data).unwrap();
        write_manifest(&uf2, &prior_data, "prior-generation").unwrap();

        publish_uf2_with_cleanup_writer(
            &root,
            &uf2,
            &replacement_data,
            |path, data, generation| write_manifest(path, data, generation),
            |path| std::fs::remove_file(path),
        )
        .unwrap();

        let generations: Vec<PathBuf> = std::fs::read_dir(&recovery_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .map(|extension| extension == "uf2")
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(generations.len(), 1);
        let prior_path = &generations[0];
        let prior_manifest = prior_path.with_extension("uf2.json");
        let prior_signature = prior_manifest.with_extension("json.sig");
        let current_manifest = uf2.with_extension("uf2.json");
        let current_signature = current_manifest.with_extension("json.sig");

        for path in [
            prior_path.as_path(),
            prior_manifest.as_path(),
            prior_signature.as_path(),
        ] {
            assert!(
                path.is_file(),
                "missing retained artifact: {}",
                path.display()
            );
        }
        for path in [
            uf2.as_path(),
            current_manifest.as_path(),
            current_signature.as_path(),
        ] {
            assert!(
                path.is_file(),
                "missing current artifact: {}",
                path.display()
            );
        }
        for path in [
            uf2.with_extension("uf2.previous"),
            uf2.with_extension("uf2.previous.json"),
            uf2.with_extension("uf2.previous.json.sig"),
        ] {
            assert!(
                !path.exists(),
                "recovery sentinel remained: {}",
                path.display()
            );
        }

        let prior = inspect_artifact(prior_path).unwrap();
        let current = inspect_artifact(&uf2).unwrap();
        assert!(prior.generation.starts_with('g'));
        assert!(current.generation.starts_with('g'));
        assert_ne!(prior.generation, current.generation);
        assert_ne!(prior.sha256, current.sha256);
        assert_ne!(prior.manifest_digest, current.manifest_digest);
        assert_eq!(std::fs::read(prior_path).unwrap(), prior_data);
        assert_eq!(std::fs::read(&uf2).unwrap(), replacement_data);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_previous_sentinel_is_never_deleted_or_overwritten() {
        let root = temp_root("previous-sentinel");
        std::fs::create_dir_all(&root).unwrap();
        let uf2 = root.join("sensor-watch.uf2");
        let previous = root.join("sensor-watch.uf2.previous");
        let current = sensor_watch_core::uf2::convert_to_uf2(&[1; 1024]);
        let sentinel = b"known-good-sentinel";
        std::fs::write(&uf2, &current).unwrap();
        std::fs::write(&previous, sentinel).unwrap();

        let error = publish_uf2(&root, &uf2, &current).unwrap_err();

        assert!(error.contains("recovery sentinel"));
        assert_eq!(std::fs::read(&previous).unwrap(), sentinel);
        assert_eq!(std::fs::read(&uf2).unwrap(), current);
        assert!(!root.join("sensor-watch.uf2.tmp").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    fn assert_staging_failure_restores_current_files(label: &str, failed_step: usize) {
        let root = temp_root(label);
        std::fs::create_dir_all(&root).unwrap();
        let uf2 = root.join("sensor-watch.uf2");
        let manifest = uf2.with_extension("uf2.json");
        let signature = manifest.with_extension("json.sig");
        let old_data = sensor_watch_core::uf2::convert_to_uf2(&[8; 1024]);
        let new_data = sensor_watch_core::uf2::convert_to_uf2(&[9; 1024]);
        std::fs::write(&uf2, &old_data).unwrap();
        write_manifest(&uf2, &old_data, "old").unwrap();
        let old_manifest = std::fs::read(&manifest).unwrap();
        let old_signature = std::fs::read(&signature).unwrap();
        let backup_paths = [
            uf2.with_extension("uf2.previous"),
            uf2.with_extension("uf2.previous.json"),
            uf2.with_extension("uf2.previous.json.sig"),
        ];
        let failed_destination = backup_paths[failed_step].clone();

        let error = publish_uf2_with_writers(
            &root,
            &uf2,
            &new_data,
            |path, data, generation| write_manifest(path, data, generation),
            |path| std::fs::remove_file(path),
            move |from, to| {
                if to == failed_destination.as_path() {
                    Err(std::io::Error::other("deterministic staging failure"))
                } else {
                    std::fs::rename(from, to)
                }
            },
        )
        .unwrap_err();

        assert!(error.contains("failed to stage existing UF2"));
        assert_eq!(std::fs::read(&uf2).unwrap(), old_data);
        assert_eq!(std::fs::read(&manifest).unwrap(), old_manifest);
        assert_eq!(std::fs::read(&signature).unwrap(), old_signature);
        for path in backup_paths {
            assert!(
                !path.exists(),
                "staging artifact remained: {}",
                path.display()
            );
        }
        assert!(!uf2.with_extension("uf2.tmp").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_uf2_staging_restores_current_files_and_cleans_sentinels() {
        assert_staging_failure_restores_current_files("stage-uf2-failure", 0);
    }

    #[test]
    fn failed_manifest_staging_restores_current_files_and_cleans_sentinels() {
        assert_staging_failure_restores_current_files("stage-manifest-failure", 1);
    }

    #[test]
    fn failed_signature_staging_restores_current_files_and_cleans_sentinels() {
        assert_staging_failure_restores_current_files("stage-signature-failure", 2);
    }

    #[test]
    fn failed_recovery_staging_removes_new_artifact_and_restores_current_sidecars() {
        let root = temp_root("recovery-failure");
        std::fs::create_dir_all(root.join("recovery")).unwrap();
        std::fs::write(root.join("recovery/generations"), b"not a directory").unwrap();
        let uf2 = root.join("sensor-watch.uf2");
        let old_data = sensor_watch_core::uf2::convert_to_uf2(&[2; 1024]);
        let new_data = sensor_watch_core::uf2::convert_to_uf2(&[3; 1024]);
        std::fs::write(&uf2, &old_data).unwrap();
        let manifest =
            sensor_watch_tools::create_manifest(&uf2, Some("old".into()), Some(&uf2)).unwrap();
        sensor_watch_tools::write_manifest(&uf2.with_extension("uf2.json"), &manifest).unwrap();
        let old_manifest = std::fs::read(uf2.with_extension("uf2.json")).unwrap();
        let old_signature =
            std::fs::read(uf2.with_extension("uf2.json").with_extension("json.sig")).unwrap();

        let error = publish_uf2(&root, &uf2, &new_data).unwrap_err();

        assert!(error.contains("recovery directory"));
        assert_eq!(std::fs::read(&uf2).unwrap(), old_data);
        assert_eq!(
            std::fs::read(uf2.with_extension("uf2.json")).unwrap(),
            old_manifest
        );
        assert_eq!(
            std::fs::read(uf2.with_extension("uf2.json").with_extension("json.sig")).unwrap(),
            old_signature
        );
        assert!(!root.join("sensor-watch.uf2.previous").exists());
        assert!(!root.join("sensor-watch.uf2.tmp").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_current_manifest_cleanup_removes_new_generation_and_restores_transaction() {
        let root = temp_root("current-manifest-failure");
        let recovery_dir = root.join("recovery/generations");
        std::fs::create_dir_all(&recovery_dir).unwrap();
        let uf2 = root.join("sensor-watch.uf2");
        let old_data = sensor_watch_core::uf2::convert_to_uf2(&[4; 1024]);
        let new_data = sensor_watch_core::uf2::convert_to_uf2(&[5; 1024]);
        std::fs::write(&uf2, &old_data).unwrap();
        let manifest =
            sensor_watch_tools::create_manifest(&uf2, Some("old".into()), Some(&uf2)).unwrap();
        sensor_watch_tools::write_manifest(&uf2.with_extension("uf2.json"), &manifest).unwrap();
        let old_manifest = std::fs::read(uf2.with_extension("uf2.json")).unwrap();
        let old_signature =
            std::fs::read(uf2.with_extension("uf2.json").with_extension("json.sig")).unwrap();

        let error = publish_uf2_with_manifest_writer(
            &root,
            &uf2,
            &new_data,
            |_path, _data, _generation| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "deterministic manifest failure",
                ))
            },
        )
        .unwrap_err();

        assert!(error.contains("manifest write failed"));
        assert_eq!(std::fs::read(&uf2).unwrap(), old_data);
        assert_eq!(
            std::fs::read(uf2.with_extension("uf2.json")).unwrap(),
            old_manifest
        );
        assert_eq!(
            std::fs::read(uf2.with_extension("uf2.json").with_extension("json.sig")).unwrap(),
            old_signature
        );
        assert!(std::fs::read_dir(&recovery_dir).unwrap().next().is_none());
        assert!(!root.join("sensor-watch.uf2.previous").exists());
        assert!(!root.join("sensor-watch.uf2.previous.json").exists());
        assert!(!root.join("sensor-watch.uf2.previous.json.sig").exists());
        assert!(!root.join("sensor-watch.uf2.tmp").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_cleanup_failures_are_reported_without_rollback() {
        let root = temp_root("recovery-cleanup-failure");
        std::fs::create_dir_all(root.join("recovery/generations")).unwrap();
        let uf2 = root.join("sensor-watch.uf2");
        let old_data = sensor_watch_core::uf2::convert_to_uf2(&[6; 1024]);
        let new_data = sensor_watch_core::uf2::convert_to_uf2(&[7; 1024]);
        std::fs::write(&uf2, &old_data).unwrap();
        write_manifest(&uf2, &old_data, "old").unwrap();
        let backup_manifest = uf2.with_extension("uf2.previous.json");
        let backup_signature = backup_manifest.with_extension("json.sig");
        let attempted = std::cell::RefCell::new(Vec::new());

        let error = publish_uf2_with_cleanup_writer(
            &root,
            &uf2,
            &new_data,
            |path, data, generation| write_manifest(path, data, generation),
            |path| {
                attempted.borrow_mut().push(path.to_path_buf());
                if path == backup_manifest || path == backup_signature {
                    Err(std::io::Error::other("injected cleanup failure"))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();

        assert!(error.contains("recovery cleanup failed"));
        assert!(error.contains("sensor-watch.uf2.previous.json"));
        assert!(error.contains("sensor-watch.uf2.previous.json.sig"));
        assert!(error.contains("newly published artifact was preserved"));
        assert_eq!(attempted.borrow().len(), 3);
        assert_eq!(std::fs::read(&uf2).unwrap(), new_data);
        assert!(uf2.with_extension("uf2.json").is_file());
        assert!(backup_manifest.is_file());
        assert!(backup_signature.is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn valid_explicit_artifact_is_inspected_with_sidecars() {
        let root = temp_root("artifact-valid");
        std::fs::create_dir_all(&root).unwrap();
        let uf2 = root.join("recovery.uf2");
        let data = sensor_watch_core::uf2::convert_to_uf2(&[0; 1024]);
        std::fs::write(&uf2, &data).unwrap();
        let manifest =
            sensor_watch_tools::create_manifest(&uf2, Some("g-test".into()), Some(&uf2)).unwrap();
        sensor_watch_tools::write_manifest(&uf2.with_extension("uf2.json"), &manifest).unwrap();

        let inspection = inspect_artifact(&uf2).unwrap();
        assert_eq!(inspection.generation, "g-test");
        assert_eq!(inspection.family_id, "0x2C29472F");
        assert_eq!(inspection.path, uf2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_explicit_artifact_is_rejected() {
        let root = temp_root("artifact-invalid");
        std::fs::create_dir_all(&root).unwrap();
        let uf2 = root.join("tampered.uf2");
        std::fs::write(&uf2, [0u8; 512]).unwrap();

        let error = inspect_artifact(&uf2).unwrap_err();
        assert!(error.contains("UF2") || error.contains("manifest"));
        std::fs::remove_dir_all(root).unwrap();
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
/// exists. This is retained for explicit inspection/recovery, not startup
/// flash authorization.
#[allow(dead_code)]
pub fn last_uf2(output_dir: &Path) -> Option<PathBuf> {
    let p = output_dir.join("sensor-watch.uf2");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}
