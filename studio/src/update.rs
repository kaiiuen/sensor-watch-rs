//! Bounded, offline-capable Studio self-update foundation.
//!
//! This module deliberately has no network client and never replaces the running
//! executable. A caller supplies a local package directory (or a future safe
//! transport adapter) and an authenticator. Packages are copied into an
//! immutable, versioned directory and activated by atomically replacing one
//! small state file.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MANIFEST_FILE: &str = "sensor-watch-package.json";
const STATE_FILE: &str = "update-state.json";
const STATE_TEMP_FILE: &str = "update-state.json.pending";
const STARTUP_SUCCESS_FILE: &str = "startup-success";
static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl PackageVersion {
    pub fn parse(value: &str) -> Result<Self, UpdateError> {
        let mut parts = value.split('.');
        let version = Self {
            major: parts
                .next()
                .ok_or_else(|| UpdateError::InvalidVersion(value.into()))?
                .parse()
                .map_err(|_| UpdateError::InvalidVersion(value.into()))?,
            minor: parts
                .next()
                .ok_or_else(|| UpdateError::InvalidVersion(value.into()))?
                .parse()
                .map_err(|_| UpdateError::InvalidVersion(value.into()))?,
            patch: parts
                .next()
                .ok_or_else(|| UpdateError::InvalidVersion(value.into()))?
                .parse()
                .map_err(|_| UpdateError::InvalidVersion(value.into()))?,
        };
        if parts.next().is_some() {
            return Err(UpdateError::InvalidVersion(value.into()));
        }
        Ok(version)
    }
}

impl std::fmt::Display for PackageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageFile {
    pub path: String,
    /// Lower-case SHA-256 of the package-relative file.
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub schema_version: u32,
    pub version: PackageVersion,
    pub files: Vec<PackageFile>,
}

