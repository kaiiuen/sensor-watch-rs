//! Bounded device identity primitives shared by firmware and Studio.
//!
//! The SAM L22 128-bit serial number is handled as bytes in increasing
//! signature-row address order. It is an identifier, not an authenticator.

pub const UID_LEN: usize = 16;
pub const UID_BASE_ADDRESS: usize = 0x0080_A00C;
pub const UID_END_ADDRESS: usize = UID_BASE_ADDRESS + UID_LEN - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentitySource {
    SamL22SignatureRow,
    Unavailable,
}

impl IdentitySource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::SamL22SignatureRow => "SAM-L22-signature-row",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityConfidence {
    High,
    Unknown,
}

impl IdentityConfidence {
    pub const fn label(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Unknown => "unknown",
        }
    }
}

/// Decode four words read from the signature row. ARM MMIO words are little
/// endian; preserving each word's bytes and word order preserves address order.
pub const fn decode_uid(words: [u32; 4]) -> [u8; UID_LEN] {
    let mut uid = [0u8; UID_LEN];
    let mut word = 0;
    while word < 4 {
        let bytes = words[word].to_le_bytes();
        let offset = word * 4;
        uid[offset] = bytes[0];
        uid[offset + 1] = bytes[1];
        uid[offset + 2] = bytes[2];
        uid[offset + 3] = bytes[3];
        word += 1;
    }
    uid
}

/// A short, non-reversible display fingerprint. This intentionally avoids
/// exposing the raw silicon UID in normal shell/UI output.
pub fn masked_fingerprint(uid: &[u8; UID_LEN]) -> [u8; 16] {
    let mut a = 0xcbf2_9ce4_8422_2325u64;
    let mut b = 0x9e37_79b9_7f4a_7c15u64;
    for (index, byte) in uid.iter().copied().enumerate() {
        a ^= byte as u64;
        a = a.wrapping_mul(0x1000_0000_01b3);
        b ^= (byte as u64) << ((index & 7) * 8);
        b = b.rotate_left(13).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&a.to_be_bytes());
    out[8..].copy_from_slice(&b.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_words_decode_in_memory_byte_order() {
        assert_eq!(
            decode_uid([0x0302_0100, 0x0706_0504, 0x0b0a_0908, 0x0f0e_0d0c]),
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
    }

    #[test]
    fn fingerprint_is_fixed_length_and_masked() {
        let uid = [0x55; UID_LEN];
        let fingerprint = masked_fingerprint(&uid);
        assert_eq!(fingerprint.len(), 16);
        assert_ne!(&fingerprint, &uid);
    }
}
