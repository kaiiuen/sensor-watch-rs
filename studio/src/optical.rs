//! Studio-only optical protocol preview.
//!
//! This exercises framing in memory only. It does not access a camera, GPIO,
//! ADC, serial port, or watch hardware.

use sensor_watch_core::optical::{
    self, AuthenticationHook, CommandType, Decoder, AUTH_TAG_LEN, MAX_FRAME_LEN,
};

const PREVIEW_AUTH_TAG: [u8; AUTH_TAG_LEN] = [0xC3; AUTH_TAG_LEN];

struct PreviewAuthentication;

impl AuthenticationHook for PreviewAuthentication {
    fn verify(&self, _authenticated_part: &[u8], tag: &[u8; AUTH_TAG_LEN]) -> bool {
        tag == &PREVIEW_AUTH_TAG
    }
}

#[allow(dead_code)]
pub fn preview_time_sync(sequence: u32, unix_seconds: u64) -> String {
    let mut payload = [0u8; 8];
    payload.copy_from_slice(&unix_seconds.to_be_bytes());
    let mut bytes = [0u8; MAX_FRAME_LEN];
    match optical::encode_authenticated(
        CommandType::TimeSync,
        sequence,
        &payload,
        &PREVIEW_AUTH_TAG,
        &mut bytes,
    ) {
        Ok(len) => format!("software preview: encoded {len} bytes; no hardware"),
        Err(error) => format!("software preview failed: {error:?}"),
    }
}

pub fn self_test() -> String {
    let mut bytes = [0u8; MAX_FRAME_LEN];
    let len = match optical::encode_authenticated(
        CommandType::TimeSync,
        42,
        b"preview",
        &PREVIEW_AUTH_TAG,
        &mut bytes,
    ) {
        Ok(len) => len,
        Err(error) => return format!("codec error: {error:?}"),
    };
    let mut decoder = Decoder::new();
    let authentication = PreviewAuthentication;
    let mut decoded = None;
    for byte in bytes[..len].iter().copied() {
        decoded = decoder.push(byte, 1, Some(&authentication));
    }
    match decoded {
        Some(Ok(frame)) if frame.sequence == 42 && frame.payload() == b"preview" => {
            "PASS: optical framing/CRC/replay guard exercised in memory; no hardware".to_string()
        }
        _ => "FAIL: optical software preview codec test".to_string(),
    }
}
