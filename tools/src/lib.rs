use sensor_watch_core::uf2::{
    self, APP_START_ADDR, MAX_APPLICATION_BYTES, SAML22_FAMILY_ID, UF2_BLOCK_SIZE, UF2_PAYLOAD_SIZE,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

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
    if path.is_symlink() {
        return Err(format!("refusing symlinked file path: {}", path.display()));
    }
    let data = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if data.len() > maximum {
        return Err(format!(
            "{} is {} bytes; maximum is {maximum}",
            path.display(),
            data.len()
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
pub fn sign_manifest(manifest: &Manifest) -> String {
    let mut unsigned = manifest.clone();
    unsigned.remove("signature");
    let canonical = serde_json::to_vec(&Value::Object(unsigned))
        .map_err(|e| e.to_string())
        .unwrap_or_default();
    format!("sha256:{}", sha256(&canonical))
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

pub fn create_manifest(
    path: &Path,
    generation: Option<String>,
    artifact: Option<&Path>,
) -> ToolResult<Manifest> {
    let inspected = inspect_uf2(path)?;
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
    m.insert(
        "artifact".into(),
        artifact.unwrap_or(path).display().to_string().into(),
    );
    let signature = sign_manifest(&m);
    m.insert("signature".into(), signature.into());
    Ok(m)
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
pub fn write_manifest(path: &Path, m: &Manifest) -> ToolResult<()> {
    write_json(path, &Value::Object(m.clone()))?;
    let sig = path.with_extension(format!(
        "{}sig",
        path.extension()
            .and_then(|x| x.to_str())
            .map(|x| format!("{x}."))
            .unwrap_or_default()
    ));
    write_text_new(&sig, &(manifest_value(m, "signature") + "\n"))
}
pub fn write_binary(path: &Path, data: &[u8]) -> ToolResult<()> {
    if path.is_symlink() {
        return Err(format!(
            "refusing symlinked output path: {}",
            path.display()
        ));
    }
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .and_then(|mut f| f.write_all(data))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}
fn load_manifest(path: &Path) -> ToolResult<Manifest> {
    let data = read_file(path, 512 * 1024)?;
    serde_json::from_slice(&data).map_err(|e| format!("cannot parse manifest: {e}"))
}
fn trusted_match(m: &Manifest, trusted: Option<&str>) -> ToolResult<()> {
    if let Some(expected) = trusted
        && (expected.len() != 64
            || !expected.bytes().all(|b| b.is_ascii_hexdigit())
            || !manifest_value(m, "sha256").eq_ignore_ascii_case(expected))
    {
        return Err("UF2 does not match the trusted release SHA-256".into());
    }
    Ok(())
}
pub fn verify_uf2(
    path: &Path,
    manifest_path: Option<&Path>,
    trusted: Option<&str>,
) -> ToolResult<Manifest> {
    let mpath = manifest_path
        .map(PathBuf::from)
        .unwrap_or_else(|| path.with_extension("uf2.json"));
    if !mpath.exists() {
        let m = create_manifest(path, None, None)?;
        trusted_match(&m, trusted)?;
        return Ok(m);
    }
    let m = load_manifest(&mpath)?;
    if manifest_value(&m, "format") != MANIFEST_FORMAT
        || manifest_value(&m, "signature") != sign_manifest(&m)
    {
        return Err("manifest signature is invalid".into());
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
    let m = create_manifest(src, None, Some(dst))?;
    if dst.exists() || dst.is_symlink() {
        return Err("refusing to overwrite existing backup".into());
    }
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    fs::copy(src, dst).map_err(|e| e.to_string())?;
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
    fs::copy(src, dst).map_err(|e| e.to_string())?;
    inspect_uf2(dst)?;
    Ok(m)
}
pub fn recovery_report(path: &Path, trusted: &str) -> ToolResult<Value> {
    let m = verify_uf2(path, None, Some(trusted))?;
    let mut r = Map::new();
    r.insert("format".into(), "sensor-watch-recovery-report-v1".into());
    r.insert("artifact".into(), path.display().to_string().into());
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
pub fn build_firmware() -> ToolResult<BuildResult> {
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
    run(c)?;
    let dir = Path::new("target/thumbv6m-none-eabi/release");
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
    write_binary(&uf2_path, &encoded)?;
    let manifest_path = uf2_path.with_extension("uf2.json");
    if manifest_path.exists() {
        verify_uf2(&uf2_path, Some(&manifest_path), None)?;
    } else {
        write_manifest(&manifest_path, &create_manifest(&uf2_path, None, None)?)?;
    }
    Ok(BuildResult { uf2_path })
}
pub fn flash_firmware(elf: &Path) -> ToolResult<()> {
    if !elf.is_file() {
        return Err(format!("firmware ELF not found at {}", elf.display()));
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
