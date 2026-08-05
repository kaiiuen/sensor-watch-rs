//! Watch-face presets.
//!
//! A preset is a named, ordered selection of watch faces. The default preset is
//! the stock Casio F-91W face set. Users can create, edit, delete, and
//! reorder presets, and the simulator renders the selected preset's faces.

use std::collections::VecDeque;

/// A single watch-face preset.
#[derive(Clone, Debug)]
pub struct Preset {
    /// The preset name.
    pub name: String,
    /// The ordered list of face names in this preset.
    pub faces: Vec<String>,
}

/// The preset manager.
pub struct PresetManager {
    /// All presets.
    pub presets: Vec<Preset>,
    /// The index of the active preset.
    pub active: usize,
}

impl PresetManager {
    /// Creates the manager with the default stock preset.
    pub fn new() -> Self {
        let mut presets = Vec::new();
        // The default preset: the stock Casio F-91W face set.
        presets.push(Preset {
            name: "Stock Casio".to_string(),
            faces: vec![
                "SIMPLE_CLOCK".to_string(),
                "ALARM".to_string(),
                "STOPWATCH".to_string(),
                "TIMER".to_string(),
                "COUNTER".to_string(),
                "WORLD_CLOCK".to_string(),
            ],
        });
        PresetManager { presets, active: 0 }
    }

    /// Adds a new preset.
    pub fn add_preset(&mut self, name: &str) {
        self.presets.push(Preset {
            name: name.to_string(),
            faces: Vec::new(),
        });
        self.active = self.presets.len() - 1;
    }

    /// Deletes the active preset (keeps at least one).
    pub fn delete_active(&mut self) {
        if self.presets.len() <= 1 {
            return;
        }
        self.presets.remove(self.active);
        if self.active >= self.presets.len() {
            self.active = self.presets.len() - 1;
        }
    }

    /// Renames the active preset.
    pub fn rename_active(&mut self, name: &str) {
        self.presets[self.active].name = name.to_string();
    }

    /// Adds a face to the active preset (if not already present).
    pub fn add_face(&mut self, face: &str) {
        let preset = &mut self.presets[self.active];
        if !preset.faces.iter().any(|f| f == face) {
            preset.faces.push(face.to_string());
        }
    }

    /// Removes a face from the active preset.
    pub fn remove_face(&mut self, index: usize) {
        let preset = &mut self.presets[self.active];
        if index < preset.faces.len() {
            preset.faces.remove(index);
        }
    }

    /// Moves a face up (toward the front) in the active preset.
    pub fn move_face_up(&mut self, index: usize) {
        let preset = &mut self.presets[self.active];
        if index > 0 && index < preset.faces.len() {
            preset.faces.swap(index, index - 1);
        }
    }

    /// Moves a face down (toward the back) in the active preset.
    pub fn move_face_down(&mut self, index: usize) {
        let preset = &mut self.presets[self.active];
        if index + 1 < preset.faces.len() {
            preset.faces.swap(index, index + 1);
        }
    }

    /// Returns the ordered face list of the active preset.
    pub fn active_faces(&self) -> VecDeque<String> {
        self.presets[self.active].faces.iter().cloned().collect()
    }
}

impl Default for PresetManager {
    fn default() -> Self {
        Self::new()
    }
}
