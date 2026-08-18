use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Component, Path, PathBuf},
    process::Command,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::ToolResult;

const PACKAGE_SCHEMA: u32 = 1;
const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const STUDIO_VERSION: &str = env!("CARGO_PKG_VERSION");
const LAUNCHER_ARTIFACT_NAMES: &[&str] = &[
    "sensor-watch-studio-launcher",
    "sensor-watch-launcher",
    "sensor-watch-bootstrapper",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudioPackageResult {
    pub output: PathBuf,
    pub package_directory: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
struct PackageEntry {
    path: String,
    size: u64,
    sha256: String,
}

/// Builds Studio in release mode and packages only the known distribution inputs.
pub fn package_studio(output: Option<&Path>) -> ToolResult<StudioPackageResult> {
    let root = crate::workspace_root()?;
    let target = target_directory(&root);
    let mut command = Command::new("cargo");
    command.args(["build", "-p", "sensor-watch-studio", "--release"]);
    command.current_dir(&root);
    let status = command
        .status()
        .map_err(|e| format!("cannot start Studio release build: {e}"))?;
    if !status.success() {
        return Err(format!("Studio release build failed with {status}"));
    }
    let executable = target
        .join("release")
        .join(format!("sensor-watch-studio{EXE_SUFFIX}"));
    let launcher = find_launcher_artifact(&target.join("release"))?;
    package_studio_artifacts_with_launcher(&root, &executable, &launcher, output)
}

#[cfg(windows)]
const EXE_SUFFIX: &str = ".exe";
#[cfg(not(windows))]
const EXE_SUFFIX: &str = "";

fn target_directory(root: &Path) -> PathBuf {
    let value = env::var_os("CARGO_TARGET_DIR").map(PathBuf::from);
    match value {
        Some(path) if path.is_absolute() => path,
        Some(path) => root.join(path),
        None => root.join("target"),
    }
}

/// Packages an already-built executable. Kept separate so package tests never
/// need to invoke Cargo and can exercise the fail-closed filesystem boundary.
pub fn package_studio_artifacts(
    root: &Path,
    executable: &Path,
    output: Option<&Path>,
) -> ToolResult<StudioPackageResult> {
    let target = root.join("target");
    let launcher = find_launcher_artifact(&target.join("release"))?;
    package_studio_artifacts_with_launcher(root, executable, &launcher, output)
}

/// Packages a Studio executable with a separately built launcher/bootstrapper.
/// The explicit launcher parameter keeps tests and release pipelines independent
/// from the launcher implementation while preserving the package boundary.
pub fn package_studio_artifacts_with_launcher(
    root: &Path,
    executable: &Path,
    launcher: &Path,
    output: Option<&Path>,
) -> ToolResult<StudioPackageResult> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("cannot resolve workspace root: {e}"))?;
    let executable = regular_file(executable, "Studio release executable")?
        .canonicalize()
        .map_err(|e| format!("cannot resolve Studio release executable: {e}"))?;
    let launcher = regular_file(launcher, "required launcher/bootstrapper artifact")?
        .canonicalize()
        .map_err(|e| format!("cannot resolve launcher/bootstrapper artifact: {e}"))?;
    let target_root = root
        .join("target")
        .canonicalize()
        .map_err(|e| format!("cannot resolve workspace target directory: {e}"))?;
    if !executable.starts_with(&target_root) {
        return Err("Studio executable must be inside the workspace target directory".into());
    }
    if !launcher.starts_with(&target_root) {
        return Err("launcher/bootstrapper must be inside the workspace target directory".into());
    }
    let version = studio_version(&root)?;
    let package_directory = format!("sensor-watch-studio-{version}");
    let app_directory = format!("app/{version}");
    let output = output.map(PathBuf::from).unwrap_or_else(|| {
        root.join("target/studio-package")
            .join(format!("{package_directory}.zip"))
    });
    if output.extension().and_then(|v| v.to_str()) != Some("zip") {
        return Err("Studio package output must have a .zip extension".into());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create package output directory: {e}"))?;
    }

    let resources = root.join("studio/assets");
    let templates = root.join("studio/src");
    let firmware = root.clone();
    require_directory(&resources, "Studio resources")?;
    require_directory(&templates, "Studio templates")?;

    let mut files = Vec::<(String, PathBuf)>::new();
    files.push((
        format!("{package_directory}/{app_directory}/sensor-watch-studio{EXE_SUFFIX}"),
        executable,
    ));
    files.push((
        format!("{package_directory}/launcher/sensor-watch-studio{EXE_SUFFIX}"),
        launcher,
    ));
    add_tree(
        &mut files,
        &resources,
        &format!("{package_directory}/resources"),
        &[],
    )?;
    add_tree(
        &mut files,
        &templates,
        &format!("{package_directory}/templates"),
        &[],
    )?;
    add_firmware_tree(
        &mut files,
        &firmware,
        &format!("{package_directory}/firmware"),
    )?;

    let manifest_path = format!("{package_directory}/sensor-watch-package.json");
    let readme_path = format!("{package_directory}/README.txt");
    let capabilities_path = format!("{package_directory}/PACKAGE-CAPABILITIES.json");
    let release_metadata_path = format!("{package_directory}/release/metadata.json");
    let release_signature_path = format!("{package_directory}/release/metadata.json.sig");
    let update_policy_path = format!("{package_directory}/update-policy.json");
    let startup_marker_path = format!("{package_directory}/startup-marker.json");
    let current_pointer_path = format!("{package_directory}/versions/current.json");
    let previous_pointer_path = format!("{package_directory}/versions/previous.json");
    let mut entries = files
        .iter()
        .map(|(path, source)| package_entry(path, source))
        .collect::<ToolResult<Vec<_>>>()?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = serde_json::to_vec_pretty(&json!({
        "schema_version": PACKAGE_SCHEMA,
        "current_version": { "version": version },
        "previous_version": null,
        "launcher_executable": format!("launcher/sensor-watch-studio{EXE_SUFFIX}"),
        "app_directory": app_directory,
        "resources_directory": "resources",
        "templates_directory": "templates",
        "firmware_project_directory": "firmware",
        "tools_directory": "tools",
        "targets_directory": "targets",
        "user_data_directory": "(platform user-data directory; not included)",
        "capability_manifest": "PACKAGE-CAPABILITIES.json",
        "update_policy": "update-policy.json",
        "startup_marker": "startup-marker.json",
        "current_pointer": "versions/current.json",
        "previous_pointer": "versions/previous.json",
        "release_metadata": "release/metadata.json",
        "release_signature": "release/metadata.json.sig"
    }))
    .map_err(|e| format!("cannot serialize package metadata: {e}"))?;
    let capabilities = serde_json::to_vec_pretty(&json!({
        "schema_version": PACKAGE_SCHEMA,
        "capabilities": {
            "launcher": { "available": true, "path": "launcher/sensor-watch-studio.exe" },
            "versioned_app": { "available": true, "path": format!("{app_directory}/sensor-watch-studio{EXE_SUFFIX}") },
            "resources": { "available": true, "path": "resources" },
            "templates": { "available": true, "path": "templates" },
            "firmware_project": { "available": true, "path": "firmware" },
            "tools": { "available": false, "path": "tools", "reason": "not bundled by this builder" },
            "targets": { "available": false, "path": "targets", "reason": "not bundled by this builder" }
        }
    })).map_err(|e| format!("cannot serialize capability metadata: {e}"))?;
    let release_metadata = serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "version": version,
        "signature_status": "placeholder-required-before-publishing",
        "signature_algorithm": "ed25519",
        "signature_file": "metadata.json.sig"
    }))
    .map_err(|e| format!("cannot serialize release metadata: {e}"))?;
    let release_signature = b"SIGNATURE_PLACEHOLDER_REPLACE_FOR_SIGNED_RELEASE\n".to_vec();
    let update_policy = serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "mode": "offline",
        "network_downloads": false,
        "in_place_replacement": false,
        "verification": "required-before-activation",
        "launcher_owns_switching": true
    }))
    .map_err(|e| format!("cannot serialize update policy: {e}"))?;
    let startup_marker = serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "path": "user-data/startup.marker",
        "contract": { "written_before_launch": true, "cleared_after_success": true, "stale_marker_means_recovery": true },
        "user_data_only": true
    })).map_err(|e| format!("cannot serialize startup marker contract: {e}"))?;
    let current_pointer =
        serde_json::to_vec_pretty(&json!({ "version": version, "app_directory": app_directory }))
            .unwrap();
    let previous_pointer = b"{\n  \"version\": null,\n  \"app_directory\": null\n}\n".to_vec();
    let readme = format!(
        "Sensor-Watch Studio {version}\n\nThe launcher is the only supported entry point. It owns startup markers,\nverification, and current/previous version selection. Mutable settings and\nuser projects stay outside this ZIP in the platform user-data directory.\n\nSigned-release metadata is a placeholder and must be replaced before publishing.\nPACKAGE-CAPABILITIES.json describes bundled and unavailable capabilities.\nPACKAGE-MANIFEST.json lists the SHA-256 digest of every packaged file.\n"
    );
    files.push((manifest_path, bytes_path(&manifest)));
    files.push((readme_path, bytes_path(readme.as_bytes())));
    files.push((capabilities_path, bytes_path(&capabilities)));
    files.push((release_metadata_path, bytes_path(&release_metadata)));
    files.push((release_signature_path, bytes_path(&release_signature)));
    files.push((update_policy_path, bytes_path(&update_policy)));
    files.push((startup_marker_path, bytes_path(&startup_marker)));
    files.push((current_pointer_path, bytes_path(&current_pointer)));
    files.push((previous_pointer_path, bytes_path(&previous_pointer)));
    let mut all_entries = entries;
    all_entries.push(PackageEntry {
        path: format!("{package_directory}/sensor-watch-package.json"),
        size: manifest.len() as u64,
        sha256: digest(&manifest),
    });
    all_entries.push(PackageEntry {
        path: format!("{package_directory}/README.txt"),
        size: readme.len() as u64,
        sha256: digest(readme.as_bytes()),
    });
    all_entries.sort_by(|a, b| a.path.cmp(&b.path));
    let package_manifest =
        serde_json::to_vec_pretty(&json!({"schema_version": 1, "entries": all_entries}))
            .map_err(|e| format!("cannot serialize package file manifest: {e}"))?;
    files.push((
        format!("{package_directory}/PACKAGE-MANIFEST.json"),
        bytes_path(&package_manifest),
    ));
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let temporary = output.with_extension(format!("zip.tmp-{}", process_id()));
    remove_regular_if_present(&temporary)?;
    write_zip(&temporary, &files)?;
    replace_output(&temporary, &output)?;
    Ok(StudioPackageResult {
        output,
        package_directory,
        version,
    })
}

