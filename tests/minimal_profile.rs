//! Host-side contract tests for the Developer-only minimal profile.

use sensor_watch_core::{board, lite_test};
use std::{fs, path::PathBuf};

fn root_file(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name);
    fs::read_to_string(path).unwrap()
}

#[test]
fn profile_is_opt_in_and_exclusive() {
    let manifest = root_file("Cargo.toml");
    assert!(manifest.contains("minimal-usb = [\"usb-enum\"]"));

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

#[test]
fn lite_protocol_is_read_only_bounded_and_host_mocked() {
    use lite_test::{Command, MAX_RESPONSE, Status, format_response};
    assert_eq!(Command::parse(b"test all\r\n"), Some(Command::All));
    assert!(Command::parse(b"erase 127").is_none());
    let mut response = [0; MAX_RESPONSE];
    let length = format_response(Command::Status, Status::fresh(), &mut response);
    let text = std::str::from_utf8(&response[..length]).unwrap();
    assert!(text.contains("identity=UNKNOWN"));
    assert!(text.contains("bulk=FAIL-CLOSED"));
    assert!(text.len() <= MAX_RESPONSE);
}

#[test]
fn lite_profile_maps_only_red_a1_02_without_lcd_ownership() {
    let mapping =
        board::lite_test_lookup(board::BoardId::RedLite, board::RevisionId::SwatA1_02).unwrap();
    assert_eq!(mapping.red_led, board::PinId::new(0, 20));
    assert_eq!(mapping.green_led, board::PinId::new(0, 21));
    assert!(mapping.leds_active_low);
    assert!(board::lite_test_lookup(board::BoardId::Green, board::RevisionId::SwatA1_05).is_none());
}

#[test]
fn usb_register_and_control_stage_contract_is_explicit() {
    let source = root_file("src/watch/usb.rs");
    for expected in [
        "OSCCTRL_DFLLCTRL_OFFSET: usize = 0x18",
        "OSCCTRL_DFLLVAL_OFFSET: usize = 0x1c",
        "OSCCTRL_DFLLMUL_OFFSET: usize = 0x20",
        "OSCCTRL_DFLLSYNC_OFFSET: usize = 0x24",
        "GCLK_SOURCE_DFLL48M: u32 = 7 << 8",
        "USB_DADD_OFFSET: usize = 0x00a",
        "DFLL_STATUS_READY: u32 = 1 << 8",
        "DFLL_STATUS_LOCK_FINE: u32 = 1 << 10",
        "DFLL_STATUS_LOCK_COARSE: u32 = 1 << 11",
        "complete_status()",
        "w8(USB_DADD_OFFSET",
        "reset_software_state();",
        "USB_EPINT_ERRORS",
        "PORT_DIRCLR_OFFSET: usize = 0x04",
        "configure it before sampling the cable state",
    ] {
        assert!(
            source.contains(expected),
            "missing USB contract: {expected}"
        );
    }
}

#[test]
fn usb_transport_is_developer_only_and_hardware_fail_closed() {
    let manifest = root_file("Cargo.toml");
    let source = root_file("src/watch/usb.rs");
    assert!(manifest.contains("minimal-usb = [\"usb-enum\"]"));
    assert!(source.contains("BULK_HARDWARE_PROVEN: bool = false"));
    assert!(source.contains("CDC_QUEUE_DEPTH"));
    assert!(source.contains("CDC_REQ_SET_LINE_CODING"));
    assert!(source.contains("pub fn take_tx_packet"));
    assert!(source.contains("pub fn next_command"));
}
