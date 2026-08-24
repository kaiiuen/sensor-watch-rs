//! Firmware build logic.
//!
//! Invokes the firmware build (cargo + rust-objcopy) and converts the raw
//! binary to a `.uf2` file using the `sensor-watch-core` UF2 encoder. This is
//! the "assembler" part of Firmware Studio.
//!
use sha2::{Digest, Sha256};

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use super::firmware_inputs::{self, FirmwareInputRequest, GeneratedFirmwareInputs};

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
    // Once distribution discovery has run, use only the validated mutable
    // project. The bundled project is a read-only template.
    if crate::distribution::initialized() {
        return select_active_project(&crate::distribution::active())
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

fn select_active_project(status: &crate::distribution::PackageStatus) -> Option<PathBuf> {
    select_project_for_build(
        status.mode,
        status.active_project_dir()?.as_path(),
        status.firmware_project_dir().as_deref(),
        &status.user_data_root,
    )
}

fn select_project_for_build(
    mode: crate::distribution::DistributionMode,
    active: &Path,
    bundled: Option<&Path>,
    data_root: &Path,
) -> Option<PathBuf> {
    let project = active.canonicalize().ok()?;
    if !is_workspace_root(&project) {
        return None;
    }
    if mode == crate::distribution::DistributionMode::Packaged {
        let data_root = data_root.canonicalize().ok()?;
        if project == data_root || !project.starts_with(&data_root) {
            return None;
        }
        if bundled
            .and_then(|path| path.canonicalize().ok())
            .is_some_and(|template| template == project)
        {
            return None;
        }
    }
    Some(project)
}

/// The embedded target triple.
pub const TARGET: &str = "thumbv6m-none-eabi";

const MAX_DIAGNOSTIC_BYTES: usize = 64 * 1024;
const MAX_DIAGNOSTIC_LINES: usize = 400;

#[derive(Debug, PartialEq, Eq)]
struct CapturedCommand {
    code: Option<i32>,
    diagnostics: String,
}

#[cfg(test)]
fn capture_command(command: Command, redacted_roots: &[&Path]) -> Result<CapturedCommand, String> {
    capture_command_with_progress(
        command,
        redacted_roots,
        &crate::progress::ProgressSink::disabled(),
        crate::progress::Phase::Cargo,
    )
}

fn capture_command_with_progress(
    mut command: Command,
    redacted_roots: &[&Path],
    progress: &crate::progress::ProgressSink,
    phase: crate::progress::Phase,
) -> Result<CapturedCommand, String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start command: {error}"))?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let (line_sender, line_receiver) = std::sync::mpsc::channel();
    let stdout_thread = thread::spawn({
        let sender = line_sender.clone();
        move || read_lines_to_channel(stdout, "stdout", sender)
    });
    let stderr_thread = thread::spawn({
        let sender = line_sender;
        move || read_lines_to_channel(stderr, "stderr", sender)
    });
    let mut output = Vec::new();
    while let Ok((stream, line)) = line_receiver.recv() {
        let safe_line = sanitize_diagnostics(&format!("{stream}: {line}"), redacted_roots);
        progress.emit(phase, safe_line.clone(), None, None);
        output.push(safe_line);
    }
    let status = child
        .wait()
        .map_err(|error| format!("failed waiting for command: {error}"))?;
    stdout_thread
        .join()
        .map_err(|_| "stdout capture thread panicked".to_string())??;
    stderr_thread
        .join()
        .map_err(|_| "stderr capture thread panicked".to_string())??;
    let text = output.join("\n");
    Ok(CapturedCommand {
        code: status.code(),
        diagnostics: sanitize_diagnostics(&text, redacted_roots),
    })
}

fn read_lines_to_channel(
    reader: impl Read,
    stream: &'static str,
    sender: std::sync::mpsc::Sender<(&'static str, String)>,
) -> Result<(), String> {
    let mut bytes = 0usize;
    let mut lines = 0usize;
    let mut truncated = false;
    for line in BufReader::new(reader).lines() {
        let line = line.map_err(|error| error.to_string())?;
        if lines >= MAX_DIAGNOSTIC_LINES || bytes >= MAX_DIAGNOSTIC_BYTES {
            truncated = true;
            continue;
        }
        let clean = line
            .chars()
            .filter(|c| c.is_ascii() && !c.is_control())
            .collect::<String>();
        bytes = bytes.saturating_add(clean.len() + 1);
        lines += 1;
        sender
            .send((stream, clean))
            .map_err(|_| "output receiver closed".to_string())?;
    }
    if truncated {
        let _ = sender.send((
            stream,
            "[diagnostics truncated by line or byte limit]".into(),
        ));
    }
    Ok(())
}

fn sanitize_diagnostics(text: &str, redacted_roots: &[&Path]) -> String {
    let mut output = String::new();
    let mut lines = 0;
    for original in text.lines() {
        if lines == MAX_DIAGNOSTIC_LINES {
            output.push_str("[diagnostics truncated by line limit]\n");
            break;
        }
        let mut line = original.to_string();
        for root in redacted_roots {
            let path = root.to_string_lossy();
            if !path.is_empty() {
                line = line.replace(path.as_ref(), "<path>");
            }
        }
        let lower = line.to_ascii_lowercase();
        if ["password=", "token=", "secret=", "api_key=", "apikey="]
            .iter()
            .any(|marker| lower.contains(marker))
        {
            line = "[redacted diagnostic secret]".into();
        }
        let remaining = MAX_DIAGNOSTIC_BYTES.saturating_sub(output.len());
        if remaining == 0 {
            output.push_str("[diagnostics truncated by byte limit]\n");
            break;
        }
        let line_bytes = line.as_bytes();
        let take = line_bytes.len().min(remaining);
        output.push_str(&String::from_utf8_lossy(&line_bytes[..take]));
        output.push('\n');
        lines += 1;
        if take < line_bytes.len() {
            output.push_str("[diagnostics truncated by byte limit]\n");
            break;
        }
    }
    let mut output = output.trim_end().to_string();
    if output.len() > MAX_DIAGNOSTIC_BYTES {
        let mut end = MAX_DIAGNOSTIC_BYTES;
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        output.truncate(end);
    }
    output
}

fn write_build_log(output_dir: &Path, diagnostics: &str) {
    if diagnostics.is_empty()
        || validate_output_dir(output_dir).is_err()
        || std::fs::symlink_metadata(output_dir.join("build.log")).map_or(false, |metadata| {
            metadata.file_type().is_symlink() || !metadata.is_file()
        })
    {
        return;
    }
    let path = output_dir.join("build.log");
    let temporary = output_dir.join("build.log.tmp");
    if std::fs::write(&temporary, diagnostics).is_ok() {
        let _ = std::fs::rename(temporary, path);
    }
}

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

struct BuildTraceGuard<'a> {
    progress: &'a crate::progress::ProgressSink,
    success: bool,
}
impl Drop for BuildTraceGuard<'_> {
    fn drop(&mut self) {
        self.progress.emit(
            crate::progress::Phase::Cleanup,
            "Cleaning isolated workspace and temporary build state",
            None,
            None,
        );
        self.progress.finish(
            self.success,
            if self.success {
                "Build complete"
            } else {
                "Build failed"
            },
        );
    }
}
fn hex_digest(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
    pub generated_input_digest: String,
}

