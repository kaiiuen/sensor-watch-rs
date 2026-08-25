//! Watch-face editor.
//!
//! Provides templates and editing support for creating and modifying watch
//! faces. The editor works on the firmware's `src/movement/` source files.

use super::faces::face_identity;
use crate::file_browser::FileBrowser;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// A template for a new watch face.
pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    pub code: &'static str,
}

/// The available face templates.
pub const TEMPLATES: [Template; 3] = [
    Template {
        name: "Simple Clock",
        description: "A minimal clock face showing the time.",
        code: "//! {NAME} watch face.\n\nuse crate::movement::types::{Event, Settings, WatchFace};\nuse crate::watch;\n\npub struct {Name}Face;\n\nimpl {Name}Face {\n    pub const fn new_static() -> Self { {Name}Face }\n    pub fn new() -> Self { {Name}Face }\n}\n\nimpl WatchFace for {Name}Face {\n    fn setup(&mut self, _settings: &Settings, _index: usize) {}\n    fn activate(&mut self, _settings: &Settings) {}\n    fn loop_(&mut self, event: Event, _settings: &mut Settings) {\n        match event {\n            Event::Activate | Event::Tick => {\n                watch::slcd::display_string(\"HELLO\", 0);\n            }\n            _ => {}\n        }\n    }\n    fn resign(&mut self, _settings: &mut Settings) {}\n}\n",
    },
    Template {
        name: "Counter",
        description: "A tally counter that increments on a button press.",
        code: "//! {NAME} watch face.\n\nuse crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};\nuse crate::watch;\n\npub struct {Name}Face {\n    count: u32,\n}\n\nimpl {Name}Face {\n    pub const fn new_static() -> Self { {Name}Face { count: 0 } }\n    pub fn new() -> Self { {Name}Face { count: 0 } }\n}\n\nimpl WatchFace for {Name}Face {\n    fn setup(&mut self, _settings: &Settings, _index: usize) {}\n    fn activate(&mut self, _settings: &Settings) {}\n    fn loop_(&mut self, event: Event, _settings: &mut Settings) {\n        match event {\n            Event::Button(Button::Alarm, ButtonEvent::Up) => {\n                self.count += 1;\n                let mut buf = [0u8; 11];\n                let v = self.count;\n                buf[0] = b'0' + (v / 1000 % 10) as u8;\n                buf[1] = b'0' + (v / 100 % 10) as u8;\n                buf[2] = b'0' + (v / 10 % 10) as u8;\n                buf[3] = b'0' + (v % 10) as u8;\n                watch::slcd::display_string(core::str::from_utf8(&buf[..4]).unwrap_or(\"\"), 0);\n            }\n            _ => {}\n        }\n    }\n    fn resign(&mut self, _settings: &mut Settings) {}\n}\n",
    },
    Template {
        name: "Blank",
        description: "An empty face to fill in.",
        code: "//! {NAME} watch face.\n\nuse crate::movement::types::{Event, Settings, WatchFace};\n\npub struct {Name}Face;\n\nimpl {Name}Face {\n    pub const fn new_static() -> Self { {Name}Face }\n    pub fn new() -> Self { {Name}Face }\n}\n\nimpl WatchFace for {Name}Face {\n    fn setup(&mut self, _settings: &Settings, _index: usize) {}\n    fn activate(&mut self, _settings: &Settings) {}\n    fn loop_(&mut self, event: Event, _settings: &mut Settings) {\n        match event {\n            _ => {}\n        }\n    }\n    fn resign(&mut self, _settings: &mut Settings) {}\n}\n",
    },
];

/// Generates the source for a new face from a template, optionally including
/// a human-readable description as a doc comment.
pub fn generate_face(name: &str, template: &Template, description: &str) -> String {
    // Convert "my_face" to "MyFace" for the struct name.
    let struct_name = name
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<String>();
    let desc = if description.trim().is_empty() {
        String::new()
    } else {
        format!("\n/// Description: {}\n", description.trim())
    };
    let mut out = template
        .code
        .replace("{NAME}", &name.to_uppercase())
        .replace("{Name}", &struct_name);
    // Insert the description after the leading `//! {NAME} watch face.` line.
    if let Some(pos) = out.find("\n") {
        out.insert_str(pos, &desc);
    }
    out
}

/// Validates the user-controlled face name used to construct a source path.
pub fn validate_face_name(name: &str) -> Result<(), String> {
    if name.eq_ignore_ascii_case("mod")
        || name.is_empty()
        || name.len() > 64
        || !name.is_ascii()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || name.starts_with(|c: char| c.is_ascii_digit())
    {
        return Err(format!("invalid face name {name:?}: use ASCII snake_case"));
    }
    Ok(())
}

