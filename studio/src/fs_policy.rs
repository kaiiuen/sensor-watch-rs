//! Central filesystem policy for the Studio File Browser and text editor.

use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

pub const MAX_TEXT_BYTES: u64 = 512 * 1024;
pub const MAX_SEARCH_DEPTH: usize = 12;
pub const MAX_SEARCH_ENTRIES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootKind {
    AppData,
    ActiveProject,
}

impl RootKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::AppData => "App data",
            Self::ActiveProject => "Active project",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roots {
    pub app_data: PathBuf,
    pub active_project: Option<PathBuf>,
    immutable: Vec<PathBuf>,
}

impl Roots {
    pub fn empty() -> Self {
        Self {
            app_data: PathBuf::new(),
            active_project: None,
            immutable: Vec::new(),
        }
    }

    pub fn from_distribution(status: &crate::distribution::PackageStatus) -> Self {
        let mut immutable = Vec::new();
        for path in [
            status.root.clone(),
            status.app_directory.clone(),
            status.resources.clone(),
            status.templates.clone(),
            status.firmware_project.clone(),
            status.tools.clone(),
            status.targets.clone(),
        ]
        .into_iter()
        .flatten()
        {
            let is_allowed_ancestor = status
                .active_project
                .as_deref()
                .is_some_and(|project| project.starts_with(&path))
                || status.user_data_root.starts_with(&path);
            if status.active_project.as_deref() != Some(path.as_path()) && !is_allowed_ancestor {
                immutable.push(path);
            }
        }
        Self {
            app_data: status.user_data_root.clone(),
            active_project: status.active_project.clone(),
            immutable,
        }
    }

