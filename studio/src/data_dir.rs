//! Validation and transactional migration for the user-selected Studio root.

use std::path::{Path, PathBuf};

pub const SETTINGS_FILE: &str = "studio-settings.json";
pub const RESTORE_FILE: &str = "studio-restore-points.json";
pub const RUNTIME_FILE: &str = "studio-runtime.json";

pub fn default_path() -> PathBuf {
    super::test_runtime::normal_config_dir()
}

pub fn validate_packaged_root(path: &Path, package_root: &Path) -> Result<(), String> {
    let expected = package_root.join("data");
    let candidate = canonical_for_overlap(path)?;
    let expected = canonical_for_overlap(&expected)?;
    if candidate != expected {
        return Err(format!(
            "packaged data root must be exactly {}",
            expected.display()
        ));
    }
    validate(path, &[])
}

pub fn validate(path: &Path, protected: &[&Path]) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("Studio data folder cannot be empty".into());
    }
    if !path.is_absolute() {
        return Err("Studio data folder must be an absolute path".into());
    }
    if path.to_string_lossy().chars().any(char::is_control) {
        return Err("Studio data folder cannot contain control characters".into());
    }
    if path.parent().is_none() || path.components().count() <= 1 {
        return Err("Studio data folder cannot be a filesystem root".into());
    }
    validate_path_chain(path)?;
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.is_dir() {
            return Err("Studio data folder must be a directory, not a file".into());
        }
    }
    let candidate = canonical_for_overlap(path)?;
    for protected_path in protected {
        validate_path_chain(protected_path)?;
        let protected = canonical_for_overlap(protected_path)?;
        if candidate == protected
            || candidate.starts_with(&protected)
            || protected.starts_with(&candidate)
        {
            return Err(format!(
                "Studio data folder overlaps protected path: {}",
                protected_path.display()
            ));
        }
    }
    probe_writable(path)
}

fn validate_path_chain(path: &Path) -> Result<(), String> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        match std::fs::symlink_metadata(candidate) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
                    return Err(format!(
                        "Studio data folder cannot use a symlink or reparse-point path: {}",
                        candidate.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("cannot inspect Studio data folder path: {error}"));
            }
        }
        current = candidate.parent();
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn canonical_for_overlap(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|e| format!("cannot resolve {}: {e}", path.display()));
    }
    let mut current = path;
    let mut missing = Vec::new();
    while !current.exists() {
        missing.push(
            current
                .file_name()
                .ok_or_else(|| "invalid Studio data folder".to_string())?,
        );
        current = current
            .parent()
            .ok_or_else(|| "cannot resolve Studio data folder".to_string())?;
    }
    let mut result = current
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", current.display()))?;
    for component in missing.iter().rev() {
        result.push(component);
    }
    Ok(result)
}

fn probe_writable(path: &Path) -> Result<(), String> {
    let mut created = false;
    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| format!("cannot create Studio data folder: {e}"))?;
        created = true;
    }
    let probe = path.join(".studio-write-probe");
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map(|_| ())
        .map_err(|e| format!("Studio data folder is not writable: {e}"));
    let _ = std::fs::remove_file(&probe);
    if created {
        // The newly created selected root is intentionally retained for next launch.
    }
    result
}

