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

/// UF2 magic numbers.
const UF2_MAGIC_START0: u32 = 0x0A32_4655; // "UF2\n"
const UF2_MAGIC_START1: u32 = 0x9E5D_5157;
const UF2_MAGIC_END: u32 = 0x0AB1_6F30;

/// The SAM L22 family ID (used by the bootloader to verify the target).
pub const SAML22_FAMILY_ID: u32 = 0x2C29_472F;

/// The flash address where the firmware begins (after the bootloader).
pub const APP_START_ADDR: u32 = 0x2000;

/// The size of each UF2 data block (256 bytes of payload).
const BLOCK_SIZE: usize = 256;
/// The total size of each UF2 block including header and footer (512 bytes).
const TOTAL_BLOCK_SIZE: usize = 512;

/// Converts a raw firmware image into a UF2 file.
///
/// `image` is the raw binary firmware (e.g. the contents of the `.bin` file
/// extracted from the ELF). Returns the UF2 data as a `Vec<u8>`.
pub fn convert_to_uf2(image: &[u8]) -> Vec<u8> {
    let num_blocks = (image.len() + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let mut out = Vec::with_capacity(num_blocks * TOTAL_BLOCK_SIZE);

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
        out.extend(core::iter::repeat(0u8).take(BLOCK_SIZE - chunk.len()));

        // Padding to 512 bytes total, then the end magic.
        let padding = TOTAL_BLOCK_SIZE - 32 - BLOCK_SIZE - 4;
        out.extend(core::iter::repeat(0u8).take(padding));
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
}
