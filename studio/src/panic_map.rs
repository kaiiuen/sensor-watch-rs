//! Host-side resolution of the firmware's 24-bit panic fingerprints.
//!
//! The firmware stores only a truncated hash, so this module reconstructs the
//! candidates from the source tree used to build the firmware. A build manifest
//! next to the ELF ties that source tree to the exact ELF and rejects stale or
//! mismatched checkouts before scanning.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const FNV_OFFSET: u32 = 0x811c9dc5;
const FNV_PRIME: u32 = 0x01000193;
const COLUMN_MULTIPLIER: u32 = 2654435761;
const MANIFEST_FORMAT: &str = "sensor-watch-panic-map-v1";

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

/// Write the source/ELF identity used by the resolver.
///
/// The manifest is deliberately content-addressed: changing any Rust source
/// file or the ELF makes resolution fail closed instead of returning a match
/// from a different build.
pub fn write_manifest(elf: &Path, root: &Path) -> Result<PathBuf, String> {
    let source_hash = source_tree_hash(root)?;
    let elf_hash = sha256_file(elf)?;
    let root = root
        .canonicalize()
        .map_err(|e| format!("cannot canonicalize source root: {e}"))?;
    let manifest = elf.with_file_name("sensor-watch.panic-map.json");
    let value = serde_json::json!({
        "format": MANIFEST_FORMAT,
        "elf_sha256": elf_hash,
        "source_root": root.to_string_lossy(),
        "source_sha256": source_hash,
    });
    std::fs::write(&manifest, serde_json::to_vec_pretty(&value).unwrap())
        .map_err(|e| format!("failed to write {}: {e}", manifest.display()))?;
    Ok(manifest)
}

/// Resolve against the ELF's build manifest and its exact source tree.
pub fn resolve_against_elf(input: &str, elf: &Path) -> Result<Vec<SourceLocation>, String> {
    let manifest_path = elf.with_file_name("sensor-watch.panic-map.json");
    let bytes = std::fs::read(&manifest_path).map_err(|_| {
        format!(
            "no panic map manifest beside ELF {}; build the firmware through Studio first",
            elf.display()
        )
    })?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid panic map manifest: {e}"))?;
    let root = elf
        .ancestors()
        .find(|path| path.join("Cargo.toml").is_file() || path.join("src").is_dir())
        .ok_or_else(|| "could not locate firmware workspace for ELF".to_string())?;
    if manifest.get("format").and_then(|v| v.as_str()) != Some(MANIFEST_FORMAT) {
        return Err("panic map manifest format is incompatible; rebuild the firmware".into());
    }
    let expected_root = root
        .canonicalize()
        .map_err(|e| format!("cannot canonicalize firmware workspace: {e}"))?;
    if manifest.get("source_root").and_then(|v| v.as_str())
        != Some(expected_root.to_string_lossy().as_ref())
    {
        return Err(
            "panic map source tree does not match the ELF build path; rebuild from this workspace"
                .into(),
        );
    }
    if manifest.get("elf_sha256").and_then(|v| v.as_str()) != Some(sha256_file(elf)?.as_str()) {
        return Err(
            "panic map ELF checksum does not match; the ELF was replaced after mapping".into(),
        );
    }
    if manifest.get("source_sha256").and_then(|v| v.as_str())
        != Some(source_tree_hash(root)?.as_str())
    {
        return Err("panic map source checksum does not match; use the exact source tree used to build the ELF".into());
    }
    resolve(input, root)
}

/// Resolve a fingerprint against a source root. Used by the manifest-checked
/// entry point and kept small so the hashing algorithm has one implementation.
pub fn resolve(input: &str, root: &Path) -> Result<Vec<SourceLocation>, String> {
    let fingerprint = parse_fingerprint(input)?;
    let mut files = source_files(root)?;
    let mut matches = Vec::new();
    for (path, relative) in files.drain(..) {
        let bytes =
            std::fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let relative = relative.to_string_lossy().replace('\\', "/");
        let absolute = path.to_string_lossy().replace('\\', "/");
        for (line_index, line) in text.split('\n').enumerate() {
            let line_number = line_index as u32 + 1;
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

fn source_files(root: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let mut files = Vec::new();
    for relative_root in [Path::new("src"), Path::new("core/src")] {
        let source_root = root.join(relative_root);
        if source_root.exists() {
            collect_sources(root, &source_root, &mut files)
                .map_err(|e| format!("failed to scan sources: {e}"))?;
        }
    }
    files.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(files)
}

fn source_tree_hash(root: &Path) -> Result<String, String> {
    let mut hash = Sha256::new();
    for (path, relative) in source_files(root)? {
        hash.update(relative.to_string_lossy().replace('\\', "/").as_bytes());
        hash.update([0]);
        hash.update(
            std::fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?,
        );
        hash.update([0]);
    }
    Ok(hex(hash.finalize().as_ref()))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    Ok(hex(Sha256::digest(&bytes).as_ref()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn collect_sources(
    root: &Path,
    dir: &Path,
    output: &mut Vec<(PathBuf, PathBuf)>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
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

    fn temp_root() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sensor-watch-panic-map-{suffix}"))
    }

    #[test]
    fn firmware_hash_is_stable_for_synthetic_location() {
        assert_eq!(fingerprint_for("src/synthetic.rs", 2, 1), 0xf832e7);
    }

    #[test]
    fn malformed_fingerprints_are_errors() {
        for input in ["", "P123", "Pzzzzzz", "1234567", "P1234567"] {
            assert!(parse_fingerprint(input).is_err());
        }
        assert_eq!(parse_fingerprint("pABC123"), Ok(0xabc123));
    }

    #[test]
    fn manifest_checked_resolution_rejects_source_and_elf_mismatch() {
        let root = temp_root();
        let source = root.join("src/synthetic.rs");
        let elf = root.join("target/thumbv6m-none-eabi/release/sensor-watch");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::create_dir_all(elf.parent().unwrap()).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(&source, "fn main() {}\npanic!(\"boom\");\n").unwrap();
        std::fs::write(&elf, b"original elf").unwrap();
        write_manifest(&elf, &root).unwrap();
        let fp = fingerprint_for("src/synthetic.rs", 2, 1);
        assert!(resolve_against_elf(&format!("P{fp:06x}"), &elf)
            .unwrap()
            .iter()
            .any(|m| m.line == 2 && m.column == 1));
        assert!(resolve_against_elf("P000000", &elf).unwrap().is_empty());
        std::fs::write(&elf, b"replaced elf").unwrap();
        assert!(resolve_against_elf("P000000", &elf)
            .unwrap_err()
            .contains("ELF checksum"));
        std::fs::write(&elf, b"original elf").unwrap();
        std::fs::write(&source, "changed\n").unwrap();
        assert!(resolve_against_elf("P000000", &elf)
            .unwrap_err()
            .contains("source checksum"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_manifest_is_an_explicit_error() {
        let root = temp_root();
        let elf = root.join("target/release/sensor-watch");
        std::fs::create_dir_all(elf.parent().unwrap()).unwrap();
        std::fs::write(&elf, b"elf").unwrap();
        assert!(resolve_against_elf("P000000", &elf)
            .unwrap_err()
            .contains("no panic map manifest"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
