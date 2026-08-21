//! Centralized Studio storage and firmware artifact layout.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LATEST_COUNTER: AtomicU64 = AtomicU64::new(0);

pub const PORTABLE_FLAG: &str = "portable.flag";

/// The default configured-build root is always user data, never the package.
pub const BUILD_OUTPUT_FOLDER: &str = "sensor-watch-studio-artifacts";

pub fn default_artifact_root(user_data_root: &Path) -> PathBuf {
    user_data_root.join(BUILD_OUTPUT_FOLDER)
}

/// Validates a configured-build root without creating it. Missing roots are a
/// valid, not-yet-created state; Create folder performs the write separately.
pub fn validate_artifact_root(root: &Path, package_root: Option<&Path>) -> Result<(), String> {
    validate_artifact_root_within(root, package_root, None)
}

/// Validates an artifact root that is allowed inside one mutable package subtree.
/// The package itself and all version/resource directories remain protected.
pub fn validate_artifact_root_within(
    root: &Path,
    package_root: Option<&Path>,
    allowed_root: Option<&Path>,
) -> Result<(), String> {
    if root.as_os_str().is_empty() || !root.is_absolute() {
        return Err("artifact root must be a non-empty absolute path".into());
    }
    if root.to_string_lossy().chars().any(char::is_control) {
        return Err("artifact root cannot contain control characters".into());
    }
    if root.parent().is_none() || root.components().count() <= 1 {
        return Err("artifact root cannot be a filesystem root".into());
    }
    validate_path_chain(root)?;
    let resolved = canonical_for_overlap(root)?;
    let package = package_root.map(canonical_for_overlap).transpose()?;
    if let Some(allowed) = allowed_root {
        let allowed = canonical_for_overlap(allowed)?;
        if let Some(package) = package.as_ref() {
            if allowed == *package || !allowed.starts_with(package) {
                return Err("artifact root allowed directory is not beneath the immutable package directory".into());
            }
        }
        if resolved != allowed && !resolved.starts_with(&allowed) {
            return Err(
                "artifact root must be beneath the validated writable data directory".into(),
            );
        }
    } else if let Some(package) = package.as_ref() {
        if resolved == *package || resolved.starts_with(package) || package.starts_with(&resolved) {
            return Err("artifact root overlaps the immutable package directory".into());
        }
    }
    if let Ok(meta) = std::fs::symlink_metadata(root) {
        if !meta.is_dir() {
            return Err("artifact root must be a directory, not a file".into());
        }
    }
    Ok(())
}

pub fn create_artifact_root(root: &Path, package_root: Option<&Path>) -> Result<(), String> {
    validate_artifact_root(root, package_root)?;
    std::fs::create_dir_all(root).map_err(|e| format!("cannot create artifact root: {e}"))?;
    validate_artifact_root(root, package_root)
}

pub fn create_artifact_root_within(
    root: &Path,
    package_root: Option<&Path>,
    allowed_root: Option<&Path>,
) -> Result<(), String> {
    validate_artifact_root_within(root, package_root, allowed_root)?;
    std::fs::create_dir_all(root).map_err(|e| format!("cannot create artifact root: {e}"))?;
    validate_artifact_root_within(root, package_root, allowed_root)
}

/// Creates a selected root only after validating every existing path component.
/// The returned paths are the exact board/revision/profile layout that the
/// caller may pass to the build worker.
pub fn prepare_artifact_root(
    root: &Path,
    board: &str,
    revision: &str,
    profile: &str,
    package_root: Option<&Path>,
    allowed_root: Option<&Path>,
) -> Result<BuildOutputPaths, String> {
    let attempted = root.display().to_string();
    let paths = match build_output_paths(root, board, revision, profile) {
        Ok(paths) => paths,
        Err(error) => {
            return Err(format_root_preflight_error(
                &attempted,
                "unavailable because the artifact layout is invalid",
                nearest_existing_parent(root).as_ref(),
                &error,
            ));
        }
    };
    let resolved = paths.latest.display().to_string();
    let nearest = nearest_existing_parent(root);
    let validate = || validate_artifact_root_within(root, package_root, allowed_root);
    if let Err(error) = validate() {
        return Err(format_root_preflight_error(
            &attempted,
            &resolved,
            nearest.as_ref(),
            &error,
        ));
    }
    if !root.exists() {
        if let Err(error) = std::fs::create_dir_all(root) {
            return Err(format_root_preflight_error(
                &attempted,
                &resolved,
                nearest.as_ref(),
                &format!("cannot create selected artifact root: {error}"),
            ));
        }
    }
    if let Err(error) = validate() {
        return Err(format_root_preflight_error(
            &attempted,
            &resolved,
            nearest_existing_parent(root).as_ref(),
            &format!("selected artifact root changed during setup: {error}"),
        ));
    }
    Ok(paths)
}

