//! Centralized Studio storage and firmware artifact layout.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LATEST_COUNTER: AtomicU64 = AtomicU64::new(0);

pub const PORTABLE_FLAG: &str = "portable.flag";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageMode {
    Portable,
    Installed,
    Developer,
    Unavailable,
}

impl StorageMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Portable => "Portable",
            Self::Installed => "Installed",
            Self::Developer => "Developer",
            Self::Unavailable => "Unavailable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRoots {
    pub mode: StorageMode,
    pub package_root: Option<PathBuf>,
    pub user_data_root: PathBuf,
}

/// A portable marker is explicit: it must be a regular, non-link file containing
/// either `portable` or the versioned marker. A copied executable alone is never
/// enough to select this mode.
pub fn has_valid_portable_flag(launcher: &Path) -> bool {
    let Some(parent) = launcher.parent() else {
        return false;
    };
    let flag = parent.join(PORTABLE_FLAG);
    let Ok(meta) = std::fs::symlink_metadata(&flag) else {
        return false;
    };
    if !meta.is_file() || meta.file_type().is_symlink() || is_reparse_point(&meta) {
        return false;
    }
    std::fs::read_to_string(flag)
        .map(|value| matches!(value.trim(), "portable" | "sensor-watch-portable-v1"))
        .unwrap_or(false)
}

