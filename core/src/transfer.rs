//! Safe UART-only transfer protocol for non-secret watch data.
//!
//! This module intentionally does not access UART, flash, or a filesystem. It
//! defines a fixed-size wire format and a bounded write receiver that an
//! embedded adapter can connect to the existing wear-levelled storage API.
//! There are no filesystem paths on the wire: [`ObjectId`] is an allowlist.
//!
//! Authentication is a hook, not a cryptographic claim. Production firmware
//! must provide an [`Authenticator`] that verifies the tag before accepting a
//! frame; [`RejectAll`] is the safe default.

#![allow(clippy::module_name_repetitions)]

/// Wire frame size, including the CRC.
pub const FRAME_SIZE: usize = 256;
/// Maximum bytes carried by one frame.
pub const MAX_PAYLOAD: usize = 228;
/// Maximum complete object accepted by the receiver.
pub const MAX_OBJECT_SIZE: u32 = 4096;
const MAGIC: [u8; 2] = *b"SW";
const VERSION: u8 = 1;
const HEADER_SIZE: usize = 26;
const CRC_SIZE: usize = 2;

/// Explicitly allowlisted non-secret objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ObjectId {
    Settings = 1,
    Activity = 2,
    TotpMetadata = 3,
}

impl ObjectId {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Settings),
            2 => Some(Self::Activity),
            3 => Some(Self::TotpMetadata),
            _ => None,
        }
    }

    /// Maximum object size for this allowlisted object.
    pub const fn max_size(self) -> u32 {
        match self {
            Self::Settings => 512,
            Self::Activity => 4096,
            Self::TotpMetadata => 2048,
        }
    }
}

/// Commands carried by a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    Read = 1,
    WriteData = 2,
    Commit = 3,
    Abort = 4,
}

impl Command {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Read),
            2 => Some(Self::WriteData),
            3 => Some(Self::Commit),
            4 => Some(Self::Abort),
            _ => None,
        }
    }
}

/// A decoded protocol frame. `payload` is always bounded by `MAX_PAYLOAD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub command: Command,
    pub object: ObjectId,
    pub sequence: u16,
    pub offset: u32,
    pub total_len: u32,
    pub payload: [u8; MAX_PAYLOAD],
    pub payload_len: u16,
    /// Opaque authenticator-provided tag; its meaning is outside this module.
    pub auth_tag: [u8; 8],
}

impl Frame {
    /// Creates a frame. Invalid sizes are rejected rather than truncated.
    pub fn new(
        command: Command,
        object: ObjectId,
        sequence: u16,
        offset: u32,
        total_len: u32,
        payload: &[u8],
        auth_tag: [u8; 8],
    ) -> Option<Self> {
        if payload.len() > MAX_PAYLOAD
            || total_len > object.max_size()
            || offset > total_len
            || offset
                .checked_add(payload.len() as u32)
                .is_none_or(|end| end > total_len)
        {
            return None;
        }
        let mut data = [0; MAX_PAYLOAD];
        data[..payload.len()].copy_from_slice(payload);
        Some(Self {
            command,
            object,
            sequence,
            offset,
            total_len,
            payload: data,
            payload_len: payload.len() as u16,
            auth_tag,
        })
    }

    /// Serializes into exactly one fixed-size frame.
    pub fn encode(&self) -> [u8; FRAME_SIZE] {
        let mut out = [0; FRAME_SIZE];
        out[0..2].copy_from_slice(&MAGIC);
        out[2] = VERSION;
        out[3] = self.command as u8;
        out[4] = self.object as u8;
        out[5] = 0;
        out[6..8].copy_from_slice(&self.sequence.to_le_bytes());
        out[8..12].copy_from_slice(&self.offset.to_le_bytes());
        out[12..16].copy_from_slice(&self.total_len.to_le_bytes());
        out[16..18].copy_from_slice(&self.payload_len.to_le_bytes());
        out[18..HEADER_SIZE].copy_from_slice(&self.auth_tag);
        out[26..26 + self.payload_len as usize]
            .copy_from_slice(&self.payload[..self.payload_len as usize]);
        let crc = crc16(&out[..FRAME_SIZE - CRC_SIZE]);
        out[FRAME_SIZE - CRC_SIZE..].copy_from_slice(&crc.to_le_bytes());
        out
    }

