//! Software-only optical command and time-sync framing.
//!
//! This module deliberately contains no GPIO/ADC access and uses only fixed-size
//! buffers. The production board has no proven light sensor: LIGHT is a button,
//! and an optical receiver needs an external accessory ADC. Firmware integration
//! therefore stays disabled until a board-specific receiver is proven.

#![allow(clippy::result_unit_err)]

pub const PREAMBLE: [u8; 2] = [0xA5, 0x5A];
pub const VERSION: u8 = 1;
pub const MAX_PAYLOAD: usize = 32;
pub const AUTH_TAG_LEN: usize = 8;
pub const HEADER_LEN: usize = 2 + 1 + 1 + 1 + 4;
pub const CRC_LEN: usize = 2;
pub const MAX_FRAME_LEN: usize = HEADER_LEN + MAX_PAYLOAD + AUTH_TAG_LEN + CRC_LEN;
pub const RX_TIMEOUT_MS: u32 = 100;
pub const DUTY_WINDOW_MS: u32 = 1_000;
pub const MAX_FRAMES_PER_WINDOW: u8 = 4;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandType {
    TimeSync = 1,
    TimeQuery = 2,
    Status = 3,
}

impl CommandType {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::TimeSync),
            2 => Some(Self::TimeQuery),
            3 => Some(Self::Status),
            // Deliberately no firmware-update, erase, unlock, or actuator command.
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub command: CommandType,
    pub sequence: u32,
    pub payload: [u8; MAX_PAYLOAD],
    pub payload_len: usize,
}

impl Frame {
    pub const fn empty() -> Self {
        Self {
            command: CommandType::Status,
            sequence: 0,
            payload: [0; MAX_PAYLOAD],
            payload_len: 0,
        }
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload[..self.payload_len]
    }
}

pub trait AuthenticationHook {
    /// Verifies the optional tag over the header and payload (including preamble).
    fn verify(&self, authenticated_part: &[u8], tag: &[u8; AUTH_TAG_LEN]) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    PayloadTooLarge,
    OutputTooSmall,
    UnsupportedCommand,
    AuthenticationRequired,
    AuthenticationNotAllowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Timeout,
    FrameTooLong,
    UnsupportedVersion,
    UnsupportedCommand,
    InvalidLength,
    Crc,
    Authentication,
    Replay,
    DutyCycle,
}

pub fn encode(
    command: CommandType,
    sequence: u32,
    payload: &[u8],
    output: &mut [u8; MAX_FRAME_LEN],
) -> Result<usize, EncodeError> {
    encode_inner(command, sequence, payload, None, output)
}

pub fn encode_authenticated(
    command: CommandType,
    sequence: u32,
    payload: &[u8],
    tag: &[u8; AUTH_TAG_LEN],
    output: &mut [u8; MAX_FRAME_LEN],
) -> Result<usize, EncodeError> {
    encode_inner(command, sequence, payload, Some(tag), output)
}

fn encode_inner(
    command: CommandType,
    sequence: u32,
    payload: &[u8],
    tag: Option<&[u8; AUTH_TAG_LEN]>,
    output: &mut [u8; MAX_FRAME_LEN],
) -> Result<usize, EncodeError> {
    if payload.len() > MAX_PAYLOAD {
        return Err(EncodeError::PayloadTooLarge);
    }
    let requires_auth = matches!(command, CommandType::TimeSync);
    if requires_auth && tag.is_none() {
        return Err(EncodeError::AuthenticationRequired);
    }
    if !requires_auth && tag.is_some() {
        return Err(EncodeError::AuthenticationNotAllowed);
    }
    let auth_len = tag.map_or(0, |_| AUTH_TAG_LEN);
    let length = payload.len() + auth_len;
    let total = HEADER_LEN + length + CRC_LEN;
    if output.len() < total {
        return Err(EncodeError::OutputTooSmall);
    }
    output[..2].copy_from_slice(&PREAMBLE);
    output[2] = VERSION;
    output[3] = command as u8;
    output[4] = length as u8;
    output[5..9].copy_from_slice(&sequence.to_be_bytes());
    output[9..9 + payload.len()].copy_from_slice(payload);
    if let Some(tag) = tag {
        output[9 + payload.len()..9 + length].copy_from_slice(tag);
    }
    let crc = crc16(&output[..HEADER_LEN + length]);
    output[HEADER_LEN + length..total].copy_from_slice(&crc.to_be_bytes());
    Ok(total)
}