pub fn roots(
    launcher: &Path,
    package_root: Option<&Path>,
    mode: StorageMode,
    explicit_portable: bool,
    installed_root: PathBuf,
) -> StorageRoots {
    let portable = explicit_portable || has_valid_portable_flag(launcher);
    if portable {
        if let Some(root) = package_root {
            return StorageRoots {
                mode: StorageMode::Portable,
                package_root: Some(root.to_path_buf()),
                user_data_root: root.join("user-data"),
            };
        }
    }
    StorageRoots {
        mode,
        package_root: package_root.map(Path::to_path_buf),
        user_data_root: installed_root,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactPaths {
    pub root: PathBuf,
    pub latest: PathBuf,
    pub uf2: PathBuf,
    pub manifest: PathBuf,
    pub sidecar: PathBuf,
    pub inputs: PathBuf,
    pub latest_json: PathBuf,
    pub recovery_generations: PathBuf,
}

pub fn artifact_paths(
    user_data_root: &Path,
    board: &str,
    revision: &str,
    profile: &str,
) -> Result<ArtifactPaths, String> {
    for value in [board, revision, profile] {
        validate_segment(value)?;
    }
    let root = user_data_root
        .join("firmware")
        .join(board)
        .join(revision)
        .join(profile);
    let latest = root.join("latest");
    Ok(ArtifactPaths {
        root: root.clone(),
        uf2: latest.join("sensor-watch.uf2"),
        manifest: latest.join("sensor-watch.uf2.json"),
        sidecar: latest.join("sensor-watch.uf2.json.sig"),
        inputs: latest.join("sensor-watch.uf2.inputs"),
        latest_json: latest.join("latest.json"),
        latest,
        recovery_generations: user_data_root.join("recovery/generations"),
    })
}

fn validate_segment(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err("artifact path component is invalid".into());
    }
    let path = Path::new(value);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("artifact path component must be a single safe name".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatestMetadata {
    pub format: String,
    pub board: String,
    pub revision: String,
    pub profile: String,
    pub generated_input_digest: String,
    pub artifact: String,
}

pub fn write_latest_atomic(paths: &ArtifactPaths, metadata: &LatestMetadata) -> Result<(), String> {
    validate_path_chain(&paths.latest)?;
    std::fs::create_dir_all(&paths.latest)
        .map_err(|e| format!("cannot create artifact root: {e}"))?;
    let lock = paths.latest.join("latest.json.lock");
    let _guard = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|e| format!("latest pointer is busy or unavailable: {e}"))?;
    let json = serde_json::to_vec_pretty(metadata).map_err(|e| e.to_string())?;
    let token = format!(
        "{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        LATEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let temp = paths.latest.join(format!(".latest-{token}.json.tmp"));
    let result = (|| {
        ensure_regular_or_absent(&temp)?;
        let mut file = std::fs::File::create(&temp)
            .map_err(|e| format!("cannot stage latest pointer: {e}"))?;
        file.write_all(&json)
            .map_err(|e| format!("cannot stage latest pointer: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("cannot flush latest pointer: {e}"))?;
        ensure_regular_or_absent(&paths.latest_json)?;
        #[cfg(windows)]
        if paths.latest_json.exists() {
            std::fs::remove_file(&paths.latest_json)
                .map_err(|e| format!("cannot replace latest pointer: {e}"))?;
        }
        std::fs::rename(&temp, &paths.latest_json)
            .map_err(|e| format!("cannot install latest pointer: {e}"))
    })();
    let _ = std::fs::remove_file(&temp);
    let _ = std::fs::remove_file(&lock);
    result
}

pub fn read_latest(
    paths: &ArtifactPaths,
    expected_generated_input_digest: &str,
) -> Result<LatestMetadata, String> {
    validate_path_chain(&paths.latest)?;
    let metadata: LatestMetadata = serde_json::from_slice(
        &std::fs::read(&paths.latest_json)
            .map_err(|e| format!("cannot read latest pointer: {e}"))?,
    )
    .map_err(|e| format!("invalid latest pointer: {e}"))?;
    validate_latest_metadata(paths, &metadata, expected_generated_input_digest)?;
    Ok(metadata)
}

fn validate_latest_metadata(
    paths: &ArtifactPaths,
    metadata: &LatestMetadata,
    expected_digest: &str,
) -> Result<(), String> {
    let expected_artifact = paths.uf2.file_name().and_then(|v| v.to_str()).unwrap_or("");
    if metadata.format != "sensor-watch-latest-v1"
        || metadata.board
            != paths
                .root
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .and_then(|v| v.to_str())
                .unwrap_or("")
        || metadata.revision
            != paths
                .root
                .parent()
                .and_then(Path::file_name)
                .and_then(|v| v.to_str())
                .unwrap_or("")
        || metadata.profile
            != paths
                .latest
                .parent()
                .and_then(Path::file_name)
                .and_then(|v| v.to_str())
                .unwrap_or("")
        || metadata.artifact != expected_artifact
        || metadata.generated_input_digest != expected_digest
    {
        return Err("latest pointer does not match the expected board, revision, profile, artifact, or generated-input digest".into());
    }
    Ok(())
}

fn ensure_regular_or_absent(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() || is_reparse_point(&meta) => Err(format!(
            "refusing link or reparse point: {}",
            path.display()
        )),
        Ok(meta) if !meta.is_file() => Err(format!(
            "artifact path is not a regular file: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot inspect artifact path: {e}")),
    }
}

fn validate_path_chain(path: &Path) -> Result<(), String> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if let Ok(meta) = std::fs::symlink_metadata(candidate) {
            if meta.file_type().is_symlink() || is_reparse_point(&meta) {
                return Err(format!(
                    "artifact root contains a link or reparse point: {}",
                    candidate.display()
                ));
            }
        }
        current = candidate.parent();
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_meta: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "studio-storage-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn portable_marker_and_cli_context_are_explicit() {
        let root = temp("portable");
        let launcher = root.join("app/studio.exe");
        std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
        std::fs::write(&launcher, b"exe").unwrap();
        assert!(!has_valid_portable_flag(&launcher));
        std::fs::write(launcher.parent().unwrap().join(PORTABLE_FLAG), "portable\n").unwrap();
        assert!(has_valid_portable_flag(&launcher));
        let roots = roots(
            &launcher,
            Some(&root),
            StorageMode::Installed,
            false,
            temp("installed"),
        );
        assert_eq!(roots.mode, StorageMode::Portable);
        assert_eq!(roots.user_data_root, root.join("user-data"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn board_profile_separation_and_atomic_pointer() {
        let root = temp("artifacts");
        let a = artifact_paths(&root, "Green", "rev-a", "stock").unwrap();
        let b = artifact_paths(&root, "Blue", "rev-a", "stock").unwrap();
        assert_ne!(a.latest, b.latest);
        write_latest_atomic(
            &a,
            &LatestMetadata {
                format: "sensor-watch-latest-v1".into(),
                board: "Green".into(),
                revision: "rev-a".into(),
                profile: "stock".into(),
                generated_input_digest: "abc".into(),
                artifact: "sensor-watch.uf2".into(),
            },
        )
        .unwrap();
        assert!(a.latest_json.exists());
        assert!(!a.latest_json.with_extension("json.tmp").exists());
        assert_eq!(read_latest(&a, "abc").unwrap().board, "Green");
        assert!(read_latest(&a, "changed").is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_traversal_and_link_overlap() {
        let root = temp("unsafe");
        assert!(artifact_paths(&root, "..", "rev", "profile").is_err());
        std::fs::create_dir_all(root.join("firmware")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("firmware"), root.join("firmware-link")).unwrap();
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