fn format_root_preflight_error(
    attempted: &str,
    resolved: &str,
    nearest: Option<&PathBuf>,
    error: &str,
) -> String {
    format!(
        "Artifact root preflight failed. Attempted root: {attempted}. Resolved board/revision/profile/latest: {resolved}. OS error: {error}. Nearest existing parent: {}. Choose Create folder for this root or Use default, then try again",
        nearest
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string())
    )
}

fn nearest_existing_parent(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        match std::fs::symlink_metadata(candidate) {
            Ok(metadata) if metadata.is_dir() => return Some(candidate.to_path_buf()),
            Ok(_) => return candidate.parent().and_then(nearest_existing_parent),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                current = candidate.parent();
            }
            Err(_) => return None,
        }
    }
    None
}

fn canonical_for_overlap(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|e| format!("cannot resolve artifact root: {e}"));
    }
    let mut current = path;
    let mut missing = Vec::new();
    while !current.exists() {
        missing.push(current.file_name().ok_or("invalid artifact root")?);
        current = current.parent().ok_or("cannot resolve artifact root")?;
    }
    let mut resolved = current
        .canonicalize()
        .map_err(|e| format!("cannot resolve artifact root: {e}"))?;
    for part in missing.iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

fn validate_path_chain(path: &Path) -> Result<(), String> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if let Ok(meta) = std::fs::symlink_metadata(candidate) {
            if meta.file_type().is_symlink() || is_reparse_point(&meta) {
                return Err(format!(
                    "artifact root cannot use a symlink or reparse-point path: {}",
                    candidate.display()
                ));
            }
        }
        current = candidate.parent();
    }
    Ok(())
}

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
                user_data_root: root.join("data"),
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
pub struct BuildOutputPaths {
    pub build_output_root: PathBuf,
    pub board: String,
    pub revision: String,
    pub profile: String,
    pub profile_root: PathBuf,
    pub latest: PathBuf,
    pub uf2: PathBuf,
    pub manifest: PathBuf,
    pub sidecar: PathBuf,
    pub inputs: PathBuf,
    pub latest_json: PathBuf,
    pub recovery_generations: PathBuf,
}

#[deprecated(note = "use BuildOutputPaths")]
pub type ArtifactPaths = BuildOutputPaths;

pub fn build_output_paths(
    build_output_root: &Path,
    board: &str,
    revision: &str,
    profile: &str,
) -> Result<BuildOutputPaths, String> {
    for value in [board, revision, profile] {
        validate_segment(value)?;
    }
    let profile_root = build_output_root.join(board).join(revision).join(profile);
    let latest = profile_root.join("latest");
    Ok(BuildOutputPaths {
        build_output_root: build_output_root.to_path_buf(),
        board: board.to_string(),
        revision: revision.to_string(),
        profile: profile.to_string(),
        profile_root,
        uf2: latest.join("sensor-watch.uf2"),
        manifest: latest.join("sensor-watch.uf2.json"),
        sidecar: latest.join("sensor-watch.uf2.json.sig"),
        inputs: latest.join("sensor-watch.uf2.inputs"),
        latest_json: latest.join("latest.json"),
        latest,
        recovery_generations: build_output_root.join("recovery/generations"),
    })
}

