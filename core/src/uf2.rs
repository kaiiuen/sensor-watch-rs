//! UF2 firmware image conversion.
//!
//! Converts a raw binary firmware image into the UF2 format used by the
//! Sensor Watch's drag-and-drop bootloader. The UF2 format splits the image
//! into 256-byte blocks, each wrapped in a 512-byte header/footer, so the
//! bootloader can write it to flash.
//!
//! This is a pure-logic port of the reference `uf2conv.py`, placed in the
//! `core` crate so it can be unit-tested on the host.

use alloc::vec::Vec;

/// The number of bytes in one UF2 block.
pub const UF2_BLOCK_SIZE: usize = 512;
/// The number of payload bytes in one UF2 block.
pub const UF2_PAYLOAD_SIZE: usize = 256;

/// UF2 magic numbers.
pub const UF2_MAGIC_START0: u32 = 0x0A32_4655; // "UF2\n"
pub const UF2_MAGIC_START1: u32 = 0x9E5D_5157;
pub const UF2_MAGIC_END: u32 = 0x0AB1_6F30;

/// The SAM L22 family ID (used by the bootloader to verify the target).
pub const SAML22_FAMILY_ID: u32 = 0x2C29_472F;

/// The flash address where the firmware begins (after the bootloader).
pub const APP_START_ADDR: u32 = 0x2000;
/// The maximum application size supported by the bootloader.
pub const MAX_APPLICATION_BYTES: usize = 0x3A000;

/// The size of each UF2 data block (256 bytes of payload).
const BLOCK_SIZE: usize = UF2_PAYLOAD_SIZE;
/// The total size of each UF2 block including header and footer (512 bytes).
const TOTAL_BLOCK_SIZE: usize = UF2_BLOCK_SIZE;

/// A validated UF2 image and its reconstructed, padded payload.
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedUf2 {
    pub image: Vec<u8>,
    pub block_count: usize,
}

/// Validates UF2 framing, block ordering, target addresses, family metadata,
/// and reconstructs the payload. The final block is returned with its normal
/// 256-byte UF2 padding because the format does not store the original length.
pub fn validate(uf2: &[u8]) -> Result<ValidatedUf2, &'static str> {
    if uf2.is_empty() || !uf2.len().is_multiple_of(UF2_BLOCK_SIZE) {
        return Err("UF2 size is not a non-empty multiple of 512 bytes");
    }
    let max_block_count = MAX_APPLICATION_BYTES / UF2_PAYLOAD_SIZE;
    let block_count = uf2.len() / UF2_BLOCK_SIZE;
    if block_count == 0 || block_count > max_block_count {
        return Err("UF2 application payload is empty or exceeds the maximum size");
    }
    if block_count > u32::MAX as usize {
        return Err("UF2 block count is too large");
    }
    let mut image = Vec::with_capacity(block_count * UF2_PAYLOAD_SIZE);

    for blockno in 0..block_count {
        let block = &uf2[blockno * UF2_BLOCK_SIZE..(blockno + 1) * UF2_BLOCK_SIZE];
        let word = |offset: usize| -> u32 {
            u32::from_le_bytes(block[offset..offset + 4].try_into().unwrap())
        };
        if word(0) != UF2_MAGIC_START0 || word(4) != UF2_MAGIC_START1 {
            return Err("UF2 start magic is invalid");
        }
        if word(8) & 0x2000 == 0 {
            return Err("UF2 family-ID flag is missing");
        }
        if word(12) != APP_START_ADDR + (blockno as u32 * UF2_PAYLOAD_SIZE as u32)
            || word(16) != UF2_PAYLOAD_SIZE as u32
            || word(20) != blockno as u32
            || word(24) != block_count as u32
            || word(28) != SAML22_FAMILY_ID
        {
            return Err("UF2 block metadata does not match the Sensor-Watch board");
        }
        if word(508) != UF2_MAGIC_END {
            return Err("UF2 end magic is invalid");
        }
        image.extend_from_slice(&block[32..32 + UF2_PAYLOAD_SIZE]);
    }

    Ok(ValidatedUf2 { image, block_count })
}