    #[cfg(test)]
    pub(crate) fn test(app_data: PathBuf, active_project: Option<PathBuf>) -> Self {
        Self {
            app_data,
            active_project,
            immutable: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_with_immutable(
        app_data: PathBuf,
        active_project: PathBuf,
        immutable: PathBuf,
    ) -> Self {
        Self {
            app_data,
            active_project: Some(active_project),
            immutable: vec![immutable],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub path: PathBuf,
    pub relative: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyError {
    Unavailable(String),
    OutsideRoot,
    Immutable,
    InvalidName,
    Sensitive,
    Link,
    Reserved,
    RootMutation,
    Collision,
    TooLarge,
    TooDeep,
    TooManyEntries,
    NotText,
    Changed,
    Io(String),
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Unavailable(v) | Self::Io(v) => v.clone(),
            Self::OutsideRoot => "path is outside the selected Studio root".into(),
            Self::Immutable => "package and reference files are read-only".into(),
            Self::InvalidName => "invalid file name or traversal component".into(),
            Self::Sensitive => "credentials and secret files are not available".into(),
            Self::Link => "symlinks and reparse points are not allowed".into(),
            Self::Reserved => "reserved or generated Studio path".into(),
            Self::RootMutation => "the selected root cannot be deleted or moved".into(),
            Self::Collision => "destination already exists".into(),
            Self::TooLarge => "file exceeds the Studio text size limit".into(),
            Self::TooDeep => "directory depth exceeds the Studio limit".into(),
            Self::TooManyEntries => "entry limit exceeded".into(),
            Self::NotText => "file is not safe UTF-8 text".into(),
            Self::Changed => "file changed externally, reload or resolve the conflict".into(),
        };
        f.write_str(&text)
    }
}

impl From<io::Error> for PolicyError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct Policy {
    roots: Roots,
}

impl Policy {
    pub fn new(roots: Roots) -> Self {
        Self { roots }
    }

    pub fn root(&self, kind: RootKind) -> Result<PathBuf, PolicyError> {
        let path =
            match kind {
                RootKind::AppData => self.roots.app_data.clone(),
                RootKind::ActiveProject => self.roots.active_project.clone().ok_or_else(|| {
                    PolicyError::Unavailable("active project is unavailable".into())
                })?,
            };
        canonical_dir(&path)
    }

    pub fn list(&self, kind: RootKind, relative: &Path) -> Result<Vec<Item>, PolicyError> {
        let root = self.root(kind)?;
        let dir = self.resolve_existing(kind, relative, true)?;
        let mut result = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink()
                || is_reparse(&metadata)
                || is_sensitive_path(&path)
            {
                continue;
            }
            let canonical = path.canonicalize()?;
            if !canonical.starts_with(&root) {
                continue;
            }
            result.push(Item {
                relative: canonical
                    .strip_prefix(&root)
                    .unwrap_or(Path::new(""))
                    .to_path_buf(),
                path: canonical.clone(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                modified: metadata.modified().ok(),
                read_only: self.is_read_only(kind, &canonical),
            });
        }
        result.sort_by_key(|item| {
            (
                !item.is_dir,
                item.relative.to_string_lossy().to_ascii_lowercase(),
            )
        });
        Ok(result)
    }

    pub fn search(&self, kind: RootKind, query: &str) -> Result<Vec<Item>, PolicyError> {
        let root = self.root(kind)?;
        let mut found = Vec::new();
        self.search_dir(kind, &root, &root, query, 0, &mut found)?;
        Ok(found)
    }

    fn search_dir(
        &self,
        kind: RootKind,
        root: &Path,
        dir: &Path,
        query: &str,
        depth: usize,
        found: &mut Vec<Item>,
    ) -> Result<(), PolicyError> {
        if depth > MAX_SEARCH_DEPTH {
            return Err(PolicyError::TooDeep);
        }
        for entry in fs::read_dir(dir)? {
            if found.len() >= MAX_SEARCH_ENTRIES {
                return Err(PolicyError::TooManyEntries);
            }
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink()
                || is_reparse(&metadata)
                || is_sensitive_path(&path)
            {
                continue;
            }
            let canonical = path.canonicalize()?;
            if !canonical.starts_with(root) {
                return Err(PolicyError::OutsideRoot);
            }
            let item = Item {
                relative: canonical
                    .strip_prefix(root)
                    .unwrap_or(Path::new(""))
                    .to_path_buf(),
                path: canonical.clone(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                modified: metadata.modified().ok(),
                read_only: self.is_read_only(kind, &canonical),
            };
            if query.is_empty()
                || item
                    .relative
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .contains(&query.to_ascii_lowercase())
            {
                found.push(item.clone());
            }
            if metadata.is_dir() {
                self.search_dir(kind, root, &canonical, query, depth + 1, found)?;
            }
        }
        Ok(())
    }

    pub fn is_read_only(&self, kind: RootKind, path: &Path) -> bool {
        kind == RootKind::ActiveProject
            && self
                .roots
                .immutable
                .iter()
                .any(|root| same_or_below(path, root))
    }

    pub fn read_text(&self, kind: RootKind, relative: &Path) -> Result<String, PolicyError> {
        let path = self.resolve_existing(kind, relative, false)?;
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.len() > MAX_TEXT_BYTES {
            return Err(PolicyError::TooLarge);
        }
        let bytes = fs::read(&path)?;
        String::from_utf8(bytes).map_err(|_| PolicyError::NotText)
    }

    pub fn create_dir(&self, kind: RootKind, relative: &Path) -> Result<(), PolicyError> {
        let path = self.resolve_new(kind, relative, true)?;
        fs::create_dir(&path)?;
        Ok(())
    }

    pub fn create_file(&self, kind: RootKind, relative: &Path) -> Result<(), PolicyError> {
        let path = self.resolve_new(kind, relative, false)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(b"")?;
        file.sync_all()?;
        Ok(())
    }

    pub fn write_text(
        &self,
        kind: RootKind,
        relative: &Path,
        contents: &str,
        expected: Option<&str>,
    ) -> Result<(), PolicyError> {
        if contents.len() as u64 > MAX_TEXT_BYTES {
            return Err(PolicyError::TooLarge);
        }
        let path = self.resolve_existing(kind, relative, false)?;
        if let Some(expected) = expected {
            let current = self.read_text(kind, relative)?;
            if current != expected {
                return Err(PolicyError::Changed);
            }
        }
        atomic_write(&path, contents.as_bytes()).map_err(PolicyError::from)
    }

    pub fn remove(&self, kind: RootKind, relative: &Path) -> Result<(), PolicyError> {
        let path = self.resolve_existing(kind, relative, false)?;
        if relative.as_os_str().is_empty() {
            return Err(PolicyError::RootMutation);
        }
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn rename(&self, kind: RootKind, from: &Path, to: &Path) -> Result<(), PolicyError> {
        let source = self.resolve_existing(kind, from, false)?;
        if from.as_os_str().is_empty() {
            return Err(PolicyError::RootMutation);
        }
        let destination = self.resolve_new(kind, to, false)?;
        fs::rename(source, destination)?;
        Ok(())
    }

    fn resolve_existing(
        &self,
        kind: RootKind,
        relative: &Path,
        directory: bool,
    ) -> Result<PathBuf, PolicyError> {
        let root = self.root(kind)?;
        validate_relative(relative)?;
        let requested = root.join(relative);
        let canonical = requested
            .canonicalize()
            .map_err(|_| PolicyError::OutsideRoot)?;
        self.validate_canonical(kind, &root, &canonical, directory)
    }

    fn resolve_new(
        &self,
        kind: RootKind,
        relative: &Path,
        directory: bool,
    ) -> Result<PathBuf, PolicyError> {
        let root = self.root(kind)?;
        validate_relative(relative)?;
        if relative.as_os_str().is_empty() {
            return Err(PolicyError::RootMutation);
        }
        let parent = relative.parent().unwrap_or(Path::new(""));
        let parent = self.resolve_existing(kind, parent, true)?;
        let name = relative.file_name().ok_or(PolicyError::InvalidName)?;
        validate_name(name)?;
        let path = parent.join(name);
        if path.exists() {
            return Err(PolicyError::Collision);
        }
        let canonical_parent = parent.canonicalize()?;
        if !canonical_parent.starts_with(&root) {
            return Err(PolicyError::OutsideRoot);
        }
        let _ = directory;
        self.validate_canonical(kind, &root, &path, false)
            .or_else(|error| {
                if matches!(error, PolicyError::OutsideRoot) {
                    Ok(path)
                } else {
                    Err(error)
                }
            })
    }

    fn validate_canonical(
        &self,
        kind: RootKind,
        root: &Path,
        path: &Path,
        directory: bool,
    ) -> Result<PathBuf, PolicyError> {
        if !path.starts_with(root) {
            return Err(PolicyError::OutsideRoot);
        }
        if is_sensitive_path(path) {
            return Err(PolicyError::Sensitive);
        }
        if is_reserved_path(path) {
            return Err(PolicyError::Reserved);
        }
        if let Ok(meta) = fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() || is_reparse(&meta) {
                return Err(PolicyError::Link);
            }
            if directory && !meta.is_dir() {
                return Err(PolicyError::Io("expected a directory".into()));
            }
        }
        if kind == RootKind::ActiveProject
            && self
                .roots
                .immutable
                .iter()
                .any(|item| same_or_below(path, item))
        {
            return Err(PolicyError::Immutable);
        }
        Ok(path.to_path_buf())
    }
}

fn validate_relative(path: &Path) -> Result<(), PolicyError> {
    if path.is_absolute() {
        return Err(PolicyError::InvalidName);
    }
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(PolicyError::InvalidName);
        }
        if let Component::Normal(name) = component {
            validate_name(name)?;
        }
    }
    Ok(())
}

fn validate_name(name: &std::ffi::OsStr) -> Result<(), PolicyError> {
    let value = name.to_string_lossy();
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.ends_with('.')
        || value.ends_with(' ')
    {
        return Err(PolicyError::InvalidName);
    }
    let upper = value.to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3",
    ]
    .contains(&upper.as_str())
    {
        return Err(PolicyError::InvalidName);
    }
    if is_secret_name(&value) {
        return Err(PolicyError::Sensitive);
    }
    Ok(())
}

fn is_sensitive_path(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, Component::Normal(name) if is_secret_name(&name.to_string_lossy())))
}
fn is_secret_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == ".env"
        || n.contains("secret")
        || n.contains("credential")
        || n.contains("password")
        || n.contains("token")
        || n.ends_with(".pem")
        || n.ends_with(".key")
}
fn is_reserved_path(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::Normal(name) if [".git", "target", "resources", "templates", "tools", "targets", "firmware", ".project.copying", ".studio-writing"].contains(&name.to_string_lossy().to_ascii_lowercase().as_str())))
}
fn same_or_below(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}
fn canonical_dir(path: &Path) -> Result<PathBuf, PolicyError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| PolicyError::Unavailable(e.to_string()))?;
    let meta = fs::symlink_metadata(&canonical)?;
    if !meta.is_dir() || meta.file_type().is_symlink() || is_reparse(&meta) {
        return Err(PolicyError::Link);
    }
    Ok(canonical)
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse(_: &fs::Metadata) -> bool {
    false
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_extension("studio-writing");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(name: &str) -> (PathBuf, Policy) {
        let root =
            std::env::temp_dir().join(format!("studio-policy-{name}-{}", std::process::id()));
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        (
            root.clone(),
            Policy::new(Roots::test(root.join("data"), Some(project))),
        )
    }
    #[test]
    fn crud_parent_and_atomic_text() {
        let (root, policy) = setup("crud");
        fs::create_dir_all(root.join("data/one")).unwrap();
        policy
            .create_dir(RootKind::AppData, Path::new("one/two"))
            .unwrap();
        policy
            .create_file(RootKind::AppData, Path::new("one/two/a.txt"))
            .unwrap();
        policy
            .write_text(
                RootKind::AppData,
                Path::new("one/two/a.txt"),
                "héllo",
                Some(""),
            )
            .unwrap();
        assert_eq!(
            policy
                .read_text(RootKind::AppData, Path::new("one/two/a.txt"))
                .unwrap(),
            "héllo"
        );
        policy
            .rename(
                RootKind::AppData,
                Path::new("one/two/a.txt"),
                Path::new("one/two/b.txt"),
            )
            .unwrap();
        policy
            .remove(RootKind::AppData, Path::new("one/two/b.txt"))
            .unwrap();
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn rejects_traversal_reserved_secret_and_root() {
        let (_root, policy) = setup("reject");
        assert!(policy
            .create_file(RootKind::ActiveProject, Path::new("../x"))
            .is_err());
        assert!(policy
            .create_file(RootKind::ActiveProject, Path::new("CON"))
            .is_err());
        assert!(policy
            .create_file(RootKind::ActiveProject, Path::new("secret.txt"))
            .is_err());
        assert!(policy
            .remove(RootKind::ActiveProject, Path::new(""))
            .is_err());
    }
    #[test]
    fn search_is_bounded() {
        let (root, policy) = setup("search");
        fs::create_dir_all(root.join("data/a/b/c")).unwrap();
        fs::write(root.join("data/a/b/c/note.txt"), b"x").unwrap();
        assert_eq!(policy.search(RootKind::AppData, "note").unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn atomic_write_failure_preserves_original() {
        let (root, policy) = setup("atomic-failure");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data/a.txt"), b"old").unwrap();
        fs::write(root.join("data/a.studio-writing"), b"busy").unwrap();
        assert!(policy
            .write_text(RootKind::AppData, Path::new("a.txt"), "new", None)
            .is_err());
        assert_eq!(fs::read(root.join("data/a.txt")).unwrap(), b"old");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_rejected() {
        use std::os::unix::fs::symlink;
        let (root, policy) = setup("symlink");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("outside.txt"), b"x").unwrap();
        symlink(root.join("outside.txt"), root.join("data/link.txt")).unwrap();
        assert!(policy
            .read_text(RootKind::AppData, Path::new("link.txt"))
            .is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn immutable_descendants_are_read_only_and_cannot_be_mutated() {
        let root =
            std::env::temp_dir().join(format!("studio-policy-immutable-{}", std::process::id()));
        let project = root.join("project");
        let resources = project.join("package-v1");
        fs::create_dir_all(&resources).unwrap();
        fs::write(resources.join("version.txt"), b"v1").unwrap();
        let policy = Policy::new(Roots::test_with_immutable(
            root.join("data"),
            project,
            resources.canonicalize().unwrap(),
        ));
        let entries = policy.list(RootKind::ActiveProject, Path::new("")).unwrap();
        assert!(entries
            .iter()
            .any(|entry| { entry.relative == Path::new("package-v1") && entry.read_only }));
        assert_eq!(
            policy.remove(RootKind::ActiveProject, Path::new("package-v1/version.txt")),
            Err(PolicyError::Immutable)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dirty_and_external_conflict_is_rejected() {
        let (root, policy) = setup("conflict");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data/a.txt"), b"old").unwrap();
        let original = policy
            .read_text(RootKind::AppData, Path::new("a.txt"))
            .unwrap();
        fs::write(root.join("data/a.txt"), b"external").unwrap();
        assert_eq!(
            policy.write_text(
                RootKind::AppData,
                Path::new("a.txt"),
                "new",
                Some(&original)
            ),
            Err(PolicyError::Changed)
        );
        let _ = fs::remove_dir_all(root);
    }
}
