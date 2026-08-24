//! On-demand, package-local Master Clock launcher.
//!
//! This module intentionally does not perform discovery, networking, or startup
//! work. A package manifest must name the executable and its expected digest.

use sensor_watch_desktop_update::Authentication;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub const TOOL_RELATIVE_PATH: &str = "tools/master-clock.exe";
pub const MAX_RUNTIME: Duration = Duration::from_secs(30 * 60);

pub fn action_available(advanced_mode: bool, validated_tool: bool, running: bool) -> bool {
    advanced_mode && validated_tool && !running
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageToolCapability {
    pub path: String,
    pub sha256: String,
    pub signature: Option<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub provenance: String,
}

/// Hook for the desktop-update trust policy. A signed capability is rejected
/// unless the application supplies its existing pinned-key authenticator.
pub trait CapabilityAuthenticator {
    fn authenticate(&self, path: &str, sha256: &str, signature: &str) -> Authentication;
}

pub struct NoCapabilityAuthenticator;
impl CapabilityAuthenticator for NoCapabilityAuthenticator {
    fn authenticate(&self, _: &str, _: &str, _: &str) -> Authentication {
        Authentication::Unsupported
    }
}

pub fn validate_package_tool<A: CapabilityAuthenticator>(
    package_root: &Path,
    capability: &PackageToolCapability,
    authenticator: &A,
) -> Result<PathBuf, String> {
    if capability.path != TOOL_RELATIVE_PATH {
        return Err("Master Clock capability path must be tools/master-clock.exe".into());
    }
    if capability.license.trim().is_empty() {
        return Err("Master Clock capability has no license metadata".into());
    }
    if capability.provenance.trim().is_empty() {
        return Err("Master Clock capability has no provenance metadata".into());
    }
    if capability.sha256.len() != 64 || !capability.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Master Clock capability has an invalid SHA-256".into());
    }
    if let Some(signature) = capability.signature.as_deref() {
        if !matches!(
            authenticator.authenticate(&capability.path, &capability.sha256, signature),
            Authentication::Authenticated { .. }
        ) {
            return Err("Master Clock capability signature is not trusted".into());
        }
    }
    let path = package_root.join(&capability.path);
    let canonical_root = package_root
        .canonicalize()
        .map_err(|e| format!("cannot resolve package root: {e}"))?;
    let canonical = path
        .canonicalize()
        .map_err(|_| "Master Clock executable is missing".to_string())?;
    if !canonical.starts_with(&canonical_root) || !canonical.is_file() {
        return Err("Master Clock executable is outside the package or is not a file".into());
    }
    let bytes = std::fs::read(&canonical).map_err(|e| format!("cannot read Master Clock: {e}"))?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != capability.sha256.to_ascii_lowercase() {
        return Err("Master Clock executable hash does not match the package manifest".into());
    }
    Ok(canonical)
}

pub fn validate_developer_tool(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve configured Master Clock path: {e}"))?;
    if !canonical.is_file()
        || canonical.file_name().and_then(|n| n.to_str()) != Some("master-clock.exe")
    {
        return Err(
            "configured Master Clock path must be an existing master-clock.exe file".into(),
        );
    }
    Ok(canonical)
}

pub struct MasterClockProcess {
    child: Child,
    started: Instant,
}

impl MasterClockProcess {
    pub fn launch(path: &Path) -> Result<Self, String> {
        let mut command = Command::new(path);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let child = command
            .spawn()
            .map_err(|e| format!("Master Clock failed to launch: {e}"))?;
        Ok(Self {
            child,
            started: Instant::now(),
        })
    }

    pub fn poll(&mut self) -> Result<bool, String> {
        if self.started.elapsed() > MAX_RUNTIME {
            self.terminate();
            return Err("Master Clock was terminated after the maximum runtime".into());
        }
        match self.child.try_wait() {
            Ok(Some(status)) => Ok(status.success()),
            Ok(None) => Ok(true),
            Err(e) => Err(format!("Master Clock status check failed: {e}")),
        }
    }

    pub fn terminate(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for MasterClockProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "master-clock-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn cap(bytes: &[u8], path: &str) -> PackageToolCapability {
        PackageToolCapability {
            path: path.into(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            signature: None,
            license: "MIT OR Apache-2.0".into(),
            provenance: "test fixture".into(),
        }
    }
    #[test]
    fn rejects_path_traversal() {
        let root = temp("traversal");
        std::fs::create_dir_all(&root).unwrap();
        let result = validate_package_tool(
            &root,
            &cap(b"x", "../master-clock.exe"),
            &NoCapabilityAuthenticator,
        );
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn accepts_only_validated_package_local_path() {
        let root = temp("local");
        let path = root.join(TOOL_RELATIVE_PATH);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"clock").unwrap();
        assert!(validate_package_tool(
            &root,
            &cap(b"clock", TOOL_RELATIVE_PATH),
            &NoCapabilityAuthenticator
        )
        .is_ok());
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn developer_path_is_explicit_and_validated() {
        let root = temp("developer");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("master-clock.exe");
        std::fs::write(&path, b"clock").unwrap();
        assert!(validate_developer_tool(&path).is_ok());
        assert!(validate_developer_tool(&root.join("missing.exe")).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn launch_failure_is_user_reportable() {
        assert!(MasterClockProcess::launch(Path::new("does-not-exist-master-clock.exe")).is_err());
    }
    #[test]
    fn action_is_advanced_only_and_missing_tool_is_disabled() {
        assert!(!action_available(false, true, false));
        assert!(!action_available(true, false, false));
        assert!(!action_available(true, true, true));
        assert!(action_available(true, true, false));
    }
}
