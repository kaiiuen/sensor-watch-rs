//! Self-integrity checking.
//!
//! Computes a hash of the app's own executable so the user can verify the
//! binary hasn't been modified or corrupted. The user-defined data (settings
//! file, custom NTP servers, watch faces) is intentionally NOT hashed — those
//! change at runtime and are expected to differ.

use std::io::Read;

/// A simple FNV-1a 64-bit hash (fast, deterministic, no dependencies).
pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Computes the hash of the running executable.
pub fn exe_hash() -> Option<u64> {
    let path = std::env::current_exe().ok()?;
    let mut file = std::fs::File::open(&path).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(fnv1a64(&buf))
}

/// Formats a hash as a hex string.
pub fn format_hash(h: u64) -> String {
    format!("{:016x}", h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_known_vector() {
        // FNV-1a 64-bit of the empty string is the offset basis.
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn fnv1a_deterministic() {
        let data = b"hello world";
        assert_eq!(fnv1a64(data), fnv1a64(data));
    }

    #[test]
    fn fnv1a_differs_on_change() {
        assert_ne!(fnv1a64(b"abc"), fnv1a64(b"abd"));
    }
}
