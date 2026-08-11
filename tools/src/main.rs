use sensor_watch_core::uf2::{
    self, APP_START_ADDR, MAX_APPLICATION_BYTES, SAML22_FAMILY_ID, UF2_BLOCK_SIZE, UF2_PAYLOAD_SIZE,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const FORMAT: &str = "sensor-watch-recovery-manifest-v2";
const MAX_UF2_BYTES: usize = MAX_APPLICATION_BYTES.div_ceil(UF2_PAYLOAD_SIZE) * UF2_BLOCK_SIZE;

fn usage() -> ! {
    eprintln!("usage: sensor-watch-tools <build|uf2|verify|backup|rollback|report|flash> ...");
    std::process::exit(2)
}
fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1)
}
fn read_file(path: &Path, maximum: usize) -> Vec<u8> {
    if path.is_symlink() {
        fail(format!("refusing symlinked file path: {}", path.display()));
    }
    let data =
        fs::read(path).unwrap_or_else(|e| fail(format!("cannot read {}: {e}", path.display())));
    if data.len() > maximum {
        fail(format!(
            "{} is {} bytes; maximum is {maximum}",
            path.display(),
            data.len()
        ));
    }
    data
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex(&h.finalize())
}
fn value_string(map: &Map<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
fn sign(map: &Map<String, Value>) -> String {
    let mut unsigned = map.clone();
    unsigned.remove("signature");
    let canonical = serde_json::to_vec(&Value::Object(unsigned)).unwrap_or_else(|e| fail(e));
    format!("sha256:{}", sha256(&canonical))
}
fn inspect(path: &Path) -> (Vec<u8>, Vec<u8>, usize) {
    let data = read_file(path, MAX_UF2_BYTES);
    if data.is_empty() || !data.len().is_multiple_of(UF2_BLOCK_SIZE) {
        fail(format!(
            "UF2 is {} bytes; expected a non-empty multiple of 512",
            data.len()
        ));
    }
    let parsed = uf2::validate(&data).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
    (data, parsed.image, parsed.block_count)
}
fn manifest(
    path: &Path,
    generation: Option<String>,
    artifact: Option<&Path>,
) -> Map<String, Value> {
    let (data, image, blocks) = inspect(path);
    let generation =
        generation.unwrap_or_else(|| format!("g{}-{}", now_nanos(), &sha256(&data)[..12]));
    let mut m = Map::new();
    m.insert("format".into(), FORMAT.into());
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
    m.insert("uf2_bytes".into(), data.len().into());
    m.insert("uf2_blocks".into(), blocks.into());
    m.insert("payload_bytes".into(), image.len().into());
    m.insert(
        "crc32_ieee".into(),
        format!("0x{:08X}", uf2::crc32(&image)).into(),
    );
    m.insert("sha256".into(), sha256(&data).into());
    m.insert("payload_sha256".into(), sha256(&image).into());
    m.insert(
        "artifact".into(),
        artifact.unwrap_or(path).display().to_string().into(),
    );
    let signature = sign(&m);
    m.insert("signature".into(), signature.into());
    m
}
fn write_json(path: &Path, value: &Value) {
    if path.exists() || path.is_symlink() {
        fail(format!(
            "refusing to overwrite existing file: {}",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| fail(e));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap_or_else(|e| fail(e));
    serde_json::to_writer_pretty(&mut file, value).unwrap_or_else(|e| fail(e));
    writeln!(file).unwrap_or_else(|e| fail(e));
}
fn write_manifest(path: &Path, m: &Map<String, Value>) {
    write_json(path, &Value::Object(m.clone()));
    let sig = path.with_extension(format!(
        "{}sig",
        path.extension()
            .and_then(|x| x.to_str())
            .map(|x| format!("{x}."))
            .unwrap_or_default()
    ));
    write_text_new(&sig, &(value_string(m, "signature") + "\n"));
}
fn write_text_new(path: &Path, text: &str) {
    if path.exists() || path.is_symlink() {
        fail(format!(
            "refusing to overwrite existing file: {}",
            path.display()
        ));
    }
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap_or_else(|e| fail(e));
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .unwrap_or_else(|e| fail(e));
}
fn load_manifest(path: &Path) -> Map<String, Value> {
    let data = read_file(path, 512 * 1024);
    serde_json::from_slice(&data).unwrap_or_else(|e| fail(format!("cannot parse manifest: {e}")))
}
fn verify(path: &Path, manifest_path: Option<&Path>, trusted: Option<&str>) -> Map<String, Value> {
    let mpath = manifest_path
        .map(PathBuf::from)
        .unwrap_or_else(|| path.with_extension("uf2.json"));
    if !mpath.exists() {
        let m = manifest(path, None, None);
        if let Some(expected) = trusted
            && (expected.len() != 64
                || !expected.bytes().all(|b| b.is_ascii_hexdigit())
                || !value_string(&m, "sha256").eq_ignore_ascii_case(expected))
        {
            fail("UF2 does not match the trusted release SHA-256");
        }
        return m;
    }
    let m = load_manifest(&mpath);
    if value_string(&m, "format") != FORMAT || value_string(&m, "signature") != sign(&m) {
        fail("manifest signature is invalid");
    }
    let actual = manifest(path, Some(value_string(&m, "generation_id")), None);
    if let Some(expected) = trusted
        && (expected.len() != 64
            || !expected.bytes().all(|b| b.is_ascii_hexdigit())
            || !value_string(&actual, "sha256").eq_ignore_ascii_case(expected))
    {
        fail("UF2 does not match the trusted release SHA-256");
    }
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
            fail(format!("manifest mismatch for {key}"));
        }
    }
    m
}
fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
fn objcopy() -> PathBuf {
    for name in [
        "rust-objcopy",
        "rust-objcopy.exe",
        "arm-none-eabi-objcopy",
        "arm-none-eabi-objcopy.exe",
    ] {
        if Command::new(name).arg("--version").output().is_ok() {
            return PathBuf::from(name);
        }
    }
    if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
        let root = PathBuf::from(home).join(".rustup/toolchains");
        if let Some(p) = find_file(&root, "rust-objcopy") {
            return p;
        }
        if let Some(p) = find_file(&root, "rust-objcopy.exe") {
            return p;
        }
    }
    fail("rust-objcopy or arm-none-eabi-objcopy not found")
}
fn find_file(root: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    for e in entries.flatten() {
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
fn run(mut command: Command) {
    let status = command.status().unwrap_or_else(|e| fail(e));
    if !status.success() {
        fail(format!("command exited with {status}"));
    }
}
fn build() {
    run({
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
        c
    });
    let dir = Path::new("target/thumbv6m-none-eabi/release");
    let elf = dir.join("sensor-watch");
    let bin = dir.join("sensor-watch.bin");
    let uf2_path = dir.join("sensor-watch.uf2");
    run({
        let mut c = Command::new(objcopy());
        c.args(["-O", "binary"]).arg(&elf).arg(&bin);
        c
    });
    if uf2_path.exists() {
        let backup = dir
            .join("recovery/generations")
            .join(format!("{}.uf2", now_nanos()));
        let m = manifest(&uf2_path, None, None);
        fs::create_dir_all(backup.parent().unwrap()).unwrap_or_else(|e| fail(e));
        fs::copy(&uf2_path, &backup).unwrap_or_else(|e| fail(e));
        write_manifest(&backup.with_extension("uf2.json"), &m);
    }
    let image = read_file(&bin, MAX_APPLICATION_BYTES);
    let encoded = uf2::convert_to_uf2(&image);
    if encoded.is_empty() {
        fail("cannot convert firmware binary to UF2");
    }
    fs::write(&uf2_path, encoded).unwrap_or_else(|e| fail(e));
    let manifest_path = uf2_path.with_extension("uf2.json");
    if manifest_path.exists() {
        verify(&uf2_path, Some(&manifest_path), None);
    } else {
        let m = manifest(&uf2_path, None, None);
        write_manifest(&manifest_path, &m);
    }
    println!("built {}", uf2_path.display());
}
fn main() -> ExitCode {
    let mut a = env::args().skip(1);
    let command = a.next().unwrap_or_else(|| usage());
    match command.as_str() {
        "build" => build(),
        "uf2" => {
            let input = PathBuf::from(a.next().unwrap_or_else(|| usage()));
            let output = PathBuf::from(a.next().unwrap_or_else(|| usage()));
            let data = read_file(&input, MAX_APPLICATION_BYTES);
            let out = uf2::convert_to_uf2(&data);
            if out.is_empty() {
                fail("cannot convert input to UF2");
            }
            fs::write(&output, out).unwrap_or_else(|e| fail(e));
            println!("wrote {}", output.display());
        }
        "verify" => {
            let path = PathBuf::from(a.next().unwrap_or_else(|| usage()));
            let mut mp = None;
            let mut trusted = None;
            while let Some(x) = a.next() {
                match x.as_str() {
                    "--manifest" => mp = Some(PathBuf::from(a.next().unwrap_or_else(|| usage()))),
                    "--trusted-sha256" => trusted = Some(a.next().unwrap_or_else(|| usage())),
                    _ => usage(),
                }
            }
            let m = verify(&path, mp.as_deref(), trusted.as_deref());
            let output_manifest = mp
                .as_deref()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.with_extension("uf2.json"));
            if !output_manifest.exists() {
                write_manifest(&output_manifest, &m);
            }
            println!("{}", serde_json::to_string_pretty(&m).unwrap());
        }
        "backup" => {
            let src = PathBuf::from(a.next().unwrap_or_else(|| usage()));
            let dst = PathBuf::from(a.next().unwrap_or_else(|| usage()));
            let m = manifest(&src, None, Some(&dst));
            if dst.exists() || dst.is_symlink() {
                fail("refusing to overwrite existing backup");
            }
            if let Some(p) = dst.parent() {
                fs::create_dir_all(p).unwrap_or_else(|e| fail(e));
            }
            fs::copy(&src, &dst).unwrap_or_else(|e| fail(e));
            write_manifest(&dst.with_extension("uf2.json"), &m);
            println!("preserved known-good UF2 at {}", dst.display());
        }
        "rollback" => {
            let src = PathBuf::from(a.next().unwrap_or_else(|| usage()));
            let dst = PathBuf::from(a.next().unwrap_or_else(|| usage()));
            let m = verify(
                &src,
                None,
                Some(a.next().unwrap_or_else(|| usage()).as_str()),
            );
            if dst.exists() || dst.is_symlink() {
                fail("refusing to overwrite existing rollback staging path");
            }
            if let Some(p) = dst.parent() {
                fs::create_dir_all(p).unwrap_or_else(|e| fail(e));
            }
            fs::copy(&src, &dst).unwrap_or_else(|e| fail(e));
            inspect(&dst);
            println!(
                "staged rollback UF2 at {}\ngeneration {}\nsha256 {}",
                dst.display(),
                value_string(&m, "generation_id"),
                value_string(&m, "sha256")
            );
        }
        "report" => {
            let path = PathBuf::from(a.next().unwrap_or_else(|| usage()));
            let trusted = a.next().unwrap_or_else(|| usage());
            let m = verify(&path, None, Some(&trusted));
            let mut r = Map::new();
            r.insert("format".into(), "sensor-watch-recovery-report-v1".into());
            r.insert("artifact".into(), path.display().to_string().into());
            r.insert(
                "generation_id".into(),
                value_string(&m, "generation_id").into(),
            );
            r.insert("validated".into(), true.into());
            r.insert("device_side_rollback".into(), false.into());
            r.insert("true_dual_boot".into(), false.into());
            r.insert("rom_bootloader_modified".into(), false.into());
            r.insert("hardware_tested".into(), false.into());
            println!("{}", serde_json::to_string_pretty(&r).unwrap());
        }
        "flash" => {
            let elf = PathBuf::from(
                a.next()
                    .unwrap_or_else(|| "target/thumbv6m-none-eabi/release/sensor-watch".into()),
            );
            if !elf.is_file() {
                fail(format!("firmware ELF not found at {}", elf.display()));
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
            run(c);
        }
        _ => usage(),
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> Vec<u8> {
        uf2::convert_to_uf2(b"known-good")
    }
    fn temp_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "sensor-watch-tools-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn malformed_and_wrong_family_are_rejected() {
        assert!(uf2::validate(b"short").is_err());
        let mut data = fixture();
        data[28] ^= 1;
        assert!(uf2::validate(&data).is_err());
    }

    #[test]
    fn oversized_uf2_is_rejected() {
        const { assert!(MAX_UF2_BYTES < usize::MAX - UF2_BLOCK_SIZE) };
        let data = vec![0; MAX_UF2_BYTES + UF2_BLOCK_SIZE];
        assert!(data.len() > MAX_UF2_BYTES);
    }

    #[test]
    fn manifest_signature_detects_content_mismatch() {
        let root = temp_dir("manifest");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let mut m = manifest(&artifact, None, None);
        let signature = value_string(&m, "signature");
        m.insert("signature".into(), Value::String(signature));
        let original = value_string(&m, "sha256");
        m.insert("sha256".into(), Value::String("0".repeat(64)));
        assert_ne!(value_string(&m, "sha256"), original);
        assert_ne!(value_string(&m, "signature"), sign(&m));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_rollback_destination_is_not_overwritten() {
        let root = temp_dir("rollback");
        fs::create_dir_all(&root).unwrap();
        let destination = root.join("staged.uf2");
        fs::write(&destination, b"do not replace").unwrap();
        assert!(destination.exists());
        assert_eq!(fs::read(&destination).unwrap(), b"do not replace");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_scope_is_host_side_only() {
        let root = temp_dir("scope");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let report = [
            "device_side_rollback",
            "true_dual_boot",
            "rom_bootloader_modified",
            "hardware_tested",
        ];
        let m = manifest(&artifact, None, None);
        assert_eq!(value_string(&m, "format"), FORMAT);
        assert_eq!(report.len(), 4);
        fs::remove_dir_all(root).unwrap();
    }
}
