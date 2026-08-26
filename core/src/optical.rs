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
pub const TIME_SYNC_PAYLOAD_LEN: usize = 8;
pub const MAX_TIME_SYNC_SKEW_SECONDS: u32 = 300;

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

/// Canonical UTC time-sync payload. The frame sequence is separate and is
/// checked by [`Decoder`]; freshness is the sender's UTC seconds value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeSyncPayload {
    pub packed_datetime: u32,
    pub freshness: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeSyncError {
    InvalidLength,
    InvalidDateTime,
    Stale,
    NotAuthenticated,
    NotAuthorized,
    MutationDisabled,
}

impl TimeSyncPayload {
    pub fn parse(bytes: &[u8]) -> Result<Self, TimeSyncError> {
        if bytes.len() != TIME_SYNC_PAYLOAD_LEN {
            return Err(TimeSyncError::InvalidLength);
        }
        let packed_datetime = u32::from_be_bytes(bytes[..4].try_into().unwrap());
        let freshness = u32::from_be_bytes(bytes[4..].try_into().unwrap());
        if !valid_packed_datetime(packed_datetime) {
            return Err(TimeSyncError::InvalidDateTime);
        }
        Ok(Self {
            packed_datetime,
            freshness,
        })
    }

    pub fn is_fresh(self, now: u32) -> bool {
        now.abs_diff(self.freshness) <= MAX_TIME_SYNC_SKEW_SECONDS
    }
}

/// Guards the final RTC mutation separately from framing and CRC validation.
/// Production firmware constructs this with `crypto_provisioned = false` until
/// a real key-provisioning/authentication implementation exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeSyncPolicy {
    pub crypto_provisioned: bool,
    pub authenticated: bool,
    pub physically_authorized: bool,
    pub rtc_mutation_enabled: bool,
}

impl TimeSyncPolicy {
    pub const fn receive_only() -> Self {
        Self {
            crypto_provisioned: false,
            authenticated: false,
            physically_authorized: false,
            rtc_mutation_enabled: false,
        }
    }

    pub fn authorize(self, payload: TimeSyncPayload, now: u32) -> Result<(), TimeSyncError> {
        if !payload.is_fresh(now) {
            return Err(TimeSyncError::Stale);
        }
        if !self.crypto_provisioned || !self.authenticated {
            return Err(TimeSyncError::NotAuthenticated);
        }
        if !self.physically_authorized {
            return Err(TimeSyncError::NotAuthorized);
        }
        if !self.rtc_mutation_enabled {
            return Err(TimeSyncError::MutationDisabled);
        }
        Ok(())
    }
}

fn valid_packed_datetime(reg: u32) -> bool {
    let second = reg & 0x3f;
    let minute = (reg >> 6) & 0x3f;
    let hour = (reg >> 12) & 0x1f;
    let day = (reg >> 17) & 0x1f;
    let month = (reg >> 22) & 0x0f;
    let year = (reg >> 26) & 0x3f;
    if second > 59 || minute > 59 || hour > 23 || month == 0 || month > 12 || day == 0 {
        return false;
    }
    let full_year = 2020 + year;
    let leap = full_year.is_multiple_of(4)
        && (!full_year.is_multiple_of(100) || full_year.is_multiple_of(400));
    let days = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day <= days
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
    let command = match CommandType::from_byte(bytes[3]) {
        Some(command) => command,
        None => return Err(DecodeError::UnsupportedCommand),
    };
    let requires_auth = matches!(command, CommandType::TimeSync);
    let payload_len = if requires_auth {
        let hook = match auth {
            Some(hook) => hook,
            None => return Err(DecodeError::Authentication),
        };
        if length < AUTH_TAG_LEN {
            return Err(DecodeError::Authentication);
        }
        let payload_len = length - AUTH_TAG_LEN;
        let tag = &bytes[HEADER_LEN + payload_len..HEADER_LEN + length];
        let tag: &[u8; AUTH_TAG_LEN] = match tag.try_into() {
            Ok(tag) => tag,
            Err(_) => return Err(DecodeError::Authentication),
        };
        if !hook.verify(&bytes[..HEADER_LEN + payload_len], tag) {
            return Err(DecodeError::Authentication);
        }
        payload_len
    } else {
        if length > MAX_PAYLOAD {
            return Err(DecodeError::InvalidLength);
        }
        length
    };
    let mut frame = Frame::empty();
    frame.command = command;
    let sequence: [u8; 4] = match bytes[5..9].try_into() {
        Ok(sequence) => sequence,
        Err(_) => return Err(DecodeError::InvalidLength),
    };
    frame.sequence = u32::from_be_bytes(sequence);
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
            // Preserve an overlapping first preamble byte so A5 A5 5A
            // resynchronizes to the second A5 instead of dropping the frame.
            self.len = usize::from(byte == PREAMBLE[0]);
            if self.len == 1 {
                self.buffer[0] = byte;
            }
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

/// Explicit lifecycle of one replaceable optical synchronization session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Receiving,
    Authenticated,
    Authorized,
    Applied,
    AckQueued,
    Expired,
}

