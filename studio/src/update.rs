//! Bounded, offline-capable Studio self-update foundation.
//!
//! This module deliberately has no network client and never replaces the running
//! executable. A caller supplies a local package directory (or a future safe
//! transport adapter) and an authenticator. Packages are copied into an
//! immutable, versioned directory and activated by atomically replacing one
//! small state file.

use sensor_watch_desktop_update::Authentication;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MANIFEST_FILE: &str = "sensor-watch-package.json";
const STATE_FILE: &str = "update-state.json";
const STATE_LOCK_FILE: &str = "update-state.lock";
const STARTUP_SUCCESS_FILE: &str = "startup-success";
static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);
static STARTUP_STATUS: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Arguments and environment supplied by the standalone launcher.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StartupContext {
    pub marker: Option<PathBuf>,
    pub version: Option<String>,
    pub attempt: Option<String>,
    pub user_data: Option<PathBuf>,
    pub package_root: Option<PathBuf>,
    /// Explicit launcher context; portable mode is never inferred from the exe.
    pub portable: bool,
}

pub fn startup_status() -> Option<&'static str> {
    STARTUP_STATUS.get().map(String::as_str)
}

pub fn record_startup_status(message: impl Into<String>) {
    let _ = STARTUP_STATUS.set(message.into());
}

/// Parse launcher arguments without treating them as Studio CLI commands.
pub fn parse_startup_context<I, S>(args: I) -> (StartupContext, Vec<String>)
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut context = StartupContext::default();
    let mut remaining = Vec::new();
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        let (key, value) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(k, v)| (k, Some(v)));
        let value = value.map(str::to_owned).or_else(|| {
            matches!(
                key,
                "--sensor-watch-startup-marker"
                    | "--sensor-watch-version"
                    | "--sensor-watch-startup-attempt"
                    | "--sensor-watch-user-data"
                    | "--sensor-watch-package-root"
            )
            .then(|| args.next())
            .flatten()
        });
        match key {
            "--sensor-watch-startup-marker" => context.marker = value.map(PathBuf::from),
            "--sensor-watch-version" => context.version = value,
            "--sensor-watch-startup-attempt" => context.attempt = value,
            "--sensor-watch-user-data" => context.user_data = value.map(PathBuf::from),
            "--sensor-watch-package-root" => context.package_root = value.map(PathBuf::from),
            "--portable" => {
                context.portable = value.as_deref().map_or(true, |v| v == "1" || v == "true");
            }
            _ => remaining.push(arg),
        }
    }
    if context.marker.is_none() {
        context.marker = std::env::var_os("SENSOR_WATCH_STARTUP_MARKER").map(PathBuf::from);
    }
    if context.user_data.is_none() {
        context.user_data = std::env::var_os("SENSOR_WATCH_USER_DATA").map(PathBuf::from);
    }
    if context.package_root.is_none() {
        context.package_root = std::env::var_os("SENSOR_WATCH_PACKAGE_ROOT").map(PathBuf::from);
    }
    if !context.portable {
        context.portable = std::env::var("SENSOR_WATCH_PORTABLE")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
    }
    (context, remaining)
}

/// Atomically acknowledge the exact launcher attempt. A supplied version must
/// match the running Studio version; mismatches fail closed and are never acked.
pub fn mark_startup_success(context: &StartupContext) -> Result<(), UpdateError> {
    let Some(marker) = context.marker.as_ref() else {
        return Ok(());
    };
    if let Some(version) = context.version.as_deref() {
        let actual = env!("CARGO_PKG_VERSION");
        if version != actual {
            let message = format!("Launcher requested Studio {version}, running {actual}");
            record_startup_status(format!("Startup acknowledgement rejected: {message}"));
            return Err(UpdateError::Manifest(message));
        }
    }
    let user_data = context.user_data.as_ref().ok_or_else(|| {
        UpdateError::Manifest("startup acknowledgement requires an explicit user-data path".into())
    })?;
    let state_root = user_data.join("update-state");
    ensure_state_root(&state_root)?;
    let state_root_real = state_root.canonicalize()?;
    let marker_parent = marker
        .parent()
        .ok_or_else(|| UpdateError::PathTraversal("startup marker has no parent".into()))?
        .canonicalize()?;
    if marker_parent != state_root_real
        || marker.file_name().and_then(|name| name.to_str()).is_none()
    {
        return Err(UpdateError::PathTraversal(
            "startup marker is outside the user-data update-state directory".into(),
        ));
    }
    atomic_write(&state_root_real.join(marker.file_name().unwrap()), b"ok\n")?;
    let detail = match (&context.version, &context.attempt) {
        (Some(version), Some(attempt)) => {
            format!("Startup succeeded for version {version}, attempt {attempt}")
        }
        (Some(version), None) => format!("Startup succeeded for version {version}"),
        _ => "Startup succeeded".into(),
    };
    record_startup_status(detail);
    Ok(())
}