pub fn migrate(old_root: &Path, new_root: &Path) -> Result<(), String> {
    validate(new_root, &[])?;
    std::fs::create_dir_all(new_root)
        .map_err(|e| format!("cannot create new Studio data folder: {e}"))?;
    for name in [SETTINGS_FILE, RESTORE_FILE, RUNTIME_FILE] {
        let source = old_root.join(name);
        if !source.exists() {
            continue;
        }
        let source_meta = std::fs::symlink_metadata(&source)
            .map_err(|e| format!("cannot inspect {name}: {e}"))?;
        if !source_meta.is_file()
            || source_meta.file_type().is_symlink()
            || is_reparse_point(&source_meta)
        {
            return Err(format!("refusing to migrate linked or non-file {name}"));
        }
        let target = new_root.join(name);
        if target.exists() {
            continue;
        }
        let temp = target.with_extension("json.copying");
        let _ = std::fs::remove_file(&temp);
        std::fs::copy(&source, &temp).map_err(|e| format!("cannot copy {name}: {e}"))?;
        let a = std::fs::read(&source).map_err(|e| e.to_string())?;
        let b = std::fs::read(&temp).map_err(|e| e.to_string())?;
        if a != b {
            let _ = std::fs::remove_file(&temp);
            return Err(format!("verification failed while copying {name}"));
        }
        std::fs::rename(&temp, &target).map_err(|e| format!("cannot install {name}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "studio-data-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn default_is_normal_config_dir() {
        assert_eq!(
            default_path(),
            super::super::test_runtime::normal_config_dir()
        );
    }

    #[test]
    fn packaged_root_is_exact_and_rejects_protected_overlap() {
        let package = temp("packaged");
        let data = package.join("data");
        assert!(validate_packaged_root(&data, &package).is_ok());
        assert!(validate_packaged_root(&package, &package).is_err());
        assert!(validate_packaged_root(&package.join("versions/2.4.0"), &package).is_err());
        let _ = std::fs::remove_dir_all(package);
    }
    #[test]
    fn rejects_relative_empty_and_file() {
        assert!(validate(Path::new("relative"), &[]).is_err());
        assert!(validate(Path::new(""), &[]).is_err());
        assert!(validate(Path::new("/"), &[]).is_err());
        assert!(validate(Path::new("bad\npath"), &[]).is_err());
        let p = temp("file");
        std::fs::write(&p, b"x").unwrap();
        assert!(validate(&p, &[]).is_err());
        let _ = std::fs::remove_file(p);
    }
    #[test]
    fn rejects_overlap_and_preserves_old_on_copy_failure() {
        let old = temp("old");
        let new = temp("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join(SETTINGS_FILE), b"old").unwrap();
        assert!(validate(&old, &[old.as_path()]).is_err());
        assert!(validate(&new, &[old.as_path()]).is_ok());
        assert!(migrate(&old, &new).is_ok());
        assert_eq!(std::fs::read(new.join(SETTINGS_FILE)).unwrap(), b"old");
        let bad_old = temp("bad-old");
        let failed = temp("copy-failure");
        std::fs::create_dir_all(bad_old.join(SETTINGS_FILE)).unwrap();
        assert!(migrate(&bad_old, &failed).is_err());
        assert!(bad_old.join(SETTINGS_FILE).is_dir());
        assert_eq!(std::fs::read(old.join(SETTINGS_FILE)).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(bad_old);
        let _ = std::fs::remove_dir_all(failed);
        let _ = std::fs::remove_dir_all(old);
        let _ = std::fs::remove_dir_all(new);
    }
    #[test]
    fn rejects_existing_reparse_like_path_chain_deterministically() {
        let root = temp("chain");
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        assert!(validate(&nested, &[]).is_ok());
        assert!(validate(&nested, &[root.as_path()]).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn migration_rejects_symlink_source_file() {
        use std::os::unix::fs::symlink;
        let old = temp("migration-link-old");
        let new = temp("migration-link-new");
        std::fs::create_dir_all(&old).unwrap();
        let target = temp("migration-link-target");
        std::fs::write(&target, b"secret").unwrap();
        symlink(&target, old.join(SETTINGS_FILE)).unwrap();
        assert!(migrate(&old, &new).is_err());
        let _ = std::fs::remove_dir_all(old);
        let _ = std::fs::remove_dir_all(new);
        let _ = std::fs::remove_file(target);
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_directory_symlink() {
        use std::os::windows::fs::symlink_dir;
        let real = temp("windows-real");
        let link = temp("windows-link");
        std::fs::create_dir_all(&real).unwrap();
        if symlink_dir(&real, &link).is_ok() {
            assert!(validate(&link, &[]).is_err());
            let _ = std::fs::remove_dir(link);
        }
        let _ = std::fs::remove_dir_all(real);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink() {
        use std::os::unix::fs::symlink;
        let real = temp("real");
        let link = temp("link");
        std::fs::create_dir_all(&real).unwrap();
        symlink(&real, &link).unwrap();
        assert!(validate(&link, &[]).is_err());
        let _ = std::fs::remove_file(link);
        let _ = std::fs::remove_dir_all(real);
    }
}
