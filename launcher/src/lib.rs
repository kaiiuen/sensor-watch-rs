//! Safe bootstrapper for a folder-based Studio distribution.
//!
//! The launcher executable is installed once and is never replaced while it is
//! running. Studio versions are immutable sibling directories under `versions`;
//! mutable state lives under `user-data`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

pub const STATE_FILE: &str = "launcher-state.json";
pub const STARTUP_MARKER: &str = "startup-success.marker";
pub const STARTUP_MARKER_ENV: &str = "SENSOR_WATCH_STARTUP_MARKER";
pub const STARTUP_MARKER_ARG: &str = "--sensor-watch-startup-marker";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pointers {
    pub current: Option<String>,
    pub previous: Option<String>,
}

#[derive(Debug)]
pub enum LauncherError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidPointer(String),
    MissingVersion(String),
    NoPreviousVersion,
    StartupFailed(String),
}

impl std::fmt::Display for LauncherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "launcher I/O error: {e}"),
            Self::Json(e) => write!(f, "invalid launcher state: {e}"),
            Self::InvalidPointer(p) => write!(f, "invalid version pointer: {p}"),
            Self::MissingVersion(v) => write!(f, "version directory is missing: {v}"),
            Self::NoPreviousVersion => write!(f, "no valid previous version is available"),
            Self::StartupFailed(e) => write!(f, "Studio startup failed: {e}"),
        }
    }
}
impl std::error::Error for LauncherError {}
impl From<io::Error> for LauncherError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for LauncherError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub struct Launcher {
    root: PathBuf,
    user_data: PathBuf,
    executable_name: String,
}

impl Launcher {
    pub fn new(
        root: impl Into<PathBuf>,
        user_data: impl Into<PathBuf>,
        executable_name: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            user_data: user_data.into(),
            executable_name: executable_name.into(),
        }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn user_data(&self) -> &Path {
        &self.user_data
    }
    pub fn version_root(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn read_pointers(&self) -> Result<Pointers, LauncherError> {
        let path = self.user_data.join(STATE_FILE);
        if !path.is_file() {
            return Ok(Pointers::default());
        }
        let pointers: Pointers = serde_json::from_slice(&fs::read(path)?)?;
        self.validate_pointers(&pointers)?;
        Ok(pointers)
    }
    pub fn switch_current(&self, version: &str) -> Result<Pointers, LauncherError> {
        self.validate_version(version)?;
        let old = self.read_pointers()?;
        let next = Pointers {
            current: Some(version.to_owned()),
            previous: old.current,
        };
        self.write_pointers(&next)?;
        Ok(next)
    }
    pub fn rollback(&self) -> Result<Pointers, LauncherError> {
        let old = self.read_pointers()?;
        let previous = old
            .previous
            .clone()
            .ok_or(LauncherError::NoPreviousVersion)?;
        self.validate_version(&previous)?;
        let next = Pointers {
            current: Some(previous),
            previous: old.current,
        };
        self.write_pointers(&next)?;
        Ok(next)
    }
    pub fn selected_executable(&self) -> Result<PathBuf, LauncherError> {
        let pointers = self.read_pointers()?;
        let current = pointers
            .current
            .ok_or_else(|| LauncherError::MissingVersion("<none>".into()))?;
        self.version_executable(&current)
    }
    pub fn run(&self, timeout: Duration) -> Result<(), LauncherError> {
        let executable = match self.selected_executable() {
            Ok(path) => path,
            Err(error) => {
                let _ = self.rollback();
                return Err(error);
            }
        };
        let marker = self.user_data.join(format!("{}.marker", unique_token()));
        let _ = fs::remove_file(&marker);
        let mut command = Command::new(&executable);
        command
            .arg(format!("{STARTUP_MARKER_ARG}={}", marker.display()))
            .env(STARTUP_MARKER_ENV, &marker)
            .env("SENSOR_WATCH_USER_DATA", &self.user_data)
            .current_dir(self.user_data.parent().unwrap_or(&self.user_data));
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = self.rollback();
                return Err(LauncherError::StartupFailed(error.to_string()));
            }
        };
        let started = wait_for_marker(&mut child, &marker, timeout);
        let _ = fs::remove_file(&marker);
        if started {
            return Ok(());
        }
        let _ = child.kill();
        let _ = child.wait();
        self.rollback()
            .map_err(|e| LauncherError::StartupFailed(e.to_string()))?;
        Err(LauncherError::StartupFailed(
            "startup marker was not received before timeout".into(),
        ))
    }
    fn version_executable(&self, version: &str) -> Result<PathBuf, LauncherError> {
        self.validate_version(version)?;
        let path = self
            .version_root()
            .join(version)
            .join(&self.executable_name);
        if !path.is_file() {
            return Err(LauncherError::MissingVersion(version.into()));
        }
        Ok(path)
    }
    fn validate_version(&self, version: &str) -> Result<(), LauncherError> {
        let path = Path::new(version);
        if version.is_empty()
            || path.components().count() != 1
            || !matches!(path.components().next(), Some(Component::Normal(_)))
        {
            return Err(LauncherError::InvalidPointer(version.into()));
        }
        if !self.version_root().join(version).is_dir() {
            return Err(LauncherError::MissingVersion(version.into()));
        }
        Ok(())
    }
    fn validate_pointers(&self, pointers: &Pointers) -> Result<(), LauncherError> {
        if let Some(v) = &pointers.current {
            self.validate_version(v)?;
        }
        if let Some(v) = &pointers.previous {
            self.validate_version(v)?;
        }
        Ok(())
    }
    fn write_pointers(&self, pointers: &Pointers) -> Result<(), LauncherError> {
        fs::create_dir_all(&self.user_data)?;
        let temp = self
            .user_data
            .join(format!("{STATE_FILE}.pending-{}", unique_token()));
        fs::write(&temp, serde_json::to_vec_pretty(pointers)?)?;
        atomic_replace(&temp, &self.user_data.join(STATE_FILE))
    }
}