fn project_dir() -> Result<std::path::PathBuf, String> {
    if let Some(path) = crate::distribution::active().active_project_dir() {
        return Ok(path);
    }
    if !crate::distribution::initialized() {
        let path = crate::build::firmware_dir();
        if path.is_dir() {
            return Ok(path);
        }
    }
    Err("mutable project is unavailable: bundled firmware is read-only".into())
}

/// Returns a validated face path under the active mutable project's movement directory.
fn checked_face_path(name: &str) -> Result<std::path::PathBuf, String> {
    validate_face_name(name)?;
    checked_face_path_in(&project_dir()?, name)
}

fn checked_face_path_in(
    project: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, String> {
    validate_face_name(name)?;
    let movement = project.join("src/movement");
    let root = movement
        .canonicalize()
        .map_err(|e| format!("cannot resolve face directory: {e}"))?;
    let path = root.join(format!("{name}.rs"));
    if !path.starts_with(&root) {
        return Err("face path escapes the movement directory".into());
    }
    if let Ok(canonical_path) = path.canonicalize() {
        if !canonical_path.starts_with(&root) {
            return Err("face path escapes the movement directory".into());
        }
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("face path must be a regular file, not a symlink or directory".into());
        }
    }
    Ok(path)
}

/// The path to a face's source file. Callers performing I/O must use the
/// checked helpers below; this function is retained for display purposes.
pub fn face_path(name: &str) -> std::path::PathBuf {
    crate::distribution::active()
        .active_project_dir()
        .or_else(|| (!crate::distribution::initialized()).then(crate::build::firmware_dir))
        .unwrap_or_default()
        .join("src/movement")
        .join(format!("{name}.rs"))
}

/// Writes a face source file.
pub fn write_face(name: &str, source: &str) -> Result<(), String> {
    write_face_in(&project_dir()?, name, source)
}

fn write_face_in(project: &std::path::Path, name: &str, source: &str) -> Result<(), String> {
    let path = checked_face_path_in(project, name)?;
    atomic_write(&path, source.as_bytes()).map_err(|e| format!("cannot write face: {e}"))
}

/// Reads a face source file.
pub fn read_face(name: &str) -> Result<String, String> {
    let path = checked_face_path(name)?;
    std::fs::read_to_string(&path).map_err(|e| format!("cannot read face: {e}"))
}

/// Deletes a face source file.
pub fn delete_face(name: &str) -> Result<(), String> {
    let path = checked_face_path(name)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("cannot delete face: {e}"))
    } else {
        Ok(())
    }
}

