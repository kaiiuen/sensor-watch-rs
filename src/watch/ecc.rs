//! SECDED error-correction code (Hamming) for flash storage.
//!
//! Flash bits can flip over time (memory rot). This module wraps each 32-bit
//! data word with a 7-bit SECDED Hamming code that can correct any single-bit
//! error and detect any double-bit error. The encoded word is stored as 40
//! bits (5 bytes) per 32-bit data word.

/// The number of data bits protected per code word.
const DATA_BITS: u32 = 32;
/// The number of parity bits for SECDED over 32 data bits.
const PARITY_BITS: u32 = 6;
/// The Hamming code word size in bits (data + parity), excluding the overall
/// parity bit.
const HAMMING_BITS: u32 = DATA_BITS + PARITY_BITS;
/// The total encoded size in bits (Hamming + overall parity).
const TOTAL_BITS: u32 = HAMMING_BITS + 1;

/// Computes the parity of a value (1 if odd number of set bits).
fn parity(v: u32) -> u32 {
    v.count_ones() & 1
}

/// Computes the parity of a 64-bit value (1 if odd number of set bits).
fn parity64(v: u64) -> u32 {
    v.count_ones() & 1
}

/// Encodes a 32-bit word into a 39-bit SECDED code word.
///
/// The layout: data bits occupy positions 0..32, the 6 Hamming parity bits
/// occupy positions 32..38 (covering the standard Hamming syndrome), and bit
/// 38 is the overall parity bit for the whole code word (enabling double-bit
/// detection). Returns the 39-bit code word.
pub fn encode(data: u32) -> u64 {
    // Place data bits at the non-power-of-two positions (1..=38).
    let mut out = 0u64;
    let mut data_idx = 0u32;
    for bitpos in 1..=HAMMING_BITS {
        if bitpos.is_power_of_two() {
            continue;
        }
        let dbit = (data >> data_idx) & 1;
        out |= (dbit as u64) << (bitpos - 1);
        data_idx += 1;
    }
    // Compute each parity bit by XORing all positions (1..=38) that have that
    // parity bit set, excluding the parity position itself.
    for i in 0..6 {
        let pos = 1u32 << i;
        let mut bit = 0u32;
        for bitpos in 1..=HAMMING_BITS {
            if bitpos == pos {
                continue;
            }
            if bitpos & pos != 0 {
                bit ^= ((out >> (bitpos - 1)) & 1) as u32;
            }
        }
        out |= (bit as u64) << (pos - 1);
    }
    // Overall parity bit at position 39 (bit 38, 0-indexed).
    out |= (parity64(out) as u64) << 38;
    out
}

/// Decodes a 39-bit SECDED code word, correcting single-bit errors.
///
/// Returns `(data, corrected)`: the corrected 32-bit data and whether a
/// single-bit error was corrected. A double-bit error is not correctable and
/// returns the raw data with `corrected = false` (caller should treat it as
/// corruption).
pub fn decode(code: u64) -> (u32, bool) {
    // Compute the Hamming syndrome over the 38-bit Hamming code (positions 1-38).
    let mut syndrome = 0u32;
    for i in 0..6 {
        let pos = 1u32 << i;
        let mut bit = 0u32;
        for bitpos in 1..=HAMMING_BITS {
            if bitpos & pos != 0 {
                bit ^= ((code >> (bitpos - 1)) & 1) as u32;
            }
        }
        syndrome |= bit << i;
    }

    // Overall parity of the whole code word (including the parity bit).
    let overall = parity64(code);

    if syndrome == 0 && overall == 0 {
        // No error.
        return (extract_data(code), false);
    }
    if syndrome == 0 && overall == 1 {
        // Only the overall parity bit was flipped; correct it.
        let corrected = code ^ (1 << 38);
        return (extract_data(corrected), true);
    }
    if syndrome != 0 && overall == 1 {
        // Single-bit error: correct it. The syndrome is the 1-indexed position.
        let err_pos = (syndrome - 1) as u64;
        let corrected = code ^ (1 << err_pos);
        return (extract_data(corrected), true);
    }
    // Double-bit error (or uncorrectable): return raw data, flag as not corrected.
    (extract_data(code), false)
}

/// Extracts the 32 data bits from a code word.
fn extract_data(code: u64) -> u32 {
    let mut data = 0u32;
    let mut data_idx = 0u32;
    for bitpos in 1..=HAMMING_BITS {
        if bitpos.is_power_of_two() {
            continue;
        }
        let bit = ((code >> (bitpos - 1)) & 1) as u32;
        data |= bit << data_idx;
        data_idx += 1;
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let data = 0xDEAD_BEEF;
        let code = encode(data);
        let (decoded, corrected) = decode(code);
        assert_eq!(decoded, data);
        assert!(!corrected);
    }

    #[test]
    fn corrects_single_bit_error() {
        let data = 0x1234_5678;
        let code = encode(data);
        // Flip a single data bit.
        let corrupted = code ^ (1 << 5);
        let (decoded, corrected) = decode(corrupted);
        assert_eq!(decoded, data);
        assert!(corrected);
    }

    #[test]
    fn detects_double_bit_error() {
        let data = 0xABCD_EF01;
        let code = encode(data);
        // Flip two bits -> uncorrectable.
        let corrupted = code ^ ((1 << 3) | (1 << 9));
        let (_, corrected) = decode(corrupted);
        assert!(!corrected);
    }
}