fn wait_for_marker(child: &mut Child, marker: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if marker.is_file() && fs::read(marker).map(|b| b == b"ok\n").unwrap_or(false) {
            return true;
        }
        if child.try_wait().ok().flatten().is_some() {
            return false;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(windows)]
fn atomic_replace(from: &Path, to: &Path) -> Result<(), LauncherError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let from: Vec<u16> = from
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let to: Vec<u16> = to
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(LauncherError::Io(io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace(from: &Path, to: &Path) -> Result<(), LauncherError> {
    fs::rename(from, to).map_err(LauncherError::Io)
}

fn unique_token() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{}-{}-{}",
        std::process::id(),
        now.as_secs(),
        now.subsec_nanos()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "launcher-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn setup(name: &str) -> (Launcher, PathBuf, PathBuf) {
        let root = temp(name);
        let user = root.join("user-data");
        let l = Launcher::new(&root, &user, "studio.exe");
        fs::create_dir_all(l.version_root().join("1.0.0")).unwrap();
        fs::create_dir_all(l.version_root().join("2.0.0")).unwrap();
        (l, root, user)
    }
    fn clean(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pointer_transitions_are_atomic_and_ordered() {
        let (l, root, _) = setup("pointers");
        fs::write(l.version_root().join("1.0.0/studio.exe"), b"x").unwrap();
        fs::write(l.version_root().join("2.0.0/studio.exe"), b"x").unwrap();
        l.switch_current("1.0.0").unwrap();
        let p = l.switch_current("2.0.0").unwrap();
        assert_eq!(
            p,
            Pointers {
                current: Some("2.0.0".into()),
                previous: Some("1.0.0".into())
            }
        );
        assert_eq!(l.rollback().unwrap().current.as_deref(), Some("1.0.0"));
        clean(&root);
    }
    #[test]
    fn missing_versions_fail_closed() {
        let (l, root, _) = setup("missing");
        assert!(matches!(
            l.switch_current("3.0.0"),
            Err(LauncherError::MissingVersion(_))
        ));
        clean(&root);
    }
    #[test]
    fn path_traversal_is_rejected() {
        let (l, root, _) = setup("traversal");
        assert!(matches!(
            l.switch_current("../escape"),
            Err(LauncherError::InvalidPointer(_))
        ));
        assert!(matches!(
            l.switch_current("C:\\escape"),
            Err(LauncherError::InvalidPointer(_))
        ));
        clean(&root);
    }
    #[test]
    fn user_data_is_not_inside_versions() {
        let (l, root, user) = setup("userdata");
        assert!(!user.starts_with(l.version_root()));
        fs::write(l.version_root().join("1.0.0/studio.exe"), b"x").unwrap();
        l.switch_current("1.0.0").unwrap();
        assert!(l.user_data().join(STATE_FILE).is_file());
        clean(&root);
    }
    #[test]
    fn timeout_rolls_back_pointer() {
        let (l, root, _) = setup("timeout");
        let executable = std::env::current_exe().unwrap();
        fs::copy(&executable, l.version_root().join("1.0.0/studio.exe")).unwrap();
        fs::copy(&executable, l.version_root().join("2.0.0/studio.exe")).unwrap();
        l.switch_current("1.0.0").unwrap();
        l.switch_current("2.0.0").unwrap();
        let result = l.run(Duration::from_millis(30));
        assert!(matches!(result, Err(LauncherError::StartupFailed(_))));
        assert_eq!(l.read_pointers().unwrap().current.as_deref(), Some("1.0.0"));
        clean(&root);
    }
}