/// Computes the CRC-32/IEEE used by the firmware integrity check.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Converts a raw firmware image into a UF2 file.
///
/// `image` is the raw binary firmware (e.g. the contents of the `.bin` file
/// extracted from the ELF). Returns the UF2 data as a `Vec<u8>`.
pub fn convert_to_uf2(image: &[u8]) -> Vec<u8> {
    // Reject before calculating capacity: callers may pass untrusted or
    // compiler-produced input, and capacity allocation must never precede the
    // bootloader size check.
    if image.is_empty() || image.len() > MAX_APPLICATION_BYTES {
        return Vec::new();
    }
    let num_blocks = image.len().div_ceil(BLOCK_SIZE);
    let output_bytes = match num_blocks.checked_mul(TOTAL_BLOCK_SIZE) {
        Some(bytes) => bytes,
        None => return Vec::new(),
    };
    let mut out = Vec::with_capacity(output_bytes);

    for blockno in 0..num_blocks {
        let ptr = BLOCK_SIZE * blockno;
        let chunk = &image[ptr..(ptr + BLOCK_SIZE).min(image.len())];

        // Flags: 0x2000 = has family ID.
        let flags: u32 = 0x2000;

        // Header: magic0, magic1, flags, target address, size, blockno, numblocks, familyid.
        out.extend_from_slice(&UF2_MAGIC_START0.to_le_bytes());
        out.extend_from_slice(&UF2_MAGIC_START1.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&((ptr as u32) + APP_START_ADDR).to_le_bytes());
        out.extend_from_slice(&(BLOCK_SIZE as u32).to_le_bytes());
        out.extend_from_slice(&(blockno as u32).to_le_bytes());
        out.extend_from_slice(&(num_blocks as u32).to_le_bytes());
        out.extend_from_slice(&SAML22_FAMILY_ID.to_le_bytes());

        // Payload (padded to 256 bytes).
        out.extend_from_slice(chunk);
        out.resize(out.len() + (BLOCK_SIZE - chunk.len()), 0);

        // Padding to 512 bytes total, then the end magic.
        let padding = TOTAL_BLOCK_SIZE - 32 - BLOCK_SIZE - 4;
        out.resize(out.len() + padding, 0);
        out.extend_from_slice(&UF2_MAGIC_END.to_le_bytes());
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_image_produces_no_blocks() {
        assert_eq!(convert_to_uf2(&[]).len(), 0);
    }

    #[test]
    fn small_image_produces_one_block() {
        let image = [0xAA; 100];
        let uf2 = convert_to_uf2(&image);
        assert_eq!(uf2.len(), TOTAL_BLOCK_SIZE);
    }

    #[test]
    fn oversized_image_is_rejected_before_encoding() {
        let image = alloc::vec![0xAA; MAX_APPLICATION_BYTES + 1];
        assert!(convert_to_uf2(&image).is_empty());
    }

    #[test]
    fn exact_block_size_produces_one_block() {
        let image = [0xBB; BLOCK_SIZE];
        let uf2 = convert_to_uf2(&image);
        assert_eq!(uf2.len(), TOTAL_BLOCK_SIZE);
    }

    #[test]
    fn block_plus_one_produces_two_blocks() {
        let image = [0xCC; BLOCK_SIZE + 1];
        let uf2 = convert_to_uf2(&image);
        assert_eq!(uf2.len(), 2 * TOTAL_BLOCK_SIZE);
    }

    #[test]
    fn header_is_correct() {
        let image = [0xDD; 8];
        let uf2 = convert_to_uf2(&image);
        // magic0
        assert_eq!(
            u32::from_le_bytes(uf2[0..4].try_into().unwrap()),
            UF2_MAGIC_START0
        );
        // magic1
        assert_eq!(
            u32::from_le_bytes(uf2[4..8].try_into().unwrap()),
            UF2_MAGIC_START1
        );
        // flags has family bit
        let flags = u32::from_le_bytes(uf2[8..12].try_into().unwrap());
        assert_ne!(flags & 0x2000, 0);
        // target address
        let addr = u32::from_le_bytes(uf2[12..16].try_into().unwrap());
        assert_eq!(addr, APP_START_ADDR);
        // family id
        let fam = u32::from_le_bytes(uf2[28..32].try_into().unwrap());
        assert_eq!(fam, SAML22_FAMILY_ID);
        // end magic
        let end = u32::from_le_bytes(uf2[508..512].try_into().unwrap());
        assert_eq!(end, UF2_MAGIC_END);
    }

    #[test]
    fn payload_is_preserved() {
        let image = [0xEE; 256];
        let uf2 = convert_to_uf2(&image);
        // Payload starts at byte 32.
        assert_eq!(&uf2[32..288], &image[..]);
    }

    #[test]
    fn validate_round_trips_and_checks_metadata() {
        let image = (0..300).map(|n| n as u8).collect::<Vec<_>>();
        let uf2 = convert_to_uf2(&image);
        let parsed = validate(&uf2).unwrap();
        assert_eq!(parsed.block_count, 2);
        assert_eq!(&parsed.image[..300], &image[..]);
        assert_eq!(&parsed.image[300..], &[0; 212]);
    }

    #[test]
    fn validate_rejects_corruption_and_wrong_board() {
        let mut uf2 = convert_to_uf2(&[0x5A; 10]);
        uf2[13] = 0;
        assert_eq!(
            validate(&uf2),
            Err("UF2 block metadata does not match the Sensor-Watch board")
        );

        let mut uf2 = convert_to_uf2(&[0x5A; 10]);
        uf2[508] ^= 1;
        assert_eq!(validate(&uf2), Err("UF2 end magic is invalid"));
    }

    #[test]
    fn crc32_matches_ieee_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