fn unique_token() -> String {
    format!(
        "{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        STAGE_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

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

/// Authentication is intentionally an interface: no key means fail closed.
pub trait MetadataAuthenticator {
    fn authenticate(&self, manifest_bytes: &[u8], manifest: &PackageManifest) -> Authentication;
}

pub struct NoAuthenticator;
impl MetadataAuthenticator for NoAuthenticator {
    fn authenticate(&self, _: &[u8], _: &PackageManifest) -> Authentication {
        Authentication::Unsupported
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
            Authentication::Authenticated { .. } => {}
            Authentication::Unsupported => return Err(UpdateError::AuthenticationUnsupported),
            Authentication::Untrusted(e) => return Err(UpdateError::UntrustedMetadata(e)),
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
        let current = staged
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| UpdateError::Manifest("staged package has an invalid name".into()))?;
        validate_state_entry(current)?;
        let next = UpdateState {
            current: Some(current.to_owned()),
            previous: state.current,
        };
        self.write_state(&next)
    }

    pub fn mark_startup_success(&self) -> Result<(), UpdateError> {
        let current = self
            .read_state()?
            .current
            .ok_or(UpdateError::Manifest("no current package".into()))?;
        validate_state_entry(&current)?;
        let state_root = self.state_root();
        ensure_state_root(&state_root)?;
        atomic_write(
            &state_root.join(format!("{STARTUP_SUCCESS_FILE}-{current}")),
            b"ok\n",
        )
    }

    /// Call once at startup. An activated version without its marker is rolled back.
    pub fn recover_failed_startup(&self) -> Result<Option<PathBuf>, UpdateError> {
        let state = self.read_state()?;
        let Some(current) = state.current.as_ref() else {
            return Ok(None);
        };
        validate_state_entry(current)?;
        if let Some(previous) = state.previous.as_deref() {
            validate_state_entry(previous)?;
        }
        if state.previous.is_some()
            && !self
                .state_root()
                .join(format!("{STARTUP_SUCCESS_FILE}-{current}"))
                .is_file()
        {
            let previous = state.previous.clone().unwrap();
            self.write_state(&UpdateState {
                current: Some(previous.clone()),
                previous: Some(current.clone()),
            })?;
            return Ok(Some(self.version_path(&previous)?));
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
        self.version_path(&previous)
    }

    pub fn current_path(&self) -> Result<Option<PathBuf>, UpdateError> {
        self.read_state()?
            .current
            .map(|value| self.version_path(&value))
            .transpose()
    }
    fn state_root(&self) -> PathBuf {
        self.user_data.join("update-state")
    }
    fn version_path(&self, value: &str) -> Result<PathBuf, UpdateError> {
        validate_state_entry(value)?;
        Ok(self.root.join(value))
    }
    fn read_state(&self) -> Result<UpdateState, UpdateError> {
        let path = self.state_root().join(STATE_FILE);
        if !path.exists() {
            return Ok(UpdateState::default());
        }
        let state: UpdateState = serde_json::from_slice(&fs::read(path)?)?;
        if let Some(value) = state.current.as_deref() {
            validate_state_entry(value)?;
        }
        if let Some(value) = state.previous.as_deref() {
            validate_state_entry(value)?;
        }
        Ok(state)
    }
    fn write_state(&self, state: &UpdateState) -> Result<(), UpdateError> {
        ensure_state_root(&self.state_root())?;
        if let Some(value) = state.current.as_deref() {
            validate_state_entry(value)?;
        }
        if let Some(value) = state.previous.as_deref() {
            validate_state_entry(value)?;
        }
        let bytes = serde_json::to_vec_pretty(state)?;
        atomic_write(&self.state_root().join(STATE_FILE), &bytes)
    }
}

fn validate_state_entry(value: &str) -> Result<(), UpdateError> {
    validate_relative_path(Path::new(value))?;
    if Path::new(value).components().count() != 1 {
        return Err(UpdateError::PathTraversal(value.into()));
    }
    Ok(())
}

fn ensure_state_root(path: &Path) -> Result<(), UpdateError> {
    fs::create_dir_all(path)?;
    let mut current = Some(path);
    while let Some(candidate) = current {
        let metadata = fs::symlink_metadata(candidate)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) || !metadata.is_dir() {
            return Err(UpdateError::PathTraversal(candidate.display().to_string()));
        }
        current = candidate.parent();
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), UpdateError> {
    let parent = path
        .parent()
        .ok_or_else(|| UpdateError::Manifest("state path has no parent".into()))?;
    ensure_state_root(parent)?;
    let lock = parent.join(STATE_LOCK_FILE);
    let _guard = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|e| UpdateError::Manifest(format!("update state is busy or unavailable: {e}")))?;
    let temp = parent.join(format!(
        ".{}.pending-{}",
        path.file_name().unwrap().to_string_lossy(),
        unique_token()
    ));
    let result = (|| {
        let mut file = fs::File::create(&temp)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
        #[cfg(windows)]
        if path.exists() {
            fs::remove_file(path)?;
        }
        fs::rename(&temp, path)?;
        Ok::<(), io::Error>(())
    })();
    let _ = fs::remove_file(&temp);
    let _ = fs::remove_file(&lock);
    result.map_err(UpdateError::Io)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
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
        let metadata = entry.metadata()?;
        let target = destination.join(entry.file_name());
        if ty.is_symlink() || is_reparse_point(&metadata) {
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
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
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
        fn authenticate(&self, _: &[u8], _: &PackageManifest) -> Authentication {
            Authentication::Authenticated {
                key_id: "test".into(),
            }
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
    fn launcher_arguments_are_separated_from_studio_commands() {
        let marker = temp("attempt.marker");
        let (context, remaining) = parse_startup_context([
            "--sensor-watch-startup-marker",
            marker.to_str().unwrap(),
            "--sensor-watch-version=0.1.0",
            "--sensor-watch-startup-attempt=7",
            "--sensor-watch-user-data",
            "C:/studio-user-data",
            "--portable",
            "status",
        ]);
        assert_eq!(context.marker, Some(marker));
        assert_eq!(context.version.as_deref(), Some("0.1.0"));
        assert_eq!(context.attempt.as_deref(), Some("7"));
        assert_eq!(
            context.user_data,
            Some(PathBuf::from("C:/studio-user-data"))
        );
        assert!(context.portable);
        assert_eq!(remaining, vec!["status"]);
    }

    #[test]
    fn startup_success_is_atomic_and_rejects_wrong_version() {
        let marker = temp("marker");
        let user_data = temp("startup-user");
        let wrong = StartupContext {
            marker: Some(marker.clone()),
            version: Some("99.0.0".into()),
            attempt: Some("attempt-1".into()),
            user_data: Some(user_data.clone()),
            package_root: None,
            portable: false,
        };
        let marker = user_data
            .join("update-state")
            .join("startup-success-attempt-1");
        let wrong = StartupContext {
            marker: Some(marker.clone()),
            ..wrong
        };
        assert!(matches!(
            mark_startup_success(&wrong),
            Err(UpdateError::Manifest(_))
        ));
        assert!(!marker.exists());

        let valid = StartupContext {
            version: Some(env!("CARGO_PKG_VERSION").into()),
            ..wrong
        };
        mark_startup_success(&valid).unwrap();
        assert_eq!(fs::read(&marker).unwrap(), b"ok\n");
        let _ = fs::remove_dir_all(marker.parent().unwrap());
    }

    #[test]
    fn corrupt_state_is_reported_fail_closed() {
        let root = temp("corrupt-state");
        let user = temp("corrupt-state-user");
        fs::create_dir_all(user.join("update-state")).unwrap();
        fs::write(user.join("update-state").join(STATE_FILE), b"not-json").unwrap();
        let manager = UpdateManager::new(&root, &user);
        assert!(matches!(manager.current_path(), Err(UpdateError::Json(_))));
        clean(&[&root, manager.user_data_root()]);
    }

    #[test]
    fn persisted_version_entries_must_be_single_safe_names() {
        let root = temp("unsafe-state-root");
        let user = temp("unsafe-state-user");
        fs::create_dir_all(user.join("update-state")).unwrap();
        fs::write(
            user.join("update-state").join(STATE_FILE),
            br#"{"current":"../outside","previous":"C:\\evil"}"#,
        )
        .unwrap();
        let manager = UpdateManager::new(&root, &user);
        assert!(matches!(
            manager.current_path(),
            Err(UpdateError::PathTraversal(_))
        ));
        clean(&[&root, &user]);
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
