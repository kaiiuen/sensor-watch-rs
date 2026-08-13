use sensor_watch_core::uf2::{
    self, APP_START_ADDR, MAX_APPLICATION_BYTES, SAML22_FAMILY_ID, UF2_BLOCK_SIZE, UF2_PAYLOAD_SIZE,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

struct BuildLock {
    path: PathBuf,
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_build_lock(root: &Path) -> ToolResult<BuildLock> {
    let target = root.join("target");
    fs::create_dir_all(&target).map_err(|e| format!("cannot create build directory: {e}"))?;
    let path = target.join(".sensor-watch-build.lock");
    fs::OpenOptions::new()
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

fn remove_regular_file(path: &Path) -> ToolResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(format!("refusing non-regular path: {}", path.display()))
        }
        Ok(_) => {
            fs::remove_file(path).map_err(|e| format!("cannot remove {}: {e}", path.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot inspect {}: {e}", path.display())),
    }
}

pub const MANIFEST_FORMAT: &str = "sensor-watch-recovery-manifest-v2";
pub const MAX_UF2_BYTES: usize = MAX_APPLICATION_BYTES.div_ceil(UF2_PAYLOAD_SIZE) * UF2_BLOCK_SIZE;
pub type Manifest = Map<String, Value>;
pub type ToolResult<T> = Result<T, String>;

#[derive(Debug, Clone)]
pub struct Uf2Inspection {
    pub data: Vec<u8>,
    pub image: Vec<u8>,
    pub block_count: usize,
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub uf2_path: PathBuf,
}

fn read_file(path: &Path, maximum: usize) -> ToolResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| format!("cannot inspect {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("refusing symlinked file path: {}", path.display()));
    }
    if !metadata.is_file() {
        return Err(format!("refusing non-file input path: {}", path.display()));
    }
    if metadata.len() > maximum as u64 {
        return Err(format!(
            "{} is {} bytes; maximum is {maximum}",
            path.display(),
            metadata.len()
        ));
    }

    // Read one byte beyond the limit as well: the file can grow after the
    // metadata check, and this keeps the allocation bounded in that case.
    let file = fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut data = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if data.len() > maximum {
        return Err(format!(
            "{} is larger than the maximum of {maximum} bytes",
            path.display()
        ));
    }
    Ok(data)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex(&h.finalize())
}
pub fn manifest_value(manifest: &Manifest, key: &str) -> String {
    manifest
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
/// Returns a local consistency digest for the manifest fields.
///
/// This is not a cryptographic signature and does not establish provenance or
/// authenticity. The `signature` field remains accepted as a legacy alias.
pub fn manifest_digest(manifest: &Manifest) -> String {
    let mut unsigned = manifest.clone();
    unsigned.remove("manifest_digest");
    unsigned.remove("signature");
    let canonical = serde_json::to_vec(&Value::Object(unsigned))
        .map_err(|e| e.to_string())
        .unwrap_or_default();
    format!("sha256:{}", sha256(&canonical))
}

/// Compatibility alias for callers of the pre-v3 API. This value is a local
/// digest, not a signature.
#[deprecated(note = "use manifest_digest; this is not a cryptographic signature")]
pub fn sign_manifest(manifest: &Manifest) -> String {
    manifest_digest(manifest)
}

pub fn inspect_uf2(path: &Path) -> ToolResult<Uf2Inspection> {
    let data = read_file(path, MAX_UF2_BYTES)?;
    if data.is_empty() || !data.len().is_multiple_of(UF2_BLOCK_SIZE) {
        return Err(format!(
            "UF2 is {} bytes; expected a non-empty multiple of 512",
            data.len()
        ));
    }
    let parsed = uf2::validate(&data).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Uf2Inspection {
        data,
        image: parsed.image,
        block_count: parsed.block_count,
    })
}

fn manifest_from_inspection(
    inspected: &Uf2Inspection,
    generation: Option<String>,
    artifact: &Path,
) -> Manifest {
    let generation = generation
        .unwrap_or_else(|| format!("g{}-{}", now_nanos(), &sha256(&inspected.data)[..12]));
    let mut m = Map::new();
    m.insert("format".into(), MANIFEST_FORMAT.into());
    m.insert("generation_id".into(), generation.into());
    m.insert("board".into(), "ATSAML22J18A".into());
    m.insert(
        "family_id".into(),
        format!("0x{SAML22_FAMILY_ID:08X}").into(),
    );
    m.insert(
        "application_start".into(),
        format!("0x{APP_START_ADDR:08X}").into(),
    );
    m.insert(
        "maximum_application_bytes".into(),
        MAX_APPLICATION_BYTES.into(),
    );
    m.insert("uf2_bytes".into(), inspected.data.len().into());
    m.insert("uf2_blocks".into(), inspected.block_count.into());
    m.insert("payload_bytes".into(), inspected.image.len().into());
    m.insert(
        "crc32_ieee".into(),
        format!("0x{:08X}", uf2::crc32(&inspected.image)).into(),
    );
    m.insert("sha256".into(), sha256(&inspected.data).into());
    m.insert("payload_sha256".into(), sha256(&inspected.image).into());
    m.insert("artifact".into(), artifact.display().to_string().into());
    let digest = manifest_digest(&m);
    m.insert("manifest_digest".into(), digest.clone().into());
    // Keep the old key for manifests produced by older tooling.
    m.insert("signature".into(), digest.into());
    m
}

pub fn create_manifest(
    path: &Path,
    generation: Option<String>,
    artifact: Option<&Path>,
) -> ToolResult<Manifest> {
    let inspected = inspect_uf2(path)?;
    Ok(manifest_from_inspection(
        &inspected,
        generation,
        artifact.unwrap_or(path),
    ))
}
fn write_json(path: &Path, value: &Value) -> ToolResult<()> {
    if path.exists() || path.is_symlink() {
        return Err(format!(
            "refusing to overwrite existing file: {}",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|e| e.to_string())?;
    writeln!(file).map_err(|e| e.to_string())?;
    Ok(())
}
fn write_text_new(path: &Path, text: &str) -> ToolResult<()> {
    if path.exists() || path.is_symlink() {
        return Err(format!(
            "refusing to overwrite existing file: {}",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| e.to_string())
}
fn signature_path(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "{}sig",
        path.extension()
            .and_then(|x| x.to_str())
            .map(|x| format!("{x}."))
            .unwrap_or_default()
    ))
}

pub fn write_manifest(path: &Path, m: &Manifest) -> ToolResult<()> {
    write_json(path, &Value::Object(m.clone()))?;
    write_text_new(
        &signature_path(path),
        &(manifest_digest_value(m).to_owned() + "\n"),
    )
}
pub fn write_binary(path: &Path, data: &[u8]) -> ToolResult<()> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(format!(
            "refusing non-regular output path: {}",
            path.display()
        ));
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut f| f.write_all(data))
        .map_err(|e| format!("cannot create {}: {e}", path.display()))
}
fn load_manifest(path: &Path) -> ToolResult<Manifest> {
    let data = read_file(path, 512 * 1024)?;
    serde_json::from_slice(&data).map_err(|e| format!("cannot parse manifest: {e}"))
}
fn manifest_digest_value(m: &Manifest) -> &str {
    m.get("manifest_digest")
        .and_then(Value::as_str)
        .or_else(|| m.get("signature").and_then(Value::as_str))
        .unwrap_or_default()
}

/// Checks only the optional trusted release provenance. A missing value is
/// deliberately not treated as authenticity; callers should report it.
pub fn trusted_release_status(trusted: Option<&str>) -> &'static str {
    if trusted.is_some() {
        "matched"
    } else {
        "not provided"
    }
}

fn trusted_match(m: &Manifest, trusted: Option<&str>) -> ToolResult<()> {
    let Some(expected) = trusted else {
        return Ok(());
    };
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("trusted release SHA-256 must be exactly 64 hexadecimal characters".into());
    }
    if !manifest_value(m, "sha256").eq_ignore_ascii_case(expected) {
        return Err("UF2 does not match the trusted release SHA-256".into());
    }
    Ok(())
}
pub fn verify_uf2(
    path: &Path,
    manifest_path: Option<&Path>,
    trusted: Option<&str>,
) -> ToolResult<Manifest> {
    let manifest_was_supplied = manifest_path.is_some();
    let mpath = manifest_path
        .map(PathBuf::from)
        .unwrap_or_else(|| path.with_extension("uf2.json"));
    match fs::symlink_metadata(&mpath) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "refusing symlinked manifest path: {}",
                mpath.display()
            ));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(format!(
                "manifest is not a regular file: {}",
                mpath.display()
            ));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !manifest_was_supplied => {
            let m = create_manifest(path, None, None)?;
            trusted_match(&m, trusted)?;
            return Ok(m);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("manifest not found at {}", mpath.display()));
        }
        Err(e) => return Err(format!("cannot inspect manifest {}: {e}", mpath.display())),
    }
    let m = load_manifest(&mpath)?;
    if manifest_value(&m, "format") != MANIFEST_FORMAT
        || manifest_digest_value(&m) != manifest_digest(&m)
    {
        return Err("manifest local digest is invalid".into());
    }
    let sidecar = read_file(&signature_path(&mpath), 128)?;
    if std::str::from_utf8(&sidecar).map(str::trim).ok() != Some(manifest_digest_value(&m)) {
        return Err("manifest digest sidecar is invalid".into());
    }
    let actual = create_manifest(path, Some(manifest_value(&m, "generation_id")), None)?;
    trusted_match(&actual, trusted)?;
    for key in [
        "format",
        "generation_id",
        "board",
        "family_id",
        "application_start",
        "maximum_application_bytes",
        "uf2_bytes",
        "uf2_blocks",
        "payload_bytes",
        "crc32_ieee",
        "sha256",
        "payload_sha256",
    ] {
        if actual.get(key) != m.get(key) {
            return Err(format!("manifest mismatch for {key}"));
        }
    }
    if m.contains_key("manifest_digest")
        && actual.get("manifest_digest") != m.get("manifest_digest")
    {
        return Err("manifest local digest mismatch".into());
    }
    Ok(m)
}
pub fn convert_uf2(input: &Path, output: &Path) -> ToolResult<()> {
    let data = read_file(input, MAX_APPLICATION_BYTES)?;
    let out = uf2::convert_to_uf2(&data);
    if out.is_empty() {
        return Err("cannot convert input to UF2".into());
    }
    write_binary(output, &out)
}
pub fn backup_uf2(src: &Path, dst: &Path) -> ToolResult<Manifest> {
    // Keep the validated bytes and the bytes written to the backup identical.
    // Re-reading `src` after validation would allow a local replacement race to
    // make the manifest describe different bytes than the preserved artifact.
    let data = read_file(src, MAX_UF2_BYTES)?;
    if data.is_empty() || !data.len().is_multiple_of(UF2_BLOCK_SIZE) {
        return Err("UF2 is not a non-empty multiple of 512 bytes".into());
    }
    let parsed = uf2::validate(&data).map_err(|e| format!("{}: {e}", src.display()))?;
    let inspected = Uf2Inspection {
        data,
        image: parsed.image,
        block_count: parsed.block_count,
    };
    if dst.exists() || dst.is_symlink() {
        return Err("refusing to overwrite existing backup".into());
    }
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    write_binary(dst, &inspected.data)?;
    let m = manifest_from_inspection(&inspected, None, dst);
    write_manifest(&dst.with_extension("uf2.json"), &m)?;
    Ok(m)
}
pub fn rollback_uf2(src: &Path, dst: &Path, trusted: &str) -> ToolResult<Manifest> {
    let m = verify_uf2(src, None, Some(trusted))?;
    if dst.exists() || dst.is_symlink() {
        return Err("refusing to overwrite existing rollback staging path".into());
    }
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let data = read_file(src, MAX_UF2_BYTES)?;
    write_binary(dst, &data)?;
    inspect_uf2(dst)?;
    Ok(m)
}
pub fn recovery_report(path: &Path, trusted: &str) -> ToolResult<Value> {
    let m = verify_uf2(path, None, Some(trusted))?;
    let mut r = Map::new();
    r.insert("format".into(), "sensor-watch-recovery-report-v1".into());
    r.insert("artifact".into(), path.display().to_string().into());
    r.insert(
        "trusted_release_sha256".into(),
        trusted_release_status(Some(trusted)).into(),
    );
    r.insert(
        "generation_id".into(),
        manifest_value(&m, "generation_id").into(),
    );
    for key in [
        "validated",
        "device_side_rollback",
        "true_dual_boot",
        "rom_bootloader_modified",
        "hardware_tested",
    ] {
        r.insert(key.into(), true.into());
    }
    r.insert("device_side_rollback".into(), false.into());
    r.insert("true_dual_boot".into(), false.into());
    r.insert("rom_bootloader_modified".into(), false.into());
    r.insert("hardware_tested".into(), false.into());
    Ok(Value::Object(r))
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
fn run(mut command: Command) -> ToolResult<()> {
    let status = command.status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("command exited with {status}"));
    }
    Ok(())
}
fn objcopy() -> ToolResult<PathBuf> {
    for name in [
        "rust-objcopy",
        "rust-objcopy.exe",
        "arm-none-eabi-objcopy",
        "arm-none-eabi-objcopy.exe",
    ] {
        if Command::new(name)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return Ok(PathBuf::from(name));
        }
    }
    if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
        for name in ["rust-objcopy", "rust-objcopy.exe"] {
            if let Some(p) = find_file(&PathBuf::from(&home).join(".rustup/toolchains"), name) {
                return Ok(p);
            }
        }
    }
    Err("rust-objcopy or arm-none-eabi-objcopy not found".into())
}
fn find_file(root: &Path, name: &str) -> Option<PathBuf> {
    for e in fs::read_dir(root).ok()?.flatten() {
        let p = e.path();
        if p.file_name().and_then(|x| x.to_str()) == Some(name) {
            return Some(p);
        }
        if p.is_dir()
            && let Some(found) = find_file(&p, name)
        {
            return Some(found);
        }
    }
    None
}

