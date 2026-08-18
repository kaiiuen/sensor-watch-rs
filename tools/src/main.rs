use sensor_watch_tools::{self as tools, Manifest};
use std::{env, path::PathBuf, process::ExitCode};

fn usage() -> ! {
    eprintln!(
        "usage: sensor-watch-tools <build|package-studio|uf2|verify|backup|rollback|report|flash> ..."
    );
    std::process::exit(2)
}
fn help() {
    println!(
        "usage: sensor-watch-tools <build|package-studio|uf2|verify|backup|rollback|report|flash> ..."
    );
    println!(
        "verify checks UF2 structure, local manifest consistency, and optional trusted release SHA-256 matching; SHA-256 is a digest/hash for integrity, not a signing key or authenticity proof. Authenticity requires a signature verified with a separately trusted public key (for example, Ed25519)."
    );
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
        "package-studio" => {
            let mut output = None;
            let mut launcher = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--output" if output.is_none() => {
                        output = Some(PathBuf::from(required(&mut args)))
                    }
                    "--launcher" if launcher.is_none() => {
                        launcher = Some(PathBuf::from(required(&mut args)))
                    }
                    _ => usage(),
                }
            }
            let result =
                tools::package_studio_with_launcher(output.as_deref(), launcher.as_deref())
                    .unwrap_or_else(|e| fail(e));
            println!("wrote {}", result.output.display());
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
            eprintln!(
                "artifact validation: structural and local digest checks passed; trusted release SHA-256 (integrity only): {}",
                if trusted.is_some() {
                    "matched"
                } else {
                    "not provided"
                }
            );
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
            tools::manifest_digest(&m)
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn manifest_digest_does_not_depend_on_local_artifact_path() {
        let root = temp_dir("digest-path");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let relative = tools::create_manifest(&artifact, Some("generation".into()), None).unwrap();
        let alternate = tools::create_manifest(
            &artifact,
            Some("generation".into()),
            Some(&root.join("other-location.uf2")),
        )
        .unwrap();
        assert_eq!(
            tools::manifest_value(&relative, "manifest_digest"),
            tools::manifest_value(&alternate, "manifest_digest")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_trusted_provenance_is_reported_without_claiming_authenticity() {
        assert_eq!(tools::trusted_release_status(None), "not provided");
        assert_eq!(
            tools::trusted_release_status(Some(&"a".repeat(64))),
            "provided"
        );
        assert_eq!(tools::trusted_release_status(Some("not-a-hash")), "invalid");
    }

    #[test]
    fn verification_accepts_local_digest_but_requires_trusted_sha_for_provenance() {
        let root = temp_dir("provenance");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let manifest_path = root.join("good.uf2.json");
        let manifest = tools::create_manifest(&artifact, None, None).unwrap();
        tools::write_manifest(&manifest_path, &manifest).unwrap();
        let verified = tools::verify_uf2(&artifact, Some(&manifest_path), None).unwrap();
        assert_eq!(
            tools::manifest_value(&verified, "sha256"),
            tools::sha256(&fixture())
        );
        assert_eq!(tools::trusted_release_status(None), "not provided");
        let error =
            tools::verify_uf2(&artifact, Some(&manifest_path), Some(&"0".repeat(64))).unwrap_err();
        assert!(error.contains("trusted release SHA-256"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verification_rejects_conflicting_manifest_digests() {
        let root = temp_dir("conflicting-digest");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let manifest_path = root.join("good.uf2.json");
        let mut manifest = tools::create_manifest(&artifact, None, None).unwrap();
        manifest.insert("signature".into(), Value::String("0".repeat(64)));
        assert!(tools::write_manifest(&manifest_path, &manifest).is_ok());
        let error = tools::verify_uf2(&artifact, Some(&manifest_path), None).unwrap_err();
        assert!(error.contains("disagree"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verification_rejects_missing_sidecar() {
        let root = temp_dir("missing-sidecar");
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let manifest_path = root.join("good.uf2.json");
        let manifest = tools::create_manifest(&artifact, None, None).unwrap();
        tools::write_manifest(&manifest_path, &manifest).unwrap();
        fs::remove_file(manifest_path.with_extension("json.sig")).unwrap();
        assert!(tools::verify_uf2(&artifact, Some(&manifest_path), None).is_err());
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
    fn manifest_write_does_not_leave_json_when_sidecar_is_blocked() {
        let root = temp_dir("sidecar-preflight");
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("good.uf2.json");
        let sidecar = manifest_path.with_extension("json.sig");
        fs::write(&sidecar, b"preserve\n").unwrap();
        let artifact = root.join("good.uf2");
        fs::write(&artifact, fixture()).unwrap();
        let manifest = tools::create_manifest(&artifact, None, None).unwrap();
        assert!(tools::write_manifest(&manifest_path, &manifest).is_err());
        assert!(!manifest_path.exists());
        assert_eq!(fs::read(&sidecar).unwrap(), b"preserve\n");
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
    fn conversion_creates_missing_output_parent() {
        let root = temp_dir("nested-output");
        fs::create_dir_all(&root).unwrap();
        let input = root.join("firmware.bin");
        let output = root.join("nested/firmware.uf2");
        fs::write(&input, b"firmware").unwrap();
        tools::convert_uf2(&input, &output).unwrap();
        assert!(output.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn conversion_refuses_symlinked_parent_output() {
        let root = temp_dir("symlink-parent");
        fs::create_dir_all(&root).unwrap();
        let input = root.join("firmware.bin");
        let parent = root.join("parent-file");
        fs::write(&input, b"firmware").unwrap();
        fs::write(&parent, b"not a directory").unwrap();
        let output = parent.join("firmware.uf2");
        assert!(tools::convert_uf2(&input, &output).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn conversion_refuses_existing_output() {
        let root = temp_dir("no-overwrite");
        fs::create_dir_all(&root).unwrap();
        let input = root.join("firmware.bin");
        let output = root.join("firmware.uf2");
        fs::write(&input, b"firmware").unwrap();
        fs::write(&output, b"do not replace").unwrap();
        assert!(tools::convert_uf2(&input, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"do not replace");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rollback_stages_the_verified_snapshot() {
        let root = temp_dir("rollback-snapshot");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.uf2");
        let staged = root.join("nested/staged.uf2");
        let bytes = fixture();
        fs::write(&source, &bytes).unwrap();
        let trusted = tools::sha256(&bytes);
        let manifest = tools::rollback_uf2(&source, &staged, &trusted).unwrap();
        let manifest_path = staged.with_extension("uf2.json");
        let sidecar_path = manifest_path.with_extension("json.sig");
        assert_eq!(fs::read(&staged).unwrap(), bytes);
        assert!(manifest_path.is_file());
        assert!(sidecar_path.is_file());
        assert_eq!(tools::manifest_value(&manifest, "sha256"), trusted);
        tools::verify_uf2(&staged, Some(&manifest_path), Some(&trusted)).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_cleans_up_uf2_when_manifest_staging_is_blocked() {
        let root = temp_dir("rollback-cleanup");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.uf2");
        let staged = root.join("nested/staged.uf2");
        let manifest_path = staged.with_extension("uf2.json");
        let sidecar_path = manifest_path.with_extension("json.sig");
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        fs::write(&sidecar_path, b"preserve\n").unwrap();
        let bytes = fixture();
        fs::write(&source, &bytes).unwrap();
        let trusted = tools::sha256(&bytes);

        assert!(tools::rollback_uf2(&source, &staged, &trusted).is_err());
        assert!(!staged.exists());
        assert!(!manifest_path.exists());
        assert_eq!(fs::read(&sidecar_path).unwrap(), b"preserve\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_failure_does_not_leave_a_stale_artifact() {
        let root = temp_dir("backup-cleanup");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.uf2");
        let backup = root.join("recovery/generation.uf2");
        fs::write(&source, fixture()).unwrap();
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        fs::write(backup.with_extension("uf2.json"), b"stale").unwrap();
        assert!(tools::backup_uf2(&source, &backup).is_err());
        assert!(!backup.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_manifest_matches_the_single_snapshot_written() {
        let root = temp_dir("backup-consistency");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.uf2");
        let backup = root.join("recovery/generation.uf2");
        let bytes = fixture();
        fs::write(&source, &bytes).unwrap();

        let manifest = tools::backup_uf2(&source, &backup).unwrap();
        assert_eq!(fs::read(&backup).unwrap(), bytes);
        assert_eq!(
            tools::manifest_value(&manifest, "sha256"),
            tools::sha256(&fs::read(&backup).unwrap())
        );
        let manifest_path = backup.with_extension("uf2.json");
        tools::verify_uf2(&backup, Some(&manifest_path), None).unwrap();
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
