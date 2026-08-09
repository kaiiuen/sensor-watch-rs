//! Watch-face presets.
//!
//! A preset is a named, ordered selection of watch faces. The default preset is
//! the stock Casio F-91W face set. Users can create, edit, delete, and
//! reorder presets, and the simulator renders the selected preset's faces.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// A single watch-face preset.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    /// The preset name.
    pub name: String,
    /// The ordered list of face names in this preset.
    pub faces: Vec<String>,
}

/// The preset manager.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresetManager {
    /// All presets.
    pub presets: Vec<Preset>,
    /// The index of the active preset.
    pub active: usize,
}

impl PresetManager {
    /// Creates the manager with the default stock preset.
    pub fn new() -> Self {
        // The default preset: the stock Casio F-91W face set.
        let presets = vec![Preset {
            name: "Stock Casio".to_string(),
            faces: vec![
                "SIMPLE_CLOCK".to_string(),
                "ALARM".to_string(),
                "STOPWATCH".to_string(),
                "TIMER".to_string(),
                "COUNTER".to_string(),
                "WORLD_CLOCK".to_string(),
            ],
        }];
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
        if let Some(p) = self.presets.get_mut(self.active) {
            p.name = name.to_string();
        }
    }

    /// Adds a face to the active preset (if not already present).
    pub fn add_face(&mut self, face: &str) {
        if let Some(preset) = self.presets.get_mut(self.active) {
            if !preset.faces.iter().any(|f| f == face) {
                preset.faces.push(face.to_string());
            }
        }
    }

    /// Removes a face from the active preset.
    pub fn remove_face(&mut self, index: usize) {
        if let Some(preset) = self.presets.get_mut(self.active) {
            if index < preset.faces.len() {
                preset.faces.remove(index);
            }
        }
    }

    /// Moves a face up (toward the front) in the active preset.
    pub fn move_face_up(&mut self, index: usize) {
        if let Some(preset) = self.presets.get_mut(self.active) {
            if index > 0 && index < preset.faces.len() {
                preset.faces.swap(index, index - 1);
            }
        }
    }

    /// Moves a face down (toward the back) in the active preset.
    pub fn move_face_down(&mut self, index: usize) {
        if let Some(preset) = self.presets.get_mut(self.active) {
            if index + 1 < preset.faces.len() {
                preset.faces.swap(index, index + 1);
            }
        }
    }

    /// Moves a face from one index to another in the active preset.
    pub fn move_face(&mut self, from: usize, to: usize) {
        if let Some(preset) = self.presets.get_mut(self.active) {
            if from < preset.faces.len() && to < preset.faces.len() && from != to {
                let face = preset.faces.remove(from);
                preset.faces.insert(to, face);
            }
        }
    }

    /// Returns the ordered face list of the active preset.
    ///
    /// Bounds-safe: if the active index is out of range (e.g. from a crafted or
    /// older settings file), it falls back to the first preset / an empty list
    /// instead of panicking.
    pub fn active_faces(&self) -> VecDeque<String> {
        self.presets
            .get(self.active.min(self.presets.len().saturating_sub(1)))
            .map(|p| p.faces.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Ensures the active index is in range, adjusting it if needed. Safe to
    /// call after deserializing settings.
    pub fn clamp_active(&mut self) {
        if self.presets.is_empty() {
            let mut default = Self::new();
            // Take the stock preset so the app always has at least one.
            self.presets = std::mem::take(&mut default.presets);
            self.active = 0;
        } else if self.active >= self.presets.len() {
            self.active = self.presets.len() - 1;
        }
    }
}

impl Default for PresetManager {
    fn default() -> Self {
        Self::new()
    }
}