fn is_workspace_root(path: &Path) -> bool {
    let manifest = path.join("Cargo.toml");
    let Ok(metadata) = fs::symlink_metadata(&manifest) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    let Ok(contents) = fs::read_to_string(&manifest) else {
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

/// Resolves the firmware workspace without consulting an untrusted directory.
///
/// A Studio/tools executable normally lives below `target/`, so its ancestor
/// directories are useful only when they resolve to the workspace this binary
/// was compiled from. A copied executable must not select an arbitrary
/// Cargo.toml and execute that project's build scripts.
pub fn workspace_root() -> ToolResult<PathBuf> {
    let trusted = compiled_workspace_root().ok_or_else(|| {
        format!(
            "cannot locate the compiled sensor-watch workspace ({})",
            env!("CARGO_MANIFEST_DIR")
        )
    })?;

    if let Ok(executable) = env::current_exe()
        && let Some(mut dir) = executable.parent().map(Path::to_path_buf)
    {
        loop {
            if let Some(root) = trusted_runtime_root(&dir, &trusted) {
                return Ok(root);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    // The executable may have been copied outside the checkout. Use only the
    // compile-time workspace, never a similarly named project found at runtime.
    Ok(trusted)
}

pub fn build_firmware() -> ToolResult<BuildResult> {
    let root = workspace_root()?;
    let _build_lock = acquire_build_lock(&root)?;
    let mut c = Command::new("cargo");
    c.args([
        "build",
        "--release",
        "--target",
        "thumbv6m-none-eabi",
        "-p",
        "sensor-watch",
        "--bin",
        "sensor-watch",
    ]);
    c.current_dir(&root);
    run(c)?;
    let dir = root.join("target/thumbv6m-none-eabi/release");
    let elf = dir.join("sensor-watch");
    let bin = dir.join("sensor-watch.bin");
    let uf2_path = dir.join("sensor-watch.uf2");
    let mut c = Command::new(objcopy()?);
    c.args(["-O", "binary"]).arg(&elf).arg(&bin);
    run(c)?;
    if uf2_path.exists() {
        let backup = dir
            .join("recovery/generations")
            .join(format!("{}.uf2", now_nanos()));
        let m = create_manifest(&uf2_path, None, Some(&backup))?;
        fs::create_dir_all(backup.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(&uf2_path, &backup).map_err(|e| e.to_string())?;
        write_manifest(&backup.with_extension("uf2.json"), &m)?;
    }
    let image = read_file(&bin, MAX_APPLICATION_BYTES)?;
    let encoded = uf2::convert_to_uf2(&image);
    if encoded.is_empty() {
        return Err("cannot convert firmware binary to UF2".into());
    }
    remove_regular_file(&uf2_path)?;
    write_binary(&uf2_path, &encoded)?;
    let manifest_path = uf2_path.with_extension("uf2.json");
    let signature_path = manifest_path.with_extension("json.sig");
    remove_regular_file(&manifest_path)?;
    remove_regular_file(&signature_path)?;
    write_manifest(&manifest_path, &create_manifest(&uf2_path, None, None)?)?;
    Ok(BuildResult { uf2_path })
}

pub fn flash_firmware(elf: &Path) -> ToolResult<()> {
    match fs::symlink_metadata(elf) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "refusing symlinked firmware ELF: {}",
                elf.display()
            ));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(format!(
                "firmware ELF is not a regular file: {}",
                elf.display()
            ));
        }
        Ok(_) => {}
        Err(e) => return Err(format!("firmware ELF not found at {}: {e}", elf.display())),
    }
    let mut c = Command::new("probe-rs");
    c.args([
        "run",
        "--chip",
        "ATSAML22J18A",
        "--protocol",
        "swd",
        "--connect-under-reset",
    ])
    .arg(elf);
    run(c)
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "sensor-watch-workspace-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn workspace_manifest_is_identified_by_package_and_workspace() {
        let root = temp_root("valid");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\".\"]\n\n[package]\nname = \"sensor-watch\"\n",
        )
        .unwrap();
        assert_eq!(
            canonical_workspace_root(&root),
            Some(root.canonicalize().unwrap())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unrelated_manifest_is_not_accepted_as_workspace() {
        let root = temp_root("unrelated");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\".\"]\n\n[package]\nname = \"other-project\"\n",
        )
        .unwrap();
        assert!(canonical_workspace_root(&root).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_root_does_not_depend_on_caller_cwd() {
        let expected = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        assert_eq!(workspace_root().unwrap(), expected);
    }

    #[test]
    fn explicit_missing_manifest_is_not_treated_as_unverified_artifact() {
        let root = temp_root("missing-manifest");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("firmware.uf2");
        let manifest = root.join("missing.uf2.json");
        fs::write(&artifact, uf2::convert_to_uf2(b"known-good")).unwrap();

        let error = verify_uf2(&artifact, Some(&manifest), None).unwrap_err();
        assert!(
            error.contains("manifest not found"),
            "unexpected error: {error}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn valid_workspace_elsewhere_is_not_trusted() {
        let root = temp_root("untrusted-valid");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\".\"]\n\n[package]\nname = \"sensor-watch\"\n",
        )
        .unwrap();
        let trusted = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        assert!(trusted_runtime_root(&root, &trusted).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
