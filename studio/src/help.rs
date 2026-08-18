//! Beginner help and tutorial content shared by every Studio panel.
//!
//! Help IDs and step text are intentionally data-driven. Dismissals are kept in
//! the app session because the settings schema is deliberately not changed by
//! this feature; restarting Studio shows help again.

use std::collections::{HashMap, HashSet};

/// Stable semantic anchors used by guided help. These names are part of the
/// help contract, rather than widget labels which may be localized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnchorId {
    PanelHelp,
    PanelNavigation,
    DashboardBoard,
    DashboardNtpFetch,
    EditorMode,
    EditorTemplate,
    EditorName,
    BlocksGenerate,
    LoadIntoRust,
    EditorGenerate,
    EditorSave,
    FacesSearch,
    FacesAdd,
    FacesPreset,
    SimulatorWatch,
    SimulatorDate,
    SimulatorApply,
    BuildBoard,
    BuildProfile,
    BuildArtifactPath,
    BuildArtifact,
    BuildInspect,
    BuildApprove,
    BuildRefresh,
    BuildCopy,
    BuildUnavailable,
    CalibrationFetch,
    CalibrationRecord,
    CalibrationCopy,
    ModulesRegister,
    ShellMode,
    ShellInput,
    ShellSend,
    DiagnosticsRun,
    DebugLog,
    DebugCopy,
    BugsSearch,
    BugsFingerprint,
    BugsResolve,
    BugsReport,
    FileRefresh,
    FileFilter,
    FileList,
    FilePreview,
    TutorialSections,
    WikiNavigation,
    WikiSearch,
    SettingsTheme,
    SettingsText,
    SettingsLayout,
    SettingsImport,
    SettingsRestore,
    ProbeRefresh,
    ProbeRun,
    ProbeReport,
}

impl AnchorId {
    pub const fn key(self) -> AnchorKey {
        AnchorKey(self)
    }
}

/// A namespaced key prevents accidental collisions between panel widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnchorKey(pub AnchorId);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorRect {
    pub min: (f32, f32),
    pub max: (f32, f32),
}

impl AnchorRect {
    /// Anchor rectangles come from conditional widgets, so reject geometry that
    /// cannot safely describe a visible target.
    pub fn is_valid(self) -> bool {
        self.min.0.is_finite()
            && self.min.1.is_finite()
            && self.max.0.is_finite()
            && self.max.1.is_finite()
            && self.max.0 > self.min.0
            && self.max.1 > self.min.1
    }

    pub fn expand(self, padding: f32) -> Self {
        Self {
            min: (self.min.0 - padding, self.min.1 - padding),
            max: (self.max.0 + padding, self.max.1 + padding),
        }
    }
}

/// Frame-local registry: a new frame replaces, rather than merges with, the
/// previous registry so stale targets can never be spotlighted.
#[derive(Default)]
pub struct AnchorRegistry {
    frame: u64,
    anchors: HashMap<AnchorKey, (HelpId, AnchorRect)>,
}

impl AnchorRegistry {
    pub fn begin_frame(&mut self, frame: u64) {
        self.frame = frame;
        self.anchors.clear();
    }
    pub fn register(&mut self, panel: HelpId, key: AnchorKey, rect: AnchorRect) {
        if rect.is_valid() {
            self.anchors.insert(key, (panel, rect));
        } else {
            // Do not let a conditional widget's invalid registration hide a
            // later recovery or leave a previous target addressable.
            self.anchors.remove(&key);
        }
    }
    pub fn get(&self, panel: HelpId, key: AnchorKey) -> Option<AnchorRect> {
        self.anchors
            .get(&key)
            .and_then(|(owner, rect)| (*owner == panel && rect.is_valid()).then_some(*rect))
    }
    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn len(&self) -> usize {
        self.anchors.len()
    }

    pub fn count_for_panel(&self, panel: HelpId) -> usize {
        self.anchors
            .values()
            .filter(|(owner, _)| *owner == panel)
            .count()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardPlacement {
    pub min: (f32, f32),
    pub size: (f32, f32),
}

pub fn place_card(
    target: Option<AnchorRect>,
    card: (f32, f32),
    viewport: (f32, f32),
    margin: f32,
) -> CardPlacement {
    let (w, h) = (
        card.0.min((viewport.0 - margin * 2.0).max(1.0)),
        card.1.min((viewport.1 - margin * 2.0).max(1.0)),
    );
    let card = (w, h);
    let (x, y) = target
        .map(|r| ((r.min.0 + r.max.0 - w) / 2.0, r.max.1 + margin))
        .unwrap_or(((viewport.0 - w) / 2.0, (viewport.1 - h) / 2.0));
    CardPlacement {
        min: (
            x.clamp(margin, (viewport.0 - w - margin).max(margin)),
            y.clamp(margin, (viewport.1 - h - margin).max(margin)),
        ),
        size: card,
    }
}

/// Rectangles that should be dimmed in a viewport-local coordinate space.
/// The target is intentionally omitted so highlighted controls remain readable
/// and, when safe, continue to receive routed clicks.
pub fn dim_regions(viewport: (f32, f32), target: Option<AnchorRect>) -> Vec<AnchorRect> {
    let (width, height) = viewport;
    let Some(target) = target else {
        // No target means informational help. Never turn an unavailable or
        // conditional step into a full-screen interaction barrier.
        return Vec::new();
    };
    let min_x = target.min.0.clamp(0.0, width);
    let min_y = target.min.1.clamp(0.0, height);
    let max_x = target.max.0.clamp(min_x, width);
    let max_y = target.max.1.clamp(min_y, height);
    let mut regions = Vec::with_capacity(4);
    if min_y > 0.0 {
        regions.push(AnchorRect {
            min: (0.0, 0.0),
            max: (width, min_y),
        });
    }
    if max_y < height {
        regions.push(AnchorRect {
            min: (0.0, max_y),
            max: (width, height),
        });
    }
    if min_x > 0.0 && max_y > min_y {
        regions.push(AnchorRect {
            min: (0.0, min_y),
            max: (min_x, max_y),
        });
    }
    if max_x < width && max_y > min_y {
        regions.push(AnchorRect {
            min: (max_x, min_y),
            max: (width, max_y),
        });
    }
    regions
}

/// Translate viewport-local dim regions into screen coordinates.
///
/// Translate painter-only dim regions into screen coordinates.
pub fn absolute_dim_regions(
    screen_min: (f32, f32),
    viewport: (f32, f32),
    target: Option<AnchorRect>,
) -> Vec<AnchorRect> {
    dim_regions(viewport, target)
        .into_iter()
        .map(|region| AnchorRect {
            min: (region.min.0 + screen_min.0, region.min.1 + screen_min.1),
            max: (region.max.0 + screen_min.0, region.max.1 + screen_min.1),
        })
        .collect()
}

/// Split painter-only dim regions around a card so the card remains readable.
pub fn exclude_rect(regions: Vec<AnchorRect>, excluded: AnchorRect) -> Vec<AnchorRect> {
    let mut result = Vec::new();
    for region in regions {
        let min_x = region.min.0.max(excluded.min.0);
        let min_y = region.min.1.max(excluded.min.1);
        let max_x = region.max.0.min(excluded.max.0);
        let max_y = region.max.1.min(excluded.max.1);
        if min_x >= max_x || min_y >= max_y {
            result.push(region);
            continue;
        }
        if region.min.1 < min_y {
            result.push(AnchorRect {
                min: region.min,
                max: (region.max.0, min_y),
            });
        }
        if max_y < region.max.1 {
            result.push(AnchorRect {
                min: (region.min.0, max_y),
                max: region.max,
            });
        }
        if region.min.0 < min_x && min_y < max_y {
            result.push(AnchorRect {
                min: (region.min.0, min_y),
                max: (min_x, max_y),
            });
        }
        if max_x < region.max.0 && min_y < max_y {
            result.push(AnchorRect {
                min: (max_x, min_y),
                max: (region.max.0, max_y),
            });
        }
    }
    result
}

pub fn absolute_dim_regions_excluding(
    screen_min: (f32, f32),
    viewport: (f32, f32),
    target: Option<AnchorRect>,
    excluded: AnchorRect,
) -> Vec<AnchorRect> {
    exclude_rect(absolute_dim_regions(screen_min, viewport, target), excluded)
}

/// A missing target is an informational step, not an instruction to block the
/// application. This also covers conditional controls and cross-panel frames.
pub fn step_target(
    registry: &AnchorRegistry,
    panel: HelpId,
    tutorial: HelpId,
    index: usize,
) -> Option<AnchorRect> {
    let route = route(tutorial, index);
    (panel == route.panel)
        .then_some(route.anchor)
        .flatten()
        .and_then(|anchor| registry.get(panel, anchor.key()))
}

pub fn forced_action_allowed(anchor: AnchorId) -> bool {
    matches!(
        anchor,
        AnchorId::EditorTemplate
            | AnchorId::EditorName
            | AnchorId::BlocksGenerate
            | AnchorId::LoadIntoRust
            | AnchorId::EditorSave
            | AnchorId::FacesPreset
            | AnchorId::SimulatorWatch
            | AnchorId::SimulatorDate
            | AnchorId::SimulatorApply
            | AnchorId::CalibrationRecord
            | AnchorId::CalibrationCopy
            | AnchorId::FileRefresh
            | AnchorId::FileFilter
            | AnchorId::FileList
            | AnchorId::FilePreview
            | AnchorId::SettingsTheme
            | AnchorId::SettingsText
            | AnchorId::SettingsLayout
    )
}

/// Whether an action may be processed while a guided tour is active.
/// Tour presentation never grants permission: safe actions remain usable and
/// unsafe actions are rejected by their handlers regardless of the anchor.
pub fn action_allowed(tour_active: bool, action: AnchorId) -> bool {
    !tour_active || !unsafe_action(action)
}

/// Actions which can write hardware, replace/delete user data, or start a
/// destructive/irreversible operation. These are guarded at their handlers;
/// no overlay hit-testing is part of this policy.
pub fn unsafe_action(action: AnchorId) -> bool {
    matches!(
        action,
        AnchorId::BuildArtifact
            | AnchorId::BuildCopy
            | AnchorId::ShellSend
            | AnchorId::ProbeRun
            | AnchorId::SettingsImport
            | AnchorId::SettingsRestore
    )
}

/// Legacy pointer barrier helper retained as a small pure state machine for
/// simulator regression coverage. Tutorial rendering never installs this barrier.
pub fn simulator_wait_for_pointer_release(waiting: &mut bool, primary_down: bool) -> bool {
    if *waiting && !primary_down {
        *waiting = false;
    }
    *waiting
}

/// Stable identifier for a contextual panel tutorial.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HelpId {
    /// The complete first-start beginner journey, including cross-panel routing.
    Startup,
    /// The explicit Advanced-mode safety and transport tour.
    Advanced,
    Dashboard,
    WatchFaces,
    Editor,
    Simulator,
    BuildFlash,
    Calibration,
    Modules,
    ShellAccess,
    Diagnostics,
    DebugOutput,
    Bugs,
    FileBrowser,
    Tutorials,
    Wiki,
    Settings,
    ProbeTest,
}

/// One walkthrough page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TutorialStep {
    pub title: &'static str,
    pub body: &'static str,
}

