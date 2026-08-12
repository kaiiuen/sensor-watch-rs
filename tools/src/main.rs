use sensor_watch_tools::{self as tools, Manifest};
use std::{env, path::PathBuf, process::ExitCode};

fn usage() -> ! {
    eprintln!("usage: sensor-watch-tools <build|uf2|verify|backup|rollback|report|flash> ...");
    std::process::exit(2)
}
fn help() {
    println!("usage: sensor-watch-tools <build|uf2|verify|backup|rollback|report|flash> ...");
}
fn ensure_no_extra(args: &mut impl Iterator<Item = String>) {
    if let Some(extra) = args.next() {
        eprintln!("error: unexpected argument: {extra} (try --help)");
        std::process::exit(2);
    }
}
fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1)
}
fn required(args: &mut impl Iterator<Item = String>) -> String {
    args.next().unwrap_or_else(|| usage())
}
fn print_manifest(manifest: &Manifest) {
    println!("{}", serde_json::to_string_pretty(manifest).unwrap());
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| usage());
    match command.as_str() {
        "help" | "--help" | "-h" => {
            help();
        }
        "build" => {
            ensure_no_extra(&mut args);
            println!(
                "built {}",
                tools::build_firmware()
                    .unwrap_or_else(|e| fail(e))
                    .uf2_path
                    .display()
            )
        }
        "uf2" => {
            let input = PathBuf::from(required(&mut args));
            let output = PathBuf::from(required(&mut args));
            ensure_no_extra(&mut args);
            tools::convert_uf2(&input, &output).unwrap_or_else(|e| fail(e));
            println!("wrote {}", output.display());
        }
        "verify" => {
            let path = PathBuf::from(required(&mut args));
            let mut manifest = None;
            let mut trusted = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--manifest" if manifest.is_none() => {
                        manifest = Some(PathBuf::from(required(&mut args)))
                    }
                    "--trusted-sha256" if trusted.is_none() => trusted = Some(required(&mut args)),
                    _ => usage(),
                }
            }
            let result = tools::verify_uf2(&path, manifest.as_deref(), trusted.as_deref())
                .unwrap_or_else(|e| fail(e));
            let output = manifest.unwrap_or_else(|| path.with_extension("uf2.json"));
            if !output.exists() {
                tools::write_manifest(&output, &result).unwrap_or_else(|e| fail(e));
            }
            print_manifest(&result);
        }
        "backup" => {
            let src = PathBuf::from(required(&mut args));
            let dst = PathBuf::from(required(&mut args));
            ensure_no_extra(&mut args);
            tools::backup_uf2(&src, &dst).unwrap_or_else(|e| fail(e));
            println!("preserved known-good UF2 at {}", dst.display());
        }
        "rollback" => {
            let src = PathBuf::from(required(&mut args));
            let dst = PathBuf::from(required(&mut args));
            let trusted = required(&mut args);
            ensure_no_extra(&mut args);
            let m = tools::rollback_uf2(&src, &dst, &trusted).unwrap_or_else(|e| fail(e));
            println!(
                "staged rollback UF2 at {}\ngeneration {}\nsha256 {}",
                dst.display(),
                tools::manifest_value(&m, "generation_id"),
                tools::manifest_value(&m, "sha256")
            );
        }
        "report" => {
            let path = PathBuf::from(required(&mut args));
            let trusted = required(&mut args);
            ensure_no_extra(&mut args);
            let report = tools::recovery_report(&path, &trusted).unwrap_or_else(|e| fail(e));
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "flash" => {
            let elf = PathBuf::from(
                args.next()
                    .unwrap_or_else(|| "target/thumbv6m-none-eabi/release/sensor-watch".into()),
            );
            ensure_no_extra(&mut args);
            tools::flash_firmware(&elf).unwrap_or_else(|e| fail(e));
        }
        _ => usage(),
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use sensor_watch_core::uf2;
    use sensor_watch_tools::MAX_UF2_BYTES;
    use serde_json::Value;
    use std::fs;
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
        const { assert!(MAX_UF2_BYTES < usize::MAX - sensor_watch_core::uf2::UF2_BLOCK_SIZE) };
        let data = vec![0; MAX_UF2_BYTES + sensor_watch_core::uf2::UF2_BLOCK_SIZE];
        assert!(data.len() > MAX_UF2_BYTES);
    }
    #[test]
    fn manifest_signature_detects_content_mismatch() {
        let root = temp_dir("manifest");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let mut m = tools::create_manifest(&artifact, None, None).unwrap();
        let signature = tools::manifest_value(&m, "signature");
        m.insert("signature".into(), Value::String(signature));
        let original = tools::manifest_value(&m, "sha256");
        m.insert("sha256".into(), Value::String("0".repeat(64)));
        assert_ne!(tools::manifest_value(&m, "sha256"), original);
        assert_ne!(
            tools::manifest_value(&m, "signature"),
            tools::sign_manifest(&m)
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn verification_rejects_a_tampered_signature_sidecar() {
        let root = temp_dir("sidecar");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let manifest_path = root.join("good.uf2.json");
        let manifest = tools::create_manifest(&artifact, None, None).unwrap();
        tools::write_manifest(&manifest_path, &manifest).unwrap();
        let sidecar = manifest_path.with_extension("json.sig");
        fs::write(&sidecar, b"sha256:tampered\n").unwrap();
        assert!(
            tools::verify_uf2(&artifact, Some(&manifest_path), None)
                .unwrap_err()
                .contains("sidecar")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_rollback_destination_is_not_overwritten() {
        let root = temp_dir("rollback");
        fs::create_dir_all(&root).unwrap();
        let destination = root.join("staged.uf2");
        fs::write(&destination, b"do not replace").unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"do not replace");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn manifest_scope_is_host_side_only() {
        let root = temp_dir("scope");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let m = tools::create_manifest(&artifact, None, None).unwrap();
        assert_eq!(tools::manifest_value(&m, "format"), tools::MANIFEST_FORMAT);
        assert_eq!(
            4,
            [
                "device_side_rollback",
                "true_dual_boot",
                "rom_bootloader_modified",
                "hardware_tested"
            ]
            .len()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
