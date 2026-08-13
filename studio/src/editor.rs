//! Watch-face editor.
//!
//! Provides templates and editing support for creating and modifying watch
//! faces. The editor works on the firmware's `src/movement/` source files.

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
        return Err(format!("invalid face name {name:?}; use ASCII snake_case"));
    }
    Ok(())
}

/// Returns a validated face path under the firmware movement directory.
fn checked_face_path(name: &str) -> Result<std::path::PathBuf, String> {
    validate_face_name(name)?;
    let movement = crate::build::firmware_dir().join("src/movement");
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
    crate::build::firmware_dir()
        .join("src/movement")
        .join(format!("{name}.rs"))
}

/// Writes a face source file.
pub fn write_face(name: &str, source: &str) -> Result<(), String> {
    let path = checked_face_path(name)?;
    std::fs::write(&path, source).map_err(|e| format!("cannot write face: {e}"))
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
    let movement = crate::build::firmware_dir().join("src/movement");
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
    std::fs::write(&path, updated).map_err(|e| format!("cannot update movement module: {e}"))
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
    let movement = crate::build::firmware_dir().join("src/movement");
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

    // If already declared, nothing to do.
    let already_declared = content.lines().any(|l| l.trim() == declaration);
    if already_declared {
        return Ok(());
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

    std::fs::write(&path, updated).map_err(|e| e.to_string())
}