/// Inspects an explicitly selected UF2 and requires both manifest sidecars.
/// Passing the manifest path explicitly makes a missing `.uf2.json` fail closed;
/// the shared verifier then requires the matching `.json.sig` sidecar as well.
pub fn inspect_artifact(path: &Path) -> Result<ArtifactInspection, String> {
    let manifest_path = path.with_extension("uf2.json");
    // Validate UF2 bytes with the shared verifier, then validate the manifest
    // locally. Configured manifests include generated-input provenance in the
    // same canonical digest used by the shared verifier.
    sensor_watch_core::uf2::validate(
        &std::fs::read(path).map_err(|e| format!("cannot read UF2 {}: {e}", path.display()))?,
    )
    .map_err(|e| format!("invalid UF2: {e}"))?;
    let manifest: sensor_watch_tools::Manifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|e| format!("cannot read manifest {}: {e}", manifest_path.display()))?,
    )
    .map_err(|e| format!("cannot parse manifest: {e}"))?;
    let generation = sensor_watch_tools::manifest_value(&manifest, "generation_id");
    let mut baseline = sensor_watch_tools::create_manifest(
        path,
        (!generation.is_empty()).then(|| generation.clone()),
        None,
    )?;
    if let Some(value) = manifest.get("generated_input_digest") {
        let expected = value
            .as_str()
            .ok_or_else(|| "manifest generated-input digest is not a string".to_string())?;
        baseline.insert("generated_input_digest".into(), expected.into());
        let baseline_digest = sensor_watch_tools::manifest_digest(&baseline);
        baseline.insert("manifest_digest".into(), baseline_digest.clone().into());
        baseline.insert("signature".into(), baseline_digest.into());
        validate_generated_input_files(path, expected)?;
    }
    let digest = sensor_watch_tools::manifest_value(&manifest, "manifest_digest");
    if digest.is_empty() || digest != sensor_watch_tools::manifest_digest(&manifest) {
        return Err("manifest local digest is invalid".into());
    }
    if digest != sensor_watch_tools::manifest_digest(&baseline) {
        return Err("manifest local digest mismatch".into());
    }
    let sidecar = manifest_path.with_extension("json.sig");
    if std::fs::read_to_string(&sidecar)
        .map_err(|e| format!("cannot read manifest sidecar: {e}"))?
        .trim()
        != digest
    {
        return Err("manifest digest sidecar is invalid".into());
    }
    for key in [
        "format",
        "generation_id",
        "family_id",
        "uf2_bytes",
        "uf2_blocks",
        "payload_bytes",
        "sha256",
        "payload_sha256",
    ] {
        if manifest.get(key) != baseline.get(key) {
            return Err(format!("manifest mismatch for {key}"));
        }
    }
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
        generated_input_digest: sensor_watch_tools::manifest_value(
            &manifest,
            "generated_input_digest",
        ),
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

/// Plain-language explanations for the preflight panel. These deliberately say
/// what the current UI records and what it cannot generate, rather than implying
/// that a beginner can satisfy the contract by checking every option.
/// Status categories shown beside the five contract items. UI selections remain
/// planning state and never satisfy firmware-input or provenance requirements.
pub const CONFIGURATION_INPUT_STATUS: &[(&str, &str, &str)] = &[
    ("Supported and generated", "Preset and faces", "The selected preset, ordered faces, and source files are validated and copied into an isolated firmware workspace."),
    ("Supported and generated", "Target board and profile", "The matching stock board revision and profile are validated before the firmware build starts."),
    ("Supported and generated", "Component-to-firmware feature/module selections", "Stock profile selections are validated and emitted as generated firmware inputs; edited or unknown selections remain blocked."),
    ("Supported and generated", "Concrete hardware mappings", "Only documented stock mappings are emitted. Unknown mappings, unsupported buses, and invalid components fail closed."),
    ("Supported and generated", "Generated-input provenance", "The generated-input digest and provenance are written into the artifact manifest and required for configured-artifact approval."),
];

pub const CONFIGURATION_INPUT_EXPLANATIONS: &[(&str, &str)] = &[
    (
        "Preset and faces",
        "Supported stock presets are validated with their ordered face sources and copied into the isolated build workspace; missing or unsafe sources fail closed.",
    ),
    (
        "Target board and profile",
        "The four documented stock board/revision/profile combinations generate board-specific inputs. Custom, OSO, unknown, and mismatched revisions remain unsupported.",
    ),
    (
        "Component-to-firmware feature/module selections",
        "Stock component selections are emitted and validated by the generator. Edited profiles, unknown modules, invalid components, and unsupported Lite I2C/SPI are rejected rather than silently producing stock firmware.",
    ),
    (
        "Concrete hardware mappings",
        "Only documented stock pin, bus, address, power, and ownership mappings are emitted. Enabling an unsupported bus or selecting an unknown component does not infer a mapping.",
    ),
    (
        "Generated-input provenance",
        "Every generated input set is content-addressed for the firmware build. Its digest and provenance are written into the UF2 manifest, and approval is invalid unless that digest is present and unchanged.",
    ),
];

