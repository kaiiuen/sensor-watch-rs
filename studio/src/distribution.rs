//! Bounded, folder-based distribution discovery for Firmware Studio.
//!
//! A packaged Studio is identified only by a validated manifest next to (or
//! above) the launcher. Developer workspace lookup is opt-in; a copied binary
//! never silently reaches back into the checkout it was built from.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::master_clock;
use std::sync::OnceLock;

pub const MANIFEST_FILE: &str = "sensor-watch-package.json";
pub const MANIFEST_SCHEMA: u32 = 1;
pub const DEVELOPER_MODE_ENV: &str = "SENSOR_WATCH_STUDIO_DEVELOPER_MODE";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct VersionMetadata {
    pub version: String,
    #[serde(default)]
    pub installed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PackageManifest {
    pub schema_version: u32,
    pub current_version: VersionMetadata,
    #[serde(default)]
    pub previous_version: Option<VersionMetadata>,
    pub launcher_executable: String,
    pub app_directory: String,
    pub resources_directory: String,
    pub templates_directory: String,
    pub firmware_project_directory: String,
    #[serde(default)]
    pub tools_directory: Option<String>,
    #[serde(default)]
    pub targets_directory: Option<String>,
    /// Optional, explicitly licensed/tracked Master Clock capability.
    #[serde(default)]
    pub master_clock: Option<master_clock::PackageToolCapability>,
    #[serde(default)]
    pub user_data_directory: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistributionMode {
    Packaged,
    Developer,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityStatus {
    pub resources: bool,
    pub templates: bool,
    pub firmware_project: bool,
    pub tools: bool,
    pub targets: bool,
    pub master_clock: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageStatus {
    pub mode: DistributionMode,
    pub root: Option<PathBuf>,
    pub launcher: Option<PathBuf>,
    pub app_directory: Option<PathBuf>,
    pub resources: Option<PathBuf>,
    pub templates: Option<PathBuf>,
    /// Immutable packaged firmware template (or the developer checkout).
    pub firmware_project: Option<PathBuf>,
    /// Mutable project used by File Browser and Editor operations.
    pub active_project: Option<PathBuf>,
    pub tools: Option<PathBuf>,
    pub targets: Option<PathBuf>,
    /// Validated package-local Master Clock executable, if declared and present.
    pub master_clock: Option<PathBuf>,
    pub user_data_root: PathBuf,
    pub current_version: Option<String>,
    pub previous_version: Option<String>,
    pub capabilities: CapabilityStatus,
    pub warnings: Vec<String>,
}

static ACTIVE: OnceLock<PackageStatus> = OnceLock::new();

pub fn developer_mode_requested() -> bool {
    std::env::var(DEVELOPER_MODE_ENV)
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

pub fn initialize(executable: &Path, developer_mode: bool) -> PackageStatus {
    let status = resolve(executable, developer_mode, compiled_workspace_root());
    let _ = ACTIVE.set(status.clone());
    status
}

pub fn initialized() -> bool {
    ACTIVE.get().is_some()
}

pub fn active() -> PackageStatus {
    ACTIVE
        .get()
        .cloned()
        .unwrap_or_else(|| resolve(Path::new("studio.exe"), false, None))
}

pub fn resolve(
    executable: &Path,
    developer_mode: bool,
    developer_workspace: Option<PathBuf>,
) -> PackageStatus {
    let user_data_root = crate::data_dir::default_path();
    if let Some((root, manifest)) = discover_package(executable) {
        return package_status(root, manifest, user_data_root);
    }
    if developer_mode {
        let project = developer_workspace.filter(|path| path.is_dir());
        let mut warnings = Vec::new();
        if project.is_none() {
            warnings.push("Developer mode was requested, but the workspace is unavailable".into());
        }
        let caps = project.is_some();
        return PackageStatus {
            mode: DistributionMode::Developer,
            root: project.clone(),
            launcher: Some(executable.to_path_buf()),
            app_directory: project.clone(),
            resources: project.as_ref().map(|p| p.join("studio/assets")),
            templates: project.as_ref().map(|p| p.join("studio/src")),
            active_project: project.clone(),
            firmware_project: project,
            tools: None,
            targets: None,
            master_clock: None,
            user_data_root,
            current_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            previous_version: None,
            capabilities: CapabilityStatus {
                resources: caps,
                templates: caps,
                firmware_project: caps,
                tools: false,
                targets: false,
                master_clock: false,
            },
            warnings,
        };
    }
    PackageStatus {
        mode: DistributionMode::Unavailable,
        root: None,
        launcher: Some(executable.to_path_buf()),
        app_directory: None,
        resources: None,
        templates: None,
        firmware_project: None,
        active_project: None,
        tools: None,
        targets: None,
        master_clock: None,
        user_data_root,
        current_version: None,
        previous_version: None,
        capabilities: CapabilityStatus {
            resources: false,
            templates: false,
            firmware_project: false,
            tools: false,
            targets: false,
            master_clock: false,
        },
        warnings: vec!["No package manifest found; developer fallback is disabled".into()],
    }
}

fn package_status(
    root: PathBuf,
    manifest: PackageManifest,
    user_data_root: PathBuf,
) -> PackageStatus {
    let path = |value: &str| root.join(value);
    let launcher = path(&manifest.launcher_executable);
    let app_directory = path(&manifest.app_directory);
    let resources = path(&manifest.resources_directory);
    let templates = path(&manifest.templates_directory);
    let firmware_project = path(&manifest.firmware_project_directory);
    let tools = manifest.tools_directory.as_deref().map(path);
    let targets = manifest.targets_directory.as_deref().map(path);
    let mut warnings = Vec::new();
    let validated_master_clock = manifest.master_clock.as_ref().and_then(|capability| {
        match master_clock::validate_package_tool(
            &root,
            capability,
            &master_clock::NoCapabilityAuthenticator,
        ) {
            Ok(path) => Some(path),
            Err(error) => {
                warnings.push(format!("Master Clock capability unavailable: {error}"));
                None
            }
        }
    });
    if manifest.schema_version != MANIFEST_SCHEMA {
        warnings.push(format!(
            "Unsupported package manifest schema {}",
            manifest.schema_version
        ));
    }
    let exists = |p: &Path| p.is_dir();
    if !exists(&resources) {
        warnings.push("Bundled resources are missing".into());
    }
    if !exists(&templates) {
        warnings.push("Bundled templates are missing".into());
    }
    if !exists(&firmware_project) {
        warnings.push("Bundled firmware project is missing".into());
    }
    let active_project = match prepare_packaged_project(&user_data_root, &firmware_project) {
        Ok(project) => Some(project),
        Err(error) => {
            warnings.push(format!("Packaged mutable project unavailable: {error}"));
            None
        }
    };
    PackageStatus {
        mode: DistributionMode::Packaged,
        root: Some(root),
        launcher: Some(launcher),
        app_directory: Some(app_directory),
        resources: Some(resources.clone()),
        templates: Some(templates.clone()),
        firmware_project: Some(firmware_project.clone()),
        active_project,
        tools: tools.clone(),
        targets: targets.clone(),
        master_clock: validated_master_clock.clone(),
        user_data_root,
        current_version: Some(manifest.current_version.version),
        previous_version: manifest.previous_version.map(|v| v.version),
        capabilities: CapabilityStatus {
            resources: exists(&resources),
            templates: exists(&templates),
            firmware_project: exists(&firmware_project),
            tools: tools.as_deref().is_some_and(exists),
            targets: targets.as_deref().is_some_and(exists),
            master_clock: validated_master_clock.is_some(),
        },
        warnings,
    }
}

fn prepare_packaged_project(user_data_root: &Path, template: &Path) -> Result<PathBuf, String> {
    let template = template
        .canonicalize()
        .map_err(|e| format!("cannot resolve bundled firmware template: {e}"))?;
    if !template.is_dir() {
        return Err("bundled firmware template is not a directory".into());
    }
    verify_template_tree(&template)?;
    crate::data_dir::validate(user_data_root, &[&template])?;

    let project = user_data_root.join("project");
    let project_existed = project.exists();
    crate::data_dir::validate(&project, &[])?;
    let user_root = user_data_root
        .canonicalize()
        .map_err(|e| format!("cannot resolve user-data root: {e}"))?;
    let project_root = project
        .canonicalize()
        .map_err(|e| format!("cannot resolve mutable project: {e}"))?;
    if !project_root.starts_with(&user_root) || project_root == user_root {
        return Err("mutable project is not beneath the validated user-data root".into());
    }
    if project_root.join("src").is_dir() {
        return Ok(project_root);
    }
    if project_existed {
        return Err("existing mutable project is incomplete".into());
    }
    std::fs::remove_dir(&project)
        .map_err(|e| format!("cannot prepare mutable project directory: {e}"))?;

    let staging = user_data_root.join(".project.copying");
    if staging.exists() {
        return Err("incomplete mutable project staging directory exists".into());
    }
    copy_verified_tree(&template, &staging)?;
    verify_template_tree(&staging)?;
    std::fs::rename(&staging, &project)
        .map_err(|e| format!("cannot install mutable project atomically: {e}"))?;
    Ok(project)
}

fn verify_template_tree(root: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(root).map_err(|e| format!("cannot inspect template: {e}"))? {
        let entry = entry.map_err(|e| format!("cannot inspect template entry: {e}"))?;
        let metadata = std::fs::symlink_metadata(entry.path())
            .map_err(|e| format!("cannot inspect template entry: {e}"))?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(format!(
                "bundled template contains a link or reparse point: {}",
                entry.path().display()
            ));
        }
        if metadata.is_dir() {
            verify_template_tree(&entry.path())?;
        } else if !metadata.is_file() {
            return Err(format!(
                "bundled template contains a non-file: {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn copy_verified_tree(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir(destination)
        .map_err(|e| format!("cannot create project staging directory: {e}"))?;
    for entry in std::fs::read_dir(source).map_err(|e| format!("cannot copy template: {e}"))? {
        let entry = entry.map_err(|e| format!("cannot copy template entry: {e}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&source_path)
            .map_err(|e| format!("cannot inspect template entry: {e}"))?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(format!(
                "refusing to copy link or reparse point: {}",
                source_path.display()
            ));
        }
        if metadata.is_dir() {
            copy_verified_tree(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            std::fs::copy(&source_path, &destination_path)
                .map_err(|e| format!("cannot copy template file: {e}"))?;
        } else {
            return Err(format!(
                "refusing to copy non-file: {}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn discover_package(executable: &Path) -> Option<(PathBuf, PackageManifest)> {
    let mut directory = executable.parent()?.to_path_buf();
    loop {
        let manifest_path = directory.join(MANIFEST_FILE);
        if let Ok(contents) = std::fs::read_to_string(&manifest_path) {
            let manifest: PackageManifest = serde_json::from_str(&contents).ok()?;
            if manifest.schema_version != MANIFEST_SCHEMA || !safe_manifest(&manifest) {
                return None;
            }
            let launcher = directory.join(&manifest.launcher_executable);
            if launcher.canonicalize().ok()? == executable.canonicalize().ok()? {
                return Some((directory, manifest));
            }
        }
        if !directory.pop() {
            break;
        }
    }
    None
}

fn safe_manifest(manifest: &PackageManifest) -> bool {
    [
        &manifest.launcher_executable,
        &manifest.app_directory,
        &manifest.resources_directory,
        &manifest.templates_directory,
        &manifest.firmware_project_directory,
    ]
    .into_iter()
    .all(|value| safe_relative(value))
        && manifest
            .tools_directory
            .as_deref()
            .is_none_or(safe_relative)
        && manifest
            .targets_directory
            .as_deref()
            .is_none_or(safe_relative)
        && manifest
            .master_clock
            .as_ref()
            .is_none_or(|capability| capability.path == master_clock::TOOL_RELATIVE_PATH)
        && manifest
            .user_data_directory
            .as_deref()
            .is_none_or(safe_relative)
}

fn safe_relative(value: &str) -> bool {
    let path = Path::new(value);
    !path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

fn compiled_workspace_root() -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .canonicalize()
        .ok()
}

impl PackageStatus {
    pub fn firmware_project_dir(&self) -> Option<PathBuf> {
        self.firmware_project.clone().filter(|p| p.is_dir())
    }

    pub fn active_project_dir(&self) -> Option<PathBuf> {
        self.active_project.clone().filter(|p| p.is_dir())
    }

    pub fn display_label(&self) -> String {
        match self.mode {
            DistributionMode::Packaged => format!(
                "Packaged mode · {} · {}",
                self.current_version.as_deref().unwrap_or("version unknown"),
                if self.active_project_dir().is_some() {
                    "bundled template → mutable project"
                } else {
                    "mutable project unavailable"
                }
            ),
            DistributionMode::Developer => "Developer checkout mode · explicit fallback".into(),
            DistributionMode::Unavailable => {
                "Unavailable mode · package resources not found".into()
            }
        }
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn claims_self_containment(&self) -> bool {
        self.mode == DistributionMode::Packaged
            && self.capabilities.resources
            && self.capabilities.templates
            && self.capabilities.firmware_project
            && self.capabilities.tools
            && self.capabilities.targets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "studio-package-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn manifest() -> String {
        serde_json::json!({
            "schema_version": 1,
            "current_version": {"version":"2.4.0"},
            "previous_version": {"version":"2.3.1"},
            "launcher_executable":"app/sensor-watch-studio.exe",
            "app_directory":"app",
            "resources_directory":"resources",
            "templates_directory":"templates",
            "firmware_project_directory":"firmware",
            "tools_directory":"tools",
            "targets_directory":"targets",
            "master_clock": null
        })
        .to_string()
    }
    #[test]
    fn discovers_package_root_and_version_metadata() {
        let root = temp("discover");
        let exe = root.join("app/sensor-watch-studio.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        std::fs::write(root.join(MANIFEST_FILE), manifest()).unwrap();
        let status = resolve(&exe, false, None);
        assert_eq!(status.mode, DistributionMode::Packaged);
        assert_eq!(status.root, Some(root.clone()));
        assert_eq!(status.current_version.as_deref(), Some("2.4.0"));
        assert_eq!(status.previous_version.as_deref(), Some("2.3.1"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn reports_missing_resources_without_claiming_self_containment() {
        let root = temp("missing");
        let exe = root.join("app/sensor-watch-studio.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        std::fs::write(root.join(MANIFEST_FILE), manifest()).unwrap();
        let status = resolve(&exe, false, None);
        assert!(!status.capabilities.resources);
        assert!(!status.claims_self_containment());
        assert!(!status.warnings.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn separates_user_data_from_package_root() {
        let root = temp("data");
        let exe = root.join("app/sensor-watch-studio.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        std::fs::write(root.join(MANIFEST_FILE), manifest()).unwrap();
        let status = resolve(&exe, false, None);
        assert_ne!(status.user_data_root, root);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn developer_fallback_requires_explicit_mode() {
        let workspace = temp("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let exe = temp("dev.exe");
        assert_eq!(
            resolve(&exe, false, Some(workspace.clone())).mode,
            DistributionMode::Unavailable
        );
        assert_eq!(
            resolve(&exe, true, Some(workspace)).mode,
            DistributionMode::Developer
        );
        std::fs::remove_dir_all(exe.parent().unwrap()).ok();
    }

    #[test]
    fn packaged_project_is_mutable_without_changing_template() {
        let template = temp("template");
        let user_data = temp("user-data");
        let template_file = template.join("src/movement/stock.rs");
        std::fs::create_dir_all(template_file.parent().unwrap()).unwrap();
        std::fs::write(&template_file, b"stock").unwrap();
        let project = prepare_packaged_project(&user_data, &template).unwrap();
        assert!(project.starts_with(&user_data));
        assert_eq!(
            project.file_name().and_then(|name| name.to_str()),
            Some("project")
        );
        std::fs::write(project.join("src/movement/stock.rs"), b"edited").unwrap();
        assert_eq!(std::fs::read(&template_file).unwrap(), b"stock");
        assert_eq!(
            std::fs::read(project.join("src/movement/stock.rs")).unwrap(),
            b"edited"
        );
        let _ = std::fs::remove_dir_all(template);
        let _ = std::fs::remove_dir_all(user_data);
    }

    #[test]
    fn packaged_project_rejects_user_data_symlink() {
        let template = temp("safe-template");
        let user_data = temp("unsafe-user-data");
        std::fs::create_dir_all(template.join("src")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let target = temp("user-target");
            std::fs::create_dir_all(&target).unwrap();
            symlink(&target, &user_data).unwrap();
            assert!(prepare_packaged_project(&user_data, &template).is_err());
            let _ = std::fs::remove_file(user_data);
            let _ = std::fs::remove_dir_all(target);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_dir;
            let target = temp("user-target");
            std::fs::create_dir_all(&target).unwrap();
            if symlink_dir(&target, &user_data).is_ok() {
                assert!(prepare_packaged_project(&user_data, &template).is_err());
                let _ = std::fs::remove_dir(user_data);
            }
            let _ = std::fs::remove_dir_all(target);
        }
        let _ = std::fs::remove_dir_all(template);
    }
}
