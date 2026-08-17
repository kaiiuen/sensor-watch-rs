//! Watch-face presets.
//!
//! A preset is a named, ordered selection of watch faces. The default preset is
//! the stock Casio F-91W face set. Users can create, edit, delete, and
//! reorder presets, and the simulator renders the selected preset's faces.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::faces::face_identity;

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
            if !preset
                .faces
                .iter()
                .any(|candidate| face_identity(candidate) == face_identity(face))
            {
                preset.faces.push(face.to_string());
            }
        }
    }

    /// Removes case-only duplicate references, preserving the first spelling and
    /// order. Unknown/user references are intentionally retained.
    pub fn migrate_face_duplicates(&mut self) {
        for preset in &mut self.presets {
            let mut seen = std::collections::HashSet::new();
            preset.faces.retain(|face| seen.insert(face_identity(face)));
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

    /// Removes every occurrence of a face from every preset.
    pub fn remove_face_from_all(&mut self, face: &str) -> usize {
        let mut removed = 0;
        for preset in &mut self.presets {
            let before = preset.faces.len();
            let identity = face_identity(face);
            preset
                .faces
                .retain(|candidate| face_identity(candidate) != identity);
            removed += before - preset.faces.len();
        }
        removed
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

#[cfg(test)]
mod tests {
    use super::PresetManager;

    #[test]
    fn removes_deleted_face_from_every_preset() {
        let mut manager = PresetManager::new();
        manager.add_face("CUSTOM");
        manager.add_preset("Other");
        manager.add_face("CUSTOM");
        manager.add_face("KEEP");

        assert_eq!(manager.remove_face_from_all("CUSTOM"), 2);
        assert!(manager
            .presets
            .iter()
            .all(|preset| { !preset.faces.iter().any(|face| face == "CUSTOM") }));
        assert_eq!(manager.presets[1].faces, vec!["KEEP"]);
    }

    #[test]
    fn add_and_migrate_deduplicate_case_only_variants() {
        let mut manager = PresetManager::new();
        manager.presets[0].faces = vec![
            "SIMPLE_CLOCK".into(),
            "simple_clock".into(),
            "USER_FACE".into(),
            "User_Face".into(),
            "WORLD_CLOCK2".into(),
            "WORLD_CLOCK".into(),
        ];
        manager.migrate_face_duplicates();
        manager.migrate_face_duplicates();
        assert_eq!(
            manager.presets[0].faces,
            vec!["SIMPLE_CLOCK", "USER_FACE", "WORLD_CLOCK2", "WORLD_CLOCK"]
        );
        manager.add_face("Simple_Clock");
        manager.add_face("stock_stopwatch");
        assert_eq!(manager.presets[0].faces.last().unwrap(), "stock_stopwatch");
    }

    #[test]
    fn remove_is_case_insensitive_but_keeps_distinct_identities() {
        let mut manager = PresetManager::new();
        manager.presets[0].faces = vec![
            "STOPWATCH".into(),
            "STOCK_STOPWATCH".into(),
            "WORLD_CLOCK2".into(),
        ];
        assert_eq!(manager.remove_face_from_all("stopwatch"), 1);
        assert_eq!(
            manager.presets[0].faces,
            vec!["STOCK_STOPWATCH", "WORLD_CLOCK2"]
        );
    }
}