/// Validates a concrete request without creating an output artifact or running
/// the firmware toolchain. This is the UI preflight and remains fail closed.
pub fn preflight_request(request: &FirmwareInputRequest) -> Result<(), String> {
    firmware_inputs::validate_request_and_sources(request, &firmware_dir())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Validates and creates the selected root before a configured build worker is
/// started. This keeps unavailable drives, shares, and ancestor paths out of
/// the Cargo worker and gives the UI the exact attempted layout.
pub fn preflight_output_root(
    root: &Path,
    request: &FirmwareInputRequest,
    package_root: Option<&Path>,
    allowed_root: Option<&Path>,
) -> Result<crate::storage::BuildOutputPaths, String> {
    crate::storage::prepare_artifact_root(
        root,
        artifact_board_name(request.board),
        &request.revision,
        artifact_profile_name(request.board, &request.profile.name),
        package_root,
        allowed_root,
    )
}

fn artifact_board_name(board: crate::components::BoardKind) -> &'static str {
    // Board labels are presentation text; artifact directories use one safe,
    // stable component even for the Red / Lite display label.
    match board {
        crate::components::BoardKind::RedLite => "Red-Lite",
        crate::components::BoardKind::Green => "Green",
        crate::components::BoardKind::Blue => "Blue",
        crate::components::BoardKind::Pro => "Pro",
    }
}

fn artifact_profile_name(board: crate::components::BoardKind, profile: &str) -> &str {
    if profile == board.label() {
        artifact_board_name(board)
    } else {
        profile
    }
}

/// The exact inputs currently missing from the Studio-to-firmware build path.
/// This is also used by the profile UI so the disabled state cannot drift from
/// the build preflight.
pub fn missing_configuration_inputs() -> &'static [&'static str] {
    CONFIGURATION_INPUT_CONTRACT
}

/// Runs the full firmware build: cargo build, extract the raw binary, and
/// convert it to a `.uf2` file in the given output directory.
pub fn build_firmware(
    request: FirmwareInputRequest,
    paths: &crate::storage::BuildOutputPaths,
) -> BuildResult {
    build_firmware_with_progress(request, paths, &crate::progress::ProgressSink::disabled())
}

