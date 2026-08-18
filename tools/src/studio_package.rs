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
    package_studio_artifacts(&root, &executable, output)
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
    let root = root
        .canonicalize()
        .map_err(|e| format!("cannot resolve workspace root: {e}"))?;
    let executable = regular_file(executable, "Studio release executable")?
        .canonicalize()
        .map_err(|e| format!("cannot resolve Studio release executable: {e}"))?;
    let target_root = root
        .join("target")
        .canonicalize()
        .map_err(|e| format!("cannot resolve workspace target directory: {e}"))?;
    if !executable.starts_with(&target_root) {
        return Err("Studio executable must be inside the workspace target directory".into());
    }
    let version = studio_version(&root)?;
    let package_directory = format!("sensor-watch-studio-{version}");
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
        format!("{package_directory}/app/sensor-watch-studio{EXE_SUFFIX}"),
        executable,
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
    let mut entries = files
        .iter()
        .map(|(path, source)| package_entry(path, source))
        .collect::<ToolResult<Vec<_>>>()?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = serde_json::to_vec_pretty(&json!({
        "schema_version": PACKAGE_SCHEMA,
        "current_version": { "version": version },
        "launcher_executable": format!("app/sensor-watch-studio{EXE_SUFFIX}"),
        "app_directory": "app",
        "resources_directory": "resources",
        "templates_directory": "templates",
        "firmware_project_directory": "firmware",
        "user_data_directory": "(platform user-data directory; not included)"
    }))
    .map_err(|e| format!("cannot serialize package metadata: {e}"))?;
    let readme = format!(
        "Sensor-Watch Studio {version}\n\nThis is an offline folder package. Mutable settings and user projects stay\noutside this ZIP in the platform user-data directory.\n\nCapabilities:\n- Studio executable: available\n- Bundled resources: available\n- Bundled templates: available\n- Firmware project template: available\n- Firmware tools and cross-compilation targets: not bundled\n- Network self-update or cryptographic signature: not provided\n\nPACKAGE-MANIFEST.json lists the SHA-256 digest of every packaged file.\n"
    );
    files.push((manifest_path, bytes_path(&manifest)));
    files.push((readme_path, bytes_path(readme.as_bytes())));
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
    let path = env::temp_dir().join(format!("sensor-watch-package-bytes-{}", process_id()));
    fs::write(&path, bytes).expect("temporary package metadata write");
    path
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
        (root, executable)
    }
    fn zip_names(path: &Path) -> Vec<String> {
        let file = fs::File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_owned())
            .collect()
    }
    #[test]
    fn traversal_components_are_rejected() {
        assert!(!safe_component(Path::new("../secret")));
        assert!(!safe_component(Path::new("a/b")));
        assert!(safe_component(Path::new("safe")));
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
        let _ = fs::remove_dir_all(root);
    }
}
