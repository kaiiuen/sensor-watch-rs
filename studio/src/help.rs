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
    EditorGenerate,
    EditorSave,
    FacesSearch,
    FacesAdd,
    FacesPreset,
    SimulatorWatch,
    SimulatorDate,
    SimulatorApply,
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
        self.anchors.insert(key, (panel, rect));
    }
    pub fn get(&self, panel: HelpId, key: AnchorKey) -> Option<AnchorRect> {
        self.anchors
            .get(&key)
            .and_then(|(owner, rect)| (*owner == panel).then_some(*rect))
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

pub fn overlay_allows_click(anchor: Option<AnchorId>, in_target: bool) -> bool {
    in_target && anchor.is_some_and(forced_action_allowed)
}

pub fn forced_action_allowed(anchor: AnchorId) -> bool {
    matches!(
        anchor,
        AnchorId::EditorTemplate
            | AnchorId::EditorName
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

/// Whether an action may be processed while the guided tour owns the input.
/// Unsafe actions are never force-enabled by a tour; safe actions are allowed
/// only when they are the currently highlighted control.
pub fn action_allowed(active_step: Option<AnchorId>, action: AnchorId) -> bool {
    active_step.is_none() || (active_step == Some(action) && forced_action_allowed(action))
}

/// Stable identifier for a contextual panel tutorial.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HelpId {
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
        tutorial
    }
    pub fn anchor(self, tutorial: HelpId, index: usize) -> Option<AnchorId> {
        anchor_for_step(tutorial, index)
    }
    pub fn instruction(self, tutorial: HelpId, index: usize) -> &'static str {
        if self.anchor(tutorial, index).is_some() {
            "Try the highlighted control, or continue manually."
        } else {
            "Read this guidance; no action is required."
        }
    }
}

