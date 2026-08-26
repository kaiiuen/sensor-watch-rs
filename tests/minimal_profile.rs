//! Host-side contract tests for the Developer-only minimal profile.

use std::{fs, path::PathBuf};

fn root_file(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name);
    fs::read_to_string(path).unwrap()
}

#[test]
fn profile_is_opt_in_and_exclusive() {
    let manifest = root_file("Cargo.toml");
    assert!(manifest.contains("minimal-usb = []"));

    assert!(manifest.contains("required-features = [\"minimal-usb\"]"));
    let source = root_file("src/minimal_usb.rs");
    for feature in [
        "optical",
        "pro-irda-rx",
        "shell-auth",
        "usb-cdc",
        "defmt-log",
    ] {
        assert!(source.contains(&format!("feature = \"{feature}\"")));
    }
}

#[test]
fn minimal_boundary_preserves_bootloader_range_and_proof_of_life() {
    let source = root_file("src/minimal_usb.rs");
    assert!(source.contains("APP_START: usize = 0x0000_2000"));
    assert!(source.contains("APP_END: usize = 0x0003_C000"));
    assert!(source.contains("PROOF_OF_LIFE"));
    assert!(source.contains("init_wdt"));
    assert!(source.contains("init_clock"));
    assert!(source.contains("record_reset_and_fault"));
    assert!(source.contains("identity"));

    assert!(source.contains("fail closed"));
}
