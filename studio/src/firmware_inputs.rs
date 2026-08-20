//! Typed, deterministic firmware-input generation for the first stock boards.
//!
//! This module is deliberately narrower than Studio's planning model. Only board
//! revisions with local pin evidence and the four stock profiles are buildable.
//! Unknown hardware is rejected instead of being guessed.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::components::{default_profiles, BoardKind, BuildProfile, ComponentsConfig, LcdVariant};
use super::modules::ModuleManager;
use super::presets::PresetManager;
use super::watch_config::WatchConfig;

pub const FIRMWARE_INPUT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareInputRequest {
    pub board: BoardKind,
    pub revision: String,
    pub profile: BuildProfile,
    pub components: ComponentsConfig,
    pub preset_name: String,
    pub ordered_faces: Vec<String>,
    pub modules: Vec<String>,
}

impl FirmwareInputRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn from_state(
        board: BoardKind,
        revision: impl Into<String>,
        presets: &PresetManager,
        profiles: &[BuildProfile],
        selected_profile: usize,
        components: &ComponentsConfig,
        modules: &ModuleManager,
    ) -> Self {
        let (preset_name, ordered_faces) = presets
            .presets
            .get(presets.active)
            .map(|preset| (preset.name.clone(), preset.faces.clone()))
            .unwrap_or_default();
        let profile = profiles
            .get(selected_profile)
            .cloned()
            .unwrap_or_else(|| BuildProfile::new("", components.clone()));
        let mut enabled_modules: Vec<_> = modules
            .modules
            .iter()
            .filter(|module| module.enabled)
            .map(|module| module.name.clone())
            .collect();
        enabled_modules.sort();
        Self {
            board,
            revision: revision.into(),
            profile,
            components: components.clone(),
            preset_name,
            ordered_faces,
            modules: enabled_modules,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildPlan {
    pub request: FirmwareInputRequest,
    pub preflight: PreflightStatus,
    pub estimate: Option<(u32, u32)>,
    pub request_identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreflightStatus {
    Valid,
    Invalid(String),
}

#[derive(Serialize)]
struct BuildPlanIdentity<'a> {
    schema_version: u32,
    request: &'a FirmwareInputRequest,
    watch_config: &'a WatchConfig,
    requested_components: &'a ComponentsConfig,
    effective_components: &'a ComponentsConfig,
    selected_profile: usize,
    output_identity: &'a str,
}

/// Resolve all UI build state into the one canonical firmware request.
///
/// This function is pure: it validates the stock/profile contract and computes a
/// planning estimate, but does not inspect the filesystem or invoke the
/// firmware toolchain. Those side effects remain in the build phase.
#[allow(clippy::too_many_arguments)]
pub fn resolve_build_plan(
    board: BoardKind,
    revision: impl Into<String>,
    profiles: &[BuildProfile],
    selected_profile: usize,
    requested_components: &ComponentsConfig,
    effective_components: &ComponentsConfig,
    preset_name: impl Into<String>,
    ordered_faces: Vec<String>,
    mut enabled_modules: Vec<String>,
    watch_config: &WatchConfig,
    output_identity: &str,
) -> BuildPlan {
    enabled_modules.sort();
    let profile = profiles
        .get(selected_profile)
        .cloned()
        .unwrap_or_else(|| BuildProfile::new("", effective_components.clone()));
    let request = FirmwareInputRequest {
        board,
        revision: revision.into(),
        profile,
        components: effective_components.clone(),
        preset_name: preset_name.into(),
        ordered_faces,
        modules: enabled_modules,
    };
    let status = board_data(request.board, &request.revision)
        .and_then(|board_data| validate_request(&request, board_data))
        .and_then(|_| {
            if request.ordered_faces.is_empty() || request.preset_name.trim().is_empty() {
                Err(FirmwareInputError::Invalid(
                    "active preset must have a name and ordered faces".into(),
                ))
            } else {
                Ok(())
            }
        });
    let preflight = match status {
        Ok(()) => PreflightStatus::Valid,
        Err(error) => PreflightStatus::Invalid(error.to_string()),
    };
    let estimate = matches!(preflight, PreflightStatus::Valid)
        .then(|| super::components::estimate(effective_components));
    let identity = BuildPlanIdentity {
        schema_version: FIRMWARE_INPUT_SCHEMA_VERSION,
        request: &request,
        watch_config,
        requested_components,
        effective_components,
        selected_profile,
        output_identity,
    };
    let request_identity = Sha256::digest(
        serde_json::to_vec(&identity).expect("build plan identity must be serializable"),
    )
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect();
    BuildPlan {
        request,
        preflight,
        estimate,
        request_identity,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedFirmwareInputs {
    pub schema_version: u32,
    pub digest: String,
    pub directory: PathBuf,
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FirmwareManifest {
    schema_version: u32,
    board: BoardIdentity,
    features: FeatureSelection,
    mappings: PinBusMappings,
    lcd: LcdInput,
    preset: PresetInput,
    provenance: Vec<Provenance>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct BoardIdentity {
    kind: String,
    revision: String,
    target: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FeatureSelection {
    profile: String,
    components: ComponentsConfig,
    modules: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PinBusMappings {
    ownership: BTreeMap<String, String>,
    i2c: Option<BusMapping>,
    spi: Option<BusMapping>,
    addresses: BTreeMap<String, u8>,
    power: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct BusMapping {
    name: String,
    pins: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct LcdInput {
    variant: String,
    source: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PresetInput {
    name: String,
    ordered_faces: Vec<FaceInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FaceInput {
    name: String,
    source: String,
    sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Provenance {
    repository: String,
    path: String,
    fact: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FirmwareInputError {
    Unsupported(String),
    Invalid(String),
    Io(String),
}

impl std::fmt::Display for FirmwareInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(message) | Self::Invalid(message) | Self::Io(message) => {
                f.write_str(message)
            }
        }
    }
}
impl std::error::Error for FirmwareInputError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedFirmwareInputs {
    faces: Vec<FaceInput>,
    pub(crate) source_contents: Vec<(String, Vec<u8>)>,
}

/// Validate the request and the source bytes without creating any output.
pub(crate) fn validate_request_and_sources(
    request: &FirmwareInputRequest,
    source_root: &Path,
) -> Result<ValidatedFirmwareInputs, FirmwareInputError> {
    let _board = validate_request_pure(request)?;
    let mut faces = Vec::with_capacity(request.ordered_faces.len());
    let mut source_contents = Vec::with_capacity(request.ordered_faces.len());
    for name in &request.ordered_faces {
        validate_safe_component(name, "face name")?;
        let source_name = source_file_name(name);
        let source = source_root.join("src/movement").join(&source_name);
        let metadata = fs::symlink_metadata(&source).map_err(|_| {
            FirmwareInputError::Io(format!(
                "missing face source for {name}: {}",
                source.display()
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(FirmwareInputError::Invalid(format!(
                "unsafe or missing face source for {name}"
            )));
        }
        let contents = fs::read(&source).map_err(|e| {
            FirmwareInputError::Io(format!("cannot read face source {}: {e}", source.display()))
        })?;
        faces.push(FaceInput {
            name: name.clone(),
            source: format!("src/movement/{source_name}"),
            sha256: hex_digest(&contents),
        });
        source_contents.push((source_name, contents));
    }
    Ok(ValidatedFirmwareInputs {
        faces,
        source_contents,
    })
}

/// Write an isolated, content-addressed input set from a validated source snapshot.
pub(crate) fn generate(
    request: &FirmwareInputRequest,
    validated: &ValidatedFirmwareInputs,
    output_dir: impl AsRef<Path>,
) -> Result<GeneratedFirmwareInputs, FirmwareInputError> {
    let board = validate_request_pure(request)?;
    let faces = &validated.faces;

    let manifest = FirmwareManifest {
        schema_version: FIRMWARE_INPUT_SCHEMA_VERSION,
        board: BoardIdentity {
            kind: request.board.label().into(),
            revision: request.revision.clone(),
            target: board.target.into(),
        },
        features: FeatureSelection {
            profile: request.profile.name.clone(),
            components: request.components.clone(),
            modules: request.modules.clone(),
        },
        mappings: board.mappings(request.components.i2c, request.components.spi),
        lcd: LcdInput {
            variant: "standard_f91w_sensor_watch".into(),
            source: format!("sensor-watch-reference/{}", board.reference),
        },
        preset: PresetInput {
            name: request.preset_name.clone(),
            ordered_faces: faces.clone(),
        },
        provenance: board.provenance(),
    };
    let manifest_json = canonical_json(&manifest);
    let rust = format!(
        "// Generated by Studio; compiled by the isolated firmware build.\npub const FIRMWARE_INPUT_SCHEMA_VERSION: u32 = {FIRMWARE_INPUT_SCHEMA_VERSION};\npub const GENERATED_MARKER: &str = \"sensor-watch-studio-generated-inputs\";\npub const BOARD: &str = {:?};\npub const REVISION: &str = {:?};\npub const CONFIGURATION_JSON: &str = {:?};\n",
        request.board.label(),
        request.revision,
        manifest_json
    );
    let cargo = "# Generated metadata; build.rs layers this through the existing Cargo config.\n"
        .to_string();
    let provenance_json = canonical_json(&manifest.provenance);
    let mut files = BTreeMap::new();
    files.insert("firmware_inputs.json".into(), manifest_json);
    files.insert("firmware_inputs.rs".into(), rust);
    files.insert("Cargo.config.toml".into(), cargo);
    files.insert("PROVENANCE.json".into(), provenance_json);
    let directory = output_dir.as_ref().to_path_buf();
    fs::create_dir_all(&directory).map_err(|e| FirmwareInputError::Io(e.to_string()))?;
    for (name, contents) in &files {
        fs::write(directory.join(name), contents)
            .map_err(|e| FirmwareInputError::Io(e.to_string()))?;
    }
    // Hash the bytes that are actually on disk, after every generated input has
    // been finalized. SHA256 is a sidecar and is intentionally excluded from
    // its own input set to avoid a self-referential digest.
    let final_files = read_generated_files(&directory, files.keys())?;
    let digest = digest_generated_files(&final_files);
    let sha = format!("{digest}  firmware_inputs.json\n");
    fs::write(directory.join("SHA256"), &sha).map_err(|e| FirmwareInputError::Io(e.to_string()))?;
    files.insert("SHA256".into(), sha);
    Ok(GeneratedFirmwareInputs {
        schema_version: FIRMWARE_INPUT_SCHEMA_VERSION,
        digest,
        directory,
        files,
    })
}

fn validate_request_pure(
    request: &FirmwareInputRequest,
) -> Result<&'static BoardData, FirmwareInputError> {
    let board = board_data(request.board, &request.revision)?;
    validate_request(request, board)?;
    validate_preset(request)?;
    Ok(board)
}

fn validate_preset(request: &FirmwareInputRequest) -> Result<(), FirmwareInputError> {
    if request.preset_name.trim().is_empty() || request.ordered_faces.is_empty() {
        return Err(FirmwareInputError::Invalid(
            "active preset must have a name and ordered faces".into(),
        ));
    }
    Ok(())
}

fn validate_request(
    request: &FirmwareInputRequest,
    board: &BoardData,
) -> Result<(), FirmwareInputError> {
    if request.profile.name != request.board.label() || request.profile.name == "Custom" {
        return Err(FirmwareInputError::Unsupported(
            "only the matching stock Green, Red / Lite, Blue, and Pro profiles are buildable"
                .into(),
        ));
    }
    if request.profile.config != request.components {
        return Err(FirmwareInputError::Invalid(
            "profile and component configuration differ".into(),
        ));
    }
    let stock_index = BoardKind::ALL
        .iter()
        .position(|candidate| *candidate == request.board)
        .expect("BoardKind::ALL contains every board");
    if request.components != default_profiles()[stock_index].config {
        return Err(FirmwareInputError::Unsupported(
            "edited profiles and optional hardware are outside the stock production set".into(),
        ));
    }
    request
        .profile
        .validate()
        .map_err(FirmwareInputError::Invalid)?;
    if request.components.lcd != LcdVariant::Standard {
        return Err(FirmwareInputError::Unsupported(
            "OSO and Custom LCD variants are not in the first production set".into(),
        ));
    }
    if request.board == BoardKind::RedLite && (request.components.i2c || request.components.spi) {
        return Err(FirmwareInputError::Unsupported(
            "Lite I2C/SPI is not exposed by the supported revision".into(),
        ));
    }
    if request.components.thermistor && !board.thermistor {
        return Err(FirmwareInputError::Unsupported(
            "thermistor is unavailable for this board revision".into(),
        ));
    }
    for module in &request.modules {
        validate_safe_component(module, "module name")?;
    }
    Ok(())
}

fn validate_safe_component(value: &str, what: &str) -> Result<(), FirmwareInputError> {
    if value.trim().is_empty()
        || value != value.trim()
        || value.len() > 128
        || value.contains(['/', '\\', ':'])
        || Path::new(value).components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(FirmwareInputError::Invalid(format!(
            "unsafe {what}: {value:?}"
        )));
    }
    Ok(())
}

fn canonical_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("firmware input schema is serializable")
}
fn source_file_name(name: &str) -> String {
    format!("{}.rs", name.to_ascii_lowercase())
}

fn hex_digest(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_generated_files<'a>(
    directory: &Path,
    names: impl Iterator<Item = &'a String>,
) -> Result<BTreeMap<String, String>, FirmwareInputError> {
    names
        .filter(|name| name.as_str() != "SHA256")
        .map(|name| {
            fs::read_to_string(directory.join(name))
                .map(|contents| (name.clone(), contents))
                .map_err(|e| {
                    FirmwareInputError::Io(format!("cannot read generated input {}: {e}", name))
                })
        })
        .collect()
}

pub(crate) fn digest_generated_files(files: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (name, contents) in files {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(contents.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct BoardData {
    target: &'static str,
    thermistor: bool,
    reference: &'static str,
    fact: &'static str,
}

const SWAT_DATA: BoardData = BoardData {
    target: "thumbv6m-none-eabi",
    thermistor: false,
    reference: "boards/OSO-SWAT-A1-05/pins.h",
    fact: "buttons, buzzer, LEDs, LCD, and nine-pin bus ownership",
};
const LITE_DATA: BoardData = BoardData {
    target: "thumbv6m-none-eabi",
    thermistor: true,
    reference: "boards/OSO-SWAT-A1-02/pins.h",
    fact: "Lite LCD, buttons, LEDs, and documented thermistor",
};
const PRO_DATA: BoardData = BoardData {
    target: "thumbv6m-none-eabi",
    thermistor: true,
    reference: "boards/OSO-FEAL-A1-00/pins.h",
    fact: "Pro LCD, buttons, LEDs, thermistor, and nine-pin ownership",
};
impl BoardData {
    fn mappings(&self, i2c: bool, spi: bool) -> PinBusMappings {
        let mut ownership = BTreeMap::from([
            (String::from("buttons"), String::from("PA02/PB05/PA07")),
            (String::from("lcd"), String::from("SLCD0..SLCD26")),
            (String::from("buzzer"), String::from("PA27")),
            (String::from("led"), String::from("PA20/PA21")),
        ]);
        if self.thermistor {
            ownership.insert("thermistor".into(), "ADC:documented revision input".into());
        }
        PinBusMappings {
            ownership,
            i2c: i2c.then(|| BusMapping {
                name: "I2C0".into(),
                pins: vec!["PB30".into(), "PB31".into()],
            }),
            spi: spi.then(|| BusMapping {
                name: "SPI0".into(),
                pins: vec!["revision-dependent".into()],
            }),
            addresses: BTreeMap::new(),
            power: BTreeMap::from([
                (String::from("lcd"), String::from("VBAT")),
                (String::from("thermistor"), String::from("VBAT")),
            ]),
        }
    }
    fn provenance(&self) -> Vec<Provenance> {
        vec![Provenance {
            repository: "sensor-watch-reference".into(),
            path: self.reference.into(),
            fact: self.fact.into(),
        }]
    }
}
fn board_data(board: BoardKind, revision: &str) -> Result<&'static BoardData, FirmwareInputError> {
    let data = match board {
        BoardKind::Green | BoardKind::Blue => &SWAT_DATA,
        BoardKind::RedLite => &LITE_DATA,
        BoardKind::Pro => &PRO_DATA,
    };
    let expected = match board {
        BoardKind::Green | BoardKind::Blue => "OSO-SWAT-A1-05",
        BoardKind::RedLite => "OSO-SWAT-A1-02",
        BoardKind::Pro => "OSO-FEAL-A1-00",
    };
    if revision != expected {
        return Err(FirmwareInputError::Unsupported(format!(
            "unsupported {} revision {revision:?}",
            board.label()
        )));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::default_profiles;
    use crate::watch_config::WatchConfig;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn plan(board: BoardKind) -> BuildPlan {
        let profiles = default_profiles();
        let index = BoardKind::ALL
            .iter()
            .position(|candidate| *candidate == board)
            .unwrap();
        let request = request(board);
        resolve_build_plan(
            board,
            request.revision,
            &profiles,
            index,
            &profiles[index].config,
            &profiles[index].config,
            request.preset_name,
            request.ordered_faces,
            request.modules,
            &WatchConfig::default(),
            "output",
        )
    }

    #[test]
    fn resolver_accepts_all_stock_requests_with_same_plan_for_build_and_estimator() {
        for board in BoardKind::ALL {
            let first = plan(board);
            let second = plan(board);
            assert_eq!(first.request, second.request);
            assert_eq!(first.preflight, PreflightStatus::Valid);
            assert_eq!(first.preflight, second.preflight);
            assert_eq!(
                first.estimate,
                Some(super::super::components::estimate(
                    &first.request.components
                ))
            );
            assert_eq!(first.request_identity, second.request_identity);
        }
    }

    #[test]
    fn resolver_rejects_invalid_components_and_profile_mismatch() {
        let profiles = default_profiles();
        let mut invalid = profiles[0].config.clone();
        invalid.buzzer = false;
        let invalid_plan = resolve_build_plan(
            BoardKind::Green,
            "OSO-SWAT-A1-05",
            &profiles,
            0,
            &invalid,
            &invalid,
            "Stock Casio",
            vec!["SIMPLE_CLOCK".into()],
            vec![],
            &WatchConfig::default(),
            "output",
        );
        assert!(matches!(
            invalid_plan.preflight,
            PreflightStatus::Invalid(_)
        ));

        let mismatch = resolve_build_plan(
            BoardKind::RedLite,
            "OSO-SWAT-A1-02",
            &profiles,
            0,
            &profiles[0].config,
            &profiles[0].config,
            "Stock Casio",
            vec!["SIMPLE_CLOCK".into()],
            vec![],
            &WatchConfig::default(),
            "output",
        );
        assert!(matches!(mismatch.preflight, PreflightStatus::Invalid(_)));
        assert!(mismatch.estimate.is_none());
    }

    #[test]
    fn resolver_identity_tracks_face_order_and_modules() {
        let profiles = default_profiles();
        let faces = vec!["SIMPLE_CLOCK".into(), "ALARM".into()];
        let left = resolve_build_plan(
            BoardKind::Green,
            "OSO-SWAT-A1-05",
            &profiles,
            0,
            &profiles[0].config,
            &profiles[0].config,
            "Stock Casio",
            faces.clone(),
            vec!["zeta".into(), "alpha".into()],
            &WatchConfig::default(),
            "output",
        );
        let reordered_faces = resolve_build_plan(
            BoardKind::Green,
            "OSO-SWAT-A1-05",
            &profiles,
            0,
            &profiles[0].config,
            &profiles[0].config,
            "Stock Casio",
            vec![faces[1].clone(), faces[0].clone()],
            vec!["alpha".into(), "zeta".into()],
            &WatchConfig::default(),
            "output",
        );
        let changed_modules = resolve_build_plan(
            BoardKind::Green,
            "OSO-SWAT-A1-05",
            &profiles,
            0,
            &profiles[0].config,
            &profiles[0].config,
            "Stock Casio",
            faces,
            vec!["alpha".into(), "new".into()],
            &WatchConfig::default(),
            "output",
        );
        assert_eq!(left.request.modules, vec!["alpha", "zeta"]);
        assert_ne!(left.request_identity, reordered_faces.request_identity);
        assert_ne!(left.request_identity, changed_modules.request_identity);
        assert_eq!(left.preflight, reordered_faces.preflight);
    }

    #[test]
    fn resolver_rejects_unsupported_revision_without_estimate() {
        let profiles = default_profiles();
        let result = resolve_build_plan(
            BoardKind::Green,
            "unknown",
            &profiles,
            0,
            &profiles[0].config,
            &profiles[0].config,
            "Stock Casio",
            vec!["SIMPLE_CLOCK".into()],
            vec![],
            &WatchConfig::default(),
            "output",
        );
        assert!(
            matches!(result.preflight, PreflightStatus::Invalid(reason) if reason.contains("unsupported"))
        );
        assert!(result.estimate.is_none());
    }

    fn request(board: BoardKind) -> FirmwareInputRequest {
        let profiles = default_profiles();
        let index = BoardKind::ALL
            .iter()
            .position(|candidate| *candidate == board)
            .unwrap();
        FirmwareInputRequest {
            board,
            revision: match board {
                BoardKind::Green | BoardKind::Blue => "OSO-SWAT-A1-05",
                BoardKind::RedLite => "OSO-SWAT-A1-02",
                BoardKind::Pro => "OSO-FEAL-A1-00",
            }
            .into(),
            profile: profiles[index].clone(),
            components: profiles[index].config.clone(),
            preset_name: "Stock Casio".into(),
            ordered_faces: vec!["SIMPLE_CLOCK".into(), "ALARM".into()],
            modules: vec![],
        }
    }
    fn output() -> PathBuf {
        std::env::temp_dir().join(format!(
            "sensor-watch-studio-inputs-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn pure_validation_has_no_generated_output_side_effects() {
        let request = request(BoardKind::Green);
        let source_root = super::super::build::firmware_dir();
        let output = output();
        assert!(!output.exists());
        validate_request_and_sources(&request, &source_root).unwrap();
        assert!(!output.exists());
    }

    #[test]
    fn source_changes_between_preflight_and_worker_validation_are_detected() {
        let source_root = output();
        let movement = source_root.join("src/movement");
        std::fs::create_dir_all(&movement).unwrap();
        std::fs::write(movement.join("simple_clock.rs"), "original").unwrap();
        std::fs::write(movement.join("alarm.rs"), "alarm").unwrap();
        let request = request(BoardKind::Green);
        let preflight = validate_request_and_sources(&request, &source_root).unwrap();
        std::fs::write(movement.join("simple_clock.rs"), "changed").unwrap();
        let worker = validate_request_and_sources(&request, &source_root).unwrap();
        assert_ne!(preflight.faces[0].sha256, worker.faces[0].sha256);
        assert_ne!(preflight.faces[0].sha256, hex_digest(b"changed"));
        assert_eq!(worker.faces[0].sha256, hex_digest(b"changed"));
        std::fs::remove_dir_all(source_root).unwrap();
    }

    #[test]
    fn each_stock_board_generates() {
        for board in BoardKind::ALL {
            let request = request(board);
            let validated =
                validate_request_and_sources(&request, &super::super::build::firmware_dir())
                    .unwrap();
            let result = generate(&request, &validated, output()).unwrap();
            assert_eq!(result.schema_version, 1);
            assert!(result.files["firmware_inputs.json"].contains("ordered_faces"));
        }
    }
    #[test]
    fn lite_and_pro_thermistors_are_distinct() {
        for board in [BoardKind::RedLite, BoardKind::Pro] {
            let request = request(board);
            let validated =
                validate_request_and_sources(&request, &super::super::build::firmware_dir())
                    .unwrap();
            assert!(generate(&request, &validated, output()).is_ok());
        }
    }
    #[test]
    fn unsupported_combinations_fail_closed() {
        let mut req = request(BoardKind::Green);
        req.profile.name = "Custom".into();
        assert!(matches!(
            validate_request_and_sources(&req, &super::super::build::firmware_dir()),
            Err(FirmwareInputError::Unsupported(_))
        ));
        req = request(BoardKind::RedLite);
        req.components.i2c = true;
        req.profile.config.i2c = true;
        assert!(validate_request_and_sources(&req, &super::super::build::firmware_dir()).is_err());
    }
    #[test]
    fn worker_generation_uses_the_validated_source_snapshot() {
        let request = request(BoardKind::Green);
        let source_root = super::super::build::firmware_dir();
        let validated = validate_request_and_sources(&request, &source_root).unwrap();
        let generated = generate(&request, &validated, output()).unwrap();
        let manifest = &generated.files["firmware_inputs.json"];
        assert!(manifest.contains(&validated.faces[0].sha256));
    }

    #[test]
    fn output_is_deterministic_and_digest_sensitive() {
        let req = request(BoardKind::Green);
        let source_root = super::super::build::firmware_dir();
        let left_validated = validate_request_and_sources(&req, &source_root).unwrap();
        let right_validated = validate_request_and_sources(&req, &source_root).unwrap();
        let left = generate(&req, &left_validated, output()).unwrap();
        let right = generate(&req, &right_validated, output()).unwrap();
        assert_eq!(left.digest, right.digest);
        let mut changed = req;
        changed.ordered_faces.reverse();
        let changed_validated = validate_request_and_sources(&changed, &source_root).unwrap();
        assert_ne!(
            left.digest,
            generate(&changed, &changed_validated, output())
                .unwrap()
                .digest
        );
    }
    #[test]
    fn unsafe_face_names_fail() {
        let mut req = request(BoardKind::Green);
        req.ordered_faces[0] = "../escape".into();
        assert!(matches!(
            validate_request_and_sources(&req, &super::super::build::firmware_dir()),
            Err(FirmwareInputError::Invalid(_))
        ));
    }
    #[test]
    fn generated_provenance_is_complete() {
        let request = request(BoardKind::Pro);
        let validated =
            validate_request_and_sources(&request, &super::super::build::firmware_dir()).unwrap();
        let result = generate(&request, &validated, output()).unwrap();
        let provenance = &result.files["PROVENANCE.json"];
        assert!(provenance.contains("sensor-watch-reference"));
        assert!(provenance.contains("OSO-FEAL-A1-00"));
    }
}