/// Removes a face's exact module declaration from `movement/mod.rs`.
///
/// This is intentionally best-effort for older projects: a missing declaration
/// is already the desired state, while malformed or unsafe module files fail.
pub fn unregister_face(name: &str) -> Result<(), String> {
    validate_face_name(name)?;
    let movement = project_dir()?.join("src/movement");
    let root = movement
        .canonicalize()
        .map_err(|e| format!("cannot resolve face directory: {e}"))?;
    let path = root.join("mod.rs");
    if std::fs::symlink_metadata(&path)
        .map(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
        .unwrap_or(false)
    {
        return Err("movement module must be a regular file, not a symlink or directory".into());
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read movement module: {e}"))?;
    let declaration = format!("pub mod {name};");
    let updated: String = content
        .lines()
        .filter(|line| line.trim() != declaration)
        .map(|line| format!("{line}\n"))
        .collect();
    if updated == content {
        return Ok(());
    }
    atomic_write(&path, updated.as_bytes())
        .map_err(|e| format!("cannot update movement module: {e}"))
}

/// Registers a face so it becomes visible to `discover_faces` and compiles into
/// the firmware.
///
/// Best-effort and defensive: this only guarantees the `pub mod <name>;`
/// declaration exists in `src/movement/mod.rs`. It does NOT touch the
/// `WATCH_FACES[]` array, the `MOVEMENT_NUM_FACES` const, or any `#[used]`
/// static declaration - those require matching struct storage the template may
/// not guarantee, and editing numeric consts is risky. If the declaration is
/// already present, this is treated as success.
pub fn register_face(name: &str) -> Result<(), String> {
    validate_face_name(name)?;
    let movement = project_dir()?.join("src/movement");
    let root = movement
        .canonicalize()
        .map_err(|e| format!("cannot resolve face directory: {e}"))?;
    let path = root.join("mod.rs");
    if !path.starts_with(&root) {
        return Err("movement module path escapes the movement directory".into());
    }
    if let Ok(canonical_path) = path.canonicalize() {
        if !canonical_path.starts_with(&root) {
            return Err("movement module path escapes the movement directory".into());
        }
    }
    if std::fs::symlink_metadata(&path)
        .map(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
        .unwrap_or(false)
    {
        return Err("movement module must be a regular file, not a symlink or directory".into());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let declaration = format!("pub mod {name};");
    let identity = face_identity(name);

    // Rust module names and source filenames collide on case-insensitive
    // filesystems. Exact registration is idempotent; a case-only variant is
    // rejected instead of creating an ambiguous or silently overwritten face.
    for line in content.lines() {
        let Some(module) = line
            .trim()
            .strip_prefix("pub mod ")
            .and_then(|rest| rest.strip_suffix(';'))
            .map(str::trim)
        else {
            continue;
        };
        if face_identity(module) == identity {
            if module == name {
                return Ok(());
            }
            return Err(format!(
                "face name {name:?} collides with existing module {module:?}"
            ));
        }
    }
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            let Some(stem) = file_name.strip_suffix(".rs") else {
                continue;
            };
            if face_identity(stem) == identity && stem != name {
                return Err(format!(
                    "face name {name:?} collides with existing source {file_name:?}"
                ));
            }
        }
    }

    // Insert before the `use crate::movement::types::*;` line, which ends the
    // block of `pub mod` declarations.
    const ANCHOR: &str = "use crate::movement::types::*;";
    let insertion = format!("{declaration}\n\n");
    let updated = if let Some(pos) = content.find(ANCHOR) {
        let mut s = content.clone();
        s.insert_str(pos, &insertion);
        s
    } else {
        // Fallback: append at the end of the file.
        let mut s = content;
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&declaration);
        s.push('\n');
        s
    };

    atomic_write(&path, updated.as_bytes()).map_err(|e| e.to_string())
}

fn atomic_write(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    let temp = path.with_extension("rs.studio-writing");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    use std::io::Write;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temp, path)
}

/// A safe text document owned by an IDE editor tab.
#[derive(Clone, Debug)]
pub struct DocumentTab {
    pub path: PathBuf,
    pub contents: String,
    pub original_hash: [u8; 32],
    original_contents: String,
    pub dirty: bool,
    pub external_conflict: bool,
}

impl DocumentTab {
    fn hash(contents: &str) -> [u8; 32] {
        Sha256::digest(contents.as_bytes()).into()
    }

    pub fn new(path: PathBuf, contents: String) -> Self {
        Self {
            original_hash: Self::hash(&contents),
            original_contents: contents.clone(),
            path,
            contents,
            dirty: false,
            external_conflict: false,
        }
    }

    pub fn set_contents(&mut self, contents: String) {
        self.dirty = contents != self.contents || Self::hash(&contents) != self.original_hash;
        self.contents = contents;
    }

    pub fn original_contents(&self) -> &str {
        &self.original_contents
    }

    pub fn mark_saved(&mut self) {
        self.original_hash = Self::hash(&self.contents);
        self.original_contents = self.contents.clone();
        self.dirty = false;
        self.external_conflict = false;
    }

    pub fn check_external_change(&mut self, browser: &FileBrowser) -> bool {
        let changed = browser
            .read_text_path(&self.path)
            .map(|source| Self::hash(&source) != self.original_hash)
            .unwrap_or(true);
        self.external_conflict = changed;
        changed
    }

    pub fn reload(&mut self, browser: &FileBrowser) -> Result<(), String> {
        let source = browser
            .read_text_path(&self.path)
            .map_err(|e| e.to_string())?;
        self.original_hash = Self::hash(&source);
        self.original_contents = source.clone();
        self.contents = source;
        self.dirty = false;
        self.external_conflict = false;
        Ok(())
    }