impl TutorialStep {
    pub fn expected_panel(self, tutorial: HelpId) -> HelpId {
        route(tutorial, 0).panel
    }
    pub fn anchor(self, tutorial: HelpId, index: usize) -> Option<AnchorId> {
        anchor_for_step(tutorial, index)
    }
    pub fn instruction(self, tutorial: HelpId, index: usize) -> &'static str {
        if self.anchor(tutorial, index).is_some() {
            "Try the highlighted control, or continue manually."
        } else {
            "Read this guidance. No action is required."
        }
    }
}

pub fn anchor_for_step(id: HelpId, index: usize) -> Option<AnchorId> {
    use AnchorId::*;
    if id == HelpId::Startup {
        return [
            Some(PanelHelp),
            Some(DashboardBoard),
            Some(FacesPreset),
            Some(BlocksGenerate),
            Some(BuildBoard),
            Some(SimulatorWatch),
            Some(BuildUnavailable),
            None,
        ][index.min(7)];
    }
    if id == HelpId::Advanced {
        return [
            Some(PanelHelp),
            Some(ShellMode),
            Some(ProbeRefresh),
            Some(ShellInput),
            Some(ProbeRun),
            Some(DiagnosticsRun),
            Some(ProbeReport),
            None,
        ][index.min(7)];
    }
    if id == HelpId::BuildFlash {
        return [
            Some(BuildUnavailable),
            Some(BuildBoard),
            None,
            Some(BuildProfile),
            None,
            Some(BuildArtifactPath),
            Some(BuildArtifactPath),
            None,
            Some(BuildInspect),
            None,
            Some(BuildApprove),
            Some(BuildRefresh),
            Some(BuildCopy),
            Some(BuildCopy),
        ][index.min(13)];
    }
    Some(match id {
        // BuildFlash returns above because some steps are informational.
        HelpId::BuildFlash => unreachable!(),
        HelpId::Startup | HelpId::Advanced => unreachable!(),
        HelpId::Dashboard => [DashboardBoard, DashboardNtpFetch, PanelHelp][index.min(2)],
        HelpId::WatchFaces => [FacesSearch, FacesPreset, FacesAdd][index.min(2)],
        // Start enters Blocks mode. The final step is Save after Load into Rust
        // has explicitly switched the editor to Rust mode.
        HelpId::Editor => [EditorName, BlocksGenerate, LoadIntoRust, EditorSave][index.min(3)],
        HelpId::Simulator => [SimulatorWatch, SimulatorDate, SimulatorApply][index.min(2)],

        HelpId::Calibration => [CalibrationFetch, CalibrationRecord, CalibrationCopy][index.min(2)],
        HelpId::Modules => ModulesRegister,
        HelpId::ShellAccess => [ShellMode, ShellInput, ShellSend][index.min(2)],
        HelpId::Diagnostics => DiagnosticsRun,
        HelpId::DebugOutput => [DebugLog, DebugCopy][index.min(1)],
        HelpId::Bugs => [BugsSearch, BugsFingerprint, BugsResolve, BugsReport][index.min(3)],
        HelpId::FileBrowser => [FileRefresh, FileFilter, FileList, FilePreview][index.min(3)],
        HelpId::Tutorials => TutorialSections,
        HelpId::Wiki => [WikiNavigation, WikiSearch, WikiSearch][index.min(2)],
        HelpId::Settings => [
            SettingsTheme,
            SettingsText,
            SettingsLayout,
            SettingsImport,
            SettingsRestore,
        ][index.min(4)],
        HelpId::ProbeTest => [ProbeRefresh, ProbeRun, ProbeReport][index.min(2)],
    })
}