/// The bounded hardware/application seam for a session.
///
/// Implementations own the physical byte source and RTC adapter. The core
/// session never creates a transmitter and never mutates an RTC unless both
/// the policy and the implementation explicitly permit it.
pub trait OpticalIo {
    fn read_byte(&mut self) -> Option<u8>;
    fn now_ms(&mut self) -> u32;
    fn queue_ack(&mut self, sequence: u32);
    fn apply_rtc(&mut self, packed_datetime: u32) -> Result<(), ()>;
}

pub const MAX_POLL_BYTES: usize = 64;
pub const DEFAULT_AUTHORIZATION_MS: u32 = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    Decode(DecodeError),
    Payload(TimeSyncError),
    AuthorizationExpired,
    RtcApply,
}

/// A fixed-storage optical session. Construct a new value per association or
/// call [`OpticalSession::reset`] before accepting another one.
pub struct OpticalSession {
    decoder: Decoder,
    policy: TimeSyncPolicy,
    state: SessionState,
    authorization_until_ms: Option<u32>,
    ack: Option<u32>,
}

impl OpticalSession {
    pub const fn new(policy: TimeSyncPolicy) -> Self {
        Self {
            decoder: Decoder::new(),
            policy,
            state: SessionState::Idle,
            authorization_until_ms: None,
            ack: None,
        }
    }

    pub const fn receive_only() -> Self {
        Self::new(TimeSyncPolicy::receive_only())
    }

    pub const fn state(&self) -> SessionState {
        self.state
    }

    pub const fn policy(&self) -> TimeSyncPolicy {
        self.policy
    }

    /// Grants physical authorization only until the supplied wrapping-safe
    /// monotonic deadline. This is separate from cryptographic authentication.
    pub fn authorize_until(&mut self, deadline_ms: u32) {
        self.authorization_until_ms = Some(deadline_ms);
    }

    pub fn clear_authorization(&mut self) {
        self.authorization_until_ms = None;
        if matches!(self.state, SessionState::Authorized) {
            self.state = SessionState::Expired;
        }
    }

    pub const fn ack_pending(&self) -> Option<u32> {
        self.ack
    }

    pub fn reset(&mut self) {
        self.decoder = Decoder::new();
        self.state = SessionState::Idle;
        self.authorization_until_ms = None;
        self.ack = None;
    }

    pub fn expire(&mut self, now_ms: u32) {
        if self
            .authorization_until_ms
            .is_some_and(|deadline| now_ms.wrapping_sub(deadline) < 0x8000_0000)
        {
            self.state = SessionState::Expired;
            self.ack = None;
        }
    }

    /// Accepts one encoded frame. It is useful for host tests and for I/O
    /// adapters that already have a bounded frame buffer.
    pub fn receive(
        &mut self,
        bytes: &[u8],
        now_ms: u32,
        auth: Option<&dyn AuthenticationHook>,
    ) -> Result<Frame, SessionError> {
        self.expire(now_ms);
        if self.state == SessionState::Expired {
            return Err(SessionError::AuthorizationExpired);
        }
        self.state = SessionState::Receiving;
        let frame = decode(bytes, auth).map_err(SessionError::Decode)?;
        self.state = SessionState::Authenticated;
        if frame.command == CommandType::TimeSync {
            let payload = TimeSyncPayload::parse(frame.payload()).map_err(SessionError::Payload)?;
            if !payload.is_fresh(now_ms / 1_000) {
                return Err(SessionError::Payload(TimeSyncError::Stale));
            }
            let authorized = self.policy.crypto_provisioned
                && self.policy.authenticated
                && self.policy.physically_authorized
                && self
                    .authorization_until_ms
                    .is_some_and(|deadline| now_ms.wrapping_sub(deadline) >= 0x8000_0000);
            if !authorized {
                self.state = if self.authorization_until_ms.is_some() {
                    SessionState::Expired
                } else {
                    SessionState::Authenticated
                };
                return Err(SessionError::Payload(
                    if self.authorization_until_ms.is_some() {
                        TimeSyncError::NotAuthorized
                    } else {
                        TimeSyncError::NotAuthenticated
                    },
                ));
            }
            self.state = SessionState::Authorized;
            if self.policy.rtc_mutation_enabled {
                // Applying is deliberately delegated to the caller's I/O seam.
                // `receive` cannot mutate because it has no I/O object.
            }
        }
        Ok(frame)
    }