pub fn decode(bytes: &[u8], auth: Option<&dyn AuthenticationHook>) -> Result<Frame, DecodeError> {
    if bytes.len() < HEADER_LEN + CRC_LEN {
        return Err(DecodeError::InvalidLength);
    }
    if bytes[..2] != PREAMBLE {
        return Err(DecodeError::InvalidLength);
    }
    if bytes[2] != VERSION {
        return Err(DecodeError::UnsupportedVersion);
    }
    let length = bytes[4] as usize;
    let total = HEADER_LEN + length + CRC_LEN;
    if length > MAX_PAYLOAD + AUTH_TAG_LEN || bytes.len() != total {
        return Err(DecodeError::InvalidLength);
    }
    if crc16(&bytes[..HEADER_LEN + length])
        != u16::from_be_bytes([bytes[total - 2], bytes[total - 1]])
    {
        return Err(DecodeError::Crc);
    }
    let command = CommandType::from_byte(bytes[3]).ok_or(DecodeError::UnsupportedCommand)?;
    let requires_auth = matches!(command, CommandType::TimeSync);
    let payload_len = if requires_auth {
        let hook = auth.ok_or(DecodeError::Authentication)?;
        if length < AUTH_TAG_LEN {
            return Err(DecodeError::Authentication);
        }
        let payload_len = length - AUTH_TAG_LEN;
        let tag = &bytes[HEADER_LEN + payload_len..HEADER_LEN + length];
        let tag: &[u8; AUTH_TAG_LEN] = tag.try_into().map_err(|_| DecodeError::Authentication)?;
        if !hook.verify(&bytes[..HEADER_LEN + payload_len], tag) {
            return Err(DecodeError::Authentication);
        }
        payload_len
    } else {
        length
    };
    let mut frame = Frame::empty();
    frame.command = command;
    frame.sequence = u32::from_be_bytes(
        bytes[5..9]
            .try_into()
            .map_err(|_| DecodeError::InvalidLength)?,
    );
    frame.payload_len = payload_len;
    frame.payload[..payload_len].copy_from_slice(&bytes[9..9 + payload_len]);
    Ok(frame)
}

pub struct Decoder {
    buffer: [u8; MAX_FRAME_LEN],
    len: usize,
    last_byte_ms: Option<u32>,
    window_start_ms: u32,
    frames_in_window: u8,
    last_sequence: Option<u32>,
}

impl Decoder {
    pub const fn new() -> Self {
        Self {
            buffer: [0; MAX_FRAME_LEN],
            len: 0,
            last_byte_ms: None,
            window_start_ms: 0,
            frames_in_window: 0,
            last_sequence: None,
        }
    }

    pub fn push(
        &mut self,
        byte: u8,
        now_ms: u32,
        auth: Option<&dyn AuthenticationHook>,
    ) -> Option<Result<Frame, DecodeError>> {
        if let Some(last) = self.last_byte_ms
            && now_ms.wrapping_sub(last) > RX_TIMEOUT_MS
        {
            self.reset();
            self.last_byte_ms = Some(now_ms);
            return Some(Err(DecodeError::Timeout));
        }
        self.last_byte_ms = Some(now_ms);
        if self.len == 0 {
            if byte != PREAMBLE[0] {
                return None;
            }
            self.buffer[0] = byte;
            self.len = 1;
            return None;
        }
        if self.len == 1 && byte != PREAMBLE[1] {
            self.len = 0;
            return None;
        }
        if self.len >= MAX_FRAME_LEN {
            self.reset();
            return Some(Err(DecodeError::FrameTooLong));
        }
        self.buffer[self.len] = byte;
        self.len += 1;
        if self.len < HEADER_LEN {
            return None;
        }
        let expected = HEADER_LEN + self.buffer[4] as usize + CRC_LEN;
        if expected > MAX_FRAME_LEN {
            self.reset();
            return Some(Err(DecodeError::FrameTooLong));
        }
        if self.len < expected {
            return None;
        }
        let result = decode(&self.buffer[..expected], auth)
            .and_then(|frame| self.accept_sequence(frame, now_ms));
        self.reset();
        Some(result)
    }

    fn accept_sequence(&mut self, frame: Frame, now_ms: u32) -> Result<Frame, DecodeError> {
        if now_ms.wrapping_sub(self.window_start_ms) >= DUTY_WINDOW_MS {
            self.window_start_ms = now_ms;
            self.frames_in_window = 0;
        }
        if self.frames_in_window >= MAX_FRAMES_PER_WINDOW {
            return Err(DecodeError::DutyCycle);
        }
        if let Some(last) = self.last_sequence
            && (frame.sequence == last || frame.sequence.wrapping_sub(last) > 0x8000_0000)
        {
            return Err(DecodeError::Replay);
        }
        self.frames_in_window += 1;
        self.last_sequence = Some(frame.sequence);
        Ok(frame)
    }