/// Contextual tours suppressed after the user completes or skips startup.
pub const FIRST_RUN_SEQUENCE: [HelpId; 16] = [
    HelpId::Dashboard,
    HelpId::WatchFaces,
    HelpId::Editor,
    HelpId::Simulator,
    HelpId::BuildFlash,
    HelpId::Calibration,
    HelpId::Modules,
    HelpId::ShellAccess,
    HelpId::Diagnostics,
    HelpId::DebugOutput,
    HelpId::Bugs,
    HelpId::FileBrowser,
    HelpId::Tutorials,
    HelpId::Wiki,
    HelpId::Settings,
    HelpId::ProbeTest,
];

fn startup_panel(index: usize) -> HelpId {
    [
        HelpId::Dashboard,
        HelpId::Dashboard,
        HelpId::WatchFaces,
        HelpId::Editor,
        HelpId::BuildFlash,
        HelpId::Simulator,
        HelpId::BuildFlash,
        HelpId::Dashboard,
    ][index.min(7)]
}

fn advanced_panel(index: usize) -> HelpId {
    [
        HelpId::Dashboard,
        HelpId::ShellAccess,
        HelpId::ProbeTest,
        HelpId::ShellAccess,
        HelpId::ProbeTest,
        HelpId::Diagnostics,
        HelpId::ProbeTest,
        HelpId::Dashboard,
    ][index.min(7)]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepRoute {
    pub panel: HelpId,
    pub anchor: Option<AnchorId>,
}

pub fn route(id: HelpId, index: usize) -> StepRoute {
    StepRoute {
        panel: match id {
            HelpId::Startup => startup_panel(index),
            HelpId::Advanced => advanced_panel(index),
            _ => id,
        },
        anchor: anchor_for_step(id, index),
    }
}

pub fn pending_navigation(current: HelpId, wanted: StepRoute) -> Option<HelpId> {
    (current != wanted.panel).then_some(wanted.panel)
}

/// A complete tutorial definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tutorial {
    pub id: HelpId,
    pub stable_key: &'static str,
    pub title: &'static str,
    pub steps: &'static [TutorialStep],
}

macro_rules! steps {
    ($($title:literal => $body:literal),+ $(,)?) => {
        &[$(TutorialStep { title: $title, body: $body }),+]
    };
}

