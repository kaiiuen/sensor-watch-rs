//! Host-side resolution of the firmware's 24-bit panic fingerprints.
//!
//! The firmware stores only a truncated hash, so this module reconstructs the
//! candidates from the source tree used to build the firmware. It deliberately
//! does not parse or execute the ELF; the ELF path is used by callers to select
//! the matching firmware build/source tree.

use std::path::{Path, PathBuf};

const FNV_OFFSET: u32 = 0x811c9dc5;
const FNV_PRIME: u32 = 0x01000193;
const COLUMN_MULTIPLIER: u32 = 2654435761;

/// A source location that produces a stored fingerprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
}

impl SourceLocation {
    pub fn display(&self) -> String {
        format!("{}:{}:{}", self.path.display(), self.line, self.column)
    }
}

/// Parse the shell's `Pxxxxxx` representation without panicking on bad input.
pub fn parse_fingerprint(input: &str) -> Result<u32, String> {
    let value = input.trim();
    let digits = value.strip_prefix(['P', 'p']).unwrap_or(value);
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected a 6-digit hexadecimal fingerprint such as P3fa862".into());
    }
    u32::from_str_radix(digits, 16).map_err(|_| "invalid hexadecimal fingerprint".into())
}

/// Reproduces `src/panic.rs`, including its final 24-bit storage truncation.
pub fn fingerprint_for(file: &str, line: u32, column: u32) -> u32 {
    let mut hash = FNV_OFFSET;
    for byte in file.as_bytes() {
        hash = (hash ^ u32::from(*byte)).wrapping_mul(FNV_PRIME);
    }
    hash ^= line.reverse_bits();
    hash ^= column.wrapping_mul(COLUMN_MULTIPLIER);
    hash & 0x00ff_ffff
}

/// Resolve a `Pxxxxxx` fingerprint against firmware Rust sources below `root`.
///
/// The source path is hashed in the same slash-separated relative form Cargo
/// passes to the firmware compiler (`src/foo.rs` or `core/src/foo.rs`). An
/// absolute-path candidate is also checked for toolchains that retain absolute
/// `file!()` paths in panic metadata.
pub fn resolve(input: &str, root: &Path) -> Result<Vec<SourceLocation>, String> {
    let fingerprint = parse_fingerprint(input)?;
    let mut files = Vec::new();
    for relative_root in [Path::new("src"), Path::new("core/src")] {
        let source_root = root.join(relative_root);
        if source_root.exists() {
            collect_sources(root, &source_root, &mut files)
                .map_err(|e| format!("failed to scan sources: {e}"))?;
        }
    }
    let mut matches = Vec::new();

    for (path, relative) in files {
        let bytes =
            std::fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let relative = relative.to_string_lossy().replace('\\', "/");
        let absolute = path.to_string_lossy().replace('\\', "/");
        for (line_index, line) in text.split('\n').enumerate() {
            let line_number = line_index as u32 + 1;
            // Rust columns are byte-oriented and 1-based; include the position
            // immediately after the final byte for empty/trailing locations.
            for column in 1..=line.len() as u32 + 1 {
                if fingerprint_for(&relative, line_number, column) == fingerprint
                    || fingerprint_for(&absolute, line_number, column) == fingerprint
                {
                    matches.push(SourceLocation {
                        path: path.clone(),
                        line: line_number,
                        column,
                    });
                }
            }
        }
    }

    Ok(matches)
}

fn collect_sources(
    root: &Path,
    dir: &Path,
    output: &mut Vec<(PathBuf, PathBuf)>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .is_some_and(|name| name == "target" || name == ".git")
        {
            continue;
        }
        if path.is_dir() {
            collect_sources(root, &path, output)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(relative) = path.strip_prefix(root) {
                output.push((path.clone(), relative.to_path_buf()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn firmware_hash_is_stable_for_synthetic_location() {
        assert_eq!(fingerprint_for("src/synthetic.rs", 2, 1), 0xf832e7);
    }

    #[test]
    fn malformed_fingerprints_are_errors() {
        for input in ["", "P123", "Pzzzzzz", "1234567", "P1234567"] {
            assert!(parse_fingerprint(input).is_err(), "accepted {input:?}");
        }
        assert_eq!(parse_fingerprint("pABC123"), Ok(0xabc123));
    }

    #[test]
    fn scans_synthetic_source_and_reports_location() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("sensor-watch-panic-map-{suffix}"));
        let source = root.join("src/synthetic.rs");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "fn main() {}\npanic!(\"boom\");\n").unwrap();

        let fp = fingerprint_for("src/synthetic.rs", 2, 1);
        let matches = resolve(&format!("P{fp:06x}"), &root).unwrap();
        assert!(matches
            .iter()
            .any(|m| m.path == source && m.line == 2 && m.column == 1));
        let _ = std::fs::remove_dir_all(root);
    }
}