    /// Decodes and validates magic, version, lengths, allowlist, and CRC.
    pub fn decode(bytes: &[u8; FRAME_SIZE]) -> Result<Self, DecodeError> {
        if bytes[0..2] != MAGIC {
            return Err(DecodeError::Magic);
        }
        if bytes[2] != VERSION {
            return Err(DecodeError::Version);
        }
        if bytes[5] != 0 {
            return Err(DecodeError::Length);
        }
        let expected = u16::from_le_bytes([bytes[FRAME_SIZE - 2], bytes[FRAME_SIZE - 1]]);
        if crc16(&bytes[..FRAME_SIZE - CRC_SIZE]) != expected {
            return Err(DecodeError::Crc);
        }
        let command = Command::from_byte(bytes[3]).ok_or(DecodeError::Command)?;
        let object = ObjectId::from_byte(bytes[4]).ok_or(DecodeError::Object)?;
        let payload_len = u16::from_le_bytes([bytes[16], bytes[17]]) as usize;
        if payload_len > MAX_PAYLOAD {
            return Err(DecodeError::Length);
        }
        let total_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let offset = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        if total_len > object.max_size()
            || offset
                .checked_add(payload_len as u32)
                .is_none_or(|end| end > total_len)
        {
            return Err(DecodeError::Length);
        }
        let mut payload = [0; MAX_PAYLOAD];
        payload[..payload_len].copy_from_slice(&bytes[26..26 + payload_len]);
        let mut auth_tag = [0; 8];
        auth_tag.copy_from_slice(&bytes[18..26]);
        Ok(Self {
            command,
            object,
            sequence: u16::from_le_bytes([bytes[6], bytes[7]]),
            offset,
            total_len,
            payload,
            payload_len: payload_len as u16,
            auth_tag,
        })
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload[..self.payload_len as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Magic,
    Version,
    Command,
    Object,
    Length,
    Crc,
}

/// Authentication policy supplied by firmware or a host application.
pub trait Authenticator {
    fn authenticate(&self, frame: &Frame) -> bool;
}

/// Safe default: no frame is accepted until authentication is configured.
pub struct RejectAll;
impl Authenticator for RejectAll {
    fn authenticate(&self, _frame: &Frame) -> bool {
        false
    }
}

/// Store contract for atomic, wear-levelled writes.
pub trait AtomicStore {
    type Error;
    fn begin(&mut self, object: ObjectId, total_len: u32) -> Result<(), Self::Error>;
    fn write_chunk(&mut self, offset: u32, data: &[u8]) -> Result<(), Self::Error>;
    /// Must make the complete object durable in one atomic commit.
    fn commit(&mut self) -> Result<(), Self::Error>;
    fn abort(&mut self);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiveError<E> {
    Decode(DecodeError),
    Unauthenticated,
    Sequence,
    State,
    Store(E),
}

/// Bounded receiver. Chunks must arrive in order and are committed only once.
pub struct Receiver<S, A> {
    store: S,
    auth: A,
    object: Option<ObjectId>,
    total_len: u32,
    next_offset: u32,
    next_sequence: u16,
}

impl<S, A> Receiver<S, A> {
    pub const fn new(store: S, auth: A) -> Self {
        Self {
            store,
            auth,
            object: None,
            total_len: 0,
            next_offset: 0,
            next_sequence: 0,
        }
    }

    pub fn receive(&mut self, bytes: &[u8; FRAME_SIZE]) -> Result<(), ReceiveError<S::Error>>
    where
        S: AtomicStore,
        A: Authenticator,
    {
        let frame = Frame::decode(bytes).map_err(ReceiveError::Decode)?;
        if !self.auth.authenticate(&frame) {
            return Err(ReceiveError::Unauthenticated);
        }
        match frame.command {
            Command::WriteData => {
                if frame.sequence != self.next_sequence
                    || frame.offset != self.next_offset
                    || frame.total_len == 0
                    || frame.payload_len == 0
                {
                    return Err(ReceiveError::Sequence);
                }
                if self.object.is_none() {
                    self.store
                        .begin(frame.object, frame.total_len)
                        .map_err(ReceiveError::Store)?;
                    self.object = Some(frame.object);
                    self.total_len = frame.total_len;
                }
                if self.object != Some(frame.object) || self.total_len != frame.total_len {
                    return Err(ReceiveError::State);
                }
                self.store
                    .write_chunk(frame.offset, frame.payload())
                    .map_err(ReceiveError::Store)?;
                self.next_offset += frame.payload_len as u32;
                self.next_sequence = self.next_sequence.wrapping_add(1);
                Ok(())
            }
            Command::Commit
                if frame.sequence == self.next_sequence
                    && frame.offset == self.total_len
                    && frame.payload_len == 0 =>
            {
                if self.object != Some(frame.object) {
                    return Err(ReceiveError::State);
                }
                self.store.commit().map_err(ReceiveError::Store)?;
                self.object = None;
                self.total_len = 0;
                self.next_offset = 0;
                Ok(())
            }
            Command::Abort => {
                self.store.abort();
                self.object = None;
                self.total_len = 0;
                self.next_offset = 0;
                Err(ReceiveError::State)
            }
            _ => Err(ReceiveError::State),
        }
    }
}

fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFF;
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
    struct Auth;
    impl Authenticator for Auth {
        fn authenticate(&self, _: &Frame) -> bool {
            true
        }
    }
    #[derive(Default)]
    struct Store {
        data: [u8; 8],
        begun: bool,
        committed: bool,
        aborted: bool,
    }
    impl AtomicStore for Store {
        type Error = ();
        fn begin(&mut self, _: ObjectId, _: u32) -> Result<(), ()> {
            self.begun = true;
            Ok(())
        }
        fn write_chunk(&mut self, offset: u32, data: &[u8]) -> Result<(), ()> {
            self.data[offset as usize..offset as usize + data.len()].copy_from_slice(data);
            Ok(())
        }
        fn commit(&mut self) -> Result<(), ()> {
            self.committed = true;
            Ok(())
        }
        fn abort(&mut self) {
            self.aborted = true;
        }
    }
    #[test]
    fn frame_round_trip_and_crc() {
        let f = Frame::new(
            Command::WriteData,
            ObjectId::Settings,
            2,
            0,
            3,
            b"abc",
            [7; 8],
        )
        .unwrap();
        let encoded = f.encode();
        assert_eq!(Frame::decode(&encoded).unwrap(), f);
        let mut corrupt = encoded;
        corrupt[30] ^= 1;
        assert_eq!(Frame::decode(&corrupt), Err(DecodeError::Crc));
    }
    #[test]
    fn rejects_nonzero_reserved_header_byte() {
        let frame = Frame::new(Command::Read, ObjectId::Settings, 0, 0, 0, &[], [0; 8]).unwrap();
        let mut bytes = frame.encode();
        bytes[5] = 1;
        let crc = crc16(&bytes[..FRAME_SIZE - 2]);
        bytes[FRAME_SIZE - 2..].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(Frame::decode(&bytes), Err(DecodeError::Length));
    }

    #[test]
    fn rejects_payload_outside_declared_object() {
        assert!(
            Frame::new(
                Command::WriteData,
                ObjectId::Settings,
                0,
                2,
                3,
                b"ab",
                [0; 8],
            )
            .is_none()
        );
        assert!(
            Frame::new(
                Command::WriteData,
                ObjectId::Settings,
                0,
                u32::MAX,
                u32::MAX,
                b"x",
                [0; 8],
            )
            .is_none()
        );
    }

    #[test]
    fn receiver_commits_only_after_complete_ordered_transfer() {
        let mut r = Receiver::new(Store::default(), Auth);
        let a = Frame::new(
            Command::WriteData,
            ObjectId::Settings,
            0,
            0,
            3,
            b"ab",
            [0; 8],
        )
        .unwrap()
        .encode();
        let b = Frame::new(
            Command::WriteData,
            ObjectId::Settings,
            1,
            2,
            3,
            b"c",
            [0; 8],
        )
        .unwrap()
        .encode();
        let c = Frame::new(Command::Commit, ObjectId::Settings, 2, 3, 3, &[], [0; 8])
            .unwrap()
            .encode();
        r.receive(&a).unwrap();
        assert!(!r.store.committed);
        r.receive(&b).unwrap();
        r.receive(&c).unwrap();
        assert_eq!(&r.store.data[..3], b"abc");
        assert!(r.store.committed);
    }
    #[test]
    fn rejects_unknown_object_and_unauthenticated_frames() {
        let mut bytes = Frame::new(Command::Read, ObjectId::Settings, 0, 0, 0, &[], [0; 8])
            .unwrap()
            .encode();
        bytes[4] = 99;
        let crc = crc16(&bytes[..FRAME_SIZE - 2]);
        bytes[FRAME_SIZE - 2..].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(Frame::decode(&bytes), Err(DecodeError::Object));
        let f = Frame::new(
            Command::WriteData,
            ObjectId::Settings,
            0,
            0,
            1,
            b"x",
            [0; 8],
        )
        .unwrap()
        .encode();
        let mut r = Receiver::new(Store::default(), RejectAll);
        assert_eq!(r.receive(&f), Err(ReceiveError::Unauthenticated));
    }
}