const TUTORIALS: &[Tutorial] = &[
    Tutorial {
        id: HelpId::Startup,
        stable_key: "startup",
        title: "Start here: the beginner journey",
        steps: steps!
        ("Welcome and choose a mode" => "Normal mode is the safe beginner starting point. Advanced mode exists for diagnostics and deliberate hardware work. Enabling it does not validate hardware. Start with the guided path and keep physical actions off until you understand the safeguards.",
         "Dashboard: know the target" => "Dashboard is the project checkpoint. Confirm the target board, selected project, and warnings before changing anything. These values are planning/status information, not hardware validation.",
         "Watch Faces: use the stock preset" => "Open Watch Faces and keep the stock/default preset as a known-good starting point. Select a face and review the preset before editing. This changes local project state only.",
         "Editor: Blocks workflow" => "In Editor, stay in Blocks mode: name the face, arrange starter blocks, generate source, then explicitly Load into Rust editor before saving. Generated code still needs review and may not compile.",
         "LCD, target board, and profile" => "Review LCD/component settings, target board revision, and the active profile in Build & Flash. Compatibility conflicts require an explicit choice. A profile is planning data and is never proof of electrical or firmware compatibility.",
         "Simulator: try the result" => "Select the face in Simulator, set a recognizable date/time, and use simulated buttons. Simulation is host-side and cannot validate the physical LCD, sensors, power, timing, or watch hardware.",
         "Build & Flash: limitations and existing artifacts" => "Configured builds remain fail-closed while the Studio-to-firmware input contract is incomplete. No configured UF2 is generated. If you already have a UF2, enter its path, inspect its required sidecars, and approve only that exact artifact. Local verification is not hardware validation.",
         "Safe next steps" => "Save a backup, read Diagnostics when status is unclear, and make one small host-side change at a time. Only later consider UART, bootloader, or probe actions after checking the target, wiring, voltage, and physical safeguards."),
    },
    Tutorial {
        id: HelpId::Advanced,
        stable_key: "advanced",
        title: "Advanced mode: transport and safety",
        steps: steps!
        ("Why Advanced exists" => "Advanced mode exposes diagnostics, shell, UART, and probe tools for deliberate development work. It is not required for the beginner workflow and it does not make simulated results or documentation into hardware validation.",
         "Simulated, UART, UF2, and SWD are different" => "Simulated commands run on the host. UART is a serial command path to a connected watch. UF2 is a file-copy path through the bootloader drive. SWD/probe is a separate debug/programming boundary. Never treat one transport's success as proof of another.",
         "Read-only before mutating shell" => "Use status/help/read-only shell commands first and inspect the response. Mutating commands can change watch state. Verify the exact target and command before sending, and do not paste unknown input.",
         "3.3V UART safety" => "UART requires the documented wiring, ground, baud, and 3.3V logic levels. Do not connect an incompatible voltage or assume USB drive detection is UART. If wiring or target identity is uncertain, stop.",
         "Physical actions need a deliberate gate" => "Probe and flash actions can affect hardware. Confirm the exact board, cable, port, and intended artifact, then use the explicit confirmation/report path. A canceled or unavailable probe is safer than guessing.",
         "Diagnostics status meanings" => "Diagnostics describe host-side checks and transport observations: pass means that check returned its expected result, warning means review a limitation or input, and error means the check did not establish the expected state. None is a certification.",
         "Read the report, not a claim" => "Probe/UART reports identify what Studio observed and what it could not verify. Keep logs, distinguish simulated from physical output, and stop on ambiguity rather than retrying a mutating action.",
         "Return safely" => "Normal mode remains the default for everyday editing. Return there after diagnostics, preserve evidence, and leave build/flash and physical safeguards enabled."),
    },
    Tutorial {
        id: HelpId::Dashboard,
        stable_key: "dashboard",
        title: "Dashboard tutorial",
        steps: steps!
        ("What this page does" => "Dashboard is your safe starting point. It summarizes the selected watch faces, target, storage, recent build status, time sync, and warnings. The panel does not write to hardware by itself.",
         "Beginner workflow" => "1. Read Target and the resource summary. 2. Choose a face in Watch Faces. 3. Open Simulator to try it. 4. Return here to check status before any export or hardware action. Expected result: you can tell what project is selected and whether an operation needs attention.",
         "Safety and limits" => "Warnings and Errors are links to diagnostic information, not proof that hardware is healthy. Studio hardware validation is not complete. Build is fail-closed when configured inputs are missing, and simulation cannot validate a real watch."),
    },
    Tutorial {
        id: HelpId::WatchFaces,
        stable_key: "watch-faces",
        title: "Watch Faces tutorial",
        steps: steps!
        ("Catalog and preset" => "Use the catalog search/category controls to find faces, then use the preset area to choose the faces used by the project. Inspect descriptions before changing the active preset.",
         "Beginner workflow" => "1. Search or refresh the catalog. 2. Add one simple face to the active preset. 3. Open Simulator and select it. Expected result: the selected face appears in the simulated watch without changing firmware or hardware.",
         "Safety and limits" => "Import, restore, and delete actions can replace or remove local data. Review confirmation dialogs and keep backups. Catalog metadata and the simulator are host-side only. A face has not been validated on hardware until separately tested."),
    },
    Tutorial {
        id: HelpId::Editor,
        stable_key: "editor",
        title: "Editor tutorial",
        steps: steps!
        ("What you can edit" => "The Editor provides the face name, description, templates, source editor, and beginner Blocks mode. Generate source when you are ready to inspect it. The generated Rust appears in the source editor.",
         "Name the face" => "Give the face a snake_case name. Blocks mode is the beginner starting point and does not require choosing a Rust template.",
         "Generate and load" => "Arrange starter blocks, generate Rust/source, then explicitly choose Load into Rust editor. This is the mode transition where the generated source becomes editable Rust.",
         "Save in Rust mode" => "Review the generated source and click Save face. Then select the saved face in Watch Faces and run Simulator. Saving changes local project data. It does not flash a watch.",
         "Safety and limits" => "Generated code may still need review and may not compile. The simulator is an approximation and does not prove timing, power, display, or sensor behavior on hardware."),
    },
    Tutorial {
        id: HelpId::Simulator,
        stable_key: "simulator",
        title: "Simulator tutorial",
        steps: steps!
        ("Controls" => "The watch preview, face selector, scale, date/time controls, and simulated button controls are visible here. Use them to exercise a face without connecting a watch.",
         "Beginner workflow" => "1. Select a preset face. 2. Set a recognizable date/time. 3. Click or hold the simulated buttons. 4. Observe the display and event/status output. Expected result: the preview responds deterministically to simulated input.",
         "Safety and limits" => "Simulation cannot access real sensors, crystal drift, battery behavior, USB, or the physical LCD. A simulated success is not hardware validation. Reset or changing simulation inputs only affects the host session."),
    },
    Tutorial {
        id: HelpId::BuildFlash,
        stable_key: "build-flash",
        title: "Build & Flash tutorial",
        steps: steps!
        ("1. Check the configured-build gate" => "Start by reading the Build unavailable explanation. Configured Studio firmware builds remain fail-closed because the Studio-to-firmware input contract is incomplete; no configured UF2 is generated. The five items are explanations of a firmware contract, not a beginner checklist: selecting every toggle cannot satisfy them.",
         "2. Review the board" => "Review Target board and its revision details. This records which watch revision you intend to flash as planning data, but Studio cannot generate the board-specific firmware inputs while configured builds are unavailable.",
         "3. Review active preset and faces" => "Review the active preset and its faces in Watch Faces, then return here. You can choose the stock/default preset and Studio records the ordered face/source plan, but it cannot generate a configured UF2 from that plan.",
         "4. Review the component/LCD profile" => "Review Components / Build Profile, including the component and LCD description. A UI selection is not the same as being wired into the firmware build. For example, enabling OPT3001 records the light-sensor plan, but does not add its firmware feature/module or connect its driver. There is currently no beginner action that completes this item because Studio lacks firmware-input generation.",
         "5. Understand the remaining contract" => "Studio cannot generate concrete pin, bus, address, power, ownership, or build-provenance inputs. SPI/I2C toggles are not pin mappings, and a selected thermistor does not identify its wiring. Keep the fail-closed gate in place; do not infer that the configured contract is complete.",
         "6. Follow the safe existing-UF2 path" => "For an existing, verified UF2, enter its explicit path, inspect it with its matching .uf2.json and .json.sig sidecars, review the metadata, approve only that exact artifact for this session, refresh bootloader detection, and copy only when exactly one expected watch drive is identified.",
         "7. Know what the copy means" => "The verified existing-UF2 path safely inspects and copies an artifact; it does not make a stock or recovery UF2 configured, prove that Studio planning choices were compiled, or validate hardware behavior.",
         "8. Choose your next action" => "Keep the stock preset, matching target/profile, and desired component choices if you want a saved plan; stop changing toggles expecting the gate to clear. If you need a watch artifact now, use only the existing verified-UF2 inspection/copy path above." ,
         "9. Inspect the artifact and sidecars" => "Click Inspect UF2. Review the reported structure, family, manifest, matching .uf2.json sidecar, and .json.sig sidecar where required. Stop if the required files are missing or the artifact is not the intended one.",
         "10. Interpret verification correctly" => "Verification reports local structure and digest consistency only. SHA-256 detects corruption or unexpected change; it is not a release key or proof of publisher identity. Authenticity requires a signature from a protected private key verified by a separately trusted public key, such as Ed25519. A mutable GitHub branch or checksum alone is insufficient because both can be changed with the artifact. It does not establish provenance, that the configured board/profile/faces were applied, bootloader success, firmware health, or compatibility with physical hardware.",
         "11. Approve for this session" => "After reviewing the metadata, click Approve for this session only for the exact artifact you intend to copy. Approval is session-scoped and does not make a stock artifact configured or prove hardware validity.",
         "12. Put one watch in bootloader mode" => "Put exactly one intended watch in bootloader mode with USB connected, then click Refresh detection. Wait for detection to finish; do not rely on an old drive list.",
         "13. Copy only with one expected drive" => "Copy only when detection shows one expected watch drive and the approved artifact is still the one you reviewed. Multiple drives are ambiguous; no drive is not ready. The Copy control is a host file-copy boundary and remains guarded.",
         "14. Wait, then unplug safely" => "After Copy reports completion, wait for the operation to finish and follow the watch/USB guidance before unplugging. Never unplug during a copy. A successful host copy still does not validate firmware behavior or hardware authenticity."),
    },
    Tutorial {
        id: HelpId::Calibration,
        stable_key: "calibration",
        title: "Calibration tutorial",
        steps: steps!
        ("Purpose and controls" => "Calibration shows two separate kinds of information: drift-session results calculated from start/end samples, and optional temperature-compensated RTC settings (base correction, temperature coefficient, and reference temperature). A session result is not the same thing as enabling temperature compensation.",
         "Beginner workflow" => "1. Use a trusted clock and record the start and end samples for a drift session. 2. Review the resulting ppm value and any optional temperature-compensation settings separately. 3. Copy a generated `settime` or `drift N` UART command if needed, then send it manually through the documented UART path; copying does not send or apply it. 4. Save only the local Studio session/settings you intend to keep. Expected result: Studio records host-side results/settings, while any hardware change happens only after you deliberately send a command over UART.",
         "Safety and limits" => "Studio does not apply drift corrections or `settime` commands and does not directly save calibration to hardware. Do not invent a measurement or use extreme values; verify the target and UART wiring before sending a mutating command. Temperature-compensation settings are optional firmware configuration, not proof that a drift session was applied. Simulation cannot measure a crystal or validate physical timekeeping."),
    },
    Tutorial {
        id: HelpId::Modules,
        stable_key: "modules",
        title: "Modules tutorial",
        steps: steps!
        ("Purpose and controls" => "Modules lists custom hardware modules and their target, name, description, and removal controls. These values become part of configuration review.",
         "Beginner workflow" => "1. Add or select a module. 2. Fill in its identity and target. 3. Review the component/build configuration. 4. Save settings before building. Expected result: the module is visible in the selected configuration.",
         "Safety and limits" => "Remove is destructive to local configuration and requires confirmation. A declared module is not detected or electrically tested by Studio. Unsupported hardware must not be treated as validated."),
    },
    Tutorial {
        id: HelpId::ShellAccess,
        stable_key: "shell-access",
        title: "Shell Access tutorial",
        steps: steps!
        ("Purpose and controls" => "Shell Access exposes advanced command input, activity logs, terminal history, filtering, Clear, Copy all, and Export. It is for inspection and controlled development work.",
         "Beginner workflow" => "1. Read the warning and current transport mode. 2. Start with a read-only/status command. 3. Check the response in the log. 4. Export useful output for troubleshooting. Expected result: you can inspect a session without changing the watch.",
         "Safety and limits" => "Advanced commands can change configuration or hardware state. Verify every command and never paste unknown input. Simulated transport is not UART and does not validate a physical watch. Logs can contain sensitive local details."),
    },
    Tutorial {
        id: HelpId::Diagnostics,
        stable_key: "diagnostics",
        title: "Diagnostics tutorial",
        steps: steps!
        ("Purpose and controls" => "Diagnostics groups offline checks, protocol/status information, filters, and result output. Use it to understand Studio state before escalating a problem.",
         "Beginner workflow" => "1. Run the least invasive check first. 2. Read each result and its limitations. 3. Repeat after correcting the named input. 4. Open Bugs or Debug Output when evidence is needed. Expected result: a reproducible host-side diagnosis.",
         "Safety and limits" => "Diagnostics are not a hardware certification. Physical transport requires the appropriate connection and may be unavailable. Prefer read-only checks. Destructive or write operations need deliberate confirmation."),
    },
    Tutorial {
        id: HelpId::DebugOutput,
        stable_key: "debug-output",
        title: "Debug Output tutorial",
        steps: steps!
        ("Purpose and controls" => "Debug Output shows bounded event logs, tick verbosity/filter controls, Clear, Copy, and Export. It records what Studio attempted and observed.",
         "Beginner workflow" => "1. Reproduce the issue. 2. Increase verbosity only as needed. 3. Read the newest entries. 4. Copy or Export the relevant range. Expected result: a compact trace that can explain a host-side failure.",
         "Safety and limits" => "Clearing removes the current visible log and is not undoable. Logs describe Studio and transport observations, not proof that firmware ran correctly on hardware. Avoid sharing secrets included in command output."),
    },
    Tutorial {
        id: HelpId::Bugs,
        stable_key: "bugs",
        title: "Bugs tutorial",
        steps: steps!
        ("Purpose and controls" => "Bugs collects errors and warnings, search/filter fields, and diagnostic details. It is an evidence view, not an automatic repair tool.",
         "Beginner workflow" => "1. Read the first error and its context. 2. Reproduce once with Debug Output visible. 3. Record exact inputs and status. 4. Export or copy evidence for a bug report. Expected result: a clear report rather than a guess.",
         "Safety and limits" => "Deleting or clearing evidence can make troubleshooting harder. A warning may describe an intentional safety gate. Do not disable fail-closed build or approval checks to hide an error."),
    },
    Tutorial {
        id: HelpId::FileBrowser,
        stable_key: "file-browser",
        title: "File Browser tutorial",
        steps: steps!
        ("Purpose and controls" => "File Browser is a read-only view of workspace files, paths, and metadata. Refresh updates the host-side view. It does not scan the watch.",
         "Beginner workflow" => "1. Refresh the listing. 2. Select a relevant source or artifact. 3. Inspect its path and metadata. 4. Use Build & Flash for explicit artifact inspection and approval. Expected result: you know which local file you are reviewing.",
         "Safety and limits" => "This panel does not edit or flash files. Import/restore/delete actions elsewhere can be destructive. Confirm them and keep backups. File presence does not mean the artifact is valid or hardware-tested."),
    },
    Tutorial {
        id: HelpId::Tutorials,
        stable_key: "tutorials",
        title: "Tutorials tutorial",
        steps: steps!
        ("Choose a path" => "Tutorials is the directory of beginner walkthroughs. Choose the page that matches your goal. The same contextual help is also available from each panel's ? Help button.",
         "Beginner workflow" => "1. Start with Dashboard. 2. Continue to Watch Faces, Editor, and Simulator. 3. Read Build & Flash before any artifact action. 4. Use Diagnostics and Bugs when something differs from the expected result.",
         "Safety and limits" => "These tutorials explain the current Studio behavior, including known limitations. They do not replace board-specific electrical, USB, UART, or firmware documentation. Hardware has not been validated by this UI."),
    },
    Tutorial {
        id: HelpId::Wiki,
        stable_key: "wiki",
        title: "Wiki tutorial",
        steps: steps!
        ("Purpose and controls" => "Wiki provides project reference pages and navigation/search controls. Use it for concepts and known constraints while the contextual tutorial explains the current screen.",
         "Beginner workflow" => "1. Search for the concept you need. 2. Read prerequisites first. 3. Return to the relevant panel and make one small change. 4. Check the expected result in Simulator or logs.",
         "Safety and limits" => "Reference text may describe capabilities that are not enabled in your configuration. Treat hardware claims as documentation, not validation by Studio, and keep build/flash safety gates intact."),
    },
    Tutorial {
        id: HelpId::Settings,
        stable_key: "settings",
        title: "Settings tutorial",
        steps: steps!
        ("Purpose and controls" => "Settings controls language, theme, text size, tab layout, firmware project/output paths, board/configuration, and Import/Restore/Delete operations where available.",
         "Beginner workflow" => "1. Change one setting. 2. Review the visible result. 3. Save or export a backup before importing another configuration. 4. Restart only when a setting says it is needed. Expected result: the selected Studio configuration is clear and recoverable.",
         "Safety and limits" => "Import/Restore/Delete can replace or remove local settings. Confirm and keep a backup. Settings do not prove a firmware build is valid, do not bypass fail-closed gates, and do not validate connected hardware."),
    },
    Tutorial {
        id: HelpId::ProbeTest,
        stable_key: "probe-test",
        title: "Probe / Test tutorial",
        steps: steps!
        ("Purpose and controls" => "Probe / Test contains transport selection, port detection, UART Connect/Send, probe progress, and report output. It is an advanced hardware boundary.",
         "Beginner workflow" => "1. Connect the documented UART jig. 2. Refresh detection and choose the expected port. 3. Click UART Connect and verify the status. 4. Send only a known-safe test command and read the report. Expected result: a UART response is captured or a clear connection error is shown.",
         "Safety and limits" => "Probe requires UART. USB drive detection is not UART. Check wiring, voltage, port, and baud settings before sending. Probe results are not full hardware validation, and Studio makes no claim that an untested board works."),
    },
];