    /// Services at most [`MAX_POLL_BYTES`] bytes and queues an ACK only after
    /// a complete, authorized frame. RTC mutation remains policy-gated.
    pub fn service<I: OpticalIo>(
        &mut self,
        io: &mut I,
        auth: Option<&dyn AuthenticationHook>,
    ) -> Option<Result<SessionState, SessionError>> {
        let now = io.now_ms();
        self.expire(now);
        for _ in 0..MAX_POLL_BYTES {
            let Some(byte) = io.read_byte() else { break };
            if self.state == SessionState::Idle {
                self.state = SessionState::Receiving;
            }
            if let Some(result) = self.decoder.push(byte, now, auth) {
                let frame = match result {
                    Ok(frame) => frame,
                    Err(error) => return Some(Err(SessionError::Decode(error))),
                };
                self.state = SessionState::Authenticated;
                if frame.command == CommandType::TimeSync {
                    let payload = match TimeSyncPayload::parse(frame.payload()) {
                        Ok(payload) => payload,
                        Err(error) => return Some(Err(SessionError::Payload(error))),
                    };
                    if !payload.is_fresh(now / 1_000) {
                        return Some(Err(SessionError::Payload(TimeSyncError::Stale)));
                    }
                    if !self.policy.crypto_provisioned
                        || !self.policy.authenticated
                        || !self.policy.physically_authorized
                    {
                        return Some(Err(SessionError::Payload(TimeSyncError::NotAuthenticated)));
                    }
                    if self
                        .authorization_until_ms
                        .is_none_or(|deadline| now.wrapping_sub(deadline) < 0x8000_0000)
                    {
                        self.state = SessionState::Expired;
                        return Some(Err(SessionError::AuthorizationExpired));
                    }
                    self.state = SessionState::Authorized;
                    if self.policy.rtc_mutation_enabled
                        && io.apply_rtc(payload.packed_datetime).is_err()
                    {
                        return Some(Err(SessionError::RtcApply));
                    }
                }
                self.ack = Some(frame.sequence);
                if self.policy.rtc_mutation_enabled {
                    self.state = SessionState::Applied;
                }
                io.queue_ack(frame.sequence);
                self.state = SessionState::AckQueued;
                return Some(Ok(self.state));
            }
        }
        None
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
    fn overlapping_preamble_resynchronizes() {
        let (bytes, len) = encoded(4);
        let mut decoder = Decoder::new();
        decoder.push(PREAMBLE[0], 0, None);
        decoder.push(PREAMBLE[0], 0, None);
        let mut result = None;
        for byte in bytes[1..len].iter().copied() {
            result = decoder.push(byte, 1, None);
        }
        assert_eq!(result.unwrap().unwrap().sequence, 4);
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
    fn unauthenticated_commands_reject_authenticated_length_without_panicking() {
        let mut bytes = [0; MAX_FRAME_LEN];
        let len =
            encode_inner(CommandType::Status, 1, &[0; MAX_PAYLOAD], None, &mut bytes).unwrap();
        bytes[4] = (MAX_PAYLOAD + AUTH_TAG_LEN) as u8;
        let total = HEADER_LEN + MAX_PAYLOAD + AUTH_TAG_LEN + CRC_LEN;
        let crc = crc16(&bytes[..total - CRC_LEN]);
        bytes[total - CRC_LEN..total].copy_from_slice(&crc.to_be_bytes());
        assert_eq!(
            decode(&bytes[..total], None),
            Err(DecodeError::InvalidLength)
        );
        assert_eq!(len, HEADER_LEN + MAX_PAYLOAD + CRC_LEN);
    }

    #[test]
    fn time_sync_payload_rejects_invalid_dates_and_checks_freshness() {
        let valid = (3u32 << 26) | (12u32 << 22) | (29u32 << 17) | (23u32 << 12);
        let mut bytes = [0; TIME_SYNC_PAYLOAD_LEN];
        bytes[..4].copy_from_slice(&valid.to_be_bytes());
        bytes[4..].copy_from_slice(&1_000u32.to_be_bytes());
        let payload = TimeSyncPayload::parse(&bytes).unwrap();
        assert!(payload.is_fresh(1_250));
        assert!(!payload.is_fresh(1_301));
        bytes[..4].copy_from_slice(&((3u32 << 26) | (2u32 << 22) | (29u32 << 17)).to_be_bytes());
        assert_eq!(
            TimeSyncPayload::parse(&bytes),
            Err(TimeSyncError::InvalidDateTime)
        );
    }

    #[test]
    fn time_sync_policy_requires_auth_presence_and_mutation_enable() {
        let payload = TimeSyncPayload {
            packed_datetime: 3u32 << 26,
            freshness: 100,
        };
        assert_eq!(
            TimeSyncPolicy::receive_only().authorize(payload, 100),
            Err(TimeSyncError::NotAuthenticated)
        );
        let policy = TimeSyncPolicy {
            crypto_provisioned: true,
            authenticated: true,
            physically_authorized: false,
            rtc_mutation_enabled: true,
        };
        assert_eq!(
            policy.authorize(payload, 100),
            Err(TimeSyncError::NotAuthorized)
        );
        let policy = TimeSyncPolicy {
            physically_authorized: true,
            ..policy
        };
        assert!(policy.authorize(payload, 100).is_ok());
        assert_eq!(policy.authorize(payload, 401), Err(TimeSyncError::Stale));
    }

    struct TestIo {
        bytes: [u8; MAX_FRAME_LEN],
        len: usize,
        pos: usize,
        now: u32,
        ack: Option<u32>,
        applied: bool,
    }

    impl TestIo {
        fn new(bytes: [u8; MAX_FRAME_LEN], len: usize, now: u32) -> Self {
            Self {
                bytes,
                len,
                pos: 0,
                now,
                ack: None,
                applied: false,
            }
        }
    }

    impl OpticalIo for TestIo {
        fn read_byte(&mut self) -> Option<u8> {
            let byte = (self.pos < self.len).then(|| self.bytes[self.pos]);
            self.pos += usize::from(byte.is_some());
            byte
        }
        fn now_ms(&mut self) -> u32 {
            self.now
        }
        fn queue_ack(&mut self, sequence: u32) {
            self.ack = Some(sequence);
        }
        fn apply_rtc(&mut self, _packed_datetime: u32) -> Result<(), ()> {
            self.applied = true;
            Ok(())
        }
    }

    fn authorized_session() -> OpticalSession {
        OpticalSession::new(TimeSyncPolicy {
            crypto_provisioned: true,
            authenticated: true,
            physically_authorized: true,
            rtc_mutation_enabled: false,
        })
    }

    fn sync_frame(sequence: u32, freshness: u32) -> ([u8; MAX_FRAME_LEN], usize) {
        let mut payload = [0u8; TIME_SYNC_PAYLOAD_LEN];
        payload[..4].copy_from_slice(&(3u32 << 26 | 1u32 << 22 | 1u32 << 17).to_be_bytes());
        payload[4..].copy_from_slice(&freshness.to_be_bytes());
        let mut bytes = [0; MAX_FRAME_LEN];
        let len = encode_authenticated(
            sequence_command(),
            sequence,
            &payload,
            &[0xC3; AUTH_TAG_LEN],
            &mut bytes,
        )
        .unwrap();
        (bytes, len)
    }

    fn sequence_command() -> CommandType {
        CommandType::TimeSync
    }

    #[test]
    fn session_is_bounded_and_queues_ack_without_default_rtc_mutation() {
        let (bytes, len) = sync_frame(11, 1);
        let mut session = authorized_session();
        session.authorize_until(2_000);
        let mut io = TestIo::new(bytes, len, 1_000);
        assert_eq!(
            session.service(&mut io, Some(&AcceptAuth)),
            Some(Ok(SessionState::AckQueued))
        );
        assert_eq!(io.ack, Some(11));
        assert!(!io.applied);
        assert_eq!(session.ack_pending(), Some(11));
    }

    #[test]
    fn session_reports_crc_auth_truncation_replay_timeout_and_expiry() {
        let (mut bytes, len) = sync_frame(12, 1);
        assert_eq!(
            decode(&bytes[..len - 1], Some(&AcceptAuth)),
            Err(DecodeError::InvalidLength)
        );
        bytes[10] ^= 1;
        assert_eq!(
            decode(&bytes[..len], Some(&AcceptAuth)),
            Err(DecodeError::Crc)
        );
        let (bytes, len) = sync_frame(12, 1);
        let mut decoder = Decoder::new();
        for byte in bytes[..len].iter().copied() {
            decoder.push(byte, 1, Some(&AcceptAuth));
        }
        let mut replay = None;
        for byte in bytes[..len].iter().copied() {
            replay = decoder.push(byte, 2, Some(&AcceptAuth));
        }
        assert_eq!(replay, Some(Err(DecodeError::Replay)));
        assert_eq!(
            decoder.push(PREAMBLE[0], RX_TIMEOUT_MS + 200, None),
            Some(Err(DecodeError::Timeout))
        );
        let mut session = authorized_session();
        session.authorize_until(1_000);
        let (bytes, len) = sync_frame(13, 1);
        let mut io = TestIo::new(bytes, len, 1_001);
        assert_eq!(
            session.service(&mut io, Some(&AcceptAuth)),
            Some(Err(SessionError::AuthorizationExpired))
        );
        assert_eq!(session.state(), SessionState::Expired);
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
