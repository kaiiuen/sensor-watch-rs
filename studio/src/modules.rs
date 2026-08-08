//! Custom hardware module management.
//!
//! The Sensor-Watch firmware HAL is modular: one module per peripheral in
//! `src/watch/`. A user with a modded board (e.g. a BLE board instead of the
//! accelerometer) can register a custom module here. Each module records which
//! HAL file it replaces/augments and a short description, so the app can show
//! what is installed and what it does.

use serde::{Deserialize, Serialize};

/// A registered custom hardware module.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Module {
    /// The module's display name.
    pub name: String,
    /// The HAL file in `src/watch/` it targets (e.g. "lis2dw.rs").
    pub target: String,
    /// A short human-readable description.
    pub description: String,
    /// Whether the module is enabled.
    pub enabled: bool,
}

/// Manages the list of custom modules.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModuleManager {
    /// The registered modules.
    pub modules: Vec<Module>,
}

impl ModuleManager {
    /// Adds a module, replacing any existing module with the same name.
    pub fn add(&mut self, module: Module) {
        self.modules.retain(|m| m.name != module.name);
        self.modules.push(module);
    }

    /// Removes a module by name.
    pub fn remove(&mut self, name: &str) {
        self.modules.retain(|m| m.name != name);
    }

    /// Toggles a module's enabled state by name.
    pub fn toggle(&mut self, name: &str) {
        if let Some(m) = self.modules.iter_mut().find(|m| m.name == name) {
            m.enabled = !m.enabled;
        }
    }
}