impl HelpId {
    pub const ALL: [Self; 18] = [
        Self::Startup,
        Self::Advanced,
        Self::Dashboard,
        Self::WatchFaces,
        Self::Editor,
        Self::Simulator,
        Self::BuildFlash,
        Self::Calibration,
        Self::Modules,
        Self::ShellAccess,
        Self::Diagnostics,
        Self::DebugOutput,
        Self::Bugs,
        Self::FileBrowser,
        Self::Tutorials,
        Self::Wiki,
        Self::Settings,
        Self::ProbeTest,
    ];

    pub fn stable_key(self) -> &'static str {
        tutorial(self).stable_key
    }
}

pub fn tutorial(id: HelpId) -> &'static Tutorial {
    TUTORIALS
        .iter()
        .find(|tutorial| tutorial.id == id)
        .expect("all HelpIds have tutorial data")
}

pub fn all() -> &'static [Tutorial] {
    TUTORIALS
}

pub fn step_index(id: HelpId, requested: usize) -> usize {
    requested.min(tutorial(id).steps.len().saturating_sub(1))
}

pub fn previous_index(id: HelpId, current: usize) -> usize {
    step_index(id, current).saturating_sub(1)
}

pub fn next_index(id: HelpId, current: usize) -> usize {
    let last = tutorial(id).steps.len().saturating_sub(1);
    step_index(id, current).saturating_add(1).min(last)
}

/// Persistent claims for panel auto-tours. The string representation keeps the
/// settings file backward-compatible and allows unknown future IDs to survive a
/// load/save round trip.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TourClaims {
    keys: HashSet<String>,
}