    pub fn save(&mut self, browser: &FileBrowser) -> Result<(), String> {
        if self.external_conflict || self.check_external_change(browser) {
            return Err("file changed externally, reload or resolve the conflict".into());
        }
        browser
            .write_text_path(&self.path, &self.contents, Some(&self.original_contents))
            .map_err(|e| e.to_string())?;
        self.original_hash = Self::hash(&self.contents);
        self.original_contents = self.contents.clone();
        self.dirty = false;
        self.external_conflict = false;
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct DocumentTabs {
    pub tabs: Vec<DocumentTab>,
    pub active: Option<usize>,
    pub close_prompt: Option<usize>,
}

impl DocumentTabs {
    pub fn open(&mut self, path: PathBuf, contents: String) -> usize {
        if let Some(index) = self.tabs.iter().position(|tab| tab.path == path) {
            self.active = Some(index);
            return index;
        }
        self.tabs.push(DocumentTab::new(path, contents));
        let index = self.tabs.len() - 1;
        self.active = Some(index);
        index
    }
    pub fn active(&self) -> Option<&DocumentTab> {
        self.active.and_then(|i| self.tabs.get(i))
    }

    pub fn select(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active = Some(index);
            true
        } else {
            false
        }
    }
    pub fn request_close(&mut self, index: usize) -> bool {
        if let Some(tab) = self.tabs.get(index) {
            if tab.dirty || tab.external_conflict {
                self.close_prompt = Some(index);
                return false;
            }
            self.close_confirmed(index);
            return true;
        }
        false
    }
    pub fn confirm_close(&mut self, index: usize) -> bool {
        if self.close_prompt != Some(index) {
            return false;
        }
        self.close_prompt = None;
        self.close_confirmed(index);
        true
    }
    pub fn cancel_close(&mut self) {
        self.close_prompt = None;
    }
    fn close_confirmed(&mut self, index: usize) {
        self.tabs.remove(index);
        self.active = match self.active {
            Some(_) if self.tabs.is_empty() => None,
            Some(active) if active > index => Some(active - 1),
            Some(active) if active == index => Some(active.min(self.tabs.len() - 1)),
            other => other,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_rejects_case_only_existing_module_collision() {
        let error = register_face("SIMPLE_CLOCK").expect_err("case-only collision must fail");
        assert!(error.contains("simple_clock"));
    }

    #[test]
    fn document_tabs_switch_and_protect_dirty_close() {
        let mut tabs = DocumentTabs::default();
        let first = tabs.open(PathBuf::from("one.txt"), "one".into());
        let second = tabs.open(PathBuf::from("two.txt"), "two".into());
        assert_eq!(tabs.active, Some(second));
        tabs.tabs[first].set_contents("edited".into());
        assert!(!tabs.request_close(first));
        assert_eq!(tabs.close_prompt, Some(first));
        tabs.cancel_close();
        assert!(tabs.select(first));
        assert_eq!(tabs.active().unwrap().contents, "edited");
        assert!(tabs.request_close(second));
        assert_eq!(tabs.tabs.len(), 1);
    }

    #[test]
    fn document_hash_and_external_change_are_detectable() {
        let first = DocumentTab::new(PathBuf::from("a.txt"), "same".into());
        let changed = DocumentTab::new(PathBuf::from("a.txt"), "different".into());
        assert_ne!(first.original_hash, changed.original_hash);
        let mut tabs = DocumentTabs::default();
        tabs.open(first.path.clone(), first.contents.clone());
        tabs.tabs[0].external_conflict = true;
        assert!(!tabs.request_close(0));
        assert_eq!(tabs.close_prompt, Some(0));
    }

    #[test]
    fn external_change_requires_reload_before_save() {
        let root =
            std::env::temp_dir().join(format!("studio-document-conflict-{}", std::process::id()));
        let data = root.join("data");
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let path = project.join("note.txt");
        std::fs::write(&path, "original").unwrap();
        let path = path.canonicalize().unwrap();
        let browser = FileBrowser::from_project_roots(data, project.clone());
        let mut tab = DocumentTab::new(path.clone(), "original".into());
        tab.set_contents("local edit".into());
        std::fs::write(&path, "external edit").unwrap();
        assert!(tab.check_external_change(&browser));
        assert!(tab.save(&browser).is_err());
        tab.reload(&browser).unwrap();
        assert_eq!(tab.contents, "external edit");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn project_root_constructor_starts_at_active_project() {
        let root = std::env::temp_dir().join(format!("studio-project-root-{}", std::process::id()));
        let data = root.join("data");
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let browser = FileBrowser::from_project_roots(data, project.clone());
        assert_eq!(
            browser.active_tab().root_kind,
            crate::fs_policy::RootKind::ActiveProject
        );
        assert_eq!(
            browser.active_tab().current_dir,
            project.canonicalize().unwrap()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn editor_write_uses_mutable_project_root() {
        let root = std::env::temp_dir().join(format!("studio-editor-{}", std::process::id()));
        let movement = root.join("src/movement");
        std::fs::create_dir_all(&movement).unwrap();
        std::fs::write(movement.join("mod.rs"), "use crate::movement::types::*;\n").unwrap();
        let source = "//! edited\n";
        write_face_in(&root, "edited", source).unwrap();
        assert_eq!(
            std::fs::read_to_string(movement.join("edited.rs")).unwrap(),
            source
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
