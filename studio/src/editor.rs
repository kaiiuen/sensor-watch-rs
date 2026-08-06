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

/// Generates the source for a new face from a template.
pub fn generate_face(name: &str, template: &Template) -> String {
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
    template
        .code
        .replace("{NAME}", &name.to_uppercase())
        .replace("{Name}", &struct_name)
}

/// The path to a face's source file.
pub fn face_path(name: &str) -> std::path::PathBuf {
    crate::build::firmware_dir()
        .join("src/movement")
        .join(format!("{name}.rs"))
}

/// Writes a face source file.
pub fn write_face(name: &str, source: &str) -> Result<(), String> {
    let path = face_path(name);
    std::fs::write(&path, source).map_err(|e| e.to_string())
}

/// Reads a face source file.
pub fn read_face(name: &str) -> Result<String, String> {
    let path = face_path(name);
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Deletes a face source file.
pub fn delete_face(name: &str) -> Result<(), String> {
    let path = face_path(name);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}