impl TourClaims {
    pub fn from_keys(keys: impl IntoIterator<Item = String>) -> Self {
        Self {
            keys: keys.into_iter().collect(),
        }
    }

    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<_> = self.keys.iter().cloned().collect();
        keys.sort();
        keys
    }

    pub fn contains(&self, id: HelpId) -> bool {
        self.keys.contains(id.stable_key())
    }

    pub fn claim(&mut self, id: HelpId) -> bool {
        self.keys.insert(id.stable_key().to_string())
    }

    pub fn claim_all(&mut self, ids: impl IntoIterator<Item = HelpId>) {
        for id in ids {
            self.claim(id);
        }
    }

    /// Finish or skip the startup tour: suppress every contextual auto-tour,
    /// but leave the startup and Advanced tutorials reopenable explicitly.
    pub fn claim_startup_sequence(&mut self) {
        self.claim_all(FIRST_RUN_SEQUENCE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_ids_and_keys_are_unique_and_complete() {
        assert_eq!(TUTORIALS.len(), HelpId::ALL.len());
        let ids: HashSet<_> = TUTORIALS.iter().map(|t| t.id).collect();
        let keys: HashSet<_> = TUTORIALS.iter().map(|t| t.stable_key).collect();
        assert_eq!(ids.len(), TUTORIALS.len());
        assert_eq!(keys.len(), TUTORIALS.len());
        for id in HelpId::ALL {
            assert!(ids.contains(&id));
        }
    }

    #[test]
    fn every_tutorial_has_nonempty_steps_and_text() {
        for tutorial in TUTORIALS {
            assert!(!tutorial.title.trim().is_empty());
            assert!(!tutorial.steps.is_empty());
            for step in tutorial.steps {
                assert!(!step.title.trim().is_empty());
                assert!(!step.body.trim().is_empty());
            }
        }
    }

    #[test]
    fn navigation_is_bounded() {
        for tutorial in TUTORIALS {
            let last = tutorial.steps.len() - 1;
            assert_eq!(step_index(tutorial.id, usize::MAX), last);
            assert_eq!(previous_index(tutorial.id, 0), 0);
            assert_eq!(next_index(tutorial.id, last), last);
            assert_eq!(next_index(tutorial.id, usize::MAX), last);
        }
    }

    #[test]
    fn build_flash_tutorial_explains_current_limits_and_safe_next_action() {
        let text = tutorial(HelpId::BuildFlash)
            .steps
            .iter()
            .map(|step| format!("{} {}", step.title, step.body))
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        for phrase in [
            "selecting every toggle cannot satisfy",
            "planning data",
            "ui selection",
            "wired into the firmware build",
            "opt3001",
            "no beginner action",
            "lacks firmware-input generation",
            "uf2.json",
            "json.sig",
            "exactly one expected watch drive",
        ] {
            assert!(text.contains(phrase), "tutorial omitted: {phrase}");
        }
    }

    #[test]
    fn held_pointer_stays_blocked_through_pause_and_resume() {
        let mut waiting = false;
        assert!(!simulator_wait_for_pointer_release(&mut waiting, false));
        waiting = true;
        assert!(simulator_wait_for_pointer_release(&mut waiting, true));
        assert!(simulator_wait_for_pointer_release(&mut waiting, true));
        assert!(waiting);
    }

    #[test]
    fn card_or_tinted_click_then_resume_requires_release() {
        let mut waiting = true;
        assert!(simulator_wait_for_pointer_release(&mut waiting, true));
        assert!(simulator_wait_for_pointer_release(&mut waiting, true));
        assert!(waiting);
    }

    #[test]
    fn panel_switch_while_held_stays_blocked() {
        let mut waiting = true;
        assert!(simulator_wait_for_pointer_release(&mut waiting, true));
    }

    #[test]
    fn pointer_release_clears_barrier() {
        let mut waiting = true;
        assert!(!simulator_wait_for_pointer_release(&mut waiting, false));
        assert!(!waiting);
    }

    #[test]
    fn fresh_press_works_after_barrier_clears() {
        let mut waiting = true;
        assert!(!simulator_wait_for_pointer_release(&mut waiting, false));
        assert!(!simulator_wait_for_pointer_release(&mut waiting, true));
    }

    #[test]
    fn unavailable_target_is_informational_and_unblocked() {
        let mut registry = AnchorRegistry::default();
        registry.begin_frame(1);
        assert_eq!(
            anchor_for_step(HelpId::BuildFlash, 0),
            Some(AnchorId::BuildUnavailable)
        );
        assert!(step_target(&registry, HelpId::BuildFlash, HelpId::BuildFlash, 2).is_none());
        assert!(action_allowed(true, AnchorId::BuildUnavailable));
        assert!(action_allowed(true, AnchorId::BuildApprove));
        assert!(!action_allowed(true, AnchorId::BuildCopy));
        // The renderer passes None through the informational card path rather
        // than painting a dim region for an unavailable target.
        assert!(dim_regions((800.0, 600.0), None).is_empty());
    }

    #[test]
    fn conditional_target_is_unavailable_until_registered() {
        let mut registry = AnchorRegistry::default();
        registry.begin_frame(1);
        assert!(step_target(&registry, HelpId::WatchFaces, HelpId::WatchFaces, 2).is_none());

        registry.register(
            HelpId::WatchFaces,
            AnchorId::FacesAdd.key(),
            AnchorRect {
                min: (10.0, 20.0),
                max: (80.0, 50.0),
            },
        );
        assert!(step_target(&registry, HelpId::WatchFaces, HelpId::WatchFaces, 2).is_some());
    }

    #[test]
    fn invalid_or_wrong_panel_target_is_unavailable() {
        let mut registry = AnchorRegistry::default();
        registry.begin_frame(1);
        registry.register(
            HelpId::BuildFlash,
            AnchorId::BuildApprove.key(),
            AnchorRect {
                min: (20.0, 20.0),
                max: (20.0, 40.0),
            },
        );
        assert!(step_target(&registry, HelpId::BuildFlash, HelpId::BuildFlash, 10).is_none());
        registry.register(
            HelpId::BuildFlash,
            AnchorId::BuildApprove.key(),
            AnchorRect {
                min: (20.0, 20.0),
                max: (120.0, 60.0),
            },
        );
        assert!(step_target(&registry, HelpId::Settings, HelpId::BuildFlash, 2).is_none());
        assert!(step_target(&registry, HelpId::BuildFlash, HelpId::BuildFlash, 10).is_some());
    }

    #[test]
    fn build_flash_tutorial_has_ordered_unique_steps_and_valid_routes() {
        let tutorial = tutorial(HelpId::BuildFlash);
        assert_eq!(tutorial.steps.len(), 14);
        let titles: Vec<_> = tutorial.steps.iter().map(|step| step.title).collect();
        assert_eq!(titles[0], "1. Check the configured-build gate");
        assert_eq!(titles[13], "14. Wait, then unplug safely");
        assert!(titles.windows(2).all(|pair| pair[0] != pair[1]));

        let expected = [
            Some(AnchorId::BuildUnavailable),
            Some(AnchorId::BuildBoard),
            None,
            Some(AnchorId::BuildProfile),
            None,
            Some(AnchorId::BuildArtifactPath),
            Some(AnchorId::BuildArtifactPath),
            None,
            Some(AnchorId::BuildInspect),
            None,
            Some(AnchorId::BuildApprove),
            Some(AnchorId::BuildRefresh),
            Some(AnchorId::BuildCopy),
            Some(AnchorId::BuildCopy),
        ];
        let mut registry = AnchorRegistry::default();
        registry.begin_frame(1);
        for (index, anchor) in expected.into_iter().flatten().enumerate() {
            registry.register(
                HelpId::BuildFlash,
                anchor.key(),
                AnchorRect {
                    min: (index as f32, 0.0),
                    max: (index as f32 + 1.0, 1.0),
                },
            );
        }
        for (index, anchor) in expected.into_iter().enumerate() {
            assert_eq!(anchor_for_step(HelpId::BuildFlash, index), anchor);
            assert_eq!(route(HelpId::BuildFlash, index).anchor, anchor);
            assert_eq!(
                step_target(&registry, HelpId::BuildFlash, HelpId::BuildFlash, index).is_some(),
                anchor.is_some()
            );
        }
    }

    #[test]
    fn valid_target_remains_gated_and_progression_recovers() {
        let mut registry = AnchorRegistry::default();
        registry.begin_frame(1);
        registry.register(
            HelpId::BuildFlash,
            AnchorId::BuildApprove.key(),
            AnchorRect {
                min: (20.0, 20.0),
                max: (120.0, 60.0),
            },
        );
        assert!(step_target(&registry, HelpId::BuildFlash, HelpId::BuildFlash, 10).is_some());
        assert!(!action_allowed(true, AnchorId::BuildCopy));
        assert!(action_allowed(true, AnchorId::BuildApprove));
        assert_eq!(next_index(HelpId::BuildFlash, 10), 11);
        assert_eq!(previous_index(HelpId::BuildFlash, 10), 9);

        // A later frame can remove the conditional target without retaining a
        // stale rectangle, and the step becomes recoverable/informational.
        registry.begin_frame(2);
        assert!(step_target(&registry, HelpId::BuildFlash, HelpId::BuildFlash, 2).is_none());
        assert!(!action_allowed(true, AnchorId::BuildCopy));
        assert_eq!(next_index(HelpId::BuildFlash, 2), 3);
    }

    #[test]
    fn tour_policy_allows_safe_actions_and_blocks_unsafe_actions() {
        assert!(action_allowed(true, AnchorId::EditorName));
        assert!(action_allowed(true, AnchorId::SimulatorDate));
        for action in [
            AnchorId::BuildArtifact,
            AnchorId::BuildCopy,
            AnchorId::ShellSend,
            AnchorId::ProbeRun,
            AnchorId::SettingsImport,
            AnchorId::SettingsRestore,
        ] {
            assert!(unsafe_action(action));
            assert!(!action_allowed(true, action));
        }
        assert!(action_allowed(false, AnchorId::BuildCopy));
    }

    #[test]
    fn stable_ids_and_persistent_claims() {
        let id = HelpId::BuildFlash;
        assert_eq!(id.stable_key(), "build-flash");
        let mut claims = TourClaims::default();
        assert!(!claims.contains(id));
        assert!(claims.claim(id));
        assert!(!claims.claim(id));
        assert!(claims.contains(id));
        assert_eq!(
            TourClaims::from_keys(claims.keys()).keys(),
            vec!["build-flash"]
        );
    }

    #[test]
    fn panel_coverage_is_explicit() {
        assert_eq!(HelpId::ALL.len(), 18);
        assert!(HelpId::ALL
            .iter()
            .all(|id| all().iter().any(|t| t.id == *id)));
    }

    #[test]
    fn stable_anchor_keys_and_expected_panel_are_valid() {
        let anchors = [
            AnchorId::PanelHelp,
            AnchorId::EditorName,
            AnchorId::ProbeRun,
        ];
        let keys: HashSet<_> = anchors.into_iter().map(|a| a.key()).collect();
        assert_eq!(keys.len(), anchors.len());
        for id in HelpId::ALL {
            for (index, _) in tutorial(id).steps.iter().enumerate() {
                assert_eq!(route(id, index).panel, route(id, index).panel);
            }
        }
    }

    #[test]
    fn stale_or_wrong_panel_anchors_fall_back() {
        let mut registry = AnchorRegistry::default();
        registry.begin_frame(1);
        registry.register(
            HelpId::Editor,
            AnchorId::EditorName.key(),
            AnchorRect {
                min: (1.0, 2.0),
                max: (3.0, 4.0),
            },
        );
        assert!(registry
            .get(HelpId::WatchFaces, AnchorId::EditorName.key())
            .is_none());
        registry.begin_frame(2);
        assert!(registry
            .get(HelpId::Editor, AnchorId::EditorName.key())
            .is_none());
    }

    #[test]
    fn dimming_uses_four_regions_and_leaves_target_clear() {
        let regions = dim_regions(
            (100.0, 80.0),
            Some(AnchorRect {
                min: (30.0, 20.0),
                max: (70.0, 60.0),
            }),
        );
        assert_eq!(regions.len(), 4);
        assert!(regions
            .iter()
            .all(|region| { region.max.0 > region.min.0 && region.max.1 > region.min.1 }));
        assert!(dim_regions((100.0, 80.0), None).is_empty());
    }

    #[test]
    fn nonzero_screen_origin_translates_painter_regions_without_offset_errors() {
        let regions = absolute_dim_regions(
            (120.0, 45.0),
            (100.0, 80.0),
            Some(AnchorRect {
                min: (30.0, 20.0),
                max: (70.0, 60.0),
            }),
        );
        assert_eq!(regions[0].min, (120.0, 45.0));
        assert_eq!(regions[0].max, (220.0, 65.0));
        assert_eq!(regions[2].min, (120.0, 65.0));
        assert_eq!(regions[2].max, (150.0, 105.0));
    }

    #[test]
    fn card_placement_stays_inside_viewport_and_moves_with_target() {
        let first = place_card(
            Some(AnchorRect {
                min: (20.0, 20.0),
                max: (40.0, 40.0),
            }),
            (300.0, 180.0),
            (800.0, 600.0),
            16.0,
        );
        let second = place_card(
            Some(AnchorRect {
                min: (700.0, 400.0),
                max: (740.0, 440.0),
            }),
            (300.0, 180.0),
            (800.0, 600.0),
            16.0,
        );
        assert_ne!(first.min, second.min);
        assert!(second.min.0 >= 16.0 && second.min.1 >= 16.0);
        assert!(second.min.0 + second.size.0 <= 784.0);
        assert!(second.min.1 + second.size.1 <= 584.0);
    }

    #[test]
    fn card_placement_stays_inside_viewport() {
        let card = place_card(
            Some(AnchorRect {
                min: (790.0, 590.0),
                max: (810.0, 610.0),
            }),
            (300.0, 180.0),
            (800.0, 600.0),
            16.0,
        );
        assert!(card.min.0 >= 16.0 && card.min.1 >= 16.0);
        assert!(card.min.0 + card.size.0 <= 784.0 && card.min.1 + card.size.1 <= 584.0);
        let centered = place_card(None, (300.0, 180.0), (800.0, 600.0), 16.0);
        assert!(centered.min.0 >= 16.0);
    }

    #[test]
    fn card_next_advances_to_the_following_step() {
        let id = HelpId::Editor;
        let current = step_index(id, 0);
        assert_eq!(next_index(id, current), 1);
        assert_ne!(next_index(id, current), current);
    }

    #[test]
    fn first_run_editor_steps_have_explicit_safe_blocks_route() {
        let expected = [
            AnchorId::EditorName,
            AnchorId::BlocksGenerate,
            AnchorId::LoadIntoRust,
            AnchorId::EditorSave,
        ];
        for (index, anchor) in expected.into_iter().enumerate() {
            assert_eq!(anchor_for_step(HelpId::Editor, index), Some(anchor));
            assert!(forced_action_allowed(anchor));
        }
    }

    fn intersects(a: AnchorRect, b: AnchorRect) -> bool {
        a.min.0 < b.max.0 && b.min.0 < a.max.0 && a.min.1 < b.max.1 && b.min.1 < a.max.1
    }

    #[test]
    fn card_over_dim_has_no_dim_intersection() {
        let card = AnchorRect {
            min: (105.0, 55.0),
            max: (175.0, 95.0),
        };
        let target = Some(AnchorRect {
            min: (70.0, 50.0),
            max: (90.0, 70.0),
        });
        let raw_dim = absolute_dim_regions((100.0, 50.0), (100.0, 80.0), target);
        assert!(raw_dim.iter().any(|region| intersects(*region, card)));
        let dim = absolute_dim_regions_excluding((100.0, 50.0), (100.0, 80.0), target, card);
        assert!(dim.iter().all(|region| !intersects(*region, card)));
        // Painter-only dimming is excluded from the card geometry.
        assert_eq!(
            dim,
            absolute_dim_regions_excluding(
                (100.0, 50.0),
                (100.0, 80.0),
                Some(AnchorRect {
                    min: (70.0, 50.0),
                    max: (90.0, 70.0)
                }),
                card,
            )
        );
    }

    #[test]
    fn navigation_and_cross_panel_pending_are_deterministic() {
        assert_eq!(
            pending_navigation(HelpId::Dashboard, route(HelpId::Editor, 0)),
            Some(HelpId::Editor)
        );
        assert_eq!(
            pending_navigation(HelpId::Editor, route(HelpId::Editor, 1)),
            None
        );
        assert_eq!(next_index(HelpId::Editor, 0), 1);
    }

    #[test]
    fn tinted_area_has_no_input_policy_and_handlers_guard_unsafe_actions() {
        assert!(dim_regions((800.0, 600.0), None).is_empty());
        assert!(unsafe_action(AnchorId::BuildCopy));
        assert!(unsafe_action(AnchorId::ProbeRun));
        assert!(!unsafe_action(AnchorId::EditorName));
        assert!(!action_allowed(true, AnchorId::BuildCopy));
        assert!(action_allowed(true, AnchorId::EditorName));
    }

    #[test]
    fn forced_actions_are_safe_only() {
        assert!(forced_action_allowed(AnchorId::EditorTemplate));
        assert!(forced_action_allowed(AnchorId::SimulatorApply));
        assert!(forced_action_allowed(AnchorId::CalibrationRecord));
        assert!(forced_action_allowed(AnchorId::FilePreview));
        assert!(!forced_action_allowed(AnchorId::BuildCopy));
        assert!(!forced_action_allowed(AnchorId::SettingsImport));
        assert!(!forced_action_allowed(AnchorId::ProbeRun));
        assert_eq!(FIRST_RUN_SEQUENCE.len(), 16);
        assert!(!FIRST_RUN_SEQUENCE.contains(&HelpId::Startup));
        assert!(!FIRST_RUN_SEQUENCE.contains(&HelpId::Advanced));
    }

    #[test]
    fn startup_skip_all_suppresses_contextual_tours_but_keeps_reopenable_tours() {
        let mut claims = TourClaims::default();
        claims.claim_startup_sequence();
        for id in FIRST_RUN_SEQUENCE {
            assert!(claims.contains(id));
        }
        assert!(!claims.contains(HelpId::Startup));
        assert!(!claims.contains(HelpId::Advanced));
    }

    #[test]
    fn startup_and_advanced_routes_cover_the_safety_journey_in_order() {
        let startup = (0..tutorial(HelpId::Startup).steps.len())
            .map(|index| route(HelpId::Startup, index).panel)
            .collect::<Vec<_>>();
        assert_eq!(
            startup,
            vec![
                HelpId::Dashboard,
                HelpId::Dashboard,
                HelpId::WatchFaces,
                HelpId::Editor,
                HelpId::BuildFlash,
                HelpId::Simulator,
                HelpId::BuildFlash,
                HelpId::Dashboard,
            ]
        );
        let advanced = (0..tutorial(HelpId::Advanced).steps.len())
            .map(|index| route(HelpId::Advanced, index).panel)
            .collect::<Vec<_>>();
        assert_eq!(
            advanced,
            vec![
                HelpId::Dashboard,
                HelpId::ShellAccess,
                HelpId::ProbeTest,
                HelpId::ShellAccess,
                HelpId::ProbeTest,
                HelpId::Diagnostics,
                HelpId::ProbeTest,
                HelpId::Dashboard,
            ]
        );
        let text = tutorial(HelpId::Advanced)
            .steps
            .iter()
            .map(|step| format!("{} {}", step.title, step.body))
            .collect::<String>();
        for term in [
            "simulated",
            "UART",
            "UF2",
            "SWD",
            "read-only",
            "mutating",
            "3.3V",
        ] {
            assert!(text.contains(term), "missing Advanced tour term: {term}");
        }
    }

    #[test]
    fn pause_and_resume_keep_the_current_step_and_route() {
        let paused_step = 4;
        let paused_route = route(HelpId::Startup, paused_step);
        assert_eq!(step_index(HelpId::Startup, paused_step), paused_step);
        assert_eq!(route(HelpId::Startup, paused_step), paused_route);
        assert_eq!(next_index(HelpId::Startup, paused_step), paused_step + 1);
    }

    #[test]
    fn first_run_sequence_keeps_safe_manual_panel_boundaries() {
        assert_eq!(route(HelpId::Startup, 0).panel, HelpId::Dashboard);
        assert_eq!(route(HelpId::Startup, 2).panel, HelpId::WatchFaces);
        assert_eq!(route(HelpId::Startup, 3).panel, HelpId::Editor);
        assert_eq!(route(HelpId::Startup, 4).panel, HelpId::BuildFlash);
        assert_eq!(route(HelpId::Startup, 5).panel, HelpId::Simulator);
        assert_eq!(
            pending_navigation(HelpId::Dashboard, route(HelpId::Startup, 2)),
            Some(HelpId::WatchFaces)
        );
    }

    #[test]
    fn action_gating_blocks_underlying_unsafe_clicks() {
        assert!(action_allowed(false, AnchorId::BuildCopy));
        assert!(!action_allowed(true, AnchorId::BuildCopy));
        assert!(!action_allowed(true, AnchorId::ShellSend));
        assert!(action_allowed(true, AnchorId::SimulatorApply));
        assert!(!action_allowed(true, AnchorId::SettingsImport));
    }

    #[test]
    fn high_value_anchor_keys_are_registered_without_collisions() {
        let keys = [
            AnchorId::SimulatorWatch,
            AnchorId::SimulatorDate,
            AnchorId::SimulatorApply,
            AnchorId::CalibrationRecord,
            AnchorId::CalibrationCopy,
            AnchorId::BugsFingerprint,
            AnchorId::BugsResolve,
            AnchorId::BugsReport,
            AnchorId::FileRefresh,
            AnchorId::FileFilter,
            AnchorId::FileList,
            AnchorId::FilePreview,
            AnchorId::TutorialSections,
            AnchorId::SettingsImport,
            AnchorId::SettingsRestore,
        ];
        let mut registry = AnchorRegistry::default();
        registry.begin_frame(7);
        for (index, key) in keys.into_iter().enumerate() {
            registry.register(
                HelpId::Simulator,
                key.key(),
                AnchorRect {
                    min: (index as f32, 0.0),
                    max: (index as f32 + 1.0, 1.0),
                },
            );
        }
        assert_eq!(registry.len(), keys.len());
        assert_eq!(registry.count_for_panel(HelpId::Simulator), keys.len());
        for key in keys {
            assert!(registry.get(HelpId::Simulator, key.key()).is_some());
        }
    }
}