pub fn build_firmware_with_progress(
    request: FirmwareInputRequest,
    paths: &crate::storage::BuildOutputPaths,
    progress: &crate::progress::ProgressSink,
) -> BuildResult {
    let mut trace_guard = BuildTraceGuard {
        progress,
        success: false,
    };
    progress.emit(
        crate::progress::Phase::Preflight,
        "Build preflight started",
        None,
        None,
    );
    let output_dir = &paths.latest;
    if let Err(error) = validate_output_dir(output_dir) {
        return BuildResult {
            success: false,
            message: error,
            uf2_path: None,
        };
    }
    progress.emit(
        crate::progress::Phase::OutputRoot,
        format!("Resolved output root: {}", output_dir.display()),
        None,
        None,
    );
    let source_root = firmware_dir();
    progress.emit(
        crate::progress::Phase::SourceSnapshot,
        format!("Source snapshot root: {}", source_root.display()),
        None,
        None,
    );
    let workspace = match IsolatedWorkspace::new(&source_root) {
        Ok(workspace) => workspace,
        Err(error) => {
            return BuildResult {
                success: false,
                message: error,
                uf2_path: None,
            };
        }
    };
    progress.emit(
        crate::progress::Phase::Workspace,
        format!("Isolated workspace ready: {}", workspace.root.display()),
        None,
        None,
    );
    let inputs_dir = workspace.root.join("studio-generated");
    let generated = match generate_inputs(&request, &source_root, &inputs_dir, &workspace.root) {
        Ok(generated) => generated,
        Err(error) => {
            return BuildResult {
                success: false,
                message: error,
                uf2_path: None,
            };
        }
    };
    progress.emit(
        crate::progress::Phase::GeneratedInputs,
        format!("Generated-input digest: {}", generated.digest),
        None,
        None,
    );
    progress.emit(
        crate::progress::Phase::GeneratedInputs,
        format!("Generated-input files and digest: {}", generated.digest),
        None,
        None,
    );
    if let Err(error) = validate_output_dir(output_dir) {
        return BuildResult {
            success: false,
            message: error,
            uf2_path: None,
        };
    }
    let fw_dir = &workspace.root;
    progress.emit(
        crate::progress::Phase::Lock,
        "Acquiring Cargo build lock",
        None,
        None,
    );
    let _build_lock = match acquire_build_lock(fw_dir) {
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

    progress.emit(
        crate::progress::Phase::Cargo,
        "Cargo build started",
        None,
        None,
    );
    // 1. Build the firmware in release mode. Keep the worker independent from
    // the UI while retaining bounded, safe diagnostics for the result.
    let cargo_started = std::time::Instant::now();
    let cargo = capture_command_with_progress(
        {
            let mut command = Command::new("cargo");
            command
                .arg("build")
                .arg("--release")
                .arg("--message-format=short")
                .arg("--package")
                .arg("sensor-watch")
                .arg("--bin")
                .arg("sensor-watch")
                .arg("--target")
                .arg(TARGET)
                .current_dir(fw_dir);
            command
        },
        &[fw_dir, output_dir, &source_root],
        progress,
        crate::progress::Phase::Cargo,
    );
    let cargo = match cargo {
        Ok(result) => result,
        Err(error) => {
            let message = format!("Cargo could not start: {error}");
            write_build_log(output_dir, &message);
            return BuildResult {
                success: false,
                message,
                uf2_path: None,
            };
        }
    };
    progress.emit(
        crate::progress::Phase::Cargo,
        format!(
            "Cargo exited {:?} after {} ms",
            cargo.code,
            cargo_started.elapsed().as_millis()
        ),
        None,
        None,
    );
    if !cargo.diagnostics.is_empty() {
        write_build_log(output_dir, &cargo.diagnostics);
    }
    if cargo.code != Some(0) {
        let details = if cargo.diagnostics.is_empty() {
            "no diagnostic output captured".to_string()
        } else {
            cargo.diagnostics.clone()
        };
        let message = format!(
            "Cargo build failed with exit code {:?}\n{details}",
            cargo.code
        );
        write_build_log(output_dir, &message);
        return BuildResult {
            success: false,
            message,
            uf2_path: None,
        };
    }

    progress.emit(
        crate::progress::Phase::Elf,
        "ELF discovery succeeded",
        None,
        None,
    );
    // 2. Locate the ELF and the raw binary.
    let elf = fw_dir.join(format!("target/{TARGET}/release/sensor-watch"));
    let bin = fw_dir.join(format!("target/{TARGET}/release/sensor-watch.bin"));
    let uf2 = output_dir.join("sensor-watch.uf2");

    progress.emit(
        crate::progress::Phase::PanicMap,
        "Writing panic map",
        None,
        None,
    );
    // Keep the ELF, source tree, and panic resolver tied to this exact build.
    // The manifest is host-side only and does not change firmware behavior.
    if let Err(error) = crate::panic_map::write_manifest(&elf, fw_dir) {
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
    progress.emit(
        crate::progress::Phase::Objcopy,
        "rust-objcopy started",
        None,
        None,
    );
    let objcopy_started = std::time::Instant::now();
    let objcopy_result = capture_command_with_progress(
        {
            let mut command = Command::new(&objcopy);
            command
                .arg("-O")
                .arg("binary")
                .arg(&elf)
                .arg(&bin)
                .current_dir(fw_dir);
            command
        },
        &[fw_dir, output_dir, &source_root],
        progress,
        crate::progress::Phase::Objcopy,
    );
    let objcopy_result = match objcopy_result {
        Ok(result) => result,
        Err(error) => {
            let message = format!("rust-objcopy could not start: {error}");
            write_build_log(output_dir, &message);
            return BuildResult {
                success: false,
                message,
                uf2_path: None,
            };
        }
    };
    progress.emit(
        crate::progress::Phase::Objcopy,
        format!(
            "rust-objcopy exited {:?} after {} ms",
            objcopy_result.code,
            objcopy_started.elapsed().as_millis()
        ),
        None,
        None,
    );
    if !objcopy_result.diagnostics.is_empty() {
        write_build_log(output_dir, &objcopy_result.diagnostics);
    }
    if objcopy_result.code != Some(0) {
        let details = if objcopy_result.diagnostics.is_empty() {
            "no diagnostic output captured".to_string()
        } else {
            objcopy_result.diagnostics.clone()
        };
        let message = format!(
            "rust-objcopy failed with exit code {:?}\n{details}",
            objcopy_result.code
        );
        write_build_log(output_dir, &message);
        return BuildResult {
            success: false,
            message,
            uf2_path: None,
        };
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
    progress.emit(
        crate::progress::Phase::Binary,
        format!(
            "Binary size {} bytes, SHA-256 {}",
            image.len(),
            hex_digest(&image)
        ),
        Some(image.len() as u64),
        None,
    );
    progress.emit(
        crate::progress::Phase::Uf2,
        "UF2 conversion and validation started",
        None,
        None,
    );
    let uf2_data = sensor_watch_core::uf2::convert_to_uf2(&image);
    if let Err(error) = sensor_watch_core::uf2::validate(&uf2_data) {
        return BuildResult {
            success: false,
            message: format!("generated UF2 failed validation: {error}"),
            uf2_path: None,
        };
    }

    progress.emit(
        crate::progress::Phase::Provenance,
        "Publishing generated-input provenance",
        None,
        None,
    );
    let input_bundle = uf2.with_extension("uf2.inputs");
    if let Err(error) = copy_generated_inputs(&generated.directory, &input_bundle) {
        return BuildResult {
            success: false,
            message: format!("failed to publish generated-input provenance: {error}"),
            uf2_path: None,
        };
    }
    progress.emit(
        crate::progress::Phase::Publication,
        "Publishing UF2 atomically",
        None,
        None,
    );
    match publish_uf2_with_writers_at(
        output_dir,
        &uf2,
        &uf2_data,
        |path, data, generation| {
            write_manifest_with_input_digest(path, data, generation, &generated.digest)
        },
        |path| std::fs::remove_file(path),
        Some(&paths.recovery_generations),
        |from, to| std::fs::rename(from, to),
    ) {
        Ok(()) => {
            let paths = paths.clone();
            if paths.latest != *output_dir {
                return BuildResult {
                    success: false,
                    message: "resolved build output paths changed before publication".into(),
                    uf2_path: None,
                };
            }

            progress.emit(
                crate::progress::Phase::Metadata,
                "Writing latest and recovery metadata",
                None,
                None,
            );
            if let Err(error) = crate::storage::write_latest_atomic(
                &paths,
                &crate::storage::LatestMetadata {
                    format: "sensor-watch-latest-v1".into(),
                    board: artifact_board_name(request.board).into(),
                    revision: request.revision.clone(),
                    profile: artifact_profile_name(request.board, &request.profile.name).into(),
                    generated_input_digest: generated.digest.clone(),
                    artifact: uf2
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("sensor-watch.uf2")
                        .to_string(),
                },
            ) {
                return BuildResult {
                    success: false,
                    message: error,
                    uf2_path: None,
                };
            }
            progress.emit(
                crate::progress::Phase::Approval,
                "Artifact ready for explicit approval",
                None,
                None,
            );
            trace_guard.success = true;
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
        Err(message) => BuildResult {
            success: false,
            message,
            uf2_path: None,
        },
    }
}

fn publish_uf2(output_dir: &Path, uf2: &Path, uf2_data: &[u8]) -> Result<(), String> {
    publish_uf2_with_input_digest(output_dir, uf2, uf2_data, "")
}

fn publish_uf2_with_input_digest(
    output_dir: &Path,
    uf2: &Path,
    uf2_data: &[u8],
    input_digest: &str,
) -> Result<(), String> {
    publish_uf2_with_manifest_writer(output_dir, uf2, uf2_data, |path, data, generation| {
        write_manifest_with_input_digest(path, data, generation, input_digest)
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
    write_current_manifest: F,
    remove_recovery_file: C,
    rename_file: R,
) -> Result<(), String>
where
    F: FnMut(&Path, &[u8], &str) -> std::io::Result<()>,
    C: FnMut(&Path) -> std::io::Result<()>,
    R: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    publish_uf2_with_writers_at(
        output_dir,
        uf2,
        uf2_data,
        write_current_manifest,
        remove_recovery_file,
        None,
        rename_file,
    )
}

fn publish_uf2_with_writers_at<F, C, R>(
    output_dir: &Path,
    uf2: &Path,
    uf2_data: &[u8],
    mut write_current_manifest: F,
    mut remove_recovery_file: C,
    configured_recovery_dir: Option<&Path>,
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
        let recovery_dir = configured_recovery_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(|| output_dir.join("recovery").join("generations"));
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
                "UF2 and manifest published, but {error}. Newly published artifact was preserved"
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
        Err(format!("recovery cleanup failed: {}", failures.join(", ")))
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

struct IsolatedWorkspace {
    root: PathBuf,
}

impl IsolatedWorkspace {
    fn new(source: &Path) -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!("sensor-watch-studio-build-{}", unix_nanos()));
        copy_tree(source, &root, source.join("target"), true)?;
        write_isolated_manifest(source, &root)?;
        Ok(Self { root })
    }
}

impl Drop for IsolatedWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn copy_tree(
    source: &Path,
    destination: &Path,
    excluded: PathBuf,
    firmware_root: bool,
) -> Result<(), String> {
    std::fs::create_dir_all(destination)
        .map_err(|e| format!("cannot create isolated workspace: {e}"))?;
    for entry in
        std::fs::read_dir(source).map_err(|e| format!("cannot read firmware source: {e}"))?
    {
        let entry = entry.map_err(|e| format!("cannot read firmware source entry: {e}"))?;
        let from = entry.path();
        if from == excluded
            || from.file_name().is_some_and(|name| name == ".git")
            // These are host applications/tools or unrelated workspace members.
            // The isolated build needs only the firmware package and its `core`
            // path dependency; in particular, never copy the desktop Studio crate.
            || (firmware_root
                && from.file_name().is_some_and(|name| {
                    matches!(
                        name.to_str(),
                        Some("studio" | "tools" | "desktop-update" | "launcher")
                    )
                }))
        {
            continue;
        }
        let to = destination.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&from)
            .map_err(|e| format!("cannot inspect firmware source: {e}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "refusing symlinked firmware source: {}",
                from.display()
            ));
        }
        if metadata.is_dir() {
            copy_tree(&from, &to, excluded.clone(), false)?;
        } else if metadata.is_file() {
            std::fs::copy(&from, &to)
                .map_err(|e| format!("cannot copy firmware source {}: {e}", from.display()))?;
        }
    }
    Ok(())
}

fn write_isolated_manifest(source: &Path, workspace: &Path) -> Result<(), String> {
    let original = std::fs::read_to_string(source.join("Cargo.toml"))
        .map_err(|e| format!("cannot read firmware manifest: {e}"))?;
    let package = original
        .find("[package]")
        .ok_or_else(|| "firmware manifest lacks [package]".to_string())?;
    let profiles = original.find("[profile.dev]").unwrap_or(original.len());
    let mut manifest = String::from(
        "[workspace]\nresolver = \"2\"\nmembers = [\".\", \"core\"]\ndefault-members = [\".\"]\n\n",
    );
    manifest.push_str(&original[package..profiles]);
    if profiles < original.len() {
        let profile_text = &original[profiles..];
        if let Some(package_profile) =
            profile_text.find("[profile.release.package.sensor-watch-studio]")
        {
            manifest.push_str(&profile_text[..package_profile]);
        } else {
            manifest.push_str(profile_text);
        }
    }
    std::fs::write(workspace.join("Cargo.toml"), manifest)
        .map_err(|e| format!("cannot write isolated firmware manifest: {e}"))
}

fn generate_inputs(
    request: &FirmwareInputRequest,
    source_root: &Path,
    inputs_dir: &Path,
    workspace: &Path,
) -> Result<GeneratedFirmwareInputs, String> {
    // This is the final authority: preflight may have observed an earlier source
    // state, so revalidate and retain the exact bytes used by this build.
    let validated = firmware_inputs::validate_request_and_sources(request, source_root)
        .map_err(|e| e.to_string())?;
    let generated =
        firmware_inputs::generate(request, &validated, inputs_dir).map_err(|e| e.to_string())?;
    let cargo_config = workspace.join(".cargo");
    std::fs::create_dir_all(&cargo_config)
        .map_err(|e| format!("cannot create generated Cargo overlay: {e}"))?;
    let config_path = cargo_config.join("config.toml");
    let original = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("cannot read repository Cargo config: {e}"))?;
    let generated_path = inputs_dir.join("firmware_inputs.rs");
    // Preserve the repository's linker/flip-link/link.x configuration. The
    // generated layer is additive and is exposed through Cargo's environment
    // mechanism, rather than replacing target rustflags.
    let overlay = format!(
        "\n[env]\nSENSOR_WATCH_STUDIO_INPUTS = {{ value = {:?}, relative = true }}\nSENSOR_WATCH_STUDIO_INPUT_DIGEST = {:?}\n",
        generated_path.to_string_lossy(),
        generated.digest
    );
    std::fs::write(&config_path, format!("{original}{overlay}"))
        .map_err(|e| format!("cannot install generated Cargo input layer: {e}"))?;
    // Make the generated module part of the actual firmware crate. This edits
    // only the disposable isolated copy: the repository firmware source stays
    // untouched. The marker reference prevents an unused, write-only module.
    let main_path = workspace.join("src/main.rs");
    let original_main = std::fs::read_to_string(&main_path)
        .map_err(|e| format!("cannot read isolated firmware entry: {e}"))?;
    let generated_path = inputs_dir.join("firmware_inputs.rs");
    let generated_main = insert_generated_module(&original_main, &generated_path)?;
    std::fs::write(&main_path, generated_main)
        .map_err(|e| format!("cannot install generated firmware module: {e}"))?;

    // Copy the exact bytes validated above; the manifest and compiler now refer
    // to the same source contents.
    for (source_name, contents) in validated.source_contents {
        let destination = workspace.join("src/movement").join(source_name);
        std::fs::write(&destination, contents)
            .map_err(|e| format!("cannot overlay face source {}: {e}", destination.display()))?;
    }
    Ok(generated)
}

/// Inserts generated items after the copied crate-level docs and attributes.
///
/// Inner docs and attributes must precede every ordinary item in a Rust crate;
/// putting the generated module before them changes their meaning or causes a
/// parser error. Keep the original prefix byte-for-byte and only insert after
/// the leading crate syntax.
fn insert_generated_module(original: &str, generated_path: &Path) -> Result<String, String> {
    let mut prefix_end = 0;
    let mut attribute_brackets = 0usize;

    for line in original.split_inclusive('\n') {
        let trimmed = line.trim();
        if attribute_brackets > 0 {
            attribute_brackets = attribute_brackets
                .saturating_add(line.chars().filter(|&character| character == '[').count())
                .saturating_sub(line.chars().filter(|&character| character == ']').count());
            prefix_end += line.len();
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
            prefix_end += line.len();
            continue;
        }
        if trimmed.starts_with("#![") {
            attribute_brackets = line
                .chars()
                .filter(|&character| character == '[')
                .count()
                .saturating_sub(line.chars().filter(|&character| character == ']').count());
            prefix_end += line.len();
            continue;
        }
        break;
    }

    if attribute_brackets != 0 {
        return Err("unterminated crate-level attribute in firmware entry".into());
    }

    let generated = format!(
        "mod studio_generated {{ include!(r#\"{}\"#); }}\nconst _: &str = studio_generated::GENERATED_MARKER;\n",
        generated_path.display()
    );
    Ok(format!(
        "{}{}{}",
        &original[..prefix_end],
        generated,
        &original[prefix_end..]
    ))
}

/// Writes the same locally consistent manifest used by sensor-watch-tools.
/// The resulting digest is not a cryptographic signature or authenticity claim.
fn copy_generated_inputs(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        std::fs::remove_dir_all(destination)
            .map_err(|e| format!("cannot replace generated-input bundle: {e}"))?;
    }
    std::fs::create_dir_all(destination)
        .map_err(|e| format!("cannot create generated-input bundle: {e}"))?;
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "unsafe generated-input entry: {}",
                entry.path().display()
            ));
        }
        std::fs::copy(entry.path(), destination.join(entry.file_name()))
            .map_err(|e| format!("cannot copy generated input: {e}"))?;
    }
    Ok(())
}