    fn reset(&mut self) {
        self.len = 0;
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &byte in bytes {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(seq: u32) -> ([u8; MAX_FRAME_LEN], usize) {
        let mut out = [0; MAX_FRAME_LEN];
        let len = encode(CommandType::Status, seq, b"TIME", &mut out).unwrap();
        (out, len)
    }

    #[test]
    fn round_trip_is_fixed_size_and_heap_free() {
        let (bytes, len) = encoded(7);
        let frame = decode(&bytes[..len], None).unwrap();
        assert_eq!(frame.command, CommandType::Status);
        assert_eq!(frame.sequence, 7);
        assert_eq!(frame.payload(), b"TIME");
    }

    #[test]
    fn noise_is_ignored_and_crc_is_rejected() {
        let (mut bytes, len) = encoded(1);
        let mut decoder = Decoder::new();
        assert!(decoder.push(0x00, 0, None).is_none());
        for (i, byte) in bytes[..len].iter().copied().enumerate() {
            if i == len - 1 {
                assert!(decoder.push(byte ^ 1, 1, None).unwrap().is_err());
            } else {
                decoder.push(byte, 1, None);
            }
        }
        bytes[10] ^= 1;
        assert_eq!(decode(&bytes[..len], None), Err(DecodeError::Crc));
    }

    #[test]
    fn replay_timeout_and_duty_cycle_are_rejected() {
        let (bytes, len) = encoded(2);
        let mut decoder = Decoder::new();
        let mut result = None;
        for byte in bytes[..len].iter().copied() {
            result = decoder.push(byte, 1, None);
        }
        assert!(result.unwrap().is_ok());
        let mut replay = None;
        for byte in bytes[..len].iter().copied() {
            replay = decoder.push(byte, 2, None);
        }
        assert_eq!(replay, Some(Err(DecodeError::Replay)));
        let (bytes3, len3) = encoded(3);
        for seq in 0..3 {
            let (b, l) = encoded(10 + seq);
            for byte in b[..l].iter().copied() {
                decoder.push(byte, 10, None);
            }
        }
        let mut blocked = None;
        for byte in bytes3[..len3].iter().copied() {
            blocked = decoder.push(byte, 10, None);
        }
        assert_eq!(blocked, Some(Err(DecodeError::DutyCycle)));
        let timeout = decoder.push(PREAMBLE[0], 200, None);
        assert_eq!(timeout, Some(Err(DecodeError::Timeout)));
    }

    struct AcceptAuth;

    impl AuthenticationHook for AcceptAuth {
        fn verify(&self, _authenticated_part: &[u8], tag: &[u8; AUTH_TAG_LEN]) -> bool {
            *tag == [0xC3; AUTH_TAG_LEN]
        }
    }

    #[test]
    fn state_changing_commands_require_authentication() {
        let mut bytes = [0; MAX_FRAME_LEN];
        assert_eq!(
            encode(CommandType::TimeSync, 1, b"sync", &mut bytes),
            Err(EncodeError::AuthenticationRequired)
        );
        let tag = [0xC3; AUTH_TAG_LEN];
        let len =
            encode_authenticated(CommandType::TimeSync, 1, b"sync", &tag, &mut bytes).unwrap();
        assert_eq!(
            decode(&bytes[..len], None),
            Err(DecodeError::Authentication)
        );
    }

    #[test]
    fn optional_authentication_hook_is_checked() {
        let mut bytes = [0; MAX_FRAME_LEN];
        let tag = [0xC3; AUTH_TAG_LEN];
        let len =
            encode_authenticated(CommandType::TimeSync, 9, b"sync", &tag, &mut bytes).unwrap();
        assert_eq!(
            decode(&bytes[..len], Some(&AcceptAuth)).unwrap().sequence,
            9
        );
        let mut wrong = [0; MAX_FRAME_LEN];
        let wrong_tag = [0x5A; AUTH_TAG_LEN];
        let wrong_len =
            encode_authenticated(CommandType::TimeSync, 9, b"sync", &wrong_tag, &mut wrong)
                .unwrap();
        assert_eq!(
            decode(&wrong[..wrong_len], Some(&AcceptAuth)),
            Err(DecodeError::Authentication)
        );
    }

    #[test]
    fn dangerous_command_is_not_decodable() {
        let mut bytes = [0; MAX_FRAME_LEN];
        let len = encode_inner(CommandType::Status, 1, &[], None, &mut bytes).unwrap();
        bytes[3] = 0xF0;
        let crc = crc16(&bytes[..len - CRC_LEN]);
        bytes[len - CRC_LEN..len].copy_from_slice(&crc.to_be_bytes());
        assert_eq!(
            decode(&bytes[..len], None),
            Err(DecodeError::UnsupportedCommand)
        );
    }
}