impl PackageManifest {
    pub fn validate(&self) -> Result<(), UpdateError> {
        if self.schema_version != 1 {
            return Err(UpdateError::UnsupportedSchema(self.schema_version));
        }
        if self.files.is_empty() {
            return Err(UpdateError::Manifest("files must not be empty".into()));
        }
        for file in &self.files {
            validate_relative_path(Path::new(&file.path))?;
            if file.sha256.len() != 64 || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(UpdateError::Manifest(format!(
                    "invalid SHA-256 for {}",
                    file.path
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
struct UpdateState {
    current: Option<String>,
    previous: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthenticationStatus {
    Authenticated,
    Unsupported,
    Untrusted(String),
}

/// Authentication is intentionally an interface: no key means fail closed.
pub trait MetadataAuthenticator {
    fn authenticate(
        &self,
        manifest_bytes: &[u8],
        manifest: &PackageManifest,
    ) -> AuthenticationStatus;
}

pub struct NoAuthenticator;
impl MetadataAuthenticator for NoAuthenticator {
    fn authenticate(&self, _: &[u8], _: &PackageManifest) -> AuthenticationStatus {
        AuthenticationStatus::Unsupported
    }
}

#[derive(Debug)]
pub enum UpdateError {
    Io(io::Error),
    Json(serde_json::Error),
    Manifest(String),
    InvalidVersion(String),
    UnsupportedSchema(u32),
    AuthenticationUnsupported,
    UntrustedMetadata(String),
    PathTraversal(String),
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    MissingFile(String),
    NoPreviousVersion,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "update I/O error: {e}"),
            Self::Json(e) => write!(f, "invalid package metadata: {e}"),
            Self::Manifest(e) => write!(f, "invalid package manifest: {e}"),
            Self::InvalidVersion(v) => write!(f, "invalid package version: {v}"),
            Self::UnsupportedSchema(v) => write!(f, "unsupported package schema: {v}"),
            Self::AuthenticationUnsupported => write!(
                f,
                "package authentication is unsupported: no key/authenticator configured"
            ),
            Self::UntrustedMetadata(e) => write!(f, "package metadata is untrusted: {e}"),
            Self::PathTraversal(p) => write!(f, "package path traversal rejected: {p}"),
            Self::HashMismatch { path, .. } => write!(f, "package hash mismatch: {path}"),
            Self::MissingFile(p) => write!(f, "required package file is missing: {p}"),
            Self::NoPreviousVersion => {
                write!(f, "no previous Studio version is available for rollback")
            }
        }
    }
}
impl std::error::Error for UpdateError {}
impl From<io::Error> for UpdateError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for UpdateError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub struct UpdateManager {
    root: PathBuf,
    user_data: PathBuf,
}
impl UpdateManager {
    pub fn new(root: impl Into<PathBuf>, user_data: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            user_data: user_data.into(),
        }
    }
    pub fn package_root(&self) -> &Path {
        &self.root
    }
    pub fn user_data_root(&self) -> &Path {
        &self.user_data
    }

    pub fn stage<A: MetadataAuthenticator>(
        &self,
        source: &Path,
        auth: &A,
    ) -> Result<PathBuf, UpdateError> {
        let bytes = fs::read(source.join(MANIFEST_FILE))?;
        let manifest: PackageManifest = serde_json::from_slice(&bytes)?;
        manifest.validate()?;
        match auth.authenticate(&bytes, &manifest) {
            AuthenticationStatus::Authenticated => {}
            AuthenticationStatus::Unsupported => {
                return Err(UpdateError::AuthenticationUnsupported)
            }
            AuthenticationStatus::Untrusted(e) => return Err(UpdateError::UntrustedMetadata(e)),
        }
        fs::create_dir_all(&self.root)?;
        let name = format!(
            "{}-{}-{}",
            manifest.version,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            STAGE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let destination = self.root.join(name);
        copy_tree(source, &destination)?;
        validate_package(&destination, &manifest)?;
        Ok(destination)
    }

    pub fn activate(&self, staged: &Path) -> Result<(), UpdateError> {
        let staged = staged.canonicalize()?;
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        if staged.parent() != Some(root.as_path()) {
            return Err(UpdateError::Manifest(
                "staged package is outside the package root".into(),
            ));
        }
        let state = self.read_state()?;
        let next = UpdateState {
            current: Some(staged.file_name().unwrap().to_string_lossy().into_owned()),
            previous: state.current,
        };
        self.write_state(&next)
    }

    pub fn mark_startup_success(&self) -> Result<(), UpdateError> {
        let current = self
            .current_path()?
            .ok_or(UpdateError::Manifest("no current package".into()))?;
        fs::write(current.join(STARTUP_SUCCESS_FILE), b"ok\n")?;
        Ok(())
    }

    /// Call once at startup. An activated version without its marker is rolled back.
    pub fn recover_failed_startup(&self) -> Result<Option<PathBuf>, UpdateError> {
        let state = self.read_state()?;
        let Some(current) = state.current.as_ref() else {
            return Ok(None);
        };
        if state.previous.is_some() && !self.root.join(current).join(STARTUP_SUCCESS_FILE).is_file()
        {
            let previous = state.previous.clone().unwrap();
            self.write_state(&UpdateState {
                current: Some(previous.clone()),
                previous: Some(current.clone()),
            })?;
            return Ok(Some(self.root.join(previous)));
        }
        Ok(None)
    }

    pub fn rollback(&self) -> Result<PathBuf, UpdateError> {
        let state = self.read_state()?;
        let previous = state.previous.ok_or(UpdateError::NoPreviousVersion)?;
        let current = state.current;
        self.write_state(&UpdateState {
            current: Some(previous.clone()),
            previous: current,
        })?;
        Ok(self.root.join(previous))
    }

    pub fn current_path(&self) -> Result<Option<PathBuf>, UpdateError> {
        Ok(self.read_state()?.current.map(|v| self.root.join(v)))
    }
    fn read_state(&self) -> Result<UpdateState, UpdateError> {
        if !self.root.join(STATE_FILE).exists() {
            return Ok(UpdateState::default());
        }
        Ok(serde_json::from_slice(&fs::read(
            self.root.join(STATE_FILE),
        )?)?)
    }
    fn write_state(&self, state: &UpdateState) -> Result<(), UpdateError> {
        fs::create_dir_all(&self.root)?;
        let bytes = serde_json::to_vec_pretty(state)?;
        fs::write(self.root.join(STATE_TEMP_FILE), bytes)?;
        fs::rename(self.root.join(STATE_TEMP_FILE), self.root.join(STATE_FILE))?;
        Ok(())
    }
}

fn validate_relative_path(path: &Path) -> Result<(), UpdateError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(UpdateError::PathTraversal(path.display().to_string()));
    }
    Ok(())
}
fn copy_tree(source: &Path, destination: &Path) -> Result<(), UpdateError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if ty.is_symlink() {
            return Err(UpdateError::PathTraversal(
                entry.path().display().to_string(),
            ));
        }
        if ty.is_dir() {
            copy_tree(&entry.path(), &target)?;
            continue;
        }
        if ty.is_file() {
            fs::copy(entry.path(), target)?;
            continue;
        }
        return Err(UpdateError::Manifest(format!(
            "unsupported package entry: {}",
            entry.path().display()
        )));
    }
    Ok(())
}
fn validate_package(root: &Path, manifest: &PackageManifest) -> Result<(), UpdateError> {
    for file in &manifest.files {
        let path = root.join(&file.path);
        if !path.is_file() {
            return Err(UpdateError::MissingFile(file.path.clone()));
        }
        let actual = hex(&Sha256::digest(fs::read(path)?));
        if actual != file.sha256.to_ascii_lowercase() {
            return Err(UpdateError::HashMismatch {
                path: file.path.clone(),
                expected: file.sha256.clone(),
                actual,
            });
        }
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    struct Trusted;
    impl MetadataAuthenticator for Trusted {
        fn authenticate(&self, _: &[u8], _: &PackageManifest) -> AuthenticationStatus {
            AuthenticationStatus::Authenticated
        }
    }
    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "studio-update-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn package(root: &Path, version: PackageVersion, file: &str, data: &[u8]) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join(file), data).unwrap();
        let hash = hex(&Sha256::digest(data));
        let m = PackageManifest {
            schema_version: 1,
            version,
            files: vec![PackageFile {
                path: file.into(),
                sha256: hash,
            }],
        };
        fs::write(root.join(MANIFEST_FILE), serde_json::to_vec(&m).unwrap()).unwrap();
    }
    fn clean(paths: &[&Path]) {
        for path in paths {
            let _ = fs::remove_dir_all(path);
        }
    }

    #[test]
    fn manifest_validation_is_typed_and_deterministic() {
        let mut m = PackageManifest {
            schema_version: 1,
            version: PackageVersion::parse("1.2.3").unwrap(),
            files: vec![PackageFile {
                path: "app.exe".into(),
                sha256: "a".repeat(64),
            }],
        };
        assert!(m.validate().is_ok());
        m.schema_version = 2;
        assert!(matches!(
            m.validate(),
            Err(UpdateError::UnsupportedSchema(2))
        ));
    }
    #[test]
    fn unsupported_signature_is_explicit() {
        let root = temp("unsupported");
        package(
            &root,
            PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            "app.exe",
            b"x",
        );
        let manager = UpdateManager::new(temp("versions"), temp("user"));
        assert!(matches!(
            manager.stage(&root, &NoAuthenticator),
            Err(UpdateError::AuthenticationUnsupported)
        ));
        clean(&[&root, manager.package_root(), manager.user_data_root()]);
    }
    #[test]
    fn rejects_path_traversal() {
        assert!(matches!(
            validate_relative_path(Path::new("../app.exe")),
            Err(UpdateError::PathTraversal(_))
        ));
        assert!(matches!(
            validate_relative_path(Path::new("/app.exe")),
            Err(UpdateError::PathTraversal(_))
        ));
    }
    #[test]
    fn staging_is_isolated_and_hash_checked() {
        let source = temp("source");
        let root = temp("versions");
        let user = temp("user");
        package(
            &source,
            PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            "app.exe",
            b"x",
        );
        let manager = UpdateManager::new(&root, &user);
        let staged = manager.stage(&source, &Trusted).unwrap();
        assert_ne!(staged, source);
        assert!(staged.starts_with(&root));
        fs::write(source.join("app.exe"), b"changed").unwrap();
        assert_eq!(fs::read(staged.join("app.exe")).unwrap(), b"x");
        clean(&[&source, &root, &user]);
    }
    #[test]
    fn current_previous_and_manual_rollback_are_transactional() {
        let source = temp("source");
        let root = temp("versions");
        let user = temp("user");
        package(
            &source,
            PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            "app.exe",
            b"one",
        );
        let manager = UpdateManager::new(&root, &user);
        let one = manager.stage(&source, &Trusted).unwrap();
        manager.activate(&one).unwrap();
        manager.mark_startup_success().unwrap();
        package(
            &source,
            PackageVersion {
                major: 2,
                minor: 0,
                patch: 0,
            },
            "app.exe",
            b"two",
        );
        let two = manager.stage(&source, &Trusted).unwrap();
        manager.activate(&two).unwrap();
        assert_eq!(manager.current_path().unwrap(), Some(two.clone()));
        assert_eq!(manager.rollback().unwrap(), one);
        clean(&[&source, &root, &user]);
    }
    #[test]
    fn failed_startup_rolls_back_to_previous() {
        let source = temp("source");
        let root = temp("versions");
        let user = temp("user");
        package(
            &source,
            PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            "app.exe",
            b"one",
        );
        let manager = UpdateManager::new(&root, &user);
        let one = manager.stage(&source, &Trusted).unwrap();
        manager.activate(&one).unwrap();
        manager.mark_startup_success().unwrap();
        package(
            &source,
            PackageVersion {
                major: 2,
                minor: 0,
                patch: 0,
            },
            "app.exe",
            b"two",
        );
        let two = manager.stage(&source, &Trusted).unwrap();
        manager.activate(&two).unwrap();
        assert_eq!(manager.recover_failed_startup().unwrap(), Some(one.clone()));
        assert_eq!(manager.current_path().unwrap(), Some(one));
        assert!(two.exists());
        clean(&[&source, &root, &user]);
    }
    #[test]
    fn user_data_is_separate_from_versions() {
        let source = temp("source");
        let root = temp("versions");
        let user = temp("user");
        package(
            &source,
            PackageVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            "app.exe",
            b"x",
        );
        fs::create_dir_all(&user).unwrap();
        fs::write(user.join("projects.json"), b"project").unwrap();
        let manager = UpdateManager::new(&root, &user);
        let staged = manager.stage(&source, &Trusted).unwrap();
        assert!(!staged.join("projects.json").exists());
        assert_eq!(fs::read(user.join("projects.json")).unwrap(), b"project");
        clean(&[&source, &root, &user]);
    }
}