/// Recomputes the generated-input digest from the final published bundle.
pub fn validate_generated_input_digest(inspection: &ArtifactInspection) -> Result<(), String> {
    validate_generated_input_files(&inspection.path, &inspection.generated_input_digest)
}

fn validate_generated_input_files(path: &Path, expected: &str) -> Result<(), String> {
    if expected.is_empty() {
        return Err("configured artifact lacks generated-input provenance".into());
    }
    let directory = path.with_extension("uf2.inputs");
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(&directory)
        .map_err(|e| format!("generated-input provenance is unavailable: {e}"))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "SHA256" {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("generated-input provenance contains an unsafe entry".into());
        }
        files.insert(
            name,
            std::fs::read_to_string(entry.path()).map_err(|e| e.to_string())?,
        );
    }
    let actual = super::firmware_inputs::digest_generated_files(&files);
    if actual != expected {
        return Err(format!(
            "generated-input digest changed (manifest {expected}, files {actual})"
        ));
    }
    Ok(())
}

fn write_manifest(uf2: &Path, data: &[u8], generation: &str) -> std::io::Result<()> {
    write_manifest_with_input_digest(uf2, data, generation, "")
}

fn write_manifest_with_input_digest(
    uf2: &Path,
    data: &[u8],
    generation: &str,
    input_digest: &str,
) -> std::io::Result<()> {
    sensor_watch_core::uf2::validate(data).map_err(std::io::Error::other)?;
    let manifest_path = uf2.with_extension("uf2.json");
    let sidecar_path = manifest_path.with_extension("json.sig");
    for path in [&manifest_path, &sidecar_path] {
        ensure_regular_or_absent(path).map_err(std::io::Error::other)?;
        if path.exists() {
            std::fs::remove_file(path).map_err(std::io::Error::other)?;
        }
    }
    let mut manifest =
        sensor_watch_tools::create_manifest(uf2, Some(generation.to_string()), Some(uf2))
            .map_err(std::io::Error::other)?;
    if !input_digest.is_empty() {
        manifest.insert("generated_input_digest".into(), input_digest.into());
        let digest = sensor_watch_tools::manifest_digest(&manifest);
        manifest.insert("manifest_digest".into(), digest.clone().into());
        manifest.insert("signature".into(), digest.into());
    }
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
    fn generated_module_follows_crate_docs_and_attributes() {
        let fixture = concat!(
            "//! fixture crate docs\n",
            "//! remain first\n",
            "\n",
            "#![no_std]\n",
            "#![cfg_attr(test, allow(dead_code))]\n",
            "\n",
            "pub fn firmware_item() {}\n",
        );
        let generated =
            insert_generated_module(fixture, Path::new("studio-generated/firmware_inputs.rs"))
                .unwrap();
        let generated_offset = generated
            .find("mod studio_generated")
            .expect("generated module was inserted");
        let normal_item_offset = generated
            .find("pub fn firmware_item")
            .expect("fixture item was retained");
        assert!(generated.starts_with("//! fixture crate docs\n//! remain first\n\n#![no_std]\n"));
        assert!(generated_offset > generated.find("#![cfg_attr").unwrap());
        assert!(generated_offset < normal_item_offset);
        assert!(generated.contains("const _: &str = studio_generated::GENERATED_MARKER;"));
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
    fn isolated_manifest_is_firmware_only_and_cargo_validates_it() {
        let workspace = IsolatedWorkspace::new(&compiled_workspace_root().unwrap()).unwrap();
        let manifest = std::fs::read_to_string(workspace.root.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("members = [\".\", \"core\"]"));
        assert!(manifest.contains("default-members = [\".\"]"));
        assert!(!manifest.contains("sensor-watch-studio"));
        assert!(!workspace.root.join("studio").exists());
        assert!(!workspace.root.join("tools").exists());
        assert!(workspace.root.join("core/Cargo.toml").is_file());

        let result = Command::new("cargo")
            .args([
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(workspace.root.join("Cargo.toml"))
            .output()
            .expect("cargo metadata should start");
        assert!(
            result.status.success(),
            "isolated manifest rejected by cargo: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    #[ignore = "requires the thumbv6m-none-eabi target and rust-objcopy"]
    fn configured_green_and_red_lite_builds_use_isolated_workspace() {
        let root = temp_root("configured-builds");
        let profiles = super::super::components::default_profiles();
        for (board, profile_index) in [
            (super::super::components::BoardKind::Green, 0),
            (super::super::components::BoardKind::RedLite, 1),
        ] {
            let profile = profiles[profile_index].clone();
            let request = FirmwareInputRequest {
                board,
                revision: if board == super::super::components::BoardKind::RedLite {
                    "OSO-SWAT-A1-02"
                } else {
                    "OSO-SWAT-A1-05"
                }
                .into(),
                components: profile.config.clone(),
                profile,
                preset_name: "Stock Casio".into(),
                ordered_faces: vec!["SIMPLE_CLOCK".into()],
                modules: vec![],
            };
            let revision = if board == super::super::components::BoardKind::RedLite {
                "OSO-SWAT-A1-02"
            } else {
                "OSO-SWAT-A1-05"
            };
            let output = super::super::storage::build_output_paths(
                &root,
                artifact_board_name(board),
                revision,
                artifact_profile_name(board, &request.profile.name),
            )
            .unwrap();
            std::fs::create_dir_all(&output.latest).unwrap();
            let result = build_firmware(request, &output);
            assert!(
                result.success,
                "{} build failed: {}",
                board.label(),
                result.message
            );
            assert!(result.uf2_path.as_ref().is_some_and(|path| path.is_file()));
            println!(
                "{} artifacts: UF2={}, manifest={}, signature={}, inputs={}, latest={}",
                board.label(),
                output.uf2.display(),
                output.manifest.display(),
                output.sidecar.display(),
                output.inputs.display(),
                output.latest_json.display(),
            );
        }
        let _ = std::fs::remove_dir_all(root);
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
    fn static_contract_status_uses_explicit_categories_without_claiming_completion() {
        let text = CONFIGURATION_INPUT_STATUS
            .iter()
            .flat_map(|(category, title, explanation)| [*category, *title, *explanation])
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Supported and generated"));
        assert!(text.contains("validated"));
        assert!(text.contains("fail closed"));
        assert!(text.contains("Generated-input provenance"));
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
    fn configuration_contract_remains_separate_from_request_preflight() {
        assert_eq!(missing_configuration_inputs(), CONFIGURATION_INPUT_CONTRACT);
        assert!(!CONFIGURATION_INPUT_CONTRACT.is_empty());
    }

    #[test]
    fn contract_explanations_are_beginner_actionable_without_promising_generation() {
        assert_eq!(
            CONFIGURATION_INPUT_EXPLANATIONS.len(),
            CONFIGURATION_INPUT_CONTRACT.len()
        );
        let all = CONFIGURATION_INPUT_EXPLANATIONS
            .iter()
            .flat_map(|(title, explanation)| [*title, *explanation])
            .collect::<Vec<_>>();
        let text = all.join(" ").to_ascii_lowercase();
        for phrase in [
            "isolated build workspace",
            "validated",
            "stock component",
            "firmware build",
            "fail closed",
            "uf2",
            "provenance",
        ] {
            assert!(text.contains(phrase), "explanations omitted: {phrase}");
        }
        assert!(text.contains("unknown component"));
    }

    #[test]
    fn preflight_request_does_not_create_generated_bundle() {
        let profiles = super::super::components::default_profiles();
        let request = FirmwareInputRequest {
            board: super::super::components::BoardKind::Green,
            revision: "OSO-SWAT-A1-05".into(),
            profile: profiles[0].clone(),
            components: profiles[0].config.clone(),
            preset_name: "Stock Casio".into(),
            ordered_faces: vec!["SIMPLE_CLOCK".into(), "ALARM".into()],
            modules: vec![],
        };
        let temp = std::env::temp_dir();
        let before: Vec<_> = std::fs::read_dir(&temp)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("sensor-watch-studio-preflight-")
            })
            .map(|entry| entry.file_name())
            .collect();
        preflight_request(&request).unwrap();
        let after: Vec<_> = std::fs::read_dir(&temp)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("sensor-watch-studio-preflight-")
            })
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn output_root_preflight_fails_before_a_build_worker_can_start() {
        let profiles = super::super::components::default_profiles();
        let request = FirmwareInputRequest {
            board: super::super::components::BoardKind::Green,
            revision: "OSO-SWAT-A1-05".into(),
            profile: profiles[0].clone(),
            components: profiles[0].config.clone(),
            preset_name: "Stock Casio".into(),
            ordered_faces: vec!["SIMPLE_CLOCK".into()],
            modules: vec![],
        };
        let root = if cfg!(windows) {
            PathBuf::from(r"Z:\\sensor-watch-missing-share")
        } else {
            PathBuf::from("/proc/sensor-watch-missing-share")
        };
        let error = preflight_output_root(&root, &request, None, None).unwrap_err();
        assert!(error.contains("Artifact root preflight failed"));
        assert!(error.contains("Create folder"));
        assert!(error.contains("Use default"));
    }

    #[test]
    fn packaged_red_lite_preflight_creates_complete_layout() {
        let package = temp_root("fresh-package-red-lite");
        std::fs::create_dir_all(&package).unwrap();
        let data = package.join("data");
        let root = super::super::storage::default_artifact_root(&data);
        let profiles = super::super::components::default_profiles();
        let request = FirmwareInputRequest {
            board: super::super::components::BoardKind::RedLite,
            revision: "OSO-SWAT-A1-05".into(),
            profile: profiles[1].clone(),
            components: profiles[1].config.clone(),
            preset_name: "Stock Casio".into(),
            ordered_faces: vec!["SIMPLE_CLOCK".into()],
            modules: vec![],
        };
        let paths = preflight_output_root(&root, &request, Some(&package), Some(&data)).unwrap();
        assert!(paths.latest.is_dir());
        assert_eq!(paths.board, "Red-Lite");
        assert_eq!(
            paths.latest,
            package
                .join("data/sensor-watch-studio-artifacts/Red-Lite/OSO-SWAT-A1-05/Red-Lite/latest")
        );
        let _ = std::fs::remove_dir_all(package);
    }

    #[test]
    fn invalid_configured_build_does_not_fall_back_to_stock() {
        let profiles = super::super::components::default_profiles();
        let request = FirmwareInputRequest {
            board: super::super::components::BoardKind::Green,
            revision: "unknown-revision".into(),
            profile: profiles[0].clone(),
            components: profiles[0].config.clone(),
            preset_name: "Stock Casio".into(),
            ordered_faces: vec!["SIMPLE_CLOCK".into()],
            modules: vec![],
        };
        let output_root = temp_root("blocked-output");
        let output = super::super::storage::build_output_paths(
            &output_root,
            "Green",
            "unknown-revision",
            "Green",
        )
        .unwrap();
        let result = build_firmware(request, &output);
        assert!(!result.success);
        assert!(result.message.contains("unsupported"));
        assert!(result.uf2_path.is_none());
        assert!(!output_root.exists());
    }

    #[test]
    fn custom_artifact_root_receives_uf2_and_latest_pointer() {
        let custom_root = temp_root("custom-artifact-root");
        let default_root = temp_root("default-artifact-root");
        let custom =
            super::super::storage::artifact_paths(&custom_root, "Green", "rev-a", "stock").unwrap();
        let default =
            super::super::storage::artifact_paths(&default_root, "Green", "rev-a", "stock")
                .unwrap();
        std::fs::create_dir_all(&custom.latest).unwrap();
        let uf2_data = sensor_watch_core::uf2::convert_to_uf2(&[42; 1024]);

        publish_uf2_with_input_digest(
            &custom.latest,
            &custom.uf2,
            &uf2_data,
            "custom-input-digest",
        )
        .unwrap();
        super::super::storage::write_latest_atomic(
            &custom,
            &super::super::storage::LatestMetadata {
                format: "sensor-watch-latest-v1".into(),
                board: "Green".into(),
                revision: "rev-a".into(),
                profile: "stock".into(),
                generated_input_digest: "custom-input-digest".into(),
                artifact: "sensor-watch.uf2".into(),
            },
        )
        .unwrap();

        assert!(custom.uf2.is_file());
        assert!(custom.latest_json.is_file());
        assert!(!default.uf2.exists());
        assert!(!default.latest_json.exists());
        let _ = std::fs::remove_dir_all(custom_root);
        let _ = std::fs::remove_dir_all(default_root);
    }

    #[test]
    fn generated_input_digest_requires_provenance_and_detects_tampering() {
        let root = temp_root("digest-validation");
        let bundle = root.join("sensor-watch.uf2.inputs");
        std::fs::create_dir_all(&bundle).unwrap();
        let mut files = BTreeMap::new();
        files.insert("PROVENANCE.json".into(), "[{\"path\":\"pins.h\"}]".into());
        files.insert(
            "firmware_inputs.json".into(),
            "{\"board\":\"Green\"}".into(),
        );
        for (name, contents) in &files {
            std::fs::write(bundle.join(name), contents).unwrap();
        }
        let digest = super::super::firmware_inputs::digest_generated_files(&files);
        let inspection = ArtifactInspection {
            path: root.join("sensor-watch.uf2"),
            generation: String::new(),
            family_id: String::new(),
            uf2_bytes: String::new(),
            uf2_blocks: String::new(),
            payload_bytes: String::new(),
            sha256: String::new(),
            payload_sha256: String::new(),
            manifest_digest: String::new(),
            generated_input_digest: digest,
        };
        validate_generated_input_digest(&inspection).unwrap();
        std::fs::write(bundle.join("PROVENANCE.json"), "tampered").unwrap();
        assert!(validate_generated_input_digest(&inspection)
            .unwrap_err()
            .contains("digest changed"));
        let mut missing = inspection;
        missing.generated_input_digest.clear();
        assert!(validate_generated_input_digest(&missing)
            .unwrap_err()
            .contains("lacks generated-input provenance"));
        std::fs::remove_dir_all(root).unwrap();
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
        assert!(error.contains("Newly published artifact was preserved"));
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

#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    fn diagnostic_command() -> Command {
        #[cfg(windows)]
        {
            let mut command = Command::new("cmd");
            command.args([
                "/C",
                "echo password=do-not-log && echo C:\\private\\project",
            ]);
            command
        }
        #[cfg(not(windows))]
        {
            let mut command = Command::new("sh");
            command.args([
                "-c",
                "printf 'password=do-not-log\\n/tmp/private/project\\n'",
            ]);
            command
        }
    }

    #[test]
    fn captures_stdout_and_stderr_and_preserves_exit_code() {
        #[cfg(windows)]
        let mut command = Command::new("cmd");
        #[cfg(windows)]
        command.args(["/C", "echo stdout && echo stderr 1>&2 && exit /B 7"]);
        #[cfg(not(windows))]
        let mut command = Command::new("sh");
        #[cfg(not(windows))]
        command.args(["-c", "printf stdout; printf stderr >&2; exit 7"]);
        let result = capture_command(command, &[]).unwrap();
        assert_eq!(result.code, Some(7));
        assert!(result.diagnostics.contains("stdout"));
        assert!(result.diagnostics.contains("stderr"));
    }

    #[test]
    fn bounds_and_redacts_diagnostics() {
        let result = capture_command(diagnostic_command(), &[Path::new("C:\\private")]).unwrap();
        assert!(!result.diagnostics.contains("do-not-log"));
        assert!(!result.diagnostics.contains("C:\\private"));
        assert!(result.diagnostics.len() <= MAX_DIAGNOSTIC_BYTES);

        let text = (0..(MAX_DIAGNOSTIC_LINES + 20))
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let sanitized = sanitize_diagnostics(&text, &[]);
        assert!(sanitized.contains("diagnostics truncated by line limit"));
        assert!(sanitized.lines().count() <= MAX_DIAGNOSTIC_LINES + 1);
    }

    #[test]
    fn packaged_build_selects_valid_mutable_project_not_template() {
        let root = std::env::temp_dir().join(format!("studio-build-selection-{}", unix_nanos()));
        let data = root.join("data");
        let project = data.join("project");
        let template = root.join("firmware");
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::create_dir_all(&template).unwrap();
        std::fs::write(
            project.join("Cargo.toml"),
            "[workspace]\n[package]\nname = \"sensor-watch\"\n",
        )
        .unwrap();
        assert_eq!(
            select_project_for_build(
                crate::distribution::DistributionMode::Packaged,
                &project,
                Some(&template),
                &data,
            ),
            Some(project.canonicalize().unwrap())
        );
        assert!(select_project_for_build(
            crate::distribution::DistributionMode::Packaged,
            &template,
            Some(&template),
            &data,
        )
        .is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}
