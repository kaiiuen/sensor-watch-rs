//! Studio-only optical protocol preview.
//!
//! This exercises framing in memory only. It does not access a camera, GPIO,
//! ADC, serial port, or watch hardware.

use sensor_watch_core::optical::{self, CommandType, Decoder, MAX_FRAME_LEN};

pub fn preview_time_sync(sequence: u32, unix_seconds: u64) -> String {
    let mut payload = [0u8; 8];
    payload.copy_from_slice(&unix_seconds.to_be_bytes());
    let mut bytes = [0u8; MAX_FRAME_LEN];
    match optical::encode(CommandType::TimeSync, sequence, &payload, &mut bytes) {
        Ok(len) => format!("software preview: encoded {len} bytes; no hardware"),
        Err(error) => format!("software preview failed: {error:?}"),
    }
}

pub fn self_test() -> String {
    let mut bytes = [0u8; MAX_FRAME_LEN];
    let len = match optical::encode(CommandType::TimeSync, 42, b"preview", &mut bytes) {
        Ok(len) => len,
        Err(error) => return format!("codec error: {error:?}"),
    };
    let mut decoder = Decoder::new();
    let mut decoded = None;
    for byte in bytes[..len].iter().copied() {
        decoded = decoder.push(byte, 1, None);
    }
    match decoded {
        Some(Ok(frame)) if frame.sequence == 42 && frame.payload() == b"preview" => {
            "PASS: optical framing/CRC/replay guard exercised in memory; no hardware".to_string()
        }
        _ => "FAIL: optical software preview codec test".to_string(),
    }
}