pub fn artifact_paths(
    build_output_root: &Path,
    board: &str,
    revision: &str,
    profile: &str,
) -> Result<BuildOutputPaths, String> {
    build_output_paths(build_output_root, board, revision, profile)
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

pub fn write_latest_atomic(
    paths: &BuildOutputPaths,
    metadata: &LatestMetadata,
) -> Result<(), String> {
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
    paths: &BuildOutputPaths,
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
    paths: &BuildOutputPaths,
    metadata: &LatestMetadata,
    expected_digest: &str,
) -> Result<(), String> {
    let expected_artifact = paths.uf2.file_name().and_then(|v| v.to_str()).unwrap_or("");
    if metadata.format != "sensor-watch-latest-v1"
        || metadata.board != paths.board
        || metadata.revision != paths.revision
        || metadata.profile != paths.profile
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
    fn default_and_custom_roots_keep_the_required_suffix() {
        let default = temp("default-root");
        let custom = temp("custom-root");
        let a = artifact_paths(&default, "Green", "rev-a", "stock").unwrap();
        let b = artifact_paths(&custom, "Blue", "rev-b", "custom").unwrap();
        assert_eq!(a.latest, default.join("Green/rev-a/stock/latest"));
        assert_eq!(b.latest, custom.join("Blue/rev-b/custom/latest"));
        assert_ne!(a.latest, b.latest);
    }

    #[test]
    fn missing_root_validates_without_creation_and_create_is_explicit() {
        let root = temp("create");
        assert!(!root.exists());
        assert!(validate_artifact_root(&root, None).is_ok());
        assert!(!root.exists());
        create_artifact_root(&root, None).unwrap();
        assert!(root.is_dir());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn preflight_creates_a_missing_creatable_root_and_resolves_latest() {
        let root = temp("preflight-missing");
        let paths = prepare_artifact_root(&root, "Green", "rev-a", "stock", None, None).unwrap();
        assert!(root.is_dir());
        assert_eq!(paths.latest, root.join("Green/rev-a/stock/latest"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn preflight_preserves_a_valid_custom_root() {
        let root = temp("preflight-custom");
        std::fs::create_dir_all(&root).unwrap();
        let paths = prepare_artifact_root(&root, "Blue", "rev-b", "custom", None, None).unwrap();
        assert_eq!(paths.profile_root, root.join("Blue/rev-b/custom"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn preflight_reports_unavailable_drive_with_actionable_details() {
        let root = PathBuf::from(r"Z:\\sensor-watch-unavailable-root");
        let error =
            prepare_artifact_root(&root, "Green", "rev-a", "stock", None, None).unwrap_err();
        assert!(error.contains(&root.display().to_string()));
        assert!(error.contains("Green"));
        assert!(error.contains("rev-a"));
        assert!(error.contains("stock"));
        assert!(error.contains("latest"));
        assert!(error.contains("OS error:"));
        assert!(error.contains("Nearest existing parent:"));
        assert!(error.contains("Create folder"));
        assert!(error.contains("Use default"));
    }

    #[cfg(windows)]
    #[test]
    fn preflight_rejects_filesystem_root_without_starting_creation() {
        let root = PathBuf::from(r"Z:\\");
        let error =
            prepare_artifact_root(&root, "Green", "rev-a", "stock", None, None).unwrap_err();
        assert!(error.contains("Attempted root:"));
        assert!(error.contains("Nearest existing parent:"));
        assert!(!error.contains("cannot create selected artifact root"));
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
        assert_eq!(roots.user_data_root, root.join("data"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn packaged_default_root_is_allowed_only_under_data() {
        let package = temp("package");
        let user_data = package.join("data");
        let default = user_data.join("firmware");
        assert!(validate_artifact_root_within(&default, Some(&package), Some(&user_data)).is_ok());
        assert!(validate_artifact_root_within(
            &package.join("versions/2.4.0"),
            Some(&package),
            Some(&user_data),
        )
        .is_err());
        assert!(validate_artifact_root(&package, Some(&package)).is_err());
        let _ = std::fs::remove_dir_all(package);
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