fn process_id() -> u32 {
    std::process::id()
}
fn bytes_path(bytes: &[u8]) -> PathBuf {
    let path = env::temp_dir().join(format!(
        "sensor-watch-package-bytes-{}-{}",
        process_id(),
        digest(bytes)
    ));
    fs::write(&path, bytes).expect("temporary package metadata write");
    path
}

fn find_launcher_artifact(release_directory: &Path) -> ToolResult<PathBuf> {
    for name in LAUNCHER_ARTIFACT_NAMES {
        let candidate = release_directory.join(format!("{name}{EXE_SUFFIX}"));
        if candidate.is_file() {
            return regular_file(&candidate, "required launcher/bootstrapper artifact");
        }
    }
    Err(format!(
        "required launcher/bootstrapper artifact is absent from {}. Build a separate launcher executable named one of: {} and place it in target/release before packaging; the Studio executable alone is not a full distribution",
        release_directory.display(),
        LAUNCHER_ARTIFACT_NAMES
            .iter()
            .map(|name| format!("{name}{EXE_SUFFIX}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn studio_version(root: &Path) -> ToolResult<String> {
    let cargo = fs::read_to_string(root.join("studio/Cargo.toml"))
        .map_err(|e| format!("cannot read Studio manifest: {e}"))?;
    cargo
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("version = \"")?
                .strip_suffix('"')
                .map(str::to_owned)
        })
        .ok_or_else(|| format!("Studio manifest has no version (tool version {STUDIO_VERSION})"))
}

fn regular_file(path: &Path, label: &str) -> ToolResult<PathBuf> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("{label} is missing: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{label} is not a regular file: {}", path.display()));
    }
    Ok(path.to_path_buf())
}
fn require_directory(path: &Path, label: &str) -> ToolResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("{label} is missing: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{label} is not a regular directory: {}",
            path.display()
        ));
    }
    Ok(())
}
fn safe_component(path: &Path) -> bool {
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}
fn add_tree(
    files: &mut Vec<(String, PathBuf)>,
    root: &Path,
    prefix: &str,
    excluded: &[&str],
) -> ToolResult<()> {
    require_directory(root, "package tree")?;
    for item in fs::read_dir(root).map_err(|e| format!("cannot read {}: {e}", root.display()))? {
        let item = item.map_err(|e| format!("cannot read package tree: {e}"))?;
        let name = item.file_name();
        if !safe_component(Path::new(&name))
            || excluded.iter().any(|v| *v == name.to_string_lossy())
        {
            return Err(format!("unsafe package entry: {}", item.path().display()));
        }
        let metadata = fs::symlink_metadata(item.path()).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() && !metadata.is_dir() {
            return Err(format!("refusing package entry: {}", item.path().display()));
        }
        let destination = format!("{prefix}/{}", name.to_string_lossy());
        if metadata.is_dir() {
            add_tree(files, &item.path(), &destination, excluded)?;
        } else {
            if metadata.len() > MAX_FILE_BYTES {
                return Err(format!(
                    "package file is too large: {}",
                    item.path().display()
                ));
            }
            files.push((destination, item.path()));
        }
    }
    Ok(())
}
fn add_firmware_tree(
    files: &mut Vec<(String, PathBuf)>,
    root: &Path,
    prefix: &str,
) -> ToolResult<()> {
    for name in ["src", "core"] {
        add_tree(
            files,
            &root.join(name),
            &format!("{prefix}/{name}"),
            &["target", ".git"],
        )?;
    }
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "memory.x",
        "rust-toolchain.toml",
    ] {
        let path = root.join(name);
        regular_file(&path, "required firmware project file")?;
        files.push((format!("{prefix}/{name}"), path));
    }
    Ok(())
}
fn package_entry(path: &str, source: &Path) -> ToolResult<PackageEntry> {
    let data = fs::read(source).map_err(|e| format!("cannot read {}: {e}", source.display()))?;
    Ok(PackageEntry {
        path: path.into(),
        size: data.len() as u64,
        sha256: digest(&data),
    })
}
fn digest(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn write_zip(path: &Path, files: &[(String, PathBuf)]) -> ToolResult<()> {
    let file = fs::File::create(path).map_err(|e| format!("cannot create temporary ZIP: {e}"))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    for (name, source) in files {
        zip.start_file(name, options)
            .map_err(|e| format!("cannot add {name}: {e}"))?;
        let mut input =
            fs::File::open(source).map_err(|e| format!("cannot open {}: {e}", source.display()))?;
        std::io::copy(&mut input, &mut zip).map_err(|e| format!("cannot write {name}: {e}"))?;
    }
    zip.finish()
        .map_err(|e| format!("cannot finish ZIP: {e}"))?
        .sync_all()
        .map_err(|e| format!("cannot sync ZIP: {e}"))?;
    Ok(())
}
fn remove_regular_if_present(path: &Path) -> ToolResult<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() || !m.is_file() => Err(format!(
            "refusing non-file temporary path: {}",
            path.display()
        )),
        Ok(_) => fs::remove_file(path).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
fn replace_output(temporary: &Path, output: &Path) -> ToolResult<()> {
    let backup = output.with_extension("zip.previous");
    remove_regular_if_present(&backup)?;
    if output.exists() {
        fs::rename(output, &backup)
            .map_err(|e| format!("cannot stage existing package for replacement: {e}"))?;
    }
    if let Err(error) = fs::rename(temporary, output) {
        if backup.exists() {
            let _ = fs::rename(&backup, output);
        }
        return Err(format!("cannot atomically install package: {error}"));
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "studio-package-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn fixture(name: &str) -> (PathBuf, PathBuf) {
        let root = temp(name);
        for directory in [
            "studio/assets",
            "studio/src",
            "src",
            "core",
            "target",
            ".git",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("studio/Cargo.toml"),
            "[package]\nname=\"sensor-watch-studio\"\nversion = \"9.8.7\"\n",
        )
        .unwrap();
        fs::write(root.join("studio/assets/watch.svg"), b"svg").unwrap();
        fs::write(root.join("studio/src/template.rs"), b"template").unwrap();
        fs::write(root.join("src/main.rs"), b"firmware").unwrap();
        fs::write(root.join("core/lib.rs"), b"core").unwrap();
        fs::write(root.join("target/secret.txt"), b"secret").unwrap();
        fs::write(root.join(".git/config"), b"secret").unwrap();
        for file in [
            "Cargo.toml",
            "Cargo.lock",
            "memory.x",
            "rust-toolchain.toml",
        ] {
            fs::write(root.join(file), file).unwrap();
        }
        fs::create_dir_all(root.join("target/release")).unwrap();
        let executable = root.join("target/release/sensor-watch-studio.exe");
        fs::write(&executable, b"exe").unwrap();
        fs::write(
            root.join("target/release/sensor-watch-studio-launcher.exe"),
            b"launcher",
        )
        .unwrap();
        (root, executable)
    }
    fn zip_names(path: &Path) -> Vec<String> {
        let file = fs::File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_owned())
            .collect()
    }
    fn zip_text(path: &Path, name: &str) -> String {
        let file = fs::File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut contents = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        contents
    }
    #[test]
    fn traversal_components_are_rejected() {
        assert!(!safe_component(Path::new("../secret")));
        assert!(!safe_component(Path::new("a/b")));
        assert!(safe_component(Path::new("safe")));
    }
    #[test]
    fn package_requires_a_separate_launcher() {
        let (root, executable) = fixture("launcher-required");
        fs::remove_file(root.join("target/release/sensor-watch-studio-launcher.exe")).unwrap();
        let error =
            package_studio_artifacts(&root, &executable, Some(&root.join("out.zip"))).unwrap_err();
        assert!(error.contains("required launcher/bootstrapper artifact is absent"));
        assert!(error.contains("target/release"));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn package_is_deterministic_and_excludes_unallowlisted_files() {
        let (root, executable) = fixture("deterministic");
        let first = root.join("out.zip");
        let second = root.join("out-2.zip");
        package_studio_artifacts(&root, &executable, Some(&first)).unwrap();
        package_studio_artifacts(&root, &executable, Some(&second)).unwrap();
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
        let names = zip_names(&first);
        assert!(
            names
                .iter()
                .any(|name| name.ends_with("/sensor-watch-package.json"))
        );
        assert!(
            names
                .iter()
                .any(|name| name.ends_with("/PACKAGE-MANIFEST.json"))
        );
        assert!(
            names
                .iter()
                .any(|name| name.ends_with("/launcher/sensor-watch-studio.exe"))
        );
        assert!(
            names
                .iter()
                .any(|name| name.ends_with("/app/9.8.7/sensor-watch-studio.exe"))
        );
        assert!(!names.iter().any(|name| name.contains("target/")
            || name.contains(".git/")
            || name.contains("secret")));
        assert!(names.windows(2).all(|pair| pair[0] <= pair[1]));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn missing_required_resources_fails_without_replacing_output() {
        let (root, executable) = fixture("missing");
        fs::remove_dir_all(root.join("studio/assets")).unwrap();
        let output = root.join("out.zip");
        fs::write(&output, b"old").unwrap();
        assert!(package_studio_artifacts(&root, &executable, Some(&output)).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"old");
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn manifest_has_sha256_entries_and_no_signature_claim() {
        let (root, executable) = fixture("manifest");
        let output = root.join("out.zip");
        package_studio_artifacts(&root, &executable, Some(&output)).unwrap();
        let file = fs::File::open(&output).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let index = (0..archive.len())
            .find(|i| {
                archive
                    .by_index(*i)
                    .unwrap()
                    .name()
                    .ends_with("/PACKAGE-MANIFEST.json")
            })
            .unwrap();
        let mut contents = String::new();
        archive
            .by_index(index)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert!(contents.contains("sha256"));
        assert!(!contents.contains("signature"));
        let package_root = "sensor-watch-studio-9.8.7";
        let metadata = zip_text(
            &output,
            &format!("{package_root}/sensor-watch-package.json"),
        );
        let value: serde_json::Value = serde_json::from_str(&metadata).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(
            value["launcher_executable"],
            "launcher/sensor-watch-studio.exe"
        );
        assert_eq!(value["app_directory"], "app/9.8.7");
        assert_eq!(value["current_pointer"], "versions/current.json");
        assert!(
            zip_names(&output)
                .iter()
                .any(|name| name.ends_with("/PACKAGE-CAPABILITIES.json"))
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn package_contains_ab_pointers_and_startup_update_contracts() {
        let (root, executable) = fixture("ab-layout");
        let output = root.join("out.zip");
        package_studio_artifacts(&root, &executable, Some(&output)).unwrap();
        let base = "sensor-watch-studio-9.8.7";
        let current = zip_text(&output, &format!("{base}/versions/current.json"));
        let previous = zip_text(&output, &format!("{base}/versions/previous.json"));
        let marker = zip_text(&output, &format!("{base}/startup-marker.json"));
        let policy = zip_text(&output, &format!("{base}/update-policy.json"));
        assert!(current.contains("app/9.8.7"));
        assert!(previous.contains("null"));
        assert!(marker.contains("stale_marker_means_recovery"));
        assert!(policy.contains("verification"));
        let _ = fs::remove_dir_all(root);
    }
}