pub fn anchor_for_step(id: HelpId, index: usize) -> Option<AnchorId> {
    use AnchorId::*;
    Some(match id {
        HelpId::Dashboard => [DashboardBoard, DashboardNtpFetch, PanelHelp][index.min(2)],
        HelpId::WatchFaces => [FacesSearch, FacesPreset, FacesAdd][index.min(2)],
        HelpId::Editor => [EditorTemplate, EditorName, EditorGenerate, EditorSave][index.min(3)],
        HelpId::Simulator => [SimulatorWatch, SimulatorDate, SimulatorApply][index.min(2)],
        HelpId::BuildFlash => [
            BuildUnavailable,
            BuildInspect,
            BuildApprove,
            BuildRefresh,
            BuildCopy,
        ][index.min(4)],
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

pub const FIRST_RUN_SEQUENCE: [HelpId; 4] = [
    HelpId::Dashboard,
    HelpId::WatchFaces,
    HelpId::Editor,
    HelpId::Simulator,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepRoute {
    pub panel: HelpId,
    pub anchor: Option<AnchorId>,
}

pub fn route(id: HelpId, index: usize) -> StepRoute {
    StepRoute {
        panel: id,
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
         "Safety and limits" => "Import, restore, and delete actions can replace or remove local data; review confirmation dialogs and keep backups. Catalog metadata and the simulator are host-side only; a face has not been validated on hardware until separately tested."),
    },
    Tutorial {
        id: HelpId::Editor,
        stable_key: "editor",
        title: "Editor tutorial",
        steps: steps!
        ("What you can edit" => "The Editor provides the face name, description, templates, source editor, and beginner Blocks mode. Generate source when you are ready to inspect it; the generated Rust appears in the source editor.",
         "Beginner workflow" => "1. Name the face. 2. Start with a template or Blocks. 3. Generate source, then review or edit it in the source editor. 4. Save the face. 5. Select it in Watch Faces and run Simulator. Expected result: a saved local face renders in simulation.",
         "Safety and limits" => "Saving changes local project data; it does not flash a watch. Generated code may still need review and may not compile. The simulator is an approximation and does not prove timing, power, display, or sensor behavior on hardware."),
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
        ("Current build state" => "Configured Studio firmware builds are currently unavailable: the input contract is incomplete, so Build .uf2 is fail-closed and no configured UF2 is generated. This is the current state, not a reason to treat a stock or unrelated artifact as your selected configuration.",
         "Beginner workflow" => "1. Review the selected board, components, faces, and output path; the configured Build .uf2 action remains unavailable for now. 2. For an explicit existing UF2, inspect it and its required sidecars, then Approve it only if it matches your intent. 3. Refresh drive detection and select the expected drive. 4. Use Copy to watch only for that approved artifact. When the missing input contract is implemented, the intended configured build flow will be review, build, inspect, approve, detect, and copy. Expected result: only an explicitly approved host artifact is copied.",
         "Safety and limits" => "Do not bypass the fail-closed configured-build gate or infer that profile selections were applied to an artifact. Artifact inspection proves local consistency, not authenticity, provenance, boot, or firmware health. UF2 Copy to watch is a host file copy; never unplug during it, and hardware behavior is not validated by Studio."),
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
         "Safety and limits" => "Remove is destructive to local configuration and requires confirmation. A declared module is not detected or electrically tested by Studio; unsupported hardware must not be treated as validated."),
    },
    Tutorial {
        id: HelpId::ShellAccess,
        stable_key: "shell-access",
        title: "Shell Access tutorial",
        steps: steps!
        ("Purpose and controls" => "Shell Access exposes advanced command input, activity logs, terminal history, filtering, Clear, Copy all, and Export. It is for inspection and controlled development work.",
         "Beginner workflow" => "1. Read the warning and current transport mode. 2. Start with a read-only/status command. 3. Check the response in the log. 4. Export useful output for troubleshooting. Expected result: you can inspect a session without changing the watch.",
         "Safety and limits" => "Advanced commands can change configuration or hardware state; verify every command and never paste unknown input. Simulated transport is not UART and does not validate a physical watch. Logs can contain sensitive local details."),
    },
    Tutorial {
        id: HelpId::Diagnostics,
        stable_key: "diagnostics",
        title: "Diagnostics tutorial",
        steps: steps!
        ("Purpose and controls" => "Diagnostics groups offline checks, protocol/status information, filters, and result output. Use it to understand Studio state before escalating a problem.",
         "Beginner workflow" => "1. Run the least invasive check first. 2. Read each result and its limitations. 3. Repeat after correcting the named input. 4. Open Bugs or Debug Output when evidence is needed. Expected result: a reproducible host-side diagnosis.",
         "Safety and limits" => "Diagnostics are not a hardware certification. Physical transport requires the appropriate connection and may be unavailable. Prefer read-only checks; destructive or write operations need deliberate confirmation."),
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
        ("Purpose and controls" => "File Browser is a read-only view of workspace files, paths, and metadata. Refresh updates the host-side view; it does not scan the watch.",
         "Beginner workflow" => "1. Refresh the listing. 2. Select a relevant source or artifact. 3. Inspect its path and metadata. 4. Use Build & Flash for explicit artifact inspection and approval. Expected result: you know which local file you are reviewing.",
         "Safety and limits" => "This panel does not edit or flash files. Import/restore/delete actions elsewhere can be destructive; confirm them and keep backups. File presence does not mean the artifact is valid or hardware-tested."),
    },
    Tutorial {
        id: HelpId::Tutorials,
        stable_key: "tutorials",
        title: "Tutorials tutorial",
        steps: steps!
        ("Choose a path" => "Tutorials is the directory of beginner walkthroughs. Choose the page that matches your goal; the same contextual help is also available from each panel's ? Help button.",
         "Beginner workflow" => "1. Start with Dashboard. 2. Continue to Watch Faces, Editor, and Simulator. 3. Read Build & Flash before any artifact action. 4. Use Diagnostics and Bugs when something differs from the expected result.",
         "Safety and limits" => "These tutorials explain the current Studio behavior, including known limitations. They do not replace board-specific electrical, USB, UART, or firmware documentation; hardware has not been validated by this UI."),
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
         "Safety and limits" => "Import/Restore/Delete can replace or remove local settings; confirm and keep a backup. Settings do not prove a firmware build is valid, do not bypass fail-closed gates, and do not validate connected hardware."),
    },
    Tutorial {
        id: HelpId::ProbeTest,
        stable_key: "probe-test",
        title: "Probe / Test tutorial",
        steps: steps!
        ("Purpose and controls" => "Probe / Test contains transport selection, port detection, UART Connect/Send, probe progress, and report output. It is an advanced hardware boundary.",
         "Beginner workflow" => "1. Connect the documented UART jig. 2. Refresh detection and choose the expected port. 3. Click UART Connect and verify the status. 4. Send only a known-safe test command and read the report. Expected result: a UART response is captured or a clear connection error is shown.",
         "Safety and limits" => "Probe requires UART; USB drive detection is not UART. Check wiring, voltage, port, and baud settings before sending. Probe results are not full hardware validation, and Studio makes no claim that an untested board works."),
    },
];

impl HelpId {
    pub const ALL: [Self; 16] = [
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

/// Session-only dismissal state. A fresh Studio process intentionally starts empty.
#[derive(Default)]
pub struct Dismissed {
    ids: HashSet<HelpId>,
}

impl Dismissed {
    pub fn contains(&self, id: HelpId) -> bool {
        self.ids.contains(&id)
    }
    pub fn insert(&mut self, id: HelpId) {
        self.ids.insert(id);
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
    fn stable_ids_and_session_dismissal() {
        let id = HelpId::BuildFlash;
        assert_eq!(id.stable_key(), "build-flash");
        let mut dismissed = Dismissed::default();
        assert!(!dismissed.contains(id));
        dismissed.insert(id);
        assert!(dismissed.contains(id));
    }

    #[test]
    fn panel_coverage_is_explicit() {
        assert_eq!(HelpId::ALL.len(), 16);
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
                assert_eq!(route(id, index).panel, id);
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
    fn overlay_blocks_background_and_destructive_actions() {
        assert!(!overlay_allows_click(Some(AnchorId::BuildCopy), true));
        assert!(!overlay_allows_click(Some(AnchorId::ProbeRun), true));
        assert!(!overlay_allows_click(Some(AnchorId::EditorName), false));
        assert!(overlay_allows_click(Some(AnchorId::EditorName), true));
        assert!(!overlay_allows_click(None, true));
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
        assert_eq!(
            FIRST_RUN_SEQUENCE,
            [
                HelpId::Dashboard,
                HelpId::WatchFaces,
                HelpId::Editor,
                HelpId::Simulator
            ]
        );
    }

    #[test]
    fn first_run_sequence_keeps_safe_manual_panel_boundaries() {
        // The current UI opens one contextual tutorial at a time. The sequence
        // is explicit, but it does not silently navigate between separate
        // Editor/Faces/Simulator tutorials; users choose the next panel safely.
        assert_eq!(FIRST_RUN_SEQUENCE[1], HelpId::WatchFaces);
        assert_eq!(FIRST_RUN_SEQUENCE[2], HelpId::Editor);
        assert_eq!(FIRST_RUN_SEQUENCE[3], HelpId::Simulator);
        assert_eq!(
            pending_navigation(HelpId::Editor, route(HelpId::Editor, 1)),
            None
        );
    }

    #[test]
    fn action_gating_blocks_underlying_unsafe_clicks() {
        assert!(action_allowed(None, AnchorId::BuildCopy));
        assert!(!action_allowed(
            Some(AnchorId::BuildCopy),
            AnchorId::BuildCopy
        ));
        assert!(!action_allowed(
            Some(AnchorId::ProbeRun),
            AnchorId::ShellSend
        ));
        assert!(action_allowed(
            Some(AnchorId::SimulatorApply),
            AnchorId::SimulatorApply
        ));
        assert!(!action_allowed(
            Some(AnchorId::SettingsImport),
            AnchorId::SettingsImport
        ));
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
