//! Deterministic, fail-closed representation of Studio build inputs.
//!
//! This records what Studio currently knows about a requested build. It is not
//! a firmware input generator: the snapshot deliberately carries an incomplete
//! contract until concrete board mappings and build provenance exist.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::components::{BuildProfile, ComponentsConfig};
use super::modules::ModuleManager;
use super::presets::PresetManager;
use super::watch_config::WatchConfig;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildInputSnapshot {
    pub schema_version: u32,
    pub board: String,
    pub active_preset: ActivePreset,
    pub watch_config: WatchConfig,
    pub modules: Vec<SnapshotModule>,
    pub selected_profile: SelectedProfile,
    pub component_draft: ComponentsConfig,
    /// Effective component configuration after board/profile compatibility resolution.
    /// This identifies planning state; it is not firmware provenance.
    pub component_effective: ComponentsConfig,
    pub output_identity: String,
    pub completeness: SnapshotCompleteness,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivePreset {
    pub name: String,
    /// Face order is semantic and must not be sorted.
    pub ordered_faces: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotModule {
    pub name: String,
    pub target: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SelectedProfile {
    pub index: usize,
    pub profile: Option<BuildProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotCompleteness {
    /// False until Studio has a validated generated-input/provenance record.
    pub build_inputs_complete: bool,
    pub missing: Vec<MissingBuildInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingBuildInput {
    BoardMappings,
    FirmwareSelections,
    GeneratedInputProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotValidationError {
    pub missing: Vec<MissingBuildInput>,
}

impl std::fmt::Display for SnapshotValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "build input snapshot is incomplete: {:?}", self.missing)
    }
}

impl std::error::Error for SnapshotValidationError {}

impl BuildInputSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn from_state(
        board: impl Into<String>,
        presets: &PresetManager,
        watch_config: &WatchConfig,
        modules: &ModuleManager,
        profiles: &[BuildProfile],
        selected_profile: usize,
        component_draft: &ComponentsConfig,
        component_effective: &ComponentsConfig,
        output_identity: impl Into<String>,
    ) -> Self {
        let active = presets
            .presets
            .get(presets.active)
            .map(|preset| ActivePreset {
                name: preset.name.clone(),
                ordered_faces: preset.faces.clone(),
            });
        let active_preset = active.unwrap_or_else(|| ActivePreset {
            name: String::new(),
            ordered_faces: Vec::new(),
        });

        // Module order is presentation-only. Sorting the copied records makes
        // equivalent configurations serialize identically without changing the
        // manager's user-visible order.
        let mut snapshot_modules: Vec<_> = modules
            .modules
            .iter()
            .map(|module| SnapshotModule {
                name: module.name.clone(),
                target: module.target.clone(),
                description: module.description.clone(),
                enabled: module.enabled,
            })
            .collect();
        snapshot_modules.sort_by(|a, b| {
            (&a.name, &a.target, &a.description, a.enabled).cmp(&(
                &b.name,
                &b.target,
                &b.description,
                b.enabled,
            ))
        });

        Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            board: board.into(),
            active_preset,
            watch_config: watch_config.clone(),
            modules: snapshot_modules,
            selected_profile: SelectedProfile {
                index: selected_profile,
                profile: profiles.get(selected_profile).cloned(),
            },
            component_draft: component_draft.clone(),
            component_effective: component_effective.clone(),
            output_identity: output_identity.into(),
            completeness: SnapshotCompleteness {
                build_inputs_complete: false,
                missing: vec![
                    MissingBuildInput::BoardMappings,
                    MissingBuildInput::FirmwareSelections,
                    MissingBuildInput::GeneratedInputProvenance,
                ],
            },
        }
    }

    /// Compact JSON with fixed struct field order and canonicalized module order.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("BuildInputSnapshot must be serializable")
    }

    pub fn canonical_json(&self) -> String {
        String::from_utf8(self.canonical_bytes()).expect("serde_json emits UTF-8")
    }

    /// SHA-256 of the canonical snapshot bytes, represented as lowercase hex.
    pub fn digest(&self) -> String {
        Sha256::digest(self.canonical_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Alias for callers that use configuration-fingerprint terminology.
    pub fn fingerprint(&self) -> String {
        self.digest()
    }

    /// A snapshot never authorizes a configured firmware build by itself.
    pub fn validate_for_build(&self) -> Result<(), SnapshotValidationError> {
        let mut missing = self.completeness.missing.clone();
        if self.board.trim().is_empty() {
            missing.push(MissingBuildInput::BoardMappings);
        }
        if self.active_preset.name.trim().is_empty() || self.active_preset.ordered_faces.is_empty()
        {
            missing.push(MissingBuildInput::FirmwareSelections);
        }
        if self.selected_profile.profile.is_none() {
            missing.push(MissingBuildInput::FirmwareSelections);
        }
        if self.output_identity.trim().is_empty() {
            missing.push(MissingBuildInput::GeneratedInputProvenance);
        }
        missing.sort_by_key(|item| format!("{item:?}"));
        missing.dedup();
        if self.completeness.build_inputs_complete && missing.is_empty() {
            Ok(())
        } else {
            Err(SnapshotValidationError { missing })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::ComponentsConfig;
    use crate::modules::Module;

    fn snapshot() -> BuildInputSnapshot {
        let presets = PresetManager::new();
        let profiles = vec![BuildProfile::new("Green", ComponentsConfig::default())];
        BuildInputSnapshot::from_state(
            "Green",
            &presets,
            &WatchConfig::default(),
            &ModuleManager::default(),
            &profiles,
            0,
            &ComponentsConfig::default(),
            &ComponentsConfig::default(),
            "output",
        )
    }

    #[test]
    fn equal_state_has_equal_canonical_bytes_and_digest() {
        let left = snapshot();
        let right = snapshot();
        assert_eq!(left.canonical_bytes(), right.canonical_bytes());
        assert_eq!(left.digest(), right.fingerprint());
    }

    #[test]
    fn meaningful_changes_change_digest() {
        let mut changed = snapshot();
        changed.watch_config.show_seconds = !changed.watch_config.show_seconds;
        assert_ne!(snapshot().digest(), changed.digest());
    }

    #[test]
    fn face_order_is_preserved_and_digest_sensitive() {
        let original = snapshot();
        let mut reordered = original.clone();
        reordered.active_preset.ordered_faces.swap(0, 1);
        assert_ne!(original.canonical_json(), reordered.canonical_json());
        assert_ne!(original.digest(), reordered.digest());
        assert_eq!(original.active_preset.ordered_faces[0], "SIMPLE_CLOCK");
        let mut modules = ModuleManager::default();
        modules.add(Module {
            name: "zeta".into(),
            target: "z.rs".into(),
            description: "Z".into(),
            enabled: true,
        });
        modules.add(Module {
            name: "alpha".into(),
            target: "a.rs".into(),
            description: "A".into(),
            enabled: false,
        });
        let mut reordered = modules.clone();
        reordered.modules.reverse();
        let left = BuildInputSnapshot::from_state(
            "Green",
            &PresetManager::new(),
            &WatchConfig::default(),
            &modules,
            &[],
            0,
            &ComponentsConfig::default(),
            &ComponentsConfig::default(),
            "output",
        );
        let right = BuildInputSnapshot::from_state(
            "Green",
            &PresetManager::new(),
            &WatchConfig::default(),
            &reordered,
            &[],
            0,
            &ComponentsConfig::default(),
            &ComponentsConfig::default(),
            "output",
        );
        assert_eq!(left.digest(), right.digest());
    }

    #[test]
    fn incomplete_contract_is_rejected() {
        let mut value = snapshot();
        assert!(value.validate_for_build().is_err());
        value.completeness.build_inputs_complete = true;
        value.completeness.missing.clear();
        value.selected_profile.profile = None;
        assert!(value.validate_for_build().is_err());
    }
}
