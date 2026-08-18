//! Firmware Studio - a GUI companion app for the Sensor-Watch firmware.
//!
//! This is the end-goal product: an editor, debugger, and assembler that
//! produces the final `.uf2` firmware file.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod block_editor;
mod build;
pub mod build_snapshot;
mod components;
mod data_dir;
mod debug;
mod diagnostics;
mod distribution;
mod drift;
mod editor;
mod error_catalog;
mod face_sim;
mod faces;
mod file_browser;
pub mod firmware_inputs;
mod flash;
mod fonts;
mod fuzz;
mod help;
mod i18n;
mod integrity;
mod master_clock;
mod modules;
mod ntp;
mod optical;
mod panic_map;
mod persist;
mod presets;
mod probe;
mod progress;
mod real_face;
mod restore;
mod settings;
mod sim_provenance;
mod sysstats;
mod test_runtime;
mod theme;
mod transport;
pub mod update;
mod watch_config;
mod watch_display;
mod watch_sim;
mod wiki;

use components::BoardKind as Board;
use eframe::egui;
use help::{AnchorId, AnchorRect, AnchorRegistry, HelpId, TourClaims};
use i18n::{tr, Key, Language};
use presets::PresetManager;
use std::error::Error as _;

const HELP_DIM_LAYER_ORDER: egui::Order = egui::Order::Middle;
const HELP_CARD_LAYER_ORDER: egui::Order = egui::Order::Foreground;

use flash::{FlashRequest, FlashResult, FlashStatus, WatchDriveSelection};
use progress::{ProgressEvent, ProgressReceiver};
use theme::Theme;
use watch_sim::CasioF91W;

/// The main application state.
const SIM_WEEKDAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

fn clamp_sim_weekday(weekday: usize) -> usize {
    weekday.min(SIM_WEEKDAY_NAMES.len() - 1)
}

fn sim_weekday_name(weekday: usize) -> &'static str {
    SIM_WEEKDAY_NAMES[clamp_sim_weekday(weekday)]
}

struct StudioApp {
    /// Whether the CJK font has been installed yet.
    fonts_installed: bool,
    /// The currently selected panel.
    current_panel: Panel,
    /// The last status message shown in the status bar.
    status: String,
    /// Package/developer distribution status shown independently of build status.
    package_status: distribution::PackageStatus,
    /// The discovered watch faces.
    face_list: Vec<faces::FaceInfo>,
    /// Whether a build is currently running.
    building: bool,
    /// Whether the window is closing; no new work is accepted after this point.
    shutting_down: bool,
    /// The handle to the background build thread.
    pending_build: Option<std::thread::JoinHandle<build::BuildResult>>,
    /// A cached watch-drive detection result; detection never runs while rendering.
    cached_watch: WatchDriveSelection,
    /// The handle to the background drive detection worker.
    pending_detection: Option<(
        u64,
        std::thread::JoinHandle<WatchDriveSelection>,
        ProgressReceiver,
    )>,
    /// The handle and bounded progress stream for the background flash worker.
    pending_flash: Option<(u64, std::thread::JoinHandle<FlashResult>, ProgressReceiver)>,
    /// The latest user-visible flash/detection event.
    current_progress: Option<ProgressEvent>,
    /// Next operation identifier; IDs make overlapping log streams diagnosable.
    next_operation_id: u64,
    /// Prevents detection and flashing from overlapping.
    flash_worker_state: flash::WorkerState,
    /// The last build result message.
    build_message: String,
    /// The explicitly approved artifact and the metadata verified at approval time.
    approved_artifact: Option<ApprovedArtifact>,
    /// Path entered for explicit artifact inspection.
    artifact_path_input: String,
    /// A verified artifact awaiting explicit approval.
    pending_artifact: Option<build::ArtifactInspection>,
    /// Configuration fingerprint captured when the pending artifact was verified.
    pending_artifact_fingerprint: Option<String>,
    /// Configuration fingerprint captured when the build was started.
    pending_build_fingerprint: Option<String>,
    /// The selected language.
    language: Language,
    /// The selected theme.
    theme: Theme,
    /// Theme value applied to egui on the previous frame.
    applied_theme: Option<Theme>,
    /// Text-size value applied to egui on the previous frame.
    applied_text_size: Option<u8>,
    /// The debug log.
    log: debug::DebugLog,
    /// Shared filter for high-frequency tick/process events.
    tick_verbosity: debug::TickVerbosity,
    /// Bounded dedicated tick/process event log.
    tick_log: debug::DebugLog,
    /// The Casio F-91W simulator.
    watch: CasioF91W,
    /// The simulator's stopwatch accumulator (centiseconds).
    sim_last_tick: std::time::Instant,
    /// The SVG watch renderer.
    watch_renderer: watch_display::WatchRenderer,
    /// The simulator display scale (0.5 - 2.0).
    sim_scale: f32,
    /// The watch-face preset manager.
    presets: PresetManager,
    /// The currently selected face in the catalog.
    selected_face: Option<usize>,
    /// The currently selected face in the active preset.
    selected_preset_face: Option<usize>,
    /// The index being dragged in the active preset (for drag-and-drop reorder).
    drag_preset_from: Option<usize>,
    /// The catalog face name being dragged (to add to the preset on drop).
    drag_catalog_face: Option<String>,
    /// The name for a new preset.
    new_preset_name: String,
    /// The editor's current face name.
    editor_name: String,
    /// The editor's current source.
    editor_source: String,
    /// The editor's face description (shown to users in the catalog).
    editor_description: String,
    /// The selected editor template.
    editor_template: usize,
    /// State for the beginner-friendly visual block editor.
    block_editor: block_editor::BlockEditor,
    /// The selected NTP server index.
    ntp_server: usize,
    /// Custom NTP servers added by the user (name, host).
    ntp_servers: Vec<(String, String)>,
    /// The name/host being edited for a custom NTP server.
    ntp_edit_name: String,
    ntp_edit_host: String,
    /// The custom NTP server currently being edited, if any.
    ntp_edit_index: Option<usize>,
    /// The NTP-derived UTC time (seconds since epoch), if fetched.
    ntp_time: Option<u64>,
    /// The NTP ping latency in ms.
    ntp_ping: f64,
    /// The NTP clock offset in seconds.
    ntp_offset: f64,
    /// Whether an NTP query is in flight.
    ntp_busy: bool,
    /// The handle to the background NTP query.
    pending_ntp: Option<std::thread::JoinHandle<Result<ntp::NtpResult, String>>>,
    /// The expected SHA-256 from the GitHub release, if fetched.
    release_sha256: Option<String>,
    /// Whether a release checksum fetch is in flight.
    checksum_busy: bool,
    /// The handle to the background checksum fetch.
    pending_checksum: Option<std::thread::JoinHandle<Result<String, String>>>,
    /// The watch configuration (mirrors the firmware Settings register).
    watch_config: watch_config::WatchConfig,
    /// Custom hardware modules.
    modules: modules::ModuleManager,
    /// The output directory for built artifacts (e.g. the .uf2).
    output_dir: String,
    /// Pending user-entered data root; changes apply on next launch only.
    pending_data_folder: String,
    /// Inline validation/status for the pending data root.
    data_folder_status: String,
    /// The editor's module name/target/description inputs.
    module_name: String,
    module_target: String,
    module_description: String,
    /// Frame-local semantic rectangles used by the guided help overlay.
    help_anchors: AnchorRegistry,
    help_frame: u64,
    /// Contextual help walkthrough currently open, if any.
    help_open: Option<HelpId>,
    /// Current step in the open walkthrough.
    help_step: usize,
    /// Destination panel requested by a cross-panel tour transition; anchors are
    /// intentionally rendered only after the destination has produced a frame.
    help_pending_panel: Option<Panel>,
    /// Whether the current walkthrough is minimized rather than closed.
    help_minimized: bool,

    /// Changes when a tour step/panel/tour changes, giving the movable card a
    /// fresh default without resetting it on ordinary repaint frames.
    help_card_generation: u64,
    /// Whether the current walkthrough was auto-opened from a panel visit.
    help_auto_opened: bool,
    /// Persistent per-panel auto-tour claims.
    tour_claims: TourClaims,
    /// The latest system resource snapshot for the footer.
    sys_stats: sysstats::SysStats,
    /// The receiver for background system resource samples.
    sys_rx: std::sync::mpsc::Receiver<sysstats::SysStats>,
    /// The catalog search query.
    catalog_search: String,
    /// The catalog category filter (empty = all).
    catalog_category: String,
    /// The read-only workspace file browser.
    file_browser: file_browser::FileBrowser,
    /// The target board revision (green, red/lite, blue, pro).
    board: Board,
    /// Named hardware component profiles and the editable active draft.
    component_profiles: Vec<components::BuildProfile>,
    component_profile: usize,
    component_draft: components::ComponentsConfig,
    component_effective: components::ComponentsConfig,
    /// A board/profile change whose compatibility conflicts await an explicit choice.
    pending_component_conflict: Option<PendingComponentConflict>,
    /// The index of the preset face currently being simulated.
    sim_face_idx: usize,
    /// Simulator date controller: year, month, day, hour, minute, weekday.
    sim_year: i32,
    sim_month: u32,
    sim_day: u32,
    sim_hour: u32,
    sim_minute: u32,
    sim_weekday: usize,
    /// Simulator button press state (edge detection + hold timing).
    btn_l_down: bool,
    btn_c_down: bool,
    btn_a_down: bool,
    btn_l_hold: f32,
    btn_c_hold: f32,
    btn_a_hold: f32,
    /// Public firmware-event state for the real-face seam. The legacy booleans
    /// above remain the face_sim/CASIO timing path.
    btn_l_events: real_face::ButtonEventState,
    btn_a_events: real_face::ButtonEventState,
    /// The time delta (seconds) since the last frame, for hold timing.
    sim_dt: f32,
    /// The button currently being held (via the on-watch SVG hotspot or a Hold
    /// button). Persists even if the pointer drifts off the widget while held;
    /// only clears when the mouse is fully released.
    held_button: Option<ButtonId>,
    /// Whether the primary pointer was down in the latest frame. Cancellation
    /// uses this to distinguish a real release from ownership changing while a
    /// physical press is still in progress.
    sim_pointer_primary_down: bool,
    /// Prevents a still-held physical pointer from becoming a new simulator
    /// press after cancellation. Cleared only after observing pointer-up.
    blocked_until_pointer_release: bool,
    /// The stateful watch-face simulation engine.
    face_engine: face_sim::FaceEngine,
    /// The optional REAL face running through the firmware `Hw` seam. Only
    /// present for faces that have been migrated into the seam; otherwise the
    /// simulator falls back to `face_engine`.
    real_face: Option<real_face::RealFace>,
    /// Whether the last rendered frame used the real-face seam (vs face_sim).
    last_render_used_real: bool,
    /// The real face and clock mode that were most recently activated.
    active_real_face_name: Option<String>,
    active_real_mode_24: Option<bool>,
    /// Accumulator for advancing the face state once per second.
    face_tick_accum: f32,
    /// The drift calibration session.
    drift_session: drift::DriftSession,
    /// Optional temperature-compensated RTC calibration settings.
    rtc_calibration: settings::RtcCalibrationSettings,
    /// The number of fuzz iterations to run.
    fuzz_iterations: usize,
    /// The terminal input line.
    terminal_input: String,
    /// Whether the terminal panel is expanded.
    terminal_open: bool,
    /// Whether terminal output wraps to the window width.
    terminal_wrap: bool,
    /// The terminal output history.
    terminal_history: Vec<String>,
    /// The Shell Access tab's command input and its own activity log.
    shell_input: String,
    shell_log: debug::DebugLog,
    /// The Shell Access tab's low-level "watch brain" log (hardware/ISR view).
    shell_hw_log: debug::DebugLog,
    /// Offline shell/simulator diagnostics state.
    diagnostics: diagnostics::DiagnosticsState,
    /// Explicit shell transport selection and optional physical UART connection.
    transport_mode: transport::TransportMode,
    serial_ports: Vec<transport::PortChoice>,
    selected_serial_port: Option<String>,
    uart: Option<transport::SerialTransport>,
    /// Most recent explicit UART connection failure, if any.
    last_uart_error: Option<String>,
    /// The latest commit message from GitHub, if the check succeeded.
    latest_commit: Option<String>,
    /// The SHA of the latest GitHub commit, if the check succeeded.
    latest_sha: Option<String>,
    /// The timestamp (unix seconds) when the update check succeeded.
    update_time: Option<u64>,
    /// Whether the update check is in flight.
    update_checking: bool,
    /// The handle to the background update check.
    pending_update: Option<std::thread::JoinHandle<Result<RemoteCommit, UpdateCheckError>>>,
    /// Whether the beep-on-minute helper is armed.
    beep_armed: bool,
    /// The target minute boundary timestamp for the beep.
    beep_target: u64,
    /// The stats sampling rate in milliseconds.
    stats_rate_ms: u64,
    /// Shared atomic for the sampler thread to read the live rate.
    stats_rate_shared: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// The UI text size (0=small, 1=normal, 2=big).
    text_size: u8,
    /// Preferred top-level tab-bar row count.
    tab_layout: settings::TabLayoutMode,
    /// Tab-bar overflow handling.
    tab_overflow: settings::TabOverflowBehavior,
    /// Persisted panel widths for the Watch Faces layout.
    catalog_width: f32,
    preset_height: f32,
    /// The window size from the last frame, used to detect resizes and reset
    /// panel ratios.
    last_window_size: egui::Vec2,
    /// The timestamp of the last successful build.
    last_build_time: Option<u64>,
    /// The number of builds performed this session.
    build_count: u32,
    /// A log of recent build times (unix seconds), newest last.
    build_history: Vec<u64>,
    /// Dedicated log for the build panel.
    build_log: debug::DebugLog,
    /// Dedicated log for the flash panel.
    flash_log: debug::DebugLog,
    /// Dedicated log for errors/warnings (shown in the Bugs tab).
    error_log: debug::DebugLog,
    /// Fingerprint entered in the Bugs/Diagnostics resolver.
    panic_fingerprint_input: String,
    /// Search text for the in-app error/fault encyclopedia.
    catalog_error_search: String,
    /// Area filter for the in-app error/fault encyclopedia.
    catalog_error_area: String,
    /// Search text for the grouped Settings credits list.
    credits_search: String,
    /// Last panic fingerprint resolution result.
    panic_resolution: String,
    /// Dedicated log for the Watch Faces tab.
    faces_log: debug::DebugLog,
    /// Dedicated log for the Simulator tab.
    sim_log: debug::DebugLog,
    /// A pending destructive action awaiting confirmation in a modal dialog.
    pending_confirm: Option<(String, ConfirmKind)>,
    /// The face source currently shown in the "View code" popup (name, source).
    code_view: Option<(String, String)>,
    /// The result message of the last "Test before adding" fuzz run.
    fuzz_test_result: Option<String>,
    /// Whether the first-run welcome overlay should be shown.
    first_run: bool,
    /// Whether the first-run welcome is minimized into its resume banner.
    welcome_minimized: bool,
    /// Whether ordinary Studio changes and close automatically save settings.
    persist_user_changes: bool,
    /// Whether a valid build starts with a fresh transient test session.
    reset_test_session_on_compile: bool,
    /// Whether debug/test executables use an isolated profile per executable.
    fresh_test_executable_profile: bool,
    /// Whether settings have been persisted on exit (guards against repeats).
    saved_on_exit: bool,
    /// The built-in reference wiki.
    wiki: wiki::Wiki,
    /// The maximum number of lines kept in each output/terminal/debug log.
    line_limit: usize,
    /// Local configuration restore points.
    restore_store: restore::RestoreStore,
    /// Name input for a manually created restore point.
    restore_name: String,
    /// Whether Developer Mode tools are visible.
    advanced_mode: bool,
    /// Persisted presentation-only preferences keyed by stable panel name.
    panel_ux_overrides: std::collections::BTreeMap<String, settings::PanelUxOverrides>,
    /// Panel selected in the compact UX override editor.
    ux_override_panel: usize,
    /// Whether the developer-mode warning is awaiting confirmation.
    advanced_mode_confirm: bool,
    /// Explicit developer-configured Master Clock path; never read from PATH.
    master_clock_path: String,
    /// The on-demand Master Clock child process.
    master_clock_process: Option<master_clock::MasterClockProcess>,
    /// Most recent physical probe report.
    probe_report: Option<probe::ProbeReport>,
    /// Background physical probe worker and its nonblocking progress channel.
    pending_probe: Option<std::thread::JoinHandle<probe::ProbeResult<transport::SerialTransport>>>,
    probe_progress_rx: Option<std::sync::mpsc::Receiver<probe::ProbeProgress>>,
    probe_progress: Option<probe::ProbeProgress>,
}

/// A short human-readable description of a board revision: what it is, how it
/// differs from the others, and its key hardware details.
fn board_info(b: Board) -> &'static str {
    match b {
        Board::Green => {
            "Green is the original reference board: a bare PCB with no case. It has a\n\
             red indicator LED (active-low, so logic 0 lights it), a piezo buzzer,\n\
             and no onboard sensors. It is the default target for new builds."
        }
        Board::RedLite => {
            "Red / Lite is a compact, low-cost variation of the Green board. It keeps\n\
             the same red LED polarity and piezo buzzer but drops the light sensor\n\
             to save cost, so faces that auto-dim by ambient light behave\n\
             differently. Pick it when flashing a Lite revision."
        }
        Board::Blue => {
            "Blue is the flagship board: it adds a blue LED (active-low, like the\n\
             red one) and a temperature sensor, so it can log temperature. The\n\
             buzzer and button layout match Green, but the LED is blue instead of\n\
             red. Choose Blue for temperature-capable faces."
        }
        Board::Pro => {
            "Pro is the most capable board: it has a 3-color RGB LED (active-low\n\
             for each channel), a temperature sensor, and a higher-timed buzzer\n\
             driver. It is the largest and most power-hungry revision. Choose Pro\n\
             when you need RGB backlight effects or the richer buzzer."
        }
    }
}

/// Country/timezone names aligned with `TIMEZONE_OFFSETS` (index = time_zone).
const COUNTRIES: [&str; 41] = [
    "UTC",
    "UK / Ireland",
    "Central Europe",
    "Eastern Europe",
    "Iran",
    "Moscow",
    "Pakistan",
    "India",
    "Sri Lanka",
    "Nepal",
    "Bangladesh",
    "Indochina",
    "China / Singapore",
    "Japan / Korea",
    "Australia East",
    "Australia Central",
    "Australia West",
    "New Zealand",
    "Samoa",
    "Hawaii",
    "Alaska",
    "Pacific US",
    "Mountain US",
    "Central US",
    "Eastern US",
    "Atlantic",
    "Brazil East",
    "Argentina",
    "Newfoundland",
    "Venezuela",
    "Bolivia",
    "Paraguay",
    "Colombia / Peru",
    "Ecuador",
    "Central America",
    "Mexico",
    "US Mountain (no DST)",
    "Pacific Mexico",
    "French Polynesia",
    "Marquesas",
    "Gambier",
];

/// Returns the country label for a timezone index.
fn country_label(index: u8) -> &'static str {
    COUNTRIES.get(index as usize).copied().unwrap_or("UTC")
}

/// The first-run walkthrough steps.
const MAX_CUSTOM_NTP_SERVERS: usize = 64;

const FIRST_RUN_STEPS: [&str; 5] = [
    "1. Choose Normal mode for the safe beginner path",
    "2. Review Dashboard, Watch Faces, and the stock/default preset",
    "3. Use Editor Blocks, then try the result in Simulator",
    "4. Review LCD, target board, profile, and compatibility safeguards",
    "5. Read Build & Flash limitations before any artifact or hardware action",
];

fn contextual_help_allowed(welcome_active: bool) -> bool {
    !welcome_active
}

/// Returns a usable preset name, rejecting empty and whitespace-only input.
fn preset_name(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Panel {
    Dashboard,
    Faces,
    Editor,
    Simulator,
    BuildFlash,
    Calibration,
    Modules,
    Shell,
    Diagnostics,
    Debug,
    Bugs,
    FileBrowser,
    Wiki,
    Tutorials,
    Settings,
    Probe,
}

/// The action a simulator button edge produced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SimAction {
    None,
    Press,
    Release,
}

/// Which simulator button is being handled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ButtonId {
    L,
    C,
    A,
}

/// A board/profile compatibility change awaiting an explicit choice.
#[derive(Clone, Debug)]
struct PendingComponentConflict {
    board: Board,
    profile: usize,
    draft: components::ComponentsConfig,
    title: String,
    issues: components::CompatibilityResult,
}

/// A destructive action awaiting confirmation in a modal dialog.
#[derive(Clone, PartialEq, Eq, Debug)]
enum ConfirmKind {
    DeletePreset(String),
    DeleteFaceFromPreset(usize),
    DeleteFaceFile(String),
    RemoveModule(String),
    RunPhysicalProbe,
    ResetTestProfile,
    LaunchMasterClock,
}

fn panel_for_help_id(id: HelpId) -> Option<Panel> {
    [
        Panel::Dashboard,
        Panel::Faces,
        Panel::Editor,
        Panel::Simulator,
        Panel::BuildFlash,
        Panel::Calibration,
        Panel::Modules,
        Panel::Shell,
        Panel::Diagnostics,
        Panel::Debug,
        Panel::Bugs,
        Panel::FileBrowser,
        Panel::Tutorials,
        Panel::Wiki,
        Panel::Settings,
        Panel::Probe,
    ]
    .into_iter()
    .find(|panel| panel.help_id() == id)
}

impl Panel {
    const ALL: [Self; 16] = [
        Self::Dashboard,
        Self::Faces,
        Self::Editor,
        Self::Simulator,
        Self::BuildFlash,
        Self::Calibration,
        Self::Modules,
        Self::Shell,
        Self::Diagnostics,
        Self::Debug,
        Self::Bugs,
        Self::FileBrowser,
        Self::Tutorials,
        Self::Wiki,
        Self::Settings,
        Self::Probe,
    ];

    fn key(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Faces => "faces",
            Self::Editor => "editor",
            Self::Simulator => "simulator",
            Self::BuildFlash => "build_flash",
            Self::Calibration => "calibration",
            Self::Modules => "modules",
            Self::Shell => "shell",
            Self::Diagnostics => "diagnostics",
            Self::Debug => "debug",
            Self::Bugs => "bugs",
            Self::FileBrowser => "file_browser",
            Self::Tutorials => "tutorials",
            Self::Wiki => "wiki",
            Self::Settings => "settings",
            Self::Probe => "probe",
        }
    }

    fn help_id(self) -> HelpId {
        match self {
            Panel::Dashboard => HelpId::Dashboard,
            Panel::Faces => HelpId::WatchFaces,
            Panel::Editor => HelpId::Editor,
            Panel::Simulator => HelpId::Simulator,
            Panel::BuildFlash => HelpId::BuildFlash,
            Panel::Calibration => HelpId::Calibration,
            Panel::Modules => HelpId::Modules,
            Panel::Shell => HelpId::ShellAccess,
            Panel::Diagnostics => HelpId::Diagnostics,
            Panel::Debug => HelpId::DebugOutput,
            Panel::Bugs => HelpId::Bugs,
            Panel::FileBrowser => HelpId::FileBrowser,
            Panel::Tutorials => HelpId::Tutorials,
            Panel::Wiki => HelpId::Wiki,
            Panel::Settings => HelpId::Settings,
            Panel::Probe => HelpId::ProbeTest,
        }
    }

    fn label(self, lang: Language) -> &'static str {
        match self {
            Panel::Dashboard => tr(lang, Key::Dashboard),
            Panel::Faces => tr(lang, Key::WatchFaces),
            Panel::Editor => "Editor",
            Panel::Simulator => "Simulator",
            Panel::BuildFlash => "Build & Flash",
            Panel::Calibration => "Calibration",
            Panel::Modules => "Modules",
            Panel::Shell => "Shell Access",
            Panel::Diagnostics => "Diagnostics",
            Panel::Debug => tr(lang, Key::DebugOutput),
            Panel::Bugs => "Bugs",
            Panel::FileBrowser => "File Browser",
            Panel::Tutorials => tr(lang, Key::Tutorials),
            Panel::Wiki => "Wiki",
            Panel::Settings => tr(lang, Key::Settings),
            Panel::Probe => "Probe / Test",
        }
    }
}

struct CreditEntry {
    name: &'static str,
    details: &'static str,
    url: Option<&'static str>,
}

struct CreditGroup {
    name: &'static str,
    entries: &'static [CreditEntry],
}

const UPSTREAM_CREDITS: &[CreditEntry] = &[
    CreditEntry {
        name: "kaiiuen",
        details: "Studio author",
        url: Some("https://github.com/kaiiuen"),
    },
    CreditEntry {
        name: "Joey Castillo / Sensor Watch",
        details: "Original C firmware, hardware, movement architecture, simulator, display work, and project coordination",
        url: Some("https://github.com/joeycastillo/Sensor-Watch"),
    },
    CreditEntry {
        name: "Second Movement contributors",
        details: "UTC/UTZ timekeeping, DST, custom LCDs, board variants, face ports, background tasks, alarms, USB/UART experiments, and hardware validation",
        url: Some("https://github.com/joeycastillo/Sensor-Watch/tree/main/movement2"),
    },
    CreditEntry {
        name: "evq / utz",
        details: "Timezone and DST library used as an upstream reference",
        url: None,
    },
    CreditEntry {
        name: "atsamd-rs and svd2rust contributors",
        details: "PAC, HAL, and register-generation work informing SAM L22 Rust support",
        url: Some("https://github.com/atsamd-rs/atsamd"),
    },
    CreditEntry {
        name: "Microchip",
        details: "SAM L22 datasheets and silicon errata",
        url: None,
    },
    CreditEntry {
        name: "STMicroelectronics",
        details: "LIS2DW, LIS2DW12, and LIS2DUX12 sensor documentation",
        url: None,
    },
];

const COMMUNITY_CREDITS: &[CreditEntry] = &[
    CreditEntry {
        name: "ZeptoBars / BarsMonster",
        details: "Precision timing, frequency correction, temperature compensation, RTC investigations, power profiling, and UltraPatch",
        url: None,
    },
    CreditEntry {
        name: "Tahnok",
        details: "Watch faces, framework work, background tasks, testing, and simulator discussions",
        url: None,
    },
    CreditEntry {
        name: "WJHRDY",
        details: "Wyoscan face and low-energy animation work",
        url: None,
    },
    CreditEntry {
        name: "Neutralinsomniac",
        details: "Smallchess/chess face and engine integration",
        url: None,
    },
    CreditEntry {
        name: "Austen Adler / austenadler",
        details: "Early Rust integration experiments",
        url: None,
    },
    CreditEntry {
        name: "Wesleyac",
        details: "Link-time optimization and review/merge assistance",
        url: None,
    },
    CreditEntry {
        name: "Matheus Moreira",
        details: "Feature integration, structured TOTP, deadline/USB/clock work, testing coordination, and preserved attribution",
        url: None,
    },
    CreditEntry {
        name: "Voloved / Devolov / devolov",
        details: "DST/UTZ, sunrise/sunset, step-count, quiet-hours, LED, battery, and display work",
        url: None,
    },
    CreditEntry {
        name: "Krzysztof Gałka / kshysztof",
        details: "Debounce and hardware button testing",
        url: None,
    },
    CreditEntry {
        name: "Atax1a",
        details: "Hardware testing, silicon-errata work, and development support",
        url: None,
    },
    CreditEntry {
        name: "Osresearch / Trammell Hudson",
        details: "MicroPython porting and power-analysis investigations",
        url: None,
    },
    CreditEntry {
        name: "Alessandro Genova / alesgenova",
        details: "Counter32, fast stopwatch, optical communications, UltraPatch integration, location faces, and sensor work",
        url: None,
    },
    CreditEntry {
        name: "Ruben Sandwich",
        details: "Custom display, step-count experimentation, and hardware testing",
        url: None,
    },
    CreditEntry {
        name: "knrd",
        details: "Step-count algorithms and benchmark testing",
        url: None,
    },
    CreditEntry {
        name: "Gabor / Gugray / eiriksm / soundblaster",
        details: "Chirpy/Fesk acoustic communications, receiver tools, tone selection, and protocol testing",
        url: None,
    },
    CreditEntry {
        name: "Jim di Griz",
        details: "Battery-drain and low-voltage investigations",
        url: None,
    },
    CreditEntry {
        name: "Faldor20",
        details: "Dive-computer and pressure-sensor experiments",
        url: None,
    },
    CreditEntry {
        name: "Nima Kalantar",
        details: "Prayer-times face work",
        url: None,
    },
    CreditEntry {
        name: "Ucodia",
        details: "Flowtime face",
        url: None,
    },
    CreditEntry {
        name: "Ganapati",
        details: "Custom faces and metronome work",
        url: None,
    },
    CreditEntry {
        name: "Aron Hegedus",
        details: "Sea Shanty face",
        url: None,
    },
    CreditEntry {
        name: "Alessandro and community testers",
        details: "Dynamic tunes, hourly chimes, and acoustic-transfer experiments",
        url: None,
    },
    CreditEntry {
        name: "James / wryun",
        details: "Calculator, builder, and simulator/tooling discussions",
        url: None,
    },
    CreditEntry {
        name: "Jeremy",
        details: "Custom-display simulator and build integration",
        url: None,
    },
    CreditEntry {
        name: "Fgergo, Crim, Jack, Alexis Philip, Michael Shriver, Benny Blue, Monican, Cyberdeath, and Agent-E11",
        details: "Faces, TOTP/HOTP, display mappings, Rust/Zig experiments, builders, documentation, and review",
        url: None,
    },
];

const TOOL_CREDITS: &[CreditEntry] = &[
    CreditEntry {
        name: "sensor-watch-ir-tools and community IrDA work",
        details: "Optical flashing tools and integrations",
        url: None,
    },
    CreditEntry {
        name: "UltraPatch and detools",
        details: "Small in-place Cortex-M update research",
        url: None,
    },
    CreditEntry {
        name: "ChirpyRX and Fesk",
        details: "Acoustic data-transfer receiver prototypes",
        url: None,
    },
    CreditEntry {
        name: "edbg, OpenOCD, GDB, J-Link, Raspberry Pi Debug Probe, and SWD",
        details: "Debugging and flashing workflows",
        url: None,
    },
    CreditEntry {
        name: "Nordic Power Profiler Kit 2, Joulescope, and EnergyTrace",
        details: "Bench-current measurement workflows",
        url: None,
    },
    CreditEntry {
        name: "Emscripten and custom-LCD tooling",
        details: "Browser simulator and display tooling",
        url: None,
    },
    CreditEntry {
        name: "LittleFS and USB mass-storage experiments",
        details: "Embedded storage and host filesystem utilities",
        url: None,
    },
    CreditEntry {
        name: "utz, gossamer, smallchesslib, nanopb/protobuf, and embedded references",
        details: "Libraries, protocols, and reference projects informing the community work",
        url: None,
    },
    CreditEntry {
        name: "egui / eframe",
        details: "Rust GUI framework used for Studio",
        url: Some("https://github.com/emilk/egui"),
    },
    CreditEntry {
        name: "resvg / usvg",
        details: "SVG rendering libraries used to draw the watch face",
        url: Some("https://github.com/RazrFalcon/resvg"),
    },
    CreditEntry {
        name: "sysinfo",
        details: "System resource usage library",
        url: Some("https://github.com/GuillaumeGomez/sysinfo"),
    },
    CreditEntry {
        name: "Casio F-91W simulator",
        details: "Online F-91W replica by Alexis Philip, used for the SVG",
        url: Some("https://github.com/alexisphilip/Casio-F-91W"),
    },
];

const CREDIT_GROUPS: &[CreditGroup] = &[
    CreditGroup {
        name: "Upstream projects and maintainers",
        entries: UPSTREAM_CREDITS,
    },
    CreditGroup {
        name: "Named community contributors",
        entries: COMMUNITY_CREDITS,
    },
    CreditGroup {
        name: "Community tools and integrations",
        entries: TOOL_CREDITS,
    },
];

fn credit_matches(entry: &CreditEntry, query: &str) -> bool {
    let query = query.to_lowercase();
    query.is_empty()
        || entry.name.to_lowercase().contains(&query)
        || entry.details.to_lowercase().contains(&query)
}

impl Default for StudioApp {
    fn default() -> Self {
        let bootstrap_preferences = persist::load_runtime_preferences();
        // Shared atomic for the stats sampler's live rate.
        let stats_rate_shared = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1000));
        let watch = CasioF91W::new();
        let (sim_year, sim_month, sim_day, sim_hour, sim_minute, _, sim_weekday) = watch.get_time();
        let mut app = StudioApp {
            fonts_installed: false,
            current_panel: Panel::Dashboard,
            status: String::new(),
            package_status: distribution::active(),
            face_list: Vec::new(),
            building: false,
            shutting_down: false,
            pending_build: None,
            cached_watch: WatchDriveSelection::None,
            pending_detection: None,
            pending_flash: None,
            current_progress: None,
            next_operation_id: 1,
            flash_worker_state: flash::WorkerState::Idle,
            build_message: String::new(),
            approved_artifact: initial_flashable_uf2(),
            artifact_path_input: String::new(),
            pending_artifact: None,
            pending_artifact_fingerprint: None,
            pending_build_fingerprint: None,
            // Default to English and Dark.
            language: Language::English,
            theme: Theme::Dark,
            applied_theme: None,
            applied_text_size: None,
            log: debug::DebugLog::new(),
            tick_verbosity: debug::TickVerbosity::Hide,
            tick_log: debug::DebugLog::new(),
            watch,
            sim_last_tick: std::time::Instant::now(),
            watch_renderer: watch_display::WatchRenderer::new(),
            sim_scale: 0.5,
            presets: PresetManager::new(),
            selected_face: None,
            selected_preset_face: None,
            drag_preset_from: None,
            drag_catalog_face: None,
            new_preset_name: String::new(),
            editor_name: String::new(),
            editor_source: String::new(),
            editor_description: String::new(),
            editor_template: 0,
            block_editor: block_editor::BlockEditor::default(),
            ntp_server: 0,
            ntp_servers: Vec::new(),
            ntp_edit_name: String::new(),
            ntp_edit_host: String::new(),
            ntp_edit_index: None,
            ntp_time: None,
            ntp_ping: 0.0,
            ntp_offset: 0.0,
            ntp_busy: false,
            pending_ntp: None,
            release_sha256: None,
            checksum_busy: false,
            pending_checksum: None,
            watch_config: watch_config::WatchConfig::default(),
            modules: modules::ModuleManager::default(),
            output_dir: settings::default_output_dir(),
            pending_data_folder: bootstrap_preferences.data_folder.clone(),
            data_folder_status: String::new(),
            module_name: String::new(),
            module_target: String::new(),
            module_description: String::new(),
            help_anchors: AnchorRegistry::default(),
            help_frame: 0,
            help_open: None,
            help_step: 0,
            help_pending_panel: None,
            help_minimized: false,

            help_card_generation: 0,
            help_auto_opened: false,
            tour_claims: TourClaims::default(),
            sys_stats: sysstats::SysStats::default(),
            stats_rate_ms: 1000,
            stats_rate_shared: stats_rate_shared.clone(),
            sys_rx: sysstats::spawn_sampler(stats_rate_shared),
            text_size: 1,
            tab_layout: settings::TabLayoutMode::default(),
            tab_overflow: settings::TabOverflowBehavior::default(),
            catalog_width: 0.0,
            preset_height: 0.0,
            last_window_size: egui::Vec2::ZERO,
            last_build_time: None,
            build_count: 0,
            build_history: Vec::new(),
            build_log: debug::DebugLog::new(),
            flash_log: debug::DebugLog::new(),
            error_log: debug::DebugLog::new(),
            panic_fingerprint_input: String::new(),
            catalog_error_search: String::new(),
            catalog_error_area: String::from("All"),
            credits_search: String::new(),
            panic_resolution: String::new(),
            faces_log: debug::DebugLog::new(),
            sim_log: debug::DebugLog::new(),
            catalog_search: String::new(),
            catalog_category: String::new(),
            file_browser: file_browser::FileBrowser::new(),
            board: Board::Green,
            component_profiles: components::default_profiles(),
            component_profile: 0,
            component_draft: components::selected_config(&components::default_profiles(), 0),
            component_effective: components::selected_config(&components::default_profiles(), 0),
            pending_component_conflict: None,
            sim_face_idx: 0,
            sim_year,
            sim_month,
            sim_day,
            sim_hour,
            sim_minute,
            sim_weekday: sim_weekday as usize,
            btn_l_down: false,
            btn_c_down: false,
            btn_a_down: false,
            btn_l_hold: 0.0,
            btn_c_hold: 0.0,
            btn_a_hold: 0.0,
            btn_l_events: real_face::ButtonEventState::default(),
            btn_a_events: real_face::ButtonEventState::default(),
            sim_dt: 0.0,
            held_button: None,
            sim_pointer_primary_down: false,
            blocked_until_pointer_release: false,
            face_engine: face_sim::FaceEngine::new("SIMPLE_CLOCK"),
            real_face: None,
            last_render_used_real: false,
            active_real_face_name: None,
            active_real_mode_24: None,
            face_tick_accum: 0.0,
            drift_session: drift::DriftSession::new(),
            rtc_calibration: settings::RtcCalibrationSettings::default(),
            fuzz_iterations: 5000,
            terminal_input: String::new(),
            terminal_open: false,
            terminal_wrap: false,
            terminal_history: Vec::new(),
            shell_input: String::new(),
            shell_log: debug::DebugLog::new(),
            shell_hw_log: debug::DebugLog::new(),
            diagnostics: diagnostics::DiagnosticsState::new(),
            transport_mode: transport::TransportMode::Simulated,
            serial_ports: transport::discover_ports().unwrap_or_default(),
            selected_serial_port: None,
            uart: None,
            last_uart_error: None,
            latest_commit: None,
            latest_sha: None,
            update_time: None,
            update_checking: false,
            pending_update: None,
            beep_armed: false,
            beep_target: 0,
            pending_confirm: None,
            code_view: None,
            fuzz_test_result: None,
            first_run: true,
            welcome_minimized: false,
            persist_user_changes: true,
            reset_test_session_on_compile: true,
            fresh_test_executable_profile: true,
            saved_on_exit: false,
            wiki: wiki::Wiki::new(),
            line_limit: settings::default_line_limit(),
            restore_store: restore::RestoreStore::load(),
            restore_name: String::new(),
            advanced_mode: false,
            panel_ux_overrides: std::collections::BTreeMap::new(),
            ux_override_panel: 0,
            advanced_mode_confirm: false,
            master_clock_path: String::new(),
            master_clock_process: None,
            probe_report: None,
            pending_probe: None,
            probe_progress_rx: None,
            probe_progress: None,
        };
        app.log.log("Firmware Studio starting");
        // Existing output files are inspection/recovery artifacts, not flashable
        // session state. A UF2 becomes flashable only after this process builds it.
        app.face_list = faces::discover_faces();
        app.log
            .log(format!("Discovered {} watch faces", app.face_list.len()));
        // Load active-profile settings, then apply launch preferences from the
        // unscoped bootstrap file so a new debug executable cannot revert them.
        if let Some(saved) = persist::load() {
            app.apply_settings(saved);
            app.log.log("Loaded persisted settings");
        }
        if app.first_run {
            // Preserve the beginner path for a new or legacy profile.
            app.block_editor.set_blocks_mode(true);
        }
        app.apply_bootstrap_preferences(&bootstrap_preferences);
        // Auto-fetch the time from the default NTP server (Cloudflare) on launch.
        app.fetch_ntp();
        // Check for updates on launch.
        app.check_for_updates();
        app.status = tr(app.language, Key::Ready).to_string();
        if let Some(startup_status) = update::startup_status() {
            app.status = startup_status.to_owned();
            app.package_status.warnings.push(startup_status.to_owned());
        }
        app
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sim_pointer_primary_down = ctx.input(|input| input.pointer.primary_down());

        // Install a CJK font once so Chinese text renders instead of empty boxes.
        if !self.fonts_installed {
            fonts::install(ctx);
            self.fonts_installed = true;
        }

        // Theme and text styles are context-wide settings. Reapplying them on
        // every frame rebuilds egui state and can trigger unnecessary repaint
        // work, so only update them when the user changes the setting.
        if self.applied_theme != Some(self.theme) {
            self.theme.apply(ctx);
            self.applied_theme = Some(self.theme);
        }
        if self.applied_text_size != Some(self.text_size) {
            let scale = match self.text_size {
                0 => 0.85,
                2 => 1.2,
                _ => 1.0,
            };
            ctx.style_mut(|s| {
                s.text_styles = std::collections::BTreeMap::from([
                    (
                        egui::TextStyle::Small,
                        egui::FontId::proportional(11.0 * scale),
                    ),
                    (
                        egui::TextStyle::Body,
                        egui::FontId::proportional(14.0 * scale),
                    ),
                    (
                        egui::TextStyle::Button,
                        egui::FontId::proportional(14.0 * scale),
                    ),
                    (
                        egui::TextStyle::Heading,
                        egui::FontId::proportional(20.0 * scale),
                    ),
                    (
                        egui::TextStyle::Monospace,
                        egui::FontId::monospace(13.0 * scale),
                    ),
                ]);
            });
            self.applied_text_size = Some(self.text_size);
        }

        // Only schedule polling while work can complete asynchronously. Static
        // tabs now sleep until input/OS events; the simulator and calibration
        // request their own timing cadence below.
        let background_work = self.pending_build.is_some()
            || self.pending_ntp.is_some()
            || self.pending_checksum.is_some()
            || self.pending_update.is_some()
            || self.pending_probe.is_some()
            || self.pending_detection.is_some()
            || self.pending_flash.is_some()
            || self.beep_armed;
        if background_work {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        // Detect a window resize (or the very first frame) and reset the Watch
        // Faces panel ratios so they re-derive proportionally to the new size
        // instead of going stale.
        let window_size = ctx.screen_rect().size();
        if self.last_window_size != window_size {
            self.last_window_size = window_size;
            self.catalog_width = 0.0;
            self.preset_height = 0.0;
        }

        // Escape pauses help first, so normal shortcuts cannot consume it.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.help_open.is_some() {
            self.minimize_help();
            ctx.request_repaint();
            return;
        }
        self.help_frame = self.help_frame.wrapping_add(1);
        self.help_anchors.begin_frame(self.help_frame);

        // Keyboard shortcuts (only when the user isn't typing in a text field).
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
                self.current_panel = Panel::BuildFlash;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F6))
                && !self.shutting_down
                && !self.building
                && self.pending_build.is_none()
            {
                self.start_build();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F7)) {
                self.current_panel = Panel::Simulator;
            }
        }

        // Do not let the UI close while a worker can still touch project files
        // or return state to this app. eframe drops the JoinHandle on shutdown;
        // cancelling the close request lets the worker be collected normally on
        // a later frame instead of racing process teardown.
        if ctx.input(|i| i.viewport().close_requested()) {
            let active_workers = self.pending_build.is_some()
                || self.pending_ntp.is_some()
                || self.pending_checksum.is_some()
                || self.pending_update.is_some()
                || self.pending_probe.is_some()
                || self.pending_detection.is_some()
                || self.pending_flash.is_some();
            if active_workers {
                if let Some(mut process) = self.master_clock_process.take() {
                    process.terminate();
                    self.status = "Master Clock terminated while Studio was closing".into();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.status = "Finish background work before closing".to_string();
                self.log
                    .log("Close postponed while background work is active");
            } else if !self.saved_on_exit {
                // Persist settings exactly once when the window is about to
                // close, so presets, watch settings, and panel sizes survive.
                self.saved_on_exit = true;
                self.shutting_down = true;
                self.save_settings_internal();
                self.save_bootstrap_preferences();
            }
        }

        self.poll_master_clock();
        self.poll_flash_workers();
        self.poll_probe_worker();
        self.invalidate_stale_artifact();

        // If a build finished, collect its result.
        if let Some(handle) = self.pending_build.take() {
            if handle.is_finished() {
                let build_fingerprint = self.pending_build_fingerprint.take();
                match handle.join() {
                    Ok(result) => {
                        self.building = false;
                        self.build_message = result.message.clone();
                        self.log.log(&result.message);
                        self.build_log.log(&result.message);
                        if result.success {
                            let current_fingerprint = self.build_configuration_fingerprint();
                            match (build_fingerprint, verified_artifact_after_build(&result)) {
                                (Some(build_fingerprint), Ok(inspection))
                                    if build_fingerprint == current_fingerprint =>
                                {
                                    set_verified_artifact_state(
                                        &mut self.status,
                                        &mut self.build_message,
                                        &mut self.approved_artifact,
                                        &mut self.pending_artifact,
                                        inspection,
                                        false,
                                    );
                                    self.pending_artifact_fingerprint = Some(current_fingerprint);
                                    self.last_build_time = Some(
                                        std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_secs())
                                            .unwrap_or(0),
                                    );
                                    self.build_count += 1;
                                    if let Some(t) = self.last_build_time {
                                        self.build_history.push(t);
                                        if self.build_history.len() > 50 {
                                            self.build_history.remove(0);
                                        }
                                    }
                                    if let Some(inspection) = &self.pending_artifact {
                                        self.log.log(format!(
                                            "UF2 verified and awaiting approval: {}",
                                            inspection.path.display()
                                        ));
                                        self.build_log.log(format!(
                                            "UF2 verified and awaiting approval: {}",
                                            inspection.path.display()
                                        ));
                                        self.push_terminal(
                                            "Artifact verified locally. Explicit approval required",
                                        );
                                    }
                                }
                                (Some(_), Ok(_)) | (None, Ok(_)) => {
                                    self.pending_artifact = None;
                                    self.pending_artifact_fingerprint = None;
                                    self.status = "Build configuration changed. Artifact discarded"
                                        .to_string();
                                    self.build_message = self.status.clone();
                                }
                                (_, Err(error)) => {
                                    self.pending_artifact = None;
                                    self.pending_artifact_fingerprint = None;
                                    self.status = "Build verification failed".to_string();
                                    self.build_message = format!(
                                        "Built artifact rejected during verification: {error}"
                                    );
                                    self.push_terminal(
                                        "Output write finished: artifact verification failed",
                                    );
                                }
                            }
                        } else {
                            self.status = tr(self.language, Key::BuildFailed).to_string();
                            // start_build normally clears this before spawning, but keep
                            // failed completion fail-closed if that invariant changes.
                            self.pending_artifact = None;
                            self.pending_artifact_fingerprint = None;
                            self.push_terminal(format!(
                                "Build/Output write failed: {}",
                                result.message
                            ));
                        }
                    }
                    Err(_) => {
                        self.building = false;
                        self.build_message =
                            tr(self.language, Key::BuildThreadPanicked).to_string();
                        self.log.log("Build thread panicked");
                        self.push_terminal("Build/Output write failed: thread panicked");
                    }
                }
            } else {
                // Not done yet; put it back.
                self.pending_build = Some(handle);
            }
        }

        // If an NTP query finished, collect its result.
        if let Some(handle) = self.pending_ntp.take() {
            if handle.is_finished() {
                match handle.join() {
                    Ok(Ok(result)) => {
                        self.ntp_busy = false;
                        self.ntp_time = Some(result.unix_seconds);
                        self.ntp_ping = result.ping_ms;
                        self.ntp_offset = result.offset_secs;
                        self.status = "NTP time fetched".to_string();
                        self.log.log(format!(
                            "NTP time: {} (ping {:.1} ms)",
                            result.unix_seconds, result.ping_ms
                        ));
                    }
                    Ok(Err(e)) => {
                        self.ntp_busy = false;
                        invalidate_ntp_reference(
                            &mut self.ntp_time,
                            &mut self.ntp_ping,
                            &mut self.ntp_offset,
                        );
                        self.beep_armed = false;
                        self.status = format!("NTP error: {e}");
                        self.log_error(&format!("NTP error: {e}"));
                    }
                    Err(_) => {
                        self.ntp_busy = false;
                        invalidate_ntp_reference(
                            &mut self.ntp_time,
                            &mut self.ntp_ping,
                            &mut self.ntp_offset,
                        );
                        self.beep_armed = false;
                        self.status = "NTP thread panicked".to_string();
                    }
                }
            } else {
                self.pending_ntp = Some(handle);
            }
        }

        // If a release checksum fetch finished, collect its result.
        if let Some(handle) = self.pending_checksum.take() {
            if handle.is_finished() {
                self.checksum_busy = false;
                match handle.join() {
                    Ok(Ok(sha)) => {
                        if !is_valid_sha256(&sha) {
                            self.status = "Checksum unavailable: invalid format".to_string();
                            self.log_error("Checksum unavailable: invalid format");
                            self.push_terminal("Integrity check failed: invalid checksum format");
                            return;
                        }
                        let short = sha.chars().take(12).collect::<String>();
                        self.release_sha256 = Some(sha.to_ascii_lowercase());
                        self.status = "Release checksum fetched".to_string();
                        self.log.log("Integrity check: release checksum fetched");
                        self.push_terminal(format!(
                            "Integrity check: release checksum fetched ({})",
                            short
                        ));
                    }
                    Ok(Err(e)) => {
                        self.status = format!("Checksum unavailable: {e}");
                        self.log_error(&format!("Checksum unavailable: {e}"));
                        self.log.log(format!("Integrity check failed: {e}"));
                        self.push_terminal(format!("Integrity check failed: {e}"));
                    }
                    Err(_) => {
                        self.status = "Checksum thread panicked".to_string();
                        self.log.log("Integrity check thread panicked");
                        self.push_terminal("Integrity check thread panicked");
                    }
                }
            } else {
                self.pending_checksum = Some(handle);
            }
        }

        // If an update check finished, collect its result.
        if let Some(handle) = self.pending_update.take() {
            if handle.is_finished() {
                self.finish_update_check(handle.join());
            } else {
                self.pending_update = Some(handle);
            }
        }

        // Top navigation bar. Keep the title/update controls separate from the
        // tab layout so scrolling owns the full available tab width.
        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Clicking the title opens the project's GitHub repo.
                let title = ui.add(
                    egui::Label::new(
                        egui::RichText::new(tr(self.language, Key::AppTitle)).strong(),
                    )
                    .sense(egui::Sense::click()),
                );
                if title.clicked() {
                    let _ = webbrowser::open("https://github.com/kaiiuen/sensor-watch-rs");
                }
                ui.separator();
                if self.advanced_mode {
                    ui.colored_label(egui::Color32::from_rgb(230, 170, 70), "ADVANCED")
                        .on_hover_text("Advanced developer controls are visible");
                    if ui.small_button("Normal mode").clicked() {
                        self.advanced_mode = false;
                        self.current_panel = Panel::Dashboard;
                        self.save_settings_internal();
                    }
                    ui.separator();
                }
                // Update status and retry control stay in the control row, never
                // beside or inside the scrollable tab strip.
                if self.update_checking {
                    ui.spinner();
                    ui.label("Checking for updates…");
                } else if let Some(commit) = &self.latest_commit {
                    match (
                        commits_match(
                            local_commit_sha(),
                            self.latest_sha.as_deref().unwrap_or_default(),
                        ),
                        self.latest_sha.as_deref(),
                    ) {
                        (Some(true), Some(remote)) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(120, 190, 140),
                                format!("Up to date ({})", short_sha(remote)),
                            );
                        }
                        (Some(false), Some(remote)) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(120, 180, 240),
                                format!("Update available ({}): {commit}", short_sha(remote)),
                            );
                        }
                        _ => {
                            ui.label(format!("Latest commit: {commit}"));
                        }
                    }
                }
                if ui
                    .small_button(if self.update_checking {
                        "Checking…"
                    } else {
                        "Check for updates"
                    })
                    .on_hover_text("Check GitHub without blocking Studio")
                    .clicked()
                {
                    self.check_for_updates();
                }
            });
            ui.separator();
            self.tab_bar(ui);
            ui.horizontal(|ui| {
                let label = format!("? Help: {}", self.current_panel.label(self.language));
                let response = ui
                    .button(label)
                    .on_hover_text("Open the beginner walkthrough for this panel");
                self.register_anchor(self.current_panel, AnchorId::PanelHelp, &response);
                if response.clicked() && contextual_help_allowed(self.first_run) {
                    // Welcome owns the first-run surface; never open a second
                    // contextual overlay beneath it.
                    self.open_help_for(self.current_panel, false);
                }
            });
        });

        // Status bar at the bottom.
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            // Drain any pending system resource samples.
            while let Ok(s) = self.sys_rx.try_recv() {
                self.sys_stats = s;
            }
            ui.horizontal(|ui| {
                // Watch project stats (based on the selected faces in the preset).
                let selected = self.presets.active_faces().len();
                ui.label("Watch:");
                ui.monospace(format!("{selected} faces selected"));
                ui.separator();
                ui.monospace(
                    "Build estimates unavailable: configuration input contract incomplete",
                )
                .on_hover_text(build::CONFIGURATION_BUILD_BLOCKED);
                ui.separator();
                // Window size.
                let size = ctx.screen_rect().size();
                ui.monospace(format!("Window: {:.0}x{:.0}", size.x, size.y));
                ui.separator();
                ui.monospace(self.package_status.display_label()).on_hover_text(
                    "Packaged mode uses only the distribution manifest. Developer checkout mode requires explicit developer mode.",
                );
                ui.separator();
                // Error/warning counter that jumps to the Bugs tab.
                let err_count = self.error_log.entries().len();
                let err_color = if err_count > 0 {
                    egui::Color32::from_rgb(200, 70, 70)
                } else {
                    ui.visuals().text_color()
                };
                let errors_text = if err_count > 0 {
                    egui::RichText::new(format!("Errors: {err_count}"))
                        .color(err_color)
                        .strong()
                } else {
                    egui::RichText::new(format!("Errors: {err_count}"))
                };
                if ui
                    .button(errors_text)
                    .on_hover_text("Open the Bugs tab to see recorded errors and warnings.")
                    .clicked()
                {
                    self.current_panel = Panel::Bugs;
                }
                ui.separator();
                // The status label turns red when it indicates a failure so
                // errors stand out at a glance.
                let lower = self.status.to_lowercase();
                let is_failure = lower.contains("fail")
                    || lower.contains("error")
                    || lower.contains("err")
                    || lower.contains("not found")
                    || lower.contains("panicked");
                if is_failure {
                    ui.colored_label(egui::Color32::from_rgb(200, 70, 70), &self.status);
                } else {
                    ui.label(&self.status);
                }
            });
        });

        // Terminal panel (collapsible) above the footer. It is intentionally
        // hidden in Normal mode because it exposes protocol and transport tools.
        if self.advanced_mode && self.panel_ux(self.current_panel).developer_tool_visibility {
            egui::TopBottomPanel::bottom("terminal")
                .resizable(true)
                .default_height(140.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.terminal_open,
                                if self.terminal_open {
                                    "Terminal ▼"
                                } else {
                                    "Terminal ▲"
                                },
                            )
                            .clicked()
                        {
                            self.terminal_open = !self.terminal_open;
                        }
                        if ui.small_button("Clear").clicked() {
                            self.terminal_history.clear();
                        }
                        if ui.small_button("Copy all").clicked() {
                            let text = self.terminal_history.join("\n");
                            let _ = ui_copy_to_clipboard(&text);
                        }
                        if ui.small_button("Export").clicked() {
                            let text = self.terminal_history.join("\n");
                            match self.export_text_file("terminal.log", text) {
                                Ok(path) => {
                                    self.status =
                                        format!("Terminal exported to {}", path.display());
                                }
                                Err(error) => {
                                    self.status = format!("Terminal export failed: {error}");
                                    self.log_error(&format!("Terminal export failed: {error}"));
                                }
                            }
                        }
                        if ui
                            .selectable_label(
                                self.terminal_wrap,
                                if self.terminal_wrap {
                                    "Wrap"
                                } else {
                                    "No wrap"
                                },
                            )
                            .clicked()
                        {
                            self.terminal_wrap = !self.terminal_wrap;
                        }
                        ui.label("Ticks:");
                        self.tick_filter_ui(ui, "terminal_tick_filter");
                        ui.weak("(max 500)");
                    });
                    if self.terminal_open {
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .stick_to_bottom(true)
                            .max_height(120.0)
                            .show(ui, |ui| {
                                for line in &self.terminal_history {
                                    if !self.show_main_event(line) {
                                        continue;
                                    }
                                    if self.terminal_wrap {
                                        ui.label(line);
                                    } else {
                                        ui.monospace(line);
                                    }
                                }
                            });
                        self.tick_log_ui(ui, "terminal_tick_log");
                        ui.horizontal(|ui| {
                            ui.label(">");
                            let resp = ui.text_edit_singleline(&mut self.terminal_input);
                            let submit =
                                resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.button("Run").clicked() || submit {
                                let cmd = self.terminal_input.trim().to_string();
                                self.terminal_input.clear();
                                self.run_terminal_command(&cmd);
                            }
                        });
                    }
                });
        }

        // Keep a hidden advanced panel from remaining selected after returning
        // to Normal mode.
        if !self.advanced_mode
            && matches!(
                self.current_panel,
                Panel::Modules
                    | Panel::Shell
                    | Panel::Diagnostics
                    | Panel::Debug
                    | Panel::Bugs
                    | Panel::FileBrowser
                    | Panel::Probe
            )
        {
            self.current_panel = Panel::Dashboard;
        }

        // Open an auto-tour before drawing the panel so it owns simulator input
        // on its first visible frame. Its target will appear after this frame's
        // anchors are registered; missing anchors use the safe centered card.
        if !self.first_run && self.help_open.is_none() {
            self.maybe_open_help_for(self.current_panel);
        }
        if self.help_owns_input() {
            self.cancel_simulator_buttons();
        }

        // The central panel.
        let was_simulator = self.current_panel == Panel::Simulator;
        egui::CentralPanel::default().show(ctx, |ui| match self.current_panel {
            Panel::Dashboard => self.dashboard(ui),
            Panel::Faces => self.faces(ui, ctx),
            Panel::Editor => self.editor(ui),
            Panel::Simulator => self.simulator(ui, ctx),
            Panel::BuildFlash => self.build_flash(ui),
            Panel::Calibration => self.calibration(ui),
            Panel::Modules => self.modules(ui),
            Panel::Shell => self.shell(ui),
            Panel::Diagnostics => self.diagnostics(ui),
            Panel::Debug => self.debug(ui),
            Panel::Bugs => self.bugs(ui),
            Panel::FileBrowser => self.file_browser(ui),
            Panel::Tutorials => self.tutorials(ui),
            Panel::Wiki => self.wiki(ui),
            Panel::Settings => self.settings(ui),
            Panel::Probe => self.probe(ui),
        });
        self.invalidate_stale_artifact();
        if was_simulator && self.current_panel != Panel::Simulator {
            self.cancel_simulator_buttons();
        }
        self.help_spotlight(ctx);
        self.show_component_conflict(ctx);

        if self.advanced_mode_confirm {
            egui::Window::new("Enable Developer Mode?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Developer Mode exposes development tools and transport views.");
                    ui.label("It never bypasses signature, UF2, drive, UART, shell, path, rollback, or physical-consent safeguards.");
                    ui.label("Simulated actions remain simulated. This mode does not make hardware claims.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Enable Developer Mode").clicked() {
                            self.advanced_mode = true;
                            self.advanced_mode_confirm = false;
                            self.open_tutorial(HelpId::Advanced, false);
                            self.save_settings_internal();
                        }
                        if ui.button("Keep Normal mode").clicked() {
                            self.advanced_mode_confirm = false;
                        }
                    });
                });
        }

        // One-time first-run welcome overlay.
        if self.first_run && !self.welcome_minimized {
            egui::Window::new("Welcome")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.heading("Welcome to Firmware Studio 👋");
                    ui.add_space(6.0);
                    ui.label("Here's how to get started:");
                    ui.add_space(6.0);
                    for step in FIRST_RUN_STEPS {
                        ui.label(step);
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Start beginner tour").clicked() {
                            self.first_run = false;
                            self.block_editor.set_blocks_mode(true);
                            self.open_tutorial(HelpId::Startup, false);
                            self.status = "Beginner tour started: Normal mode is the safe default"
                                .to_string();
                            self.save_settings_internal();
                        }
                        if ui.button("Skip entire startup tour").clicked() {
                            self.first_run = false;
                            self.tour_claims.claim_startup_sequence();
                            self.status =
                                "Startup tour skipped. Reopen any tour with ? Help".to_string();
                            self.save_settings_internal();
                        }
                        if ui.button("Pause").clicked() {
                            self.welcome_minimized = true;
                            self.save_settings_internal();
                        }
                    });
                });
        }
        if self.first_run && self.welcome_minimized {
            egui::Area::new(egui::Id::new("welcome-resume-banner"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(18.0, 18.0))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong("Welcome paused");
                            if ui.button("Resume").clicked() {
                                self.welcome_minimized = false;
                            }
                            if ui.button("Skip entire startup tour").clicked() {
                                self.first_run = false;
                                self.tour_claims.claim_startup_sequence();
                                self.status =
                                    "Startup tour skipped. Reopen any tour with ? Help".to_string();
                                self.save_settings_internal();
                            }
                        });
                    });
                });
        }

        // Confirmation modal for destructive actions. When a destructive button is
        // clicked it sets `pending_confirm` instead of acting immediately; this
        // dialog asks before proceeding and runs the action only on confirm.
        if let Some((message, kind)) = self.pending_confirm.clone() {
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("Confirm")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let compact = !self.panel_ux(self.current_panel).confirmation_verbosity;
                    if compact {
                        ui.label("Confirm this action. It remains subject to all hard safeguards.");
                    } else {
                        ui.label(&message);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        if ui.button("Confirm").clicked() {
                            confirm = true;
                        }
                    });
                });
            if cancel {
                self.pending_confirm = None;
            }
            if confirm {
                self.pending_confirm = None;
                match kind {
                    ConfirmKind::DeletePreset(name) => {
                        self.snapshot_before("Before deleting preset");
                        self.presets.delete_active();
                        self.selected_preset_face = None;
                        self.save_settings_internal();
                        self.faces_log.log(format!("Deleted preset {name}"));
                    }
                    ConfirmKind::DeleteFaceFromPreset(index) => {
                        self.snapshot_before("Before removing face from preset");
                        let face = self
                            .presets
                            .active_faces()
                            .get(index)
                            .cloned()
                            .unwrap_or_default();
                        self.presets.remove_face(index);
                        self.selected_preset_face = None;
                        self.save_settings_internal();
                        self.faces_log
                            .log(format!("Removed {face} from active preset"));
                    }
                    ConfirmKind::DeleteFaceFile(name) => {
                        self.snapshot_before("Before deleting face");
                        match editor::delete_face(&name) {
                            Ok(_) => {
                                let module_result = editor::unregister_face(&name);
                                let removed = self.presets.remove_face_from_all(&name);
                                self.selected_face = None;
                                self.selected_preset_face = None;
                                self.sim_face_idx = 0;
                                self.face_list = faces::discover_faces();
                                self.save_settings_internal();
                                match module_result {
                                    Ok(()) => {
                                        self.status = format!("Deleted face {name}");
                                        self.log.log(format!(
                                            "Deleted face {name}. Removed {removed} preset entries"
                                        ));
                                    }
                                    Err(error) => {
                                        let message = format!(
                                            "Deleted face {name}, but module cleanup failed: {error}"
                                        );
                                        self.status = message.clone();
                                        self.log_error(&message);
                                    }
                                }
                            }
                            Err(error) => {
                                self.status = format!("Delete failed: {error}");
                                self.log_error(&self.status.clone());
                            }
                        }
                    }
                    ConfirmKind::RemoveModule(name) => {
                        self.snapshot_before("Before removing module");
                        self.modules.remove(&name);
                        self.log.log(format!("Removed module {name}"));
                        self.save_settings_internal();
                    }
                    ConfirmKind::ResetTestProfile => {
                        if self.workers_active() {
                            self.status =
                                "Reset unavailable while background work is active".into();
                        } else if test_runtime::active().isolated_debug {
                            self.reset_test_profile();
                        }
                    }
                    ConfirmKind::LaunchMasterClock => {
                        if !self.advanced_mode {
                            self.status = "Master Clock requires Advanced mode".into();
                        } else if self.master_clock_process.is_some() {
                            self.status = "Master Clock is already running".into();
                        } else if let Some(path) = self.master_clock_executable() {
                            match master_clock::MasterClockProcess::launch(&path) {
                                Ok(process) => {
                                    self.master_clock_process = Some(process);
                                    self.status = "Master Clock started; NTP/geolocation network activity is external and Windows time will not change".into();
                                    self.log.log("Master Clock launched on user request");
                                }
                                Err(error) => {
                                    self.status = error;
                                    self.log_error(&self.status.clone());
                                }
                            }
                        }
                    }
                    ConfirmKind::RunPhysicalProbe => {
                        if self.advanced_mode && self.pending_probe.is_none() {
                            let artifact = self
                                .approved_artifact
                                .as_ref()
                                .map(|artifact| artifact.path.clone());
                            let ports = self.serial_ports.clone();
                            let connection_error = self.last_uart_error.clone();
                            let uart = self.uart.take();
                            let (progress_tx, progress_rx) = std::sync::mpsc::channel();
                            let handle = std::thread::spawn(move || {
                                probe::run(
                                    artifact.as_deref(),
                                    &ports,
                                    connection_error.as_deref(),
                                    uart,
                                    move |progress| {
                                        let _ = progress_tx.send(progress);
                                    },
                                )
                            });
                            self.probe_progress = Some(probe::ProbeProgress {
                                completed: 0,
                                total: probe::COMMAND_COUNT,
                                message: "Starting physical probe. Drive count pending".into(),
                            });
                            self.probe_progress_rx = Some(progress_rx);
                            self.pending_probe = Some(handle);
                            self.status = "Physical probe running in background".to_string();
                        } else if self.pending_probe.is_some() {
                            self.status = "A physical probe is already running".to_string();
                        } else {
                            self.status = "Physical probe requires Advanced mode".to_string();
                        }
                    }
                }
            }
        }

        // "View code" popup: shows a face's source read-only in a monospace
        // editor. Opened from the Watch Faces tab context menus.
        if let Some((name, source)) = self.code_view.clone() {
            // Keep the editor buffer in sync with the face being viewed so the
            // TextEdit below shows the current source (update runs every frame).
            if self.editor_name != name {
                self.editor_name = name.clone();
                self.editor_source = source;
            }
            egui::Window::new(format!("View code: {name}"))
                .default_size([560.0, 420.0])
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(&name);
                        ui.separator();
                        if ui.small_button("Close").clicked() {
                            self.code_view = None;
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.editor_source)
                                    .font(egui::TextStyle::Monospace)
                                    .code_editor()
                                    .desired_width(f32::INFINITY),
                            );
                        });
                });
        }

        // "Test before adding" result: fuzzed the face from the context menu.
        if let Some(result) = self.fuzz_test_result.clone() {
            egui::Window::new("Fuzz test result")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(&result);
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        self.fuzz_test_result = None;
                    }
                });
        }
    }
}

impl StudioApp {
    fn request_component_change(
        &mut self,
        board: Board,
        profile: usize,
        draft: components::ComponentsConfig,
        title: String,
    ) {
        let profile_data = self
            .component_profiles
            .get(profile)
            .cloned()
            .unwrap_or_else(|| components::BuildProfile::new("draft", draft.clone()));
        let issues = components::validate_compatibility(board, &profile_data, &draft);
        if issues
            .iter()
            .any(|issue| issue.severity == components::CompatibilitySeverity::Error)
        {
            self.pending_component_conflict = Some(PendingComponentConflict {
                board,
                profile,
                draft,
                title,
                issues,
            });
        } else {
            self.board = board;
            self.component_profile = profile;
            self.component_draft = draft.clone();
            self.component_effective = components::effective_config(board, &profile_data, &draft);
        }
    }

    fn show_component_conflict(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_component_conflict.clone() else {
            return;
        };
        let mut action = None;
        egui::Window::new("Component compatibility review")
            .collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.heading(&pending.title);
                ui.label("The requested configuration is preserved. Choose how the effective configuration should proceed:");
                for finding in &pending.issues {
                    ui.colored_label(egui::Color32::RED, format!("{}: {}: {}", finding.component, finding.reason, finding.suggested_action));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() { action = Some(0); }
                    if ui.button("Keep / review configuration").clicked() { action = Some(1); }
                    if ui.button("Disable incompatible options").clicked() { action = Some(2); }
                });
            });
        if let Some(action) = action {
            if action != 0 {
                self.board = pending.board;
                self.component_profile = pending.profile;
                self.component_draft = pending.draft.clone();
                let profile = self
                    .component_profiles
                    .get(pending.profile)
                    .cloned()
                    .unwrap_or_else(|| {
                        components::BuildProfile::new("draft", pending.draft.clone())
                    });
                let choice = if action == 1 {
                    components::ConflictResolution::KeepRequested
                } else {
                    components::ConflictResolution::DisableIncompatible
                };
                self.component_effective =
                    components::resolve_conflict(choice, pending.board, &profile, &pending.draft)
                        .expect("non-cancel component conflict choice must resolve");
            }
            self.pending_component_conflict = None;
        }
    }

    /// Cancels simulator input when its owner (the current face or tab) goes
    /// away. This is intentionally different from a normal release: no Up or
    /// LongUp event belongs to the replacement face.
    fn cancel_simulator_buttons(&mut self) {
        if self.sim_pointer_primary_down {
            self.blocked_until_pointer_release = true;
        }
        reset_simulator_button_state(
            &mut self.btn_l_down,
            &mut self.btn_c_down,
            &mut self.btn_a_down,
            &mut self.btn_l_hold,
            &mut self.btn_c_hold,
            &mut self.btn_a_hold,
            &mut self.btn_l_events,
            &mut self.btn_a_events,
            &mut self.held_button,
        );
        self.watch.light = false;
        self.watch.set_casio(false);
    }

    fn export_text_file(
        &self,
        filename: &str,
        contents: impl AsRef<[u8]>,
    ) -> Result<std::path::PathBuf, String> {
        let directory = std::path::PathBuf::from(&self.output_dir);
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("could not create export directory: {error}"))?;
        let path = directory.join(filename);
        std::fs::write(&path, contents)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
        Ok(path)
    }

    fn reset_help_card_position(&mut self) {
        self.help_card_generation = self.help_card_generation.wrapping_add(1);
    }

    fn open_tutorial(&mut self, id: HelpId, auto: bool) {
        self.help_open = Some(id);
        self.help_step = 0;
        self.help_pending_panel = panel_for_help_id(help::route(id, 0).panel);
        if let Some(panel) = self.help_pending_panel {
            self.current_panel = panel;
        }
        self.help_minimized = false;
        self.help_auto_opened = auto;
        self.reset_help_card_position();
    }

    fn open_help_for(&mut self, panel: Panel, auto: bool) {
        self.open_tutorial(panel.help_id(), auto);
    }

    fn maybe_open_help_for(&mut self, panel: Panel) {
        let id = panel.help_id();
        if self.help_open.is_none() && !self.tour_claims.contains(id) {
            self.open_help_for(panel, true);
        }
    }

    fn panel_ux(&self, panel: Panel) -> settings::PanelUxOverrides {
        self.panel_ux_overrides
            .get(panel.key())
            .cloned()
            .unwrap_or_default()
    }

    fn help_owns_input(&self) -> bool {
        let barrier = panel_for_help_id(self.help_open.unwrap_or(HelpId::Dashboard))
            .map(|panel| self.panel_ux(panel).tutorial_input_barrier)
            .unwrap_or(true);
        self.help_open.is_some()
            && barrier
            && (!self.help_minimized || self.help_pending_panel.is_some())
    }

    fn minimize_help(&mut self) {
        if self.help_open.is_some() && !self.help_minimized {
            self.help_minimized = true;
            self.cancel_simulator_buttons();
        }
    }

    fn close_help(&mut self, claim: bool) {
        if claim {
            if let Some(id) = self.help_open {
                if id == HelpId::Startup {
                    // Startup Skip/Finish means skip the entire startup sequence,
                    // including every contextual auto-tour.
                    self.tour_claims.claim_startup_sequence();
                } else {
                    self.tour_claims.claim(id);
                }
            }
        }
        self.cancel_simulator_buttons();
        self.help_open = None;
        self.help_step = 0;
        self.help_pending_panel = None;
        self.help_minimized = false;
        self.help_auto_opened = false;
        self.reset_help_card_position();
        self.save_settings_internal();
    }

    /// Foreground guided-help layer. Tint and spotlight are painter-only; the
    /// movable card is the only tutorial-owned UI. Missing or cross-panel
    /// anchors deliberately use an informational card.
    fn help_spotlight(&mut self, ctx: &egui::Context) {
        let Some(id) = self.help_open else { return };
        let target_panel =
            panel_for_help_id(help::route(id, self.help_step).panel).unwrap_or(self.current_panel);
        let card_active =
            self.help_open.is_some() && (!self.help_minimized || self.help_pending_panel.is_some());
        if !card_active || self.current_panel != target_panel {
            let label = if self.help_minimized {
                format!("Tour paused: {}", help::tutorial(id).title)
            } else {
                format!(
                    "Tour paused: return to {}",
                    target_panel.label(self.language)
                )
            };
            let card_id = egui::Id::new(("help-card", self.help_card_generation));
            let mut action = None;
            egui::Area::new(card_id)
                .order(HELP_CARD_LAYER_ORDER)
                .default_pos(egui::pos2(18.0, 18.0))
                .movable(true)
                .constrain(true)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.strong(label);
                        ui.horizontal(|ui| {
                            if ui.button("Resume").clicked() {
                                action = Some("resume");
                            }
                            if ui.button("Skip").clicked() {
                                action = Some("skip");
                            }
                            if ui.button("Close").clicked() {
                                action = Some("close");
                            }
                        });
                    });
                });
            match action {
                Some("resume") => {
                    self.help_minimized = false;
                    if self.current_panel != target_panel {
                        self.help_pending_panel = Some(target_panel);
                        self.current_panel = target_panel;
                    }
                    ctx.request_repaint();
                }
                Some("skip") | Some("close") => self.close_help(true),
                _ => {}
            }
            return;
        }
        if let Some(panel) = self.help_pending_panel {
            if self.current_panel == panel {
                self.help_pending_panel = None;
                ctx.request_repaint();
            }
            return;
        }
        let tutorial = help::tutorial(id);
        let index = help::step_index(id, self.help_step);
        self.help_step = index;
        let step = tutorial.steps[index];
        let target = help::step_target(&self.help_anchors, self.current_panel.help_id(), id, index);
        let target_available = target.is_some();
        let screen = ctx.screen_rect();
        let viewport = (screen.width(), screen.height());
        let target_local = target.map(|rect| AnchorRect {
            min: (rect.min.0 - screen.min.x, rect.min.1 - screen.min.y),
            max: (rect.max.0 - screen.min.x, rect.max.1 - screen.min.y),
        });
        let spotlight_target = self
            .panel_ux(self.current_panel)
            .tutorial_spotlight
            .then(|| target_local.map(|rect| rect.expand(8.0)))
            .flatten();
        let card = help::place_card(
            target_local,
            (
                viewport.0.min(560.0).max(240.0),
                viewport.1.min(220.0).max(1.0),
            ),
            viewport,
            16.0,
        );
        let card_id = egui::Id::new(("help-card", self.help_card_generation));
        // A moved card has an existing area rect; on its first frame use the
        // calculated placement. Both are screen-space rectangles, matching the
        // painter geometry below.
        let initial_card_rect = egui::Rect::from_min_size(
            screen.min + egui::vec2(card.min.0, card.min.1),
            egui::vec2(card.size.0, card.size.1),
        );
        let _card_rect = ctx
            .memory(|memory| memory.area_rect(card_id))
            .unwrap_or(initial_card_rect);

        let mut action: Option<&'static str> = None;
        if target_available {
            let tint = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 150);
            let lower_layer =
                egui::LayerId::new(HELP_DIM_LAYER_ORDER, egui::Id::new("help-dim-painter"));
            let painter = ctx.layer_painter(lower_layer);
            for region in
                help::absolute_dim_regions((screen.min.x, screen.min.y), viewport, spotlight_target)
            {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(region.min.0, region.min.1),
                        egui::pos2(region.max.0, region.max.1),
                    ),
                    0.0,
                    tint,
                );
            }
            if let Some(rect) = spotlight_target {
                let r = egui::Rect::from_min_max(
                    screen.min + egui::vec2(rect.min.0, rect.min.1),
                    screen.min + egui::vec2(rect.max.0, rect.max.1),
                );
                painter.rect_stroke(
                    r,
                    5.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 210, 80)),
                );
            }
        }
        // The card is the only interactive tutorial layer.
        egui::Area::new(card_id)
            .order(HELP_CARD_LAYER_ORDER)
            .default_pos(screen.min + egui::vec2(card.min.0, card.min.1))
            .movable(true)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(card.size.0);
                    ui.set_max_width(card.size.0);
                    egui::ScrollArea::vertical()
                        .max_height(card.size.1 - 70.0)
                        .show(ui, |ui| {
                            ui.heading(tutorial.title);
                            ui.strong(step.title);
                            ui.label(step.body);
                            if target_available {
                                ui.weak(step.instruction(id, index));
                            } else {
                                ui.weak("This target is unavailable in the current state. No action is required; continue when ready.");
                            }
                        });
                    ui.label(format!("Step {} of {}", index + 1, tutorial.steps.len()));

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(index > 0, egui::Button::new("Back"))
                            .clicked()
                        {
                            self.help_step = help::previous_index(id, index);
                            self.reset_help_card_position();
                        }
                        let last = index + 1 == tutorial.steps.len();
                        if ui.button(if last { "Finish" } else { "Next" }).clicked() {
                            action = Some(if last { "finish" } else { "next" });
                        }
                        if ui.button("Pause").clicked() {
                            action = Some("pause");
                        }
                        if ui.button("Skip").clicked() {
                            action = Some("skip");
                        }
                    });
                });
            });
        match action {
            Some("next") => {
                self.help_step = help::next_index(id, index);
                self.reset_help_card_position();
                let wanted = help::route(id, self.help_step);
                if let Some(panel) =
                    help::pending_navigation(id, wanted).and_then(panel_for_help_id)
                {
                    self.help_pending_panel = Some(panel);
                    self.help_anchors
                        .begin_frame(self.help_frame.wrapping_add(1));
                    self.current_panel = panel;
                    self.reset_help_card_position();
                }
            }
            Some("finish") => self.close_help(true),
            Some("pause") => self.minimize_help(),
            Some("skip") => self.close_help(true),
            _ => {}
        }
    }

    fn guided_action_allowed(&self, action: AnchorId) -> bool {
        help::action_allowed(self.help_open.is_some(), action)
    }

    fn unsafe_action_allowed(&self) -> bool {
        self.help_open.is_none()
    }

    fn register_anchor(&mut self, panel: Panel, key: AnchorId, response: &egui::Response) {
        self.register_anchor_rect(panel, key, response.rect);
    }

    fn register_anchor_rect(&mut self, panel: Panel, key: AnchorId, rect: egui::Rect) {
        self.help_anchors.register(
            panel.help_id(),
            key.key(),
            AnchorRect {
                min: (rect.min.x, rect.min.y),
                max: (rect.max.x, rect.max.y),
            },
        );
    }

    /// Draws the top-level tabs without allowing a narrow window to create a
    /// second, full-width horizontal scrollbar. Estimated widths are based on
    /// the localized label so rows remain stable while the window is resized.
    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let panels = [
            Panel::Dashboard,
            Panel::Faces,
            Panel::Editor,
            Panel::Simulator,
            Panel::BuildFlash,
            Panel::Calibration,
            Panel::Modules,
            Panel::Shell,
            Panel::Diagnostics,
            Panel::Debug,
            Panel::Bugs,
            Panel::FileBrowser,
            Panel::Tutorials,
            Panel::Wiki,
            Panel::Settings,
            Panel::Probe,
        ];
        let visible: Vec<Panel> = panels
            .into_iter()
            .filter(|panel| {
                (self.advanced_mode && self.panel_ux(*panel).advanced_tab_visibility)
                    || !matches!(
                        panel,
                        Panel::Modules
                            | Panel::Shell
                            | Panel::Diagnostics
                            | Panel::Debug
                            | Panel::Bugs
                            | Panel::FileBrowser
                            | Panel::Probe
                    )
            })
            .collect();

        let labels: Vec<String> = visible
            .iter()
            .map(|panel| panel.label(self.language).to_string())
            .collect();
        let widths: Vec<f32> = labels
            .iter()
            .map(|label| (label.chars().count() as f32 * 7.5 + 30.0).max(54.0))
            .collect();
        let available = ui.available_width().max(1.0);
        let total: f32 = widths.iter().sum();
        let natural_rows = if total <= available {
            1
        } else if self.advanced_mode && total > available * 2.0 {
            3
        } else {
            2
        };
        let preferred_rows = match self.tab_layout {
            settings::TabLayoutMode::Auto => natural_rows,
            settings::TabLayoutMode::OneRow => 1,
            settings::TabLayoutMode::TwoRows => 2,
            settings::TabLayoutMode::ThreeRows => 3,
        };

        if self.tab_overflow == settings::TabOverflowBehavior::HorizontalScroll {
            egui::ScrollArea::horizontal()
                .id_source("nav_tabs_scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| self.draw_tab_buttons(ui, &visible));
                });
            return;
        }

        // A one-row preference still falls back to wrapping when required,
        // clipping is never preferable to honoring the selected tab.
        let row_count = preferred_rows
            .max(natural_rows)
            .max(1)
            .min(visible.len().max(1));
        let mut rows: Vec<Vec<Panel>> = vec![Vec::new(); row_count];
        let mut row_widths = vec![0.0; row_count];
        for (index, panel) in visible.iter().enumerate() {
            // Explicit two/three-row settings must actually distribute tabs,
            // even when the total label width would fit on one line.
            let preferred_row = index * row_count / visible.len().max(1);
            let row = if row_widths[preferred_row] == 0.0
                || row_widths[preferred_row] + widths[index] <= available
            {
                preferred_row
            } else {
                row_widths
                    .iter()
                    .enumerate()
                    .min_by(|(_, left), (_, right)| left.total_cmp(right))
                    .map(|(row, _)| row)
                    .unwrap_or(preferred_row)
            };
            rows[row].push(*panel);
            row_widths[row] += widths[index];
        }
        for (row_index, panels) in rows.into_iter().enumerate() {
            if panels.is_empty() {
                continue;
            }
            egui::ScrollArea::horizontal()
                .id_source(format!("nav_tabs_row_{row_index}"))
                .show(ui, |ui| {
                    ui.horizontal(|ui| self.draw_tab_buttons(ui, &panels));
                });
        }
    }

    fn draw_tab_buttons(&mut self, ui: &mut egui::Ui, panels: &[Panel]) {
        for panel in panels {
            self.draw_tab_button(ui, *panel);
        }
    }

    fn draw_tab_button(&mut self, ui: &mut egui::Ui, panel: Panel) {
        let return_target = self.help_open.and_then(panel_for_help_id) == Some(panel)
            && self.current_panel != panel;
        let response = if return_target {
            ui.scope(|ui| {
                ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::from_rgb(120, 90, 35);
                ui.visuals_mut().widgets.hovered.bg_fill = egui::Color32::from_rgb(160, 120, 45);
                ui.selectable_label(false, format!("↩ {}", panel.label(self.language)))
            })
            .inner
        } else {
            ui.selectable_label(self.current_panel == panel, panel.label(self.language))
        };
        self.register_anchor(panel, AnchorId::PanelNavigation, &response);
        if response.clicked() {
            if self.current_panel != panel {
                self.log
                    .log(format!("Switched to panel {}", panel.label(self.language)));
            }
            self.current_panel = panel;
            // Manual panel switching pauses the active tour. Its HelpId and
            // step remain intact; returning renders fresh anchors before resume.
            if self.help_open.is_some() {
                self.minimize_help();
                self.help_anchors.begin_frame(self.help_frame);
            }
        }
    }

    /// The dashboard: an overview of the project, health, and NTP time.
    fn dashboard(&mut self, ui: &mut egui::Ui) {
        // The dashboard is long; wrap it in a scroll area so it never clips.
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.dashboard_body(ui);
            });
    }

    /// The scrollable body of the dashboard.
    fn dashboard_body(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr(self.language, Key::Dashboard));
        ui.separator();

        // Current date/time from the OS, used to sync the watch face.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let days = now.div_euclid(86400);
        let secs = now.rem_euclid(86400);
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        let dow = ((days + 4).rem_euclid(7)) as usize;
        let weekday = [
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ][dow];
        let (year, month, day) = watch_sim::civil_from_days(days);
        ui.monospace(format!(
            "{weekday}, {year:04}-{month:02}-{day:02}  {h:02}:{m:02}:{s:02}"
        ));
        ui.add_space(8.0);

        // Target board selection.
        let board_response = ui.horizontal(|ui| {
            ui.label("Target board:");
            for b in [Board::Green, Board::RedLite, Board::Blue, Board::Pro] {
                if ui.selectable_label(self.board == b, b.label()).clicked() && self.board != b {
                    self.request_component_change(
                        b,
                        self.component_profile,
                        self.component_draft.clone(),
                        format!("Switch target board to {}", b.label()),
                    );
                    self.log
                        .log(format!("Target board change requested: {}", b.label()));
                }
            }
        });
        let board_response = board_response.response;
        self.register_anchor(Panel::Dashboard, AnchorId::DashboardBoard, &board_response);
        let _ = board_response.on_hover_text(
            "Select which Sensor Watch board revision you're building/flashing for.\n\
             Different boards (Green, Red/Lite, Blue, Pro) have different LED\n\
             polarity, buzzer wiring, and optional sensors. The build and flash\n\
             steps use this selection.",
        );
        ui.add_space(8.0);

        ui.label(tr(self.language, Key::Target));
        ui.label(
            tr(self.language, Key::FlashRam).replace("{faces}", &self.face_list.len().to_string()),
        );
        ui.add_space(8.0);
        if let Some(uf2) = self
            .approved_artifact
            .as_ref()
            .map(|artifact| &artifact.path)
        {
            ui.label(
                tr(self.language, Key::LastBuild).replace("{path}", &uf2.display().to_string()),
            );
            if let Some(t) = self.last_build_time {
                let secs = (t as i64).rem_euclid(86400);
                let h = (secs / 3600) % 24;
                let m = (secs / 60) % 60;
                ui.monospace(format!(
                    "Last build: {h:02}:{m:02}  ({} builds this session)",
                    self.build_count
                ));
            }
            // Build history log.
            if !self.build_history.is_empty() {
                ui.collapsing("Build history", |ui| {
                    for (i, t) in self.build_history.iter().enumerate() {
                        let secs = (*t as i64).rem_euclid(86400);
                        let h = (secs / 3600) % 24;
                        let m = (secs / 60) % 60;
                        let s = secs % 60;
                        ui.monospace(format!("#{:02}  {h:02}:{m:02}:{s:02}", i + 1));
                    }
                });
            }
        } else {
            ui.label(tr(self.language, Key::NoBuildYet));
        }

        ui.add_space(16.0);
        ui.separator();
        egui::CollapsingHeader::new("NTP Time")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Select a server and fetch the current time. Add your own servers below.")
                    .on_hover_text(
                        "Network Time Protocol (NTP) synchronizes the watch's clock to an\n\
                     atomic time source over the internet. Pick a server, press Fetch,\n\
                     and the app shows the exact UTC time plus the network latency.",
                    );
                ui.add_space(4.0);

                // Build the full server list: built-in + custom.
                let mut all_servers: Vec<(String, String)> = ntp::SERVERS
                    .iter()
                    .map(|(n, h)| (n.to_string(), h.to_string()))
                    .collect();
                all_servers.extend(self.ntp_servers.iter().cloned());
                if self.ntp_server >= all_servers.len() {
                    self.ntp_server = 0;
                }

                // Server selection.
                ui.horizontal(|ui| {
                    ui.label("Server:");
                    egui::ComboBox::from_id_source("ntp_server")
                        .selected_text(&all_servers[self.ntp_server].0)
                        .show_ui(ui, |ui| {
                            for (i, (name, _)) in all_servers.iter().enumerate() {
                                ui.selectable_value(&mut self.ntp_server, i, name);
                            }
                        });
                    let response = ui.button("Fetch time");
                    self.register_anchor(Panel::Dashboard, AnchorId::DashboardNtpFetch, &response);
                    if response.clicked() {
                        self.fetch_ntp();
                    }
                });

                // Custom server management.
                ui.add_space(4.0);
                ui.collapsing("Manage custom servers", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.ntp_edit_name);
                        ui.label("Host:");
                        ui.text_edit_singleline(&mut self.ntp_edit_host);
                        let editing = self.ntp_edit_index.is_some();
                        if ui.button(if editing { "Save" } else { "Add" }).clicked() {
                            let name = self.ntp_edit_name.trim().to_string();
                            let host = self.ntp_edit_host.trim().to_string();
                            if name.is_empty() || host.is_empty() {
                                self.status = "NTP name and host are required".to_string();
                            } else if let Some(index) = self.ntp_edit_index {
                                if let Some(server) = self.ntp_servers.get_mut(index) {
                                    *server = (name, host);
                                    self.status = "Custom NTP server updated".to_string();
                                    self.log.log("Updated custom NTP server");
                                    self.ntp_edit_index = None;
                                    self.ntp_edit_name.clear();
                                    self.ntp_edit_host.clear();
                                    self.save_settings_internal();
                                } else {
                                    self.ntp_edit_index = None;
                                    self.status = "NTP server no longer exists".to_string();
                                }
                            } else if self.ntp_servers.len() >= MAX_CUSTOM_NTP_SERVERS {
                                self.status = format!(
                                    "Custom NTP server limit reached ({MAX_CUSTOM_NTP_SERVERS})"
                                );
                            } else {
                                self.ntp_servers.push((name, host));
                                self.ntp_edit_name.clear();
                                self.ntp_edit_host.clear();
                                self.log.log("Added custom NTP server");
                                self.save_settings_internal();
                            }
                        }
                        if editing && ui.button("Cancel").clicked() {
                            self.ntp_edit_index = None;
                            self.ntp_edit_name.clear();
                            self.ntp_edit_host.clear();
                        }
                    });
                    // List custom servers with edit/delete.
                    let mut to_delete: Option<usize> = None;
                    for (i, (name, host)) in self.ntp_servers.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.monospace(format!("{name}  ({host})"));
                            if ui.small_button("Edit").clicked() {
                                self.ntp_edit_name = name.clone();
                                self.ntp_edit_host = host.clone();
                                self.ntp_edit_index = Some(i);
                            }
                            if ui.small_button("Del").clicked() {
                                to_delete = Some(i);
                            }
                        });
                    }
                    if let Some(i) = to_delete {
                        if i < self.ntp_servers.len() {
                            let absolute_index = ntp::SERVERS.len() + i;
                            self.ntp_servers.remove(i);
                            if self.ntp_edit_index == Some(i) {
                                self.ntp_edit_index = None;
                                self.ntp_edit_name.clear();
                                self.ntp_edit_host.clear();
                            }
                            self.ntp_server =
                                selection_after_custom_ntp_removal(self.ntp_server, absolute_index);
                            self.log.log("Removed custom NTP server");
                            self.save_settings_internal();
                        }
                    }
                });

                // Show the fetched time.
                if let Some(ts) = self.ntp_time {
                    let secs = ts as i64;
                    let rem = secs.rem_euclid(86400);
                    let h = (rem / 3600) % 24;
                    let m = (rem / 60) % 60;
                    let s = rem % 60;
                    // Use the same Sunday-first weekday convention as the
                    // firmware and the local dashboard clock.
                    let dow = ntp::weekday_from_unix_seconds(ts) as usize;
                    let weekday = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][dow];
                    ui.add_space(8.0);
                    ui.monospace(format!(
                        "{weekday}  {:02}:{:02}:{:02} UTC   (ping {:.1} ms, offset {:+.2} ms)",
                        h,
                        m,
                        s,
                        self.ntp_ping,
                        self.ntp_offset * 1000.0
                    ));
                } else if self.ntp_busy {
                    ui.add_space(8.0);
                    ui.spinner();
                    ui.label("Fetching...");
                } else {
                    ui.add_space(8.0);
                    ui.weak("No time fetched yet.");
                }

                // Server info table.
                ui.add_space(8.0);
                ui.collapsing("Server list", |ui| {
                    egui::Grid::new("ntp_server_list")
                        .striped(true)
                        .spacing([16.0, 2.0])
                        .num_columns(3)
                        .show(ui, |ui| {
                            ui.strong("#");
                            ui.strong("Name");
                            ui.strong("Host");
                            ui.end_row();
                            for (i, (name, host)) in all_servers.iter().enumerate() {
                                ui.monospace(i.to_string());
                                ui.label(name);
                                ui.monospace(host);
                                ui.end_row();
                            }
                        });
                });
            });

        // Clock calibration: compute the next-minute-boundary timestamp from the
        // NTP time and generate a `settime` command for the serial shell.
        ui.add_space(16.0);
        ui.separator();
        egui::CollapsingHeader::new("Clock Calibration")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    "Generate a precise set-time command. The watch's serial shell accepts\n\
                 `settime YYMMDDHHMMSS`; send it at the exact minute boundary.",
                )
                .on_hover_text(
                    "The watch's RTC drifts slightly over time. To calibrate it precisely:\n\
                 1. Fetch the NTP time above.\n\
                 2. This generates a `settime` command for the exact next minute.\n\
                 3. Send it to the watch's serial shell at that moment.\n\n\
                 NOTE: Over USB the watch appears as a file drive (UF2 bootloader), not\n\
                 a serial port. The serial shell is used over the debug UART pins, or\n\
                 via the Studio app when a serial connection is available.",
                );
                ui.add_space(4.0);
                if let Some(ts) = self.ntp_time {
                    // Compute the next minute boundary in UTC.
                    let boundary = (ts / 60 + 1) * 60;
                    let b = boundary as i64;
                    let days = b.div_euclid(86400);
                    let rem = b.rem_euclid(86400);
                    let h = (rem / 3600) % 24;
                    let m = (rem / 60) % 60;
                    let s = rem % 60;
                    let (year, month, day) = watch_sim::civil_from_days(days);
                    let cmd = ntp::settime_command(boundary);
                    ui.monospace(format!(
                        "Next minute boundary: {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
                        year, month, day, h, m, s
                    ));
                    ui.monospace(format!("Command: {cmd}"));
                    if ui.button("Copy command").clicked() {
                        let _ = ui_copy_to_clipboard(&cmd);
                        self.status = "Calibration command copied".to_string();
                        self.log.log(format!("Calibration command: {cmd}"));
                    }
                } else {
                    ui.weak("Fetch NTP time first to generate a calibration command.");
                }
            });

        // Drift calibration: measure the watch's drift against NTP over time.
        ui.add_space(16.0);
        ui.separator();
        egui::CollapsingHeader::new("Drift Calibration")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    "Measure the watch's crystal drift (PPM) by recording two samples\n\
                 (watch time vs NTP reference) some time apart.",
                )
                .on_hover_text(
                    "Every watch crystal runs slightly fast or slow (measured in parts-per-\n\
                 million, PPM). To measure yours:\n\
                 1. Fetch the NTP time.\n\
                 2. Press 'Record sample' now.\n\
                 3. Wait hours or days.\n\
                 4. Fetch NTP again and press 'Record sample' again.\n\
                 The app computes the drift, which you can apply as a frequency\n\
                 correction to the RTC.",
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Record sample").clicked() {
                        if let Some(ts) = self.ntp_time {
                            // Use the sim watch's live time as the "watch" reading.
                            let (year, month, day, h, m, s, _) = self.watch.get_time();
                            let watch_secs = (watch_sim::days_from_civil(year, month, day) as u64)
                                .saturating_mul(86_400)
                                .saturating_add((h as u64) * 3_600 + (m as u64) * 60 + s as u64);
                            match self.drift_session.record(watch_secs, ts) {
                                Ok(role) => {
                                    self.save_settings_internal();
                                    self.log.log(format!("Drift sample recorded ({role})"));
                                }
                                Err(error) => {
                                    self.status = error.clone();
                                    self.log_error(&error);
                                }
                            }
                        } else {
                            self.status = "Fetch NTP time first".to_string();
                        }
                    }
                    if ui.button("Reset").clicked() {
                        self.drift_session.reset();
                        self.log.log("Drift session reset");
                    }
                });
                if self.drift_session.ppm != 0.0 {
                    ui.monospace(format!(
                        "Last calibrated drift: {:+.2} ppm (from last session)",
                        self.drift_session.ppm
                    ));
                }
                if self.drift_session.ppm != 0.0 {
                    ui.monospace(format!("Drift: {:+.2} ppm", self.drift_session.ppm));
                    if self.drift_session.ppm.abs() < 0.5 {
                        ui.label("The watch is running accurately (within 0.5 ppm).");
                    } else if self.drift_session.ppm > 0.0 {
                        ui.label("The watch is running FAST; apply a negative correction.");
                    } else {
                        ui.label("The watch is running SLOW; apply a positive correction.");
                    }
                } else if self.drift_session.start.is_some() {
                    ui.weak("Start sample recorded. Record a second sample later.");
                } else {
                    ui.weak("No samples yet.");
                }
            });

        // Fuzz testing: run randomized input through the face engine.
        ui.add_space(16.0);
        ui.separator();
        egui::CollapsingHeader::new("Fuzz Testing")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Run randomized button/tick sequences through a face to check stability.")
                    .on_hover_text(
                        "Fuzzing throws random button presses, ticks, and time changes at a\n\
                     watch face to make sure it never panics or produces a broken\n\
                     display. It's a quick way to find crashes before they happen on\n\
                     your wrist. Higher iterations = more thorough but slower.",
                    );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Iterations:");
                    ui.add(
                        egui::DragValue::new(&mut self.fuzz_iterations).clamp_range(100..=100_000),
                    );
                    if ui.button("Run fuzz").clicked() {
                        let faces = self.presets.active_faces();
                        let name = if faces.is_empty() {
                            "SIMPLE_CLOCK".to_string()
                        } else {
                            faces[self.sim_face_idx.min(faces.len() - 1)].clone()
                        };
                        let iters = self.fuzz_iterations;
                        let seed = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(0);
                        match fuzz::fuzz_face(&name, iters, seed) {
                            Ok(n) => {
                                self.status = format!("Fuzz passed: {n} iterations on {name}");
                                self.log
                                    .log(format!("Fuzz passed: {n} iterations on {name}"));
                            }
                            Err(e) => {
                                self.status = format!("Fuzz failed: {e}");
                                self.log.log(format!("Fuzz failed: {e}"));
                            }
                        }
                    }
                });
            });
    }

    /// Starts exactly one background firmware build.
    fn start_build(&mut self) {
        if !self.unsafe_action_allowed() {
            self.status = "Build is disabled while a guided tour is active".to_string();
            return;
        }
        if self.shutting_down || self.building || self.pending_build.is_some() || self.flash_busy()
        {
            self.push_terminal("Build or flash already running");
            return;
        }

        let profile = self
            .component_profiles
            .get(self.component_profile)
            .cloned()
            .unwrap_or_else(|| {
                components::BuildProfile::new("Custom", self.component_effective.clone())
            });
        let revision = match self.board {
            Board::Green | Board::Blue => "OSO-SWAT-A1-05",
            Board::RedLite => "OSO-SWAT-A1-02",
            Board::Pro => "OSO-FEAL-A1-00",
        };
        let (preset_name, ordered_faces) = self
            .presets
            .presets
            .get(self.presets.active)
            .map(|preset| (preset.name.clone(), preset.faces.clone()))
            .unwrap_or_default();
        let request = firmware_inputs::FirmwareInputRequest {
            board: self.board,
            revision: revision.into(),
            profile,
            components: self.component_effective.clone(),
            preset_name,
            ordered_faces,
            modules: self
                .modules
                .modules
                .iter()
                .filter(|module| module.enabled)
                .map(|module| module.name.clone())
                .collect(),
        };
        if let Err(reason) = build::preflight_request(&request) {
            self.build_message = format!("Build preflight failed: {reason}");
            self.status = "Build unavailable: unsupported or invalid configuration".to_string();
            self.log.log(&self.build_message);
            self.build_log.log(&self.build_message);
            let message = self.build_message.clone();
            self.push_terminal(message);
            return;
        }
        let build_fingerprint = self.build_configuration_fingerprint();
        let out = std::path::PathBuf::from(self.output_dir.clone());
        self.log.log("Starting firmware build");
        self.push_terminal("Output write: starting firmware build");
        if self.reset_test_session_on_compile {
            self.reset_compile_session_state(build_fingerprint);
        } else {
            self.begin_compile_session(build_fingerprint);
        }
        self.pending_build = Some(std::thread::spawn(move || {
            build::build_firmware(request, &out)
        }));
    }

    /// Resets only transient state owned by the next compile session.
    ///
    /// This is deliberately called after build preflight and concurrency checks,
    /// immediately before the worker is spawned. In particular, it does not
    /// create a restore point or touch the output directory.
    fn begin_compile_session(&mut self, build_fingerprint: String) {
        self.pending_build_fingerprint = Some(build_fingerprint);
        self.current_progress = None;
        self.status = tr(self.language, Key::Building).to_string();
        self.build_message = self.status.clone();
        self.building = true;
    }

    fn reset_compile_session_state(&mut self, build_fingerprint: String) {
        self.approved_artifact = None;
        self.pending_artifact = None;
        self.pending_artifact_fingerprint = None;
        self.begin_compile_session(build_fingerprint);
        self.cancel_simulator_buttons();
    }

    fn build_configuration_fingerprint(&self) -> String {
        configuration_fingerprint_with_effective(
            self.board,
            &self.presets,
            &self.watch_config,
            &self.modules,
            &self.component_profiles,
            self.component_profile,
            &self.component_draft,
            &self.component_effective,
            &self.output_dir,
        )
    }

    fn invalidate_stale_artifact(&mut self) {
        let fingerprint = self.build_configuration_fingerprint();
        if invalidate_stale_artifact_state(
            &mut self.approved_artifact,
            &mut self.pending_artifact,
            &mut self.pending_artifact_fingerprint,
            &fingerprint,
        ) {
            self.status = "Artifact discarded: build configuration changed".to_string();
            self.build_message = self.status.clone();
        }
    }

    /// Fetches the current time from the selected NTP server on a background thread.
    fn fetch_ntp(&mut self) {
        if self.ntp_busy {
            return;
        }
        // A refresh replaces the previous reference. Do not allow a failed or
        // in-flight refresh to leave calibration actions using stale time.
        invalidate_ntp_reference(&mut self.ntp_time, &mut self.ntp_ping, &mut self.ntp_offset);
        self.beep_armed = false;
        // Build the combined server list (built-in + custom) and resolve the
        // selected index safely. A stale index from a settings file is clamped.
        let mut all: Vec<String> = ntp::SERVERS.iter().map(|(_, h)| h.to_string()).collect();
        all.extend(self.ntp_servers.iter().map(|(_, h)| h.clone()));
        if all.is_empty() {
            self.status = "No NTP server available".to_string();
            self.ntp_busy = false;
            return;
        }
        let idx = self.ntp_server.min(all.len() - 1);
        let server = all[idx].clone();
        self.ntp_busy = true;
        let handle = std::thread::spawn(move || ntp::query_ntp(&server));
        self.pending_ntp = Some(handle);
    }

    /// Fetches the expected SHA-256 from the GitHub release on a background thread.
    fn fetch_release_checksum(&mut self) {
        if self.checksum_busy {
            return;
        }
        self.checksum_busy = true;
        self.log.log("Integrity check: verifying against release");
        self.push_terminal("Integrity check: verifying against release");
        let handle = std::thread::spawn(fetch_release_sha256);
        self.pending_checksum = Some(handle);
    }

    /// Starts a nonblocking update check unless one is already in flight.
    fn check_for_updates(&mut self) {
        if self.update_checking {
            return;
        }
        self.update_checking = true;
        let handle = std::thread::spawn(fetch_latest_commit);
        self.pending_update = Some(handle);
    }

    /// Applies every worker outcome, including join failures, so the spinner
    /// and cached notification cannot become stale.
    fn finish_update_check(
        &mut self,
        result: std::thread::Result<Result<RemoteCommit, UpdateCheckError>>,
    ) {
        self.update_checking = false;
        self.latest_commit = None;
        self.latest_sha = None;
        self.update_time = None;

        match result {
            Ok(Ok(remote)) => {
                let status = match commits_match(local_commit_sha(), &remote.sha) {
                    Some(true) => "Up to date",
                    Some(false) => "Update available",
                    None => "Latest commit checked (local SHA unavailable)",
                };
                self.latest_commit = Some(remote.message.clone());
                self.latest_sha = Some(remote.sha.clone());
                self.update_time = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                );
                self.status = status.to_string();
                self.log
                    .log(format!("Latest commit {}: {}", remote.sha, remote.message));
            }
            Ok(Err(error)) => {
                self.status = format!("Update check failed: {error}");
                self.log_error(&format!("Update check failed: {error}"));
            }
            Err(_) => {
                self.status = "Update check thread panicked".to_string();
                self.log_error("Update check thread panicked");
            }
        }
    }

    /// The watch-faces panel: catalog (left) and active preset (right), both
    /// as spreadsheets, plus preset management sub-tabs.
    fn faces(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.heading(tr(self.language, Key::WatchFaces));
        ui.separator();

        // Per-tab debug output.
        ui.collapsing("Faces debug log", |ui| {
            ui.horizontal(|ui| {
                if ui.small_button("Clear").clicked() {
                    self.faces_log.clear();
                }
            });
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .max_height(120.0)
                .show(ui, |ui| {
                    if self.faces_log.is_empty() {
                        ui.weak("(no faces activity yet)");
                    }
                    for entry in self.faces_log.entries() {
                        let secs = entry.timestamp % 60;
                        let mins = (entry.timestamp / 60) % 60;
                        let hrs = (entry.timestamp / 3600) % 24;
                        ui.monospace(format!("[{hrs:02}:{mins:02}:{secs:02}] {}", entry.message));
                    }
                });
        });
        ui.separator();
        // Work from a snapshot so preset mutations can be persisted immediately
        // without holding an immutable borrow of the catalog during UI callbacks.
        let catalog_faces = std::mem::take(&mut self.face_list);

        // Preset management: tab selectors, name field, and action buttons on
        // one row to save vertical space.
        ui.horizontal(|ui| {
            ui.label("Presets:");
            let preset_names: Vec<String> = self
                .presets
                .presets
                .iter()
                .map(|preset| preset.name.clone())
                .collect();
            for (i, name) in preset_names.iter().enumerate() {
                if ui
                    .selectable_label(self.presets.active == i, name)
                    .clicked()
                {
                    self.presets.active = i;
                    self.selected_preset_face = None;
                    self.save_settings_internal();
                }
            }
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.new_preset_name).desired_width(150.0));
            let response = ui.button("+");
            self.register_anchor(Panel::Faces, AnchorId::FacesPreset, &response);
            if response
                .on_hover_text("Add a new preset with the typed name")
                .clicked()
            {
                if let Some(name) = preset_name(&self.new_preset_name).map(str::to_owned) {
                    self.presets.add_preset(&name);
                    self.save_settings_internal();
                    self.faces_log.log(format!("Added preset {name}"));
                    self.new_preset_name.clear();
                } else {
                    self.status = "Preset name cannot be empty".to_string();
                }
            }
            if ui
                .button("Rename")
                .on_hover_text("Rename the active preset to the typed name")
                .clicked()
            {
                if let Some(name) = preset_name(&self.new_preset_name).map(str::to_owned) {
                    self.presets.rename_active(&name);
                    self.save_settings_internal();
                    self.new_preset_name.clear();
                } else {
                    self.status = "Preset name cannot be empty".to_string();
                }
            }
            if ui
                .button("Delete")
                .on_hover_text("Delete the active preset and its face list")
                .clicked()
                && self.unsafe_action_allowed()
            {
                let active = self
                    .presets
                    .active
                    .min(self.presets.presets.len().saturating_sub(1));
                let name = self.presets.presets[active].name.clone();
                self.pending_confirm = Some((
                    format!("Delete preset '{name}'? This removes its face list."),
                    ConfirmKind::DeletePreset(name),
                ));
            }
        });
        ui.separator();

        // Left column: catalog (top) and active preset (bottom), stacked.
        let default_catalog = if self.catalog_width > 0.0 {
            self.catalog_width
        } else {
            ui.available_width() * 0.45
        };
        egui::SidePanel::left("catalog")
            .resizable(true)
            .default_width(default_catalog)
            .width_range(180.0..=f32::INFINITY)
            .show_inside(ui, |ui| {
                self.catalog_width = ui.available_width();
                // Catalog on top.
                let default_preset = if self.preset_height > 0.0 {
                    self.preset_height
                } else {
                    ui.available_height() * 0.5
                };
                egui::TopBottomPanel::top("catalog_top")
                    .resizable(true)
                    .default_height(default_preset)
                    .show_inside(ui, |ui| {
                        self.preset_height = ui.available_height();
                        ui.heading("Catalog");
                        // Search and category filter on one row to save space.
                        ui.horizontal(|ui| {
                            ui.label("Search:");
                            let response = ui.text_edit_singleline(&mut self.catalog_search);
                            self.register_anchor(Panel::Faces, AnchorId::FacesSearch, &response);
                            if !self.catalog_search.is_empty() && ui.small_button("x").clicked() {
                                self.catalog_search.clear();
                            }
                            ui.separator();
                            ui.label("Category:");
                            egui::ComboBox::from_id_source("catalog_cat")
                                .selected_text(if self.catalog_category.is_empty() {
                                    "All".to_string()
                                } else {
                                    self.catalog_category.clone()
                                })
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(self.catalog_category.is_empty(), "All")
                                        .clicked()
                                    {
                                        self.catalog_category.clear();
                                    }
                                    for cat in [
                                        "Time",
                                        "Timers & Alarms",
                                        "Games",
                                        "Tools",
                                        "Sensors",
                                        "Astronomy",
                                        "System",
                                        "Other",
                                    ] {
                                        if ui
                                            .selectable_label(self.catalog_category == cat, cat)
                                            .clicked()
                                        {
                                            self.catalog_category = cat.to_string();
                                        }
                                    }
                                });
                        });
                        ui.separator();
                        let query = self.catalog_search.trim().to_lowercase();
                        // Bulk add every face currently passing the search+category
                        // filter to the active preset.
                        let mut filtered: Vec<&str> = Vec::new();
                        for face in &catalog_faces {
                            if !self.catalog_category.is_empty()
                                && face.category != self.catalog_category
                            {
                                continue;
                            }
                            if !query.is_empty()
                                && !face.name.to_lowercase().contains(&query)
                                && !face.index.to_string().contains(&query)
                                && !face.description.to_lowercase().contains(&query)
                            {
                                continue;
                            }
                            filtered.push(&face.name);
                        }
                        if ui
                            .button("Add all filtered to preset")
                            .on_hover_text(format!(
                                "Add the {} faces currently matching the search + category filter",
                                filtered.len()
                            ))
                            .clicked()
                        {
                            if filtered.is_empty() {
                                self.status = "No faces match the filter".to_string();
                                self.faces_log.log("No faces match the filter");
                            } else {
                                let mut added = 0usize;
                                for name in &filtered {
                                    self.presets.add_face(name);
                                    added += 1;
                                }
                                self.save_settings_internal();
                                self.status = format!("Added {added} faces to the active preset");
                                self.faces_log
                                    .log(format!("Added {added} filtered faces to preset"));
                            }
                        }
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                // Spreadsheet-style grid: # | Face | Description | Add.
                                egui::Grid::new("catalog_grid")
                                    .striped(true)
                                    .spacing([12.0, 4.0])
                                    .show(ui, |ui| {
                                        ui.strong("#");
                                        ui.strong("Face");
                                        ui.strong("Description");
                                        ui.strong("Add");
                                        ui.end_row();
                                        for (i, face) in catalog_faces.iter().enumerate() {
                                            // Filter by category.
                                            if !self.catalog_category.is_empty()
                                                && face.category != self.catalog_category
                                            {
                                                continue;
                                            }
                                            // Filter by search query.
                                            if !query.is_empty()
                                                && !face.name.to_lowercase().contains(&query)
                                                && !face.index.to_string().contains(&query)
                                                && !face.description.to_lowercase().contains(&query)
                                            {
                                                continue;
                                            }
                                            let selected = self.selected_face == Some(i);
                                            if ui
                                                .selectable_label(selected, face.index.to_string())
                                                .clicked()
                                            {
                                                self.selected_face = Some(i);
                                            }
                                            let name_resp = ui
                                                .add(
                                                    egui::Label::new(&face.name)
                                                        .sense(egui::Sense::drag()),
                                                )
                                                .on_hover_text(&face.description);
                                            if name_resp.drag_started() {
                                                self.drag_catalog_face = Some(face.name.clone());
                                            }
                                            if name_resp.drag_stopped() {
                                                self.drag_catalog_face = None;
                                            }
                                            // Right-click context menu on a catalog
                                            // face: add to preset, preview in the
                                            // simulator, view source, or fuzz-test it.
                                            if name_resp.context_menu(|ui| {
                                                let face_name = face.name.clone();
                                                let response = ui.button("Add to preset");
                                                self.register_anchor(Panel::Faces, AnchorId::FacesAdd, &response);
                                                if response.clicked() {
                                                    self.presets.add_face(&face_name);
                                                    self.save_settings_internal();
                                                    self.log.log(format!(
                                                        "Added {face_name} to preset"
                                                    ));
                                                    ui.close_menu();
                                                }
                                                if ui.button("Preview").clicked() {
                                                    // Select the face: if it is already in
                                                    // the active preset, jump to its index;
                                                    // otherwise add it first.
                                                    let mut idx = None;
                                                    for (j, f) in self
                                                        .presets
                                                        .active_faces()
                                                        .iter()
                                                        .enumerate()
                                                    {
                                                        if faces::face_identity(f) == faces::face_identity(&face_name) {
                                                            idx = Some(j);
                                                            break;
                                                        }
                                                    }
                                                    if idx.is_none() {
                                                        self.presets.add_face(&face_name);
                                                        self.save_settings_internal();
                                                        let len = self.presets.active_faces().len();
                                                        if len > 0 {
                                                            idx = Some(len - 1);
                                                        }
                                                    }
                                                    if let Some(j) = idx {
                                                        self.sim_face_idx = j;
                                                    }
                                                    self.current_panel = Panel::Simulator;
                                                    self.log.log(format!(
                                                        "Previewing face {face_name}"
                                                    ));
                                                    ui.close_menu();
                                                }
                                                if ui.button("View code").clicked() {
                                                    self.code_view = Some((
                                                        face_name.clone(),
                                                        editor::read_face(&face_name).unwrap_or_else(
                                                            |e| {
                                                                format!("Error reading face: {e}")
                                                            },
                                                        ),
                                                    ));
                                                    ui.close_menu();
                                                }
                                                if ui.button("Test before adding").clicked() {
                                                    let iters = self.fuzz_iterations;
                                                    let seed = std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .map(|d| d.as_nanos() as u64)
                                                        .unwrap_or(0);
                                                    self.fuzz_test_result = Some(match fuzz::fuzz_face(
                                                        &face_name, iters, seed,
                                                    ) {
                                                        Ok(n) => format!(
                                                            "Fuzz passed: {n} iterations on {face_name}"
                                                        ),
                                                        Err(e) => format!("Fuzz failed: {e}"),
                                                    });
                                                    ui.close_menu();
                                                }
                                            }).is_some() {
                                                self.drag_catalog_face = None;
                                            }
                                            ui.label(&face.description);
                                            if ui
                                                .small_button("+")
                                                .on_hover_text("Add this face to the active preset")
                                                .clicked()
                                            {
                                                self.presets.add_face(&face.name);
                                                self.save_settings_internal();
                                                self.log
                                                    .log(format!("Added {} to preset", face.name));
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                    });

                // Active preset on bottom.
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    // Drop target for dragging a catalog face here.
                    if let Some(name) = self.drag_catalog_face.clone() {
                        let drop = ui.add_sized(
                            egui::vec2(ui.available_width(), 24.0),
                            egui::Label::new(
                                egui::RichText::new(format!("Drop {name} here to add")).weak(),
                            ),
                        );
                        if drop.hovered() {
                            self.presets.add_face(&name);
                            self.save_settings_internal();
                            self.drag_catalog_face = None;
                            self.log.log(format!("Added {name} to preset (drag)"));
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.heading("Active Preset");
                        ui.separator();
                        // Add selected catalog face to the preset.
                        if let Some(i) = self.selected_face {
                            if let Some(face) = self.face_list.get(i).map(|face| face.name.clone()) {
                                if ui.button(format!("Add {face}")).clicked() {
                                    self.presets.add_face(&face);
                                    self.save_settings_internal();
                                    self.log.log(format!("Added {face} to preset"));
                                }
                            } else {
                                self.selected_face = None;
                            }
                        }
                    });
                    ui.separator();
                    // Spreadsheet-style grid: # | Face | Up | Dn | Del.
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Grid::new("preset_grid")
                                .striped(true)
                                .spacing([12.0, 4.0])
                                .show(ui, |ui| {
                                    // Header row.
                                    ui.strong("#");
                                    ui.strong("Face");
                                    ui.strong("Up");
                                    ui.strong("Dn");
                                    ui.strong("Del");
                                    ui.end_row();

                                    let faces = self.presets.active_faces();
                                    let mut drop_target: Option<usize> = None;
                                    for (i, face) in faces.iter().enumerate() {
                                        let selected = self.selected_preset_face == Some(i);
                                        if ui
                                            .selectable_label(selected, (i + 1).to_string())
                                            .clicked()
                                        {
                                            self.selected_preset_face = Some(i);
                                        }
                                        // Drag the face name to reorder.
                                        let name_resp = ui
                                            .add(egui::Label::new(face).sense(egui::Sense::drag()));
                                        if name_resp.drag_started() {
                                            self.drag_preset_from = Some(i);
                                        }
                                        if name_resp.drag_stopped() {
                                            self.drag_preset_from = None;
                                        }
                                        if name_resp.hovered()
                                            && self.drag_preset_from.is_some()
                                            && self.drag_preset_from != Some(i)
                                        {
                                            drop_target = Some(i);
                                        }
                                        // Right-click context menu on the active
                                        // preset row: same controls as the row
                                        // buttons plus preview / view code / test.
                                        if name_resp.context_menu(|ui| {
                                            let face = face.clone();
                                            if ui.button("Preview").clicked() {
                                                self.sim_face_idx = i;
                                                self.current_panel = Panel::Simulator;
                                                self.log.log(format!("Previewing face {face}"));
                                                ui.close_menu();
                                            }
                                            if ui.button("View code").clicked() {
                                                self.code_view = Some((
                                                    face.clone(),
                                                    editor::read_face(&face).unwrap_or_else(|e| {
                                                        format!("Error reading face: {e}")
                                                    }),
                                                ));
                                                ui.close_menu();
                                            }
                                            if ui.button("Test before adding").clicked() {
                                                let iters = self.fuzz_iterations;
                                                let seed = std::time::SystemTime::now()
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .map(|d| d.as_nanos() as u64)
                                                    .unwrap_or(0);
                                                self.fuzz_test_result = Some(
                                                    match fuzz::fuzz_face(&face, iters, seed) {
                                                        Ok(n) => format!(
                                                            "Fuzz passed: {n} iterations on {face}"
                                                        ),
                                                        Err(e) => format!("Fuzz failed: {e}"),
                                                    },
                                                );
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button("Move up").clicked() {
                                                self.presets.move_face_up(i);
                                                self.save_settings_internal();
                                                ui.close_menu();
                                            }
                                            if ui.button("Move down").clicked() {
                                                self.presets.move_face_down(i);
                                                self.save_settings_internal();
                                                ui.close_menu();
                                            }
                                            if ui.button("Remove").clicked() && self.unsafe_action_allowed() {
                                                let face = face.clone();
                                                self.pending_confirm = Some((
                                                    format!(
                                                        "Remove '{face}' from the active preset?"
                                                    ),
                                                    ConfirmKind::DeleteFaceFromPreset(i),
                                                ));
                                                ui.close_menu();
                                            }
                                        }).is_some() {
                                            // The context menu opening consumed the
                                            // drag; do not treat it as a reorder.
                                            self.drag_preset_from = None;
                                        }
                                        if ui
                                            .small_button("Up")
                                            .on_hover_text("Move this face up in the preset")
                                            .clicked()
                                        {
                                            self.presets.move_face_up(i);
                                            self.save_settings_internal();
                                        }
                                        if ui
                                            .small_button("Dn")
                                            .on_hover_text("Move this face down in the preset")
                                            .clicked()
                                        {
                                            self.presets.move_face_down(i);
                                            self.save_settings_internal();
                                        }
                                        if ui
                                            .small_button("Del")
                                            .on_hover_text(
                                                "Remove this face from the active preset",
                                            )
                                            .clicked()
                                            && self.unsafe_action_allowed()
                                        {
                                            let face = self.presets.active_faces()[i].clone();
                                            self.pending_confirm = Some((
                                                format!("Remove '{face}' from the active preset?"),
                                                ConfirmKind::DeleteFaceFromPreset(i),
                                            ));
                                        }
                                        ui.end_row();
                                    }
                                    // Apply the drag-and-drop reorder.
                                    if let (Some(from), Some(to)) =
                                        (self.drag_preset_from, drop_target)
                                    {
                                        self.presets.move_face(from, to);
                                        self.save_settings_internal();
                                        self.drag_preset_from = None;
                                    }
                                });
                        });
                });
            });
        self.face_list = catalog_faces;

        // Right column: watch settings.
        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.heading("Watch Settings");
                    ui.label(
                        "Configure the watch firmware (mirrors the on-watch preferences face).",
                    );
                    ui.add_space(8.0);

                    // ---- Time & Display ----
                    ui.strong("Time & Display");
                    ui.separator();
                    egui::Grid::new("watch_settings_grid")
                        .striped(true)
                        .spacing([24.0, 6.0])
                        .num_columns(2)
                        .show(ui, |ui| {
                            // Clock mode.
                            ui.label("Clock mode");
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(self.watch_config.clock_mode_24h, "24-hour")
                                    .clicked()
                                {
                                    self.watch_config.clock_mode_24h = true;
                                }
                                if ui
                                    .selectable_label(!self.watch_config.clock_mode_24h, "12-hour")
                                    .clicked()
                                {
                                    self.watch_config.clock_mode_24h = false;
                                }
                            });
                            ui.end_row();

                            // Leading zero in 24h mode.
                            ui.label("24h leading zero");
                            ui.checkbox(&mut self.watch_config.clock_24h_leading_zero, "");
                            ui.end_row();

                            // Show seconds.
                            ui.label("Show seconds");
                            ui.checkbox(&mut self.watch_config.show_seconds, "");
                            ui.end_row();

                            // Time zone.
                            ui.label("Time zone");
                            ui.horizontal(|ui| {
                                ui.add(egui::Slider::new(&mut self.watch_config.time_zone, 0..=40));
                                let off = watch_config::TIMEZONE_OFFSETS
                                    .get(self.watch_config.time_zone as usize)
                                    .copied()
                                    .unwrap_or(0);
                                let sign = if off < 0 { '-' } else { '+' };
                                let abs = off.unsigned_abs();
                                ui.label(format!("UTC{sign}{:02}:{:02}", abs / 60, abs % 60));
                            });
                            // Country dropdown for quick timezone selection.
                            ui.horizontal(|ui| {
                                ui.label("Country:");
                                egui::ComboBox::from_id_source("country_tz")
                                    .selected_text(country_label(self.watch_config.time_zone))
                                    .show_ui(ui, |ui| {
                                        for (i, name) in COUNTRIES.iter().enumerate() {
                                            if ui
                                                .selectable_value(
                                                    &mut self.watch_config.time_zone,
                                                    i as u8,
                                                    *name,
                                                )
                                                .changed()
                                            {
                                                self.log.log(format!(
                                                    "Time zone set to {name} (index {i})"
                                                ));
                                            }
                                        }
                                    });
                            });
                            ui.end_row();

                            // Imperial units.
                            ui.label("Imperial units");
                            ui.checkbox(&mut self.watch_config.use_imperial_units, "");
                            ui.end_row();

                            // Auto DST.
                            ui.label("Auto DST");
                            ui.checkbox(
                                &mut self.watch_config.auto_dst,
                                "Apply daylight-saving time automatically",
                            );
                            ui.end_row();
                        });

                    ui.add_space(12.0);
                    // ---- Sound & Buzzer ----
                    ui.strong("Sound & Buzzer");
                    ui.separator();
                    egui::Grid::new("watch_sound_grid")
                        .striped(true)
                        .spacing([24.0, 6.0])
                        .num_columns(2)
                        .show(ui, |ui| {
                            // Button sound.
                            ui.label("Button sound");
                            ui.checkbox(&mut self.watch_config.button_should_sound, "");
                            ui.end_row();

                            // Button volume.
                            ui.label("Button volume");
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(!self.watch_config.button_volume, "Soft")
                                    .on_hover_text(volume_description(false))
                                    .clicked()
                                {
                                    self.watch_config.button_volume = false;
                                }
                                if ui
                                    .selectable_label(self.watch_config.button_volume, "Loud")
                                    .on_hover_text(volume_description(true))
                                    .clicked()
                                {
                                    self.watch_config.button_volume = true;
                                }
                                ui.small(volume_description(self.watch_config.button_volume));
                            });
                            ui.end_row();

                            // Signal volume.
                            ui.label("Signal volume");
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(!self.watch_config.signal_volume, "Soft")
                                    .on_hover_text(volume_description(false))
                                    .clicked()
                                {
                                    self.watch_config.signal_volume = false;
                                }
                                if ui
                                    .selectable_label(self.watch_config.signal_volume, "Loud")
                                    .on_hover_text(volume_description(true))
                                    .clicked()
                                {
                                    self.watch_config.signal_volume = true;
                                }
                                ui.small(volume_description(self.watch_config.signal_volume));
                            });
                            ui.end_row();

                            // Alarm volume.
                            ui.label("Alarm volume");
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(!self.watch_config.alarm_volume, "Soft")
                                    .on_hover_text(volume_description(false))
                                    .clicked()
                                {
                                    self.watch_config.alarm_volume = false;
                                }
                                if ui
                                    .selectable_label(self.watch_config.alarm_volume, "Loud")
                                    .on_hover_text(volume_description(true))
                                    .clicked()
                                {
                                    self.watch_config.alarm_volume = true;
                                }
                                ui.small(volume_description(self.watch_config.alarm_volume));
                            });
                            ui.end_row();

                            // Piezo voltage (advanced).
                            ui.label("Piezo voltage");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Slider::new(
                                        &mut self.watch_config.piezo_voltage,
                                        0.0..=9.0,
                                    )
                                    .step_by(0.1),
                                );
                                ui.label(format!("{:.1} V", self.watch_config.piezo_voltage));
                            });
                            ui.end_row();
                            ui.label("Sound level guide");
                            ui.label("Hardware limit: 0.0-9.0 V. Soft is a gentle tap; Loud is a stronger knock. Actual dB varies with piezo, case, battery, and distance.");
                            ui.end_row();
                            ui.end_row();

                            // Buzzer sound type.
                            ui.label("Buzzer sound");
                            ui.horizontal(|ui| {
                                for (v, label) in
                                    [(0u8, "Click"), (1, "Beep"), (2, "Double"), (3, "Chime")]
                                {
                                    if ui
                                        .selectable_label(self.watch_config.buzzer_type == v, label)
                                        .clicked()
                                    {
                                        self.watch_config.buzzer_type = v;
                                        self.log.log(format!("Buzzer sound set to {label}"));
                                    }
                                }
                            });
                            ui.end_row();
                        });

                    ui.add_space(12.0);
                    // ---- LED / Backlight ----
                    ui.strong("LED / Backlight");
                    ui.separator();
                    egui::Grid::new("watch_led_grid")
                        .striped(true)
                        .spacing([24.0, 6.0])
                        .num_columns(2)
                        .show(ui, |ui| {
                            // LED duration.
                            ui.label("LED duration");
                            ui.horizontal(|ui| {
                                ui.add(egui::Slider::new(
                                    &mut self.watch_config.led_duration,
                                    0..=7,
                                ));
                                ui.label(match self.watch_config.led_duration {
                                    0 => "(only while pressed)".to_string(),
                                    7 => "(off)".to_string(),
                                    d => format!("({} seconds)", d * 2 - 1),
                                });
                            });
                            ui.end_row();

                            // LED red color.
                            ui.label("LED red color");
                            ui.add(egui::Slider::new(
                                &mut self.watch_config.led_red_color,
                                0..=15,
                            ));
                            ui.end_row();

                            // LED green color.
                            ui.label("LED green color");
                            ui.add(egui::Slider::new(
                                &mut self.watch_config.led_green_color,
                                0..=15,
                            ));
                            ui.end_row();

                            // LED color hex.
                            ui.label("LED color (hex)");
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut self.watch_config.led_color_hex);
                                if let Some(col) = parse_hex_color(&self.watch_config.led_color_hex)
                                {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(24.0, 16.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect, 2.0, col);
                                    // Color picker.
                                    let mut c = [
                                        col.r() as f32 / 255.0,
                                        col.g() as f32 / 255.0,
                                        col.b() as f32 / 255.0,
                                    ];
                                    if ui.color_edit_button_rgb(&mut c).changed() {
                                        self.watch_config.led_color_hex = format!(
                                            "#{:02x}{:02x}{:02x}",
                                            (c[0] * 255.0) as u8,
                                            (c[1] * 255.0) as u8,
                                            (c[2] * 255.0) as u8
                                        );
                                    }
                                }
                            });
                            ui.end_row();

                            // LED gradient toggle.
                            ui.label("LED gradient");
                            ui.checkbox(
                                &mut self.watch_config.led_gradient,
                                "Use a color gradient",
                            );
                            ui.end_row();

                            // LED gradient color.
                            ui.label("Gradient color");
                            color_picker_row(ui, &mut self.watch_config.led_gradient_hex, "gradient_color");
                            ui.end_row();

                            // Night light color.
                            ui.label("Night light");
                            ui.checkbox(
                                &mut self.watch_config.night_light_red,
                                "Use the separate night color",
                            );
                            ui.end_row();
                            ui.label("Night color");
                            color_picker_row(ui, &mut self.watch_config.night_light_color_hex, "night_light_color");
                            ui.end_row();
                        });

                    ui.add_space(12.0);
                    // ---- Power & Motion ----
                    ui.strong("Power & Motion");
                    ui.separator();
                    egui::Grid::new("watch_power_grid")
                        .striped(true)
                        .spacing([24.0, 6.0])
                        .num_columns(2)
                        .show(ui, |ui| {
                            // Timeout interval.
                            ui.label("Timeout interval");
                            ui.horizontal(|ui| {
                                ui.add(egui::Slider::new(
                                    &mut self.watch_config.to_interval,
                                    0..=3,
                                ));
                                ui.label(match self.watch_config.to_interval {
                                    0 => "(60 sec)".to_string(),
                                    1 => "(2 min)".to_string(),
                                    2 => "(5 min)".to_string(),
                                    _ => "(30 min)".to_string(),
                                });
                            });
                            ui.end_row();

                            // Low energy interval.
                            ui.label("Low energy interval");
                            ui.horizontal(|ui| {
                                ui.add(egui::Slider::new(
                                    &mut self.watch_config.le_interval,
                                    0..=7,
                                ));
                                ui.label(match self.watch_config.le_interval {
                                    0 => "(never)".to_string(),
                                    1 => "(10 min)".to_string(),
                                    2 => "(1 hour)".to_string(),
                                    3 => "(2 hour)".to_string(),
                                    4 => "(6 hour)".to_string(),
                                    5 => "(12 hr)".to_string(),
                                    6 => "(1 day)".to_string(),
                                    _ => "(7 day)".to_string(),
                                });
                            });
                            ui.end_row();

                            // Raise to wake.
                            ui.label("Raise to wake");
                            ui.checkbox(&mut self.watch_config.raise_to_wake, "Enabled");
                            ui.end_row();

                            // Raise to wake light.
                            ui.label("Raise-to-wake light");
                            ui.checkbox(
                                &mut self.watch_config.raise_to_wake_light,
                                "Light LED on wake",
                            );
                            ui.end_row();

                            // Light sensor auto-sleep.
                            ui.label("Light sensor sleep");
                            ui.checkbox(
                                &mut self.watch_config.light_sensor_sleep,
                                "Sleep when covered (saves power)",
                            )
                            .on_hover_text(
                                "When the light sensor is covered (e.g. under a sleeve),\n\
                                 the watch disables seconds and waits for light to return.\n\
                                 This saves significant battery at night.",
                            );
                            ui.end_row();

                            // Reset keybind.
                            ui.label("Reset keybind");
                            ui.checkbox(
                                &mut self.watch_config.reset_keybind,
                                "Enable HAL reset keybind",
                            )
                            .on_hover_text(
                                "A keybind that resets the watch at the HAL level, in case\n\
                                 the UI glitches or the panic handler doesn't run.",
                            );
                            ui.end_row();

                            // Temperature offset calibration.
                            ui.label("Temp offset");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Slider::new(
                                        &mut self.watch_config.temp_offset,
                                        -10.0..=10.0,
                                    )
                                    .step_by(0.1),
                                );
                                ui.label(format!("{:.1} C", self.watch_config.temp_offset));
                            })
                            .response
                            .on_hover_text(
                                "Adjusts the temperature reading to match a reference\n\
                                 thermometer when everything is at rest.",
                            );
                            ui.end_row();

                            // Alarm enabled.
                            ui.label("Alarm enabled");
                            ui.checkbox(&mut self.watch_config.alarm_enabled, "");
                            ui.end_row();
                        });

                    // Show the packed firmware settings register.
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("Packed settings register:");
                        ui.monospace(format!("0x{:08X}", self.watch_config.to_reg()));
                        if ui.button("Copy").clicked() {
                            let _ = ui_copy_to_clipboard(&format!(
                                "0x{:08X}",
                                self.watch_config.to_reg()
                            ));
                            self.status = "Settings register copied to clipboard".to_string();
                            self.log.log("Settings register copied to clipboard");
                        }
                        if ui.button("Paste").clicked() {
                            if let Ok(text) = ui_paste_from_clipboard() {
                                let trimmed = text.trim().trim_start_matches("0x");
                                if let Ok(reg) = u32::from_str_radix(trimmed, 16) {
                                    self.watch_config = watch_config::WatchConfig::from_reg(reg);
                                    self.status = "Settings register imported".to_string();
                                    self.log.log("Settings register imported");
                                } else {
                                    self.status = "Invalid register value in clipboard".to_string();
                                }
                            }
                        }
                    });
                });
            });
    }

    /// The modules panel
    /// The editor panel: create, edit, or delete watch faces.
    fn editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Editor");
            ui.separator();
            let response = ui.selectable_label(self.block_editor.is_blocks_mode(), "Blocks");
            self.register_anchor(Panel::Editor, AnchorId::EditorMode, &response);
            if response.clicked() {
                self.block_editor.set_blocks_mode(true);
            }
            if ui
                .selectable_label(!self.block_editor.is_blocks_mode(), "Rust")
                .clicked()
            {
                self.block_editor.set_blocks_mode(false);
            }
        });
        if self.block_editor.is_blocks_mode() {
            ui.separator();
            ui.strong("Face identity");
            self.editor_identity(ui);
            self.editor_actions(ui);
            ui.add_space(8.0);
            self.block_editor.show_blocks(ui, &mut self.editor_source);
            let blocks_rect = ui.min_rect();
            self.register_anchor_rect(Panel::Editor, AnchorId::BlocksGenerate, blocks_rect);
            if !self.block_editor.generated_source.is_empty() {
                self.register_anchor_rect(Panel::Editor, AnchorId::LoadIntoRust, blocks_rect);
            }
            return;
        }
        ui.separator();
        ui.label("Create, edit, or delete watch faces.");
        ui.add_space(8.0);

        // Self-IDE help so new users understand the workflow without docs.
        egui::CollapsingHeader::new("How to make a watch face")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    "1. Pick a template below (Simple Clock, Counter, or Blank).\n\
                     2. Type a snake_case name, e.g. my_face.\n\
                     3. Optionally add a description shown to users in the catalog.\n\
                     4. Click \"Generate from template\" to fill the editor.\n\
                     5. Edit the Rust source. The WatchFace trait has five methods:\n\
                        setup, activate, loop_, resign, and new_static.\n\
                     6. Click \"Save face\" to write the file to src/movement/.\n\
                     7. Add it to the active preset in Watch Faces, then try it in Simulator.\n\
                     Firmware build and flash remain unavailable until the build input contract is complete.",
                );
                ui.add_space(4.0);
                ui.weak(
                    "Tip: loop_ receives an Event each tick. Match on Event::Button\n\
                     (Button::Alarm, ButtonEvent::Up) to react to button presses, and\n\
                     Event::Tick to update the display. Use watch::slcd::display_string\n\
                     to draw text.",
                );
            });
        ui.add_space(8.0);

        // Note about registration behavior so beginners aren't surprised.
        egui::CollapsingHeader::new("About saving & registration")
            .default_open(false)
            .show(ui, |ui| {
                ui.weak(
                    "When you Save, the face is also registered in the firmware so it\n\
                     shows up in Watch Faces. Building still requires the firmware to\n\
                     compile it; if your face's code has errors the build will fail\n\
                     with that info.",
                );
            });
        ui.add_space(8.0);

        // Template selection.
        ui.label("Template:");
        for (i, t) in editor::TEMPLATES.iter().enumerate() {
            if ui
                .selectable_label(
                    self.editor_template == i,
                    format!("{} - {}", t.name, t.description),
                )
                .clicked()
            {
                self.editor_template = i;
            }
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Face name (snake_case):");
            let name_response = ui.text_edit_singleline(&mut self.editor_name);
            self.register_anchor(Panel::Editor, AnchorId::EditorName, &name_response);
            let generate_response = ui.button("Generate from template");
            self.register_anchor(Panel::Editor, AnchorId::EditorGenerate, &generate_response);
            if generate_response
                .on_hover_text("Fill the editor with a ready-made watch face")
                .clicked()
            {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() {
                    let source = editor::generate_face(
                        &name,
                        &editor::TEMPLATES[self.editor_template],
                        &self.editor_description,
                    );
                    self.editor_source = source;
                    self.log.log(format!("Generated {name} from template"));
                }
            }
        });

        ui.add_space(4.0);
        self.editor_identity(ui);

        ui.add_space(8.0);
        self.editor_actions(ui);

        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::TextEdit::multiline(&mut self.editor_source)
                    .code_editor()
                    .desired_rows(24)
                    .desired_width(f32::INFINITY)
                    .show(ui);
            });
    }

    fn editor_identity(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Face name (snake_case):");
            let name_response = ui.text_edit_singleline(&mut self.editor_name);
            self.register_anchor(Panel::Editor, AnchorId::EditorName, &name_response);
        });
        ui.horizontal(|ui| {
            ui.label("Description (shown in catalog):");
            ui.text_edit_singleline(&mut self.editor_description);
        });
    }

    fn editor_actions(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let save_response = ui.button("Save face");
            self.register_anchor(Panel::Editor, AnchorId::EditorSave, &save_response);
            if save_response
                .on_hover_text("Save the editor source to the firmware project")
                .clicked()
            {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() && !self.editor_source.is_empty() {
                    match editor::write_face(&name, &self.editor_source) {
                        Ok(_) => {
                            self.status = format!("Saved face {name}");
                            // Best-effort visibility: add a `pub mod <name>;`
                            // declaration so the face shows up in Watch Faces. A
                            // registration failure is non-fatal - the file is
                            // saved, just not wired up yet.
                            match editor::register_face(&name) {
                                Ok(_) => {
                                    self.log.log("Face saved and registered");
                                }
                                Err(e) => {
                                    let path = editor::face_path(&name).display().to_string();
                                    self.status =
                                        format!("Face saved but registration failed: {e}");
                                    self.log_error(&format!(
                                        "Face saved to {path} but not yet registered \
                                         (manual step needed): {e}"
                                    ));
                                }
                            }
                            self.face_list = faces::discover_faces();
                        }
                        Err(e) => {
                            self.status = format!("Save failed: {e}");
                            self.log_error(&self.status.clone());
                        }
                    }
                }
            }
            if ui
                .button("Load face")
                .on_hover_text("Load the face source into the editor")
                .clicked()
            {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() {
                    match editor::read_face(&name) {
                        Ok(src) => {
                            self.editor_source = src;
                            self.status = format!("Loaded face {name}");
                        }
                        Err(e) => {
                            self.status = format!("Load failed: {e}");
                            self.log_error(&self.status.clone());
                        }
                    }
                }
            }
            if ui
                .button("Delete face")
                .on_hover_text("Delete the face file from the firmware project")
                .clicked()
                && self.unsafe_action_allowed()
            {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() {
                    self.pending_confirm = Some((
                        format!(
                            "Delete face '{name}'? This deletes the file from the firmware project."
                        ),
                        ConfirmKind::DeleteFaceFile(name),
                    ));
                }
            }
        });
    }

    /// The combined build & flash panel.
    fn build_flash(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading("Build & Flash");
                ui.label(
                    "Build & Flash is unavailable: Studio does not yet provide the\n\
                     configuration input contract required to produce a configured UF2.\n\
                     Complete the missing contract inputs below before retrying.",
                );
                ui.add_space(8.0);

                // Board selection (which revision the .uf2 targets).
                ui.horizontal(|ui| {
                    ui.label("Target board:");
                    let mut board_rect = None;
                    for b in Board::ALL {
                        let response = ui.selectable_label(self.board == b, b.label());
                        board_rect = Some(
                            board_rect
                                .map(|rect: egui::Rect| rect.union(response.rect))
                                .unwrap_or(response.rect),
                        );
                        if response.clicked() && self.board != b {
                            self.request_component_change(b, self.component_profile, self.component_draft.clone(), format!("Switch target board to {}", b.label()));
                            self.log.log(format!("Target board change requested: {}", b.label()));
                        }
                    }
                    if let Some(rect) = board_rect {
                        self.register_anchor_rect(Panel::BuildFlash, AnchorId::BuildBoard, rect);
                    }
                    ui.separator();
                    // Description of the selected board: what it is, hardware
                    // details, and how it differs from the other revisions.
                    ui.weak(board_info(self.board));
                });
                ui.weak(
                    "Choose the board revision you're flashing. The firmware binary is\n\
                     the same for all boards; the board type (affecting LED polarity\n\
                     and buzzer voltage) is set at runtime on the watch itself. This\n\
                     selection records which board you're flashing and is auto-selected\n\
                     from the watch when it is detected.",
                );
                ui.add_space(8.0);

                ui.add_space(8.0);
                let profile_start = ui.min_rect();
                ui.collapsing("Board capability chart", |ui| {
                    components::show_capability_chart(ui);
                });
                let (config_changed, profile_selection) = components::show_configurator(
                    ui,
                    self.board,
                    &mut self.component_profiles,
                    &mut self.component_profile,
                    &mut self.component_draft,
                );
                if let Some(selection) = profile_selection {
                    self.request_component_change(self.board, selection.index, selection.config, format!("Apply build profile {}", self.component_profiles[selection.index].name));
                }
                if config_changed && self.pending_component_conflict.is_none() {
                    let profile = self.component_profiles.get(self.component_profile).cloned().unwrap_or_else(|| components::BuildProfile::new("draft", self.component_draft.clone()));
                    self.component_effective = components::effective_config(self.board, &profile, &self.component_draft);
                }
                self.register_anchor_rect(
                    Panel::BuildFlash,
                    AnchorId::BuildProfile,
                    profile_start.union(ui.min_rect()),
                );
                ui.add_space(8.0);

                ui.weak(
                    "Build and flash estimates are unavailable: the Studio-to-firmware\n\
                     configuration input contract is incomplete.",
                )
                .on_hover_text(build::CONFIGURATION_BUILD_BLOCKED);
                ui.add_space(8.0);

                // Build is intentionally disabled until Studio supplies the full
                // configuration input contract consumed by the firmware builder.
                ui.strong("Build");
                if self.building {
                    ui.spinner();
                    ui.label(tr(self.language, Key::Building));
                } else if build::validate_configuration_inputs().is_err() {
                    let response = ui.add_enabled(false, egui::Button::new("Build unavailable"));
                    self.register_anchor(Panel::BuildFlash, AnchorId::BuildUnavailable, &response);
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 160, 80),
                        "Build disabled: complete the Studio-to-firmware configuration input contract first.",
                    );
                    ui.collapsing("Why these five items are still shown", |ui| {
                        ui.label(
                            "These are firmware-build requirements, not a beginner checklist. Selecting every UI option does not wire the selection into the firmware build.",
                        );
                        for (title, explanation) in build::CONFIGURATION_INPUT_EXPLANATIONS {
                            ui.strong(*title);
                            ui.label(*explanation);
                            ui.add_space(4.0);
                        }
                    });
                    ui.collapsing("What to do next", |ui| {
                        ui.label("1. Keep your stock preset, matching target/profile, and component choices as Studio planning data.");
                        ui.label("2. Do not keep changing toggles expecting this gate to clear; there is no beginner action that completes the missing firmware-input generation.");
                        ui.label("3. For an existing, verified UF2, enter its path, inspect it with its matching .uf2.json and .json.sig sidecars, and review the result.");
                        ui.label("4. Approve only that exact artifact for this session, refresh bootloader detection, and copy only when exactly one expected watch drive is identified.");
                    });
                } else {
                    let build_response = ui
                        .button(tr(self.language, Key::BuildUf2))
                        .on_hover_text("Compile the firmware into a .uf2 file for the watch");
                    self.register_anchor(Panel::BuildFlash, AnchorId::BuildArtifact, &build_response);
                    if !self.shutting_down
                        && self.pending_build.is_none()
                        && !self.flash_busy()
                        && build_response.clicked()
                        && self.unsafe_action_allowed()
                    {
                        self.start_build();
                    }
                }
                if !self.build_message.is_empty() {
                    ui.label(&self.build_message);
                }

                ui.separator();
                ui.strong("Inspect an existing artifact");
                ui.label(
                    "Enter a UF2 path and inspect it explicitly. Recovery generation UF2s
                     are accepted when their matching .uf2.json and .json.sig sidecars exist.",
                );
                let artifact_actions_blocked = self.building || self.pending_build.is_some();
                ui.horizontal(|ui| {
                    let path_response = ui.add_enabled(
                        !artifact_actions_blocked,
                        egui::TextEdit::singleline(&mut self.artifact_path_input)
                            .hint_text("Path to .uf2"),
                    );
                    self.register_anchor(Panel::BuildFlash, AnchorId::BuildArtifactPath, &path_response);
                    let inspect_response = ui.add_enabled(
                        !artifact_actions_blocked,
                        egui::Button::new("Inspect UF2"),
                    );
                    self.register_anchor(Panel::BuildFlash, AnchorId::BuildInspect, &inspect_response);
                    if inspect_response
                        .on_hover_text("Verify UF2 structure, family, manifest, and sidecars")
                        .clicked()
                        && self.guided_action_allowed(AnchorId::BuildInspect)
                    {
                        self.inspect_artifact_from_input();
                    }
                });

                if artifact_actions_blocked {
                    ui.weak("Artifact inspection and approval are disabled while a build is in progress.");
                }
                if let Some(inspection) = self.pending_artifact.clone() {
                    ui.group(|ui| {
                        ui.label("Verification succeeded (local consistency only):");
                        ui.monospace(artifact_metadata(&inspection));
                        let approve_response = ui.add_enabled(
                            !artifact_actions_blocked,
                            egui::Button::new("Approve for this session"),
                        );
                        self.register_anchor(Panel::BuildFlash, AnchorId::BuildApprove, &approve_response);
                        if approve_response
                            .on_hover_text("Approve this inspected artifact only after reviewing its metadata")
                            .clicked()
                            && self.guided_action_allowed(AnchorId::BuildApprove)
                        {
                            let provenance_ok = self
                                .pending_artifact
                                .as_ref()
                                .map(build::validate_generated_input_digest)
                                .unwrap_or_else(|| Err("no artifact is awaiting approval".into()));
                            if let Err(error) = provenance_ok {
                                set_failed_artifact_state(
                                    &mut self.status,
                                    &mut self.build_message,
                                    &mut self.approved_artifact,
                                    &mut self.pending_artifact,
                                    error,
                                );
                            } else {
                                approve_artifact_state(
                                    &mut self.status,
                                    &mut self.build_message,
                                    &mut self.pending_artifact,
                                    &mut self.approved_artifact,
                                    artifact_actions_blocked,
                                );
                            }
                            let fingerprint = self.build_configuration_fingerprint();
                            if let Some(approved) = &mut self.approved_artifact {
                                approved.config_fingerprint = fingerprint;
                            }
                            self.pending_artifact_fingerprint = None;
                        }
                    });
                }
                if self.pending_artifact.is_some() {
                    ui.weak("Artifact inspected; UF2, sidecars, and hashes verified. Approval is still required.");
                }
                if self.approved_artifact.is_some() {
                    ui.weak("Artifact approved for this session.");
                }
                if let Some(uf2) = self.approved_artifact.as_ref().map(|artifact| &artifact.path) {
                    ui.label(
                        tr(self.language, Key::Output)
                            .replace("{path}", &uf2.display().to_string()),
                    );
                }

                ui.add_space(12.0);
                ui.separator();
                // Flash. Detection is cached and refreshed explicitly in a worker;
                // rendering never enumerates drive roots.
                ui.strong("Flash");
                if let Some(event) = &self.current_progress {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.strong(format!("{} · op {}", event.phase.label(), event.operation_id));
                        });
                        ui.label(&event.message);
                        if let (Some(current), Some(total)) = (event.current, event.total) {
                            if total > 0 {
                                ui.add(egui::ProgressBar::new(current as f32 / total as f32)
                                    .text(format!("{current}/{total}")));
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if self.pending_detection.is_some() {
                        ui.spinner();
                        ui.label("Detecting Sensor Watch drives…");
                    } else {
                        let refresh_response = ui.button("Refresh detection");
                        self.register_anchor(Panel::BuildFlash, AnchorId::BuildRefresh, &refresh_response);
                        if refresh_response
                        .on_hover_text("Rescan removable drives; keep only the intended watch in bootloader mode")
                        .clicked()
                        && !self.flash_busy()
                        && self.guided_action_allowed(AnchorId::BuildRefresh) {
                        self.start_watch_detection();
                        }
                    }
                });
                match &self.cached_watch {
                    WatchDriveSelection::One(candidate) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 120),
                            format!("One expected drive detected at {}", candidate.root.display()),
                        );
                    }
                    WatchDriveSelection::Multiple(count) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 160, 80),
                            format!(
                                "Ambiguous watch selection: {count} Sensor Watch drives detected; disconnect all but one."
                            ),
                        );
                    }
                    WatchDriveSelection::None => {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 160, 80),
                            "No watch detected. Put it in bootloader mode (USB connected).",
                        );
                    }
                }
                if self.flash_busy() {
                    ui.weak("Flash or drive detection is in progress; conflicting controls are disabled.");
                } else if self.building || self.pending_build.is_some() {
                    ui.weak("Build in progress; flashing is disabled until it finishes.");
                } else if let Some(approved) = &self.approved_artifact {
                    let approved = approved.clone();
                    ui.weak("Copy ready: current artifact and drive will be checked again before scheduling.");
                    let copy_response = ui.button(tr(self.language, Key::CopyToWatch));
                    self.register_anchor(Panel::BuildFlash, AnchorId::BuildCopy, &copy_response);
                    if !self.shutting_down
                        && copy_response
                            .on_hover_text(
                                "Write the firmware to the watch's USB drive (bootloader mode)",
                            )
                            .clicked()
                        && self.unsafe_action_allowed()
                    {
                        self.snapshot_before("Before flash");
                        self.copy_to_watch(&approved);
                    }
                } else {
                    ui.label(tr(self.language, Key::NoBuildYet));
                }

                ui.add_space(12.0);
                ui.separator();
                // Combined log.
                ui.horizontal(|ui| {
                    ui.strong("Build & flash log");
                    if ui.small_button("Clear").clicked() {
                        self.build_log.clear();
                        self.flash_log.clear();
                    }
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .max_height(200.0)
                    .show(ui, |ui| {
                        let mut entries: Vec<_> = self
                            .build_log
                            .entries()
                            .iter()
                            .chain(self.flash_log.entries().iter())
                            .collect();
                        entries.sort_by_key(|e| e.timestamp);
                        if entries.is_empty() {
                            ui.weak("(no activity yet)");
                        }
                        for entry in entries {
                            let secs = entry.timestamp % 60;
                            let mins = (entry.timestamp / 60) % 60;
                            let hrs = (entry.timestamp / 3600) % 24;
                            ui.monospace(format!(
                                "[{hrs:02}:{mins:02}:{secs:02}] {}",
                                entry.message
                            ));
                        }
                    });
            });
    }

    /// The calibration panel: clock and drift calibration with a beep-on-minute
    /// rollover helper.
    fn calibration(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading("Calibration");
                ui.separator();
            ui.label(
                    "Follow the steps in order: fetch a reference, set the watch, then\n\
                     record start and end samples after a useful measurement interval.",
                );
                ui.colored_label(
                    egui::Color32::from_rgb(230, 180, 80),
                    "Offline-safe: Studio does not perform serial I/O. Send the copied commands\nvia the UART jig/debug pads; USB is bootloader-only.",
                );
                ui.add_space(8.0);

                // NTP fetch.
                ui.horizontal(|ui| {
                    let response = ui.button("Fetch NTP time");
                    self.register_anchor(Panel::Calibration, AnchorId::CalibrationFetch, &response);
                    if response.clicked() {
                        self.fetch_ntp();
                    }
                    if self.ntp_busy {
                        ui.spinner();
                    }
                });
                if let Some(ts) = self.ntp_time {
                    let secs = ts as i64;
                    let rem = secs.rem_euclid(86400);
                    let h = (rem / 3600) % 24;
                    let m = (rem / 60) % 60;
                    let s = rem % 60;
                    let (year, month, day) = watch_sim::civil_from_days(secs.div_euclid(86400));
                    ui.monospace(format!(
                        "Current reference: {year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02} UTC (ping {:.1} ms, offset {:+.1} ms)",
                        self.ntp_ping,
                        self.ntp_offset * 1000.0
                    ));
                }

                ui.add_space(12.0);
                ui.separator();
                ui.strong("Step 2 - Set the clock at the next minute boundary");
                if let Some(ts) = self.ntp_time {
                    let boundary = (ts / 60 + 1) * 60;
                    let b = boundary as i64;
                    let rem = b.rem_euclid(86400);
                    let h = (rem / 3600) % 24;
                    let m = (rem / 60) % 60;
                    let s = rem % 60;
                    let days = b.div_euclid(86400);
                    let (year, month, day) = watch_sim::civil_from_days(days);
                    let cmd = ntp::settime_command(boundary);
                    ui.monospace(format!("Next minute boundary: {year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02} UTC"));
                    ui.monospace(format!("UART command: {cmd}"));
                    ui.weak("Copy this exact command and send it manually via the UART jig/debug pads.");
                    if ui.button("Copy command").clicked() {
                        let _ = ui_copy_to_clipboard(&cmd);
                        self.status = "Calibration command copied".to_string();
                    }
                } else {
                    ui.weak("Fetch NTP time first.");
                }

                ui.add_space(12.0);
                ui.separator();
                ui.strong("Optional - minute-boundary cue");
                ui.label(
                    "Arm a software cue for the exact next minute boundary. This is only a\n\
                     timing aid; Studio does not connect to or write the watch.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Arm beep").clicked() {
                        if let Some(ts) = self.ntp_time {
                            self.beep_target = (ts / 60 + 1) * 60;
                            self.beep_armed = true;
                            self.log.log("Beep armed for next minute boundary");
                        } else {
                            self.status = "Fetch NTP time first".to_string();
                        }
                    }
                    if ui.button("Disarm").clicked() {
                        self.beep_armed = false;
                    }
                });
                if self.beep_armed {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let remaining = self.beep_target.saturating_sub(now);
                    ui.monospace(format!("Cue in {remaining} s"));
                    if remaining <= 1 {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 120),
                            "BEEP! Set the watch now (software cue).",
                        );
                        self.beep_armed = false;
                    } else {
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.strong("Step 3 - Measure drift");
                ui.label(
                    "Fetch NTP before each sample. Record the start, wait at least one\n\
                     minute (hours or days is better), fetch again, then record the end.",
                );
                ui.horizontal(|ui| {
                    let record_response = ui.button("Record sample");
                    self.register_anchor(
                        Panel::Calibration,
                        AnchorId::CalibrationRecord,
                        &record_response,
                    );
                    if record_response.clicked()
                        && self.guided_action_allowed(AnchorId::CalibrationRecord)
                    {
                        if let Some(reference) = self.ntp_time {
                            let (year, month, day, hour, minute, second, _) = self.watch.get_time();
                            let watch_secs = (watch_sim::days_from_civil(year, month, day) as u64)
                                .saturating_mul(86_400)
                                .saturating_add((hour as u64) * 3_600 + (minute as u64) * 60 + second as u64);
                            match self.drift_session.record(watch_secs, reference) {
                                Ok(role) => {
                                    self.status = format!("Recorded {role} drift sample");
                                    self.log.log(format!("Drift {role} sample recorded"));
                                    self.save_settings_internal();
                                }
                                Err(error) => {
                                    self.status = error.clone();
                                    self.log_error(&error);
                                }
                            }
                        } else {
                            self.status = "Fetch NTP time first".to_string();
                        }
                    }
                    if ui.button("Reset samples").clicked() {
                        self.drift_session.reset();
                        self.beep_armed = false;
                        self.save_settings_internal();
                        self.status = "Calibration samples reset".to_string();
                    }
                });
                if let Some(start) = self.drift_session.start {
                    ui.monospace(format!("Start: watch {} / reference {}", start.watch_seconds, start.reference_seconds));
                }
                if let Some(end) = self.drift_session.end {
                    ui.monospace(format!("End: watch {} / reference {}", end.watch_seconds, end.reference_seconds));
                }
                if let (Some(start), Some(end)) = (self.drift_session.start, self.drift_session.end) {
                    match drift::measure(start, end) {
                        Ok(measurement) => {
                            ui.monospace(format!("Measured drift: {:+.2} ppm (error {:+} s over {} s)", measurement.ppm, measurement.error_seconds, measurement.elapsed_seconds));
                            ui.monospace(format!("Recommended correction: {:+} ppm", measurement.correction_ppm));
                            let command = format!("drift {}", measurement.correction_ppm);
                            ui.monospace(format!("UART command: {command}"));
                            let copy_response = ui.button("Copy correction command");
                            self.register_anchor(
                                Panel::Calibration,
                                AnchorId::CalibrationCopy,
                                &copy_response,
                            );
                            if copy_response.clicked()
                                && self.guided_action_allowed(AnchorId::CalibrationCopy)
                            {
                                let _ = ui_copy_to_clipboard(&command);
                                self.status = "Drift correction command copied".to_string();
                            }
                            ui.weak("Send this command manually through the UART jig/debug pads; Studio never performs serial I/O.");
                        }
                        Err(error) => {
                            ui.colored_label(egui::Color32::from_rgb(220, 100, 90), error);
                        }
                    }
                } else if self.drift_session.start.is_some() {
                    ui.weak("Start recorded. Fetch NTP again after the interval, then record the end sample.");
                } else {
                    ui.weak("No samples yet.");
                }

                ui.add_space(12.0);
                ui.separator();
                ui.strong("Optional temperature compensation");
                ui.label("Stored as versioned settings; disabled by default and never written to hardware by Studio.");
                let mut changed = false;
                let mut enabled = self.rtc_calibration.enabled();
                if ui.checkbox(&mut enabled, "Enable stored calibration").changed() {
                    self.rtc_calibration.version = if enabled { sensor_watch_core::rtc_calibration::CALIBRATION_VERSION } else { 0 };
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.label("Base correction (PPM)");
                    changed |= ui.add(egui::DragValue::new(&mut self.rtc_calibration.base_ppm).speed(0.1)).changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Temperature coefficient (PPM/°C)");
                    changed |= ui.add(egui::DragValue::new(&mut self.rtc_calibration.temperature_coefficient_ppm_per_c).speed(0.01)).changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Reference temperature (°C)");
                    changed |= ui.add(egui::DragValue::new(&mut self.rtc_calibration.reference_temperature_c).speed(0.1)).changed();
                });
                if changed {
                    self.rtc_calibration.clamp_values();
                    self.save_settings_internal();
                }
                if self.rtc_calibration.enabled() {
                    ui.monospace(format!("At 25 °C: {:+.2} PPM", sensor_watch_core::rtc_calibration::RtcCalibration::new(self.rtc_calibration.base_ppm, self.rtc_calibration.temperature_coefficient_ppm_per_c, self.rtc_calibration.reference_temperature_c).correction_ppm(25.0)));
                }
            });
    }

    /// The modules panel: manage custom hardware modules (e.g. BLE boards).
    fn modules(&mut self, ui: &mut egui::Ui) {
        ui.heading("Modules");
        ui.separator();
        ui.label(
            "Register custom hardware modules for modded boards (e.g. a BLE board\n\
             instead of the accelerometer). Each module targets a HAL file in\n\
             src/watch/ and is listed here so the app knows what is installed.",
        );
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Add a new module.
                ui.strong("Add module");
                egui::Grid::new("module_add")
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.module_name);
                        ui.end_row();
                        ui.label("HAL target (e.g. lis2dw.rs):");
                        ui.text_edit_singleline(&mut self.module_target);
                        ui.end_row();
                        ui.label("Description:");
                        ui.text_edit_singleline(&mut self.module_description);
                        ui.end_row();
                    });
                let response = ui.button("Register module");
                self.register_anchor(Panel::Modules, AnchorId::ModulesRegister, &response);
                if response
                    .on_hover_text("Register a new custom hardware module")
                    .clicked()
                {
                    let name = self.module_name.trim().to_string();
                    if !name.is_empty() {
                        self.modules.add(modules::Module {
                            name: name.clone(),
                            target: self.module_target.trim().to_string(),
                            description: self.module_description.trim().to_string(),
                            enabled: true,
                        });
                        self.log.log(format!("Registered module {name}"));
                        self.module_name.clear();
                        self.module_target.clear();
                        self.module_description.clear();
                        self.save_settings_internal();
                    }
                }

                ui.add_space(12.0);
                ui.separator();

                // List registered modules.
                ui.strong(format!(
                    "Registered modules ({})",
                    self.modules.modules.len()
                ));
                if self.modules.modules.is_empty() {
                    ui.weak("No custom modules registered yet.");
                }
                let mut to_toggle: Option<String> = None;
                let names: Vec<String> = self
                    .modules
                    .modules
                    .iter()
                    .map(|m| m.name.clone())
                    .collect();
                let unsafe_allowed = self.unsafe_action_allowed();
                for name in &names {
                    let Some(m) = self
                        .modules
                        .modules
                        .iter()
                        .find(|m| &m.name == name)
                        .cloned()
                    else {
                        continue;
                    };
                    ui.horizontal(|ui| {
                        let mut enabled = m.enabled;
                        if ui.checkbox(&mut enabled, &m.name).changed() {
                            to_toggle = Some(m.name.clone());
                        }
                        if !m.target.is_empty() {
                            ui.monospace(&m.target);
                        }
                        if !m.description.is_empty() {
                            ui.weak(&m.description);
                        }
                        if ui
                            .small_button("Remove")
                            .on_hover_text("Remove this module")
                            .clicked()
                            && unsafe_allowed
                        {
                            self.pending_confirm = Some((
                                format!("Remove module '{}'?", m.name),
                                ConfirmKind::RemoveModule(m.name.clone()),
                            ));
                        }
                    });
                }
                if let Some(name) = to_toggle {
                    self.modules.toggle(&name);
                    self.log.log(format!("Toggled module {name}"));
                    self.save_settings_internal();
                }

                ui.add_space(12.0);
                ui.separator();
                ui.weak(
                    "Note: registering a module here records it in the app. To\n\
                     actually add the driver, drop the Rust source into src/watch/\n\
                     and register it in src/watch/mod.rs. The app tracks which\n\
                     modules are installed and enabled for a modded board.",
                );
            });
    }

    /// Diagnostics deliberately remain simulated; physical diagnostics require
    /// an explicit UART command and are never inferred from this report.
    fn diagnostics(&mut self, ui: &mut egui::Ui) {
        ui.heading("Diagnostics");
        if !self
            .panel_ux(Panel::Diagnostics)
            .simulated_diagnostics_visibility
        {
            ui.weak("Simulated diagnostics are hidden by this panel override. Physical checks remain separate and guarded.");
            return;
        }

        ui.label("Offline checks for the watch shell and simulator. Physical hardware is never implied by simulated results.");
        if ui.button("Open error encyclopedia").clicked() {
            self.current_panel = Panel::Bugs;
        }
        ui.horizontal(|ui| {
            ui.label("Connection mode:");
            let mode_color = if self.uart.is_some() {
                egui::Color32::from_rgb(120, 210, 150)
            } else {
                egui::Color32::from_rgb(220, 180, 80)
            };
            ui.colored_label(mode_color, self.transport_mode.label());
            ui.weak(if self.uart.is_some() {
                "UART connected (diagnostics remain simulated)"
            } else {
                "UART jig not connected"
            });
            let run = ui.add_enabled(
                !self.diagnostics.running,
                egui::Button::new("Run full diagnostic"),
            );
            self.register_anchor(Panel::Diagnostics, AnchorId::DiagnosticsRun, &run);
            if run.clicked() {
                self.run_full_diagnostic();
            }
            if ui.button("Copy report").clicked() {
                let report = self.diagnostics.last_report.clone();
                if report.is_empty() {
                    self.status = "Run diagnostics before copying a report".to_string();
                } else if ui_copy_to_clipboard(&report).is_ok() {
                    self.status = "Diagnostics report copied".to_string();
                }
            }
            if ui.button("Export report").clicked() {
                let report = self.diagnostics.last_report.clone();
                if report.is_empty() {
                    self.status = "Run diagnostics before exporting a report".to_string();
                } else {
                    match self.export_text_file("diagnostics-report.txt", report) {
                        Ok(path) => {
                            self.status = format!("Diagnostics exported to {}", path.display())
                        }
                        Err(error) => self.status = format!("Diagnostics export failed: {error}"),
                    }
                }
            }
        });
        ui.colored_label(
            egui::Color32::from_rgb(220, 180, 80),
            "SIMULATED CHECKS ONLY - this report never queries physical hardware. Use Shell Access for explicit UART commands.",
        );
        ui.separator();

        egui::Grid::new("diagnostic_rows")
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Test");
                ui.strong("Status");
                ui.strong("Detail");
                ui.end_row();
                for row in &self.diagnostics.rows {
                    ui.label(row.name);
                    let color = match row.status {
                        diagnostics::Status::Pass => egui::Color32::from_rgb(120, 210, 150),
                        diagnostics::Status::Blocked => egui::Color32::from_rgb(220, 180, 80),
                        diagnostics::Status::Pending => ui.visuals().weak_text_color(),
                    };
                    ui.colored_label(color, row.status.label());
                    ui.label(&row.detail);
                    ui.end_row();
                }
            });
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Live log");
            ui.label("Ticks:");
            self.tick_filter_ui(ui, "diagnostics_tick_filter");
            ui.weak(format!(
                "{} / 200 lines - auto-scroll",
                self.diagnostics.log.len()
            ));
            if ui.small_button("Clear").clicked() {
                self.diagnostics.log.clear();
            }
        });
        egui::ScrollArea::vertical()
            .id_source("diagnostics_live_log")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                if self.diagnostics.log.is_empty() {
                    ui.weak("(run a diagnostic to populate the simulated activity log)");
                }
                for line in &self.diagnostics.log {
                    if self.show_main_event(line) {
                        ui.monospace(line);
                    }
                }
            });
        self.tick_log_ui(ui, "diagnostics_tick_log");
    }

    fn simulated_command(&mut self, command: &str) -> String {
        self.run_shell_command(command);
        self.shell_log
            .entries()
            .back()
            .map(|entry| entry.message.clone())
            .unwrap_or_default()
    }

    fn run_full_diagnostic(&mut self) {
        // Diagnostics exercise the live shell/simulator code, including a
        // settime round-trip. Preserve the user's simulator state so the
        // offline check cannot unexpectedly move their clock.
        let simulator_snapshot = (
            self.watch.time_offset,
            self.watch.time_mode,
            self.watch.light,
            self.watch.display,
            self.watch.weekday_override,
            self.watch.override_text.clone(),
            self.sim_face_idx,
            self.sim_year,
            self.sim_month,
            self.sim_day,
            self.sim_hour,
            self.sim_minute,
            self.sim_weekday,
        );

        self.diagnostics.reset();
        self.diagnostics
            .log("mode=Simulated; UART jig not connected; no hardware access");
        self.diagnostics
            .log("running existing simulated shell/simulator paths");

        let help = self.simulated_command("help");
        let help_ok = help.contains("CMDS:");
        self.diagnostics.record(
            0,
            if help_ok {
                diagnostics::Status::Pass
            } else {
                diagnostics::Status::Blocked
            },
            format!("reply: {help}"),
        );
        self.diagnostics.log(format!("shell help -> {help}"));

        let time = self.simulated_command("time");
        let time_ok = time.len() == 12 && time.bytes().all(|byte| byte.is_ascii_digit());
        self.diagnostics.record(
            1,
            if time_ok {
                diagnostics::Status::Pass
            } else {
                diagnostics::Status::Blocked
            },
            format!("reply: {time}"),
        );
        self.diagnostics.log(format!("time -> {time}"));

        let settime = self.simulated_command("settime 260101120000");
        let roundtrip = self.simulated_command("time");
        let roundtrip_ok = settime == "OK" && roundtrip.starts_with("2601011200");
        self.diagnostics.record(
            2,
            if roundtrip_ok {
                diagnostics::Status::Pass
            } else {
                diagnostics::Status::Blocked
            },
            format!("settime={settime}, readback={roundtrip}"),
        );
        self.diagnostics.log(format!(
            "settime round-trip -> {settime}; readback {roundtrip}"
        ));

        let drift = self.simulated_command("drift -12");
        let drift_ok = drift == "OK";
        self.diagnostics.record(
            3,
            if drift_ok {
                diagnostics::Status::Pass
            } else {
                diagnostics::Status::Blocked
            },
            format!("drift -12 -> {drift}"),
        );
        self.diagnostics.log(format!("drift parser -> {drift}"));

        self.watch.update_display();
        let (_, _, _, hour, minute, second, _) = self.watch.get_time();
        self.diagnostics.record(
            4,
            diagnostics::Status::Pass,
            format!("clock {hour:02}:{minute:02}:{second:02}; RTC simulator readable"),
        );
        self.diagnostics
            .log(format!("RTC/display -> {hour:02}:{minute:02}:{second:02}"));

        let face_count = self.face_list.len();
        if face_count > 0 {
            self.sim_face_idx = (self.sim_face_idx + 1) % face_count;
        }
        self.diagnostics.record(
            5,
            if face_count > 0 {
                diagnostics::Status::Pass
            } else {
                diagnostics::Status::Blocked
            },
            format!("cycled simulator face; {face_count} catalog faces available"),
        );
        self.diagnostics
            .log(format!("face cycling -> index {}", self.sim_face_idx));

        self.press_sim_button(ButtonId::L);
        self.diagnostics.record(
            6,
            diagnostics::Status::Pass,
            "simulated Light button edge accepted",
        );
        self.diagnostics.log("button input -> simulated L press");

        self.watch.update_display();
        let lcd: String = [
            self.watch.display.hour_2,
            self.watch.display.hour_1,
            self.watch.display.minute_2,
            self.watch.display.minute_1,
            self.watch.display.second_2,
            self.watch.display.second_1,
        ]
        .into_iter()
        .collect();
        self.diagnostics.record(
            7,
            diagnostics::Status::Pass,
            format!("LCD buffer rendered: {lcd}"),
        );
        self.diagnostics.log(format!("LCD output -> {lcd}"));

        self.diagnostics.record(
            8,
            diagnostics::Status::Pass,
            "simulator fault/watchdog state: healthy; physical MCU not inspected",
        );
        self.diagnostics
            .log("watchdog/fault -> simulated healthy (not hardware evidence)");

        self.diagnostics.record(
            9,
            diagnostics::Status::Pass,
            format!(
                "board {} - UF2 {}",
                self.board.label(),
                if self.approved_artifact.is_some() {
                    "available"
                } else {
                    "not built"
                }
            ),
        );
        self.diagnostics.log(format!(
            "board/UF2 -> {} / {}",
            self.board.label(),
            if self.approved_artifact.is_some() {
                "available"
            } else {
                "not built"
            }
        ));

        let optical = self.simulated_command("optical");
        self.diagnostics.record(
            10,
            diagnostics::Status::Blocked,
            format!("{optical}; protocol preview is software-only"),
        );
        self.diagnostics.log(format!("optical -> {optical}"));
        self.diagnostics.log(optical::self_test());
        self.diagnostics.finish("Simulated");

        let (
            time_offset,
            time_mode,
            light,
            display,
            weekday_override,
            override_text,
            sim_face_idx,
            sim_year,
            sim_month,
            sim_day,
            sim_hour,
            sim_minute,
            sim_weekday,
        ) = simulator_snapshot;
        self.watch.time_offset = time_offset;
        self.watch.time_mode = time_mode;
        self.watch.light = light;
        self.watch.display = display;
        self.watch.weekday_override = weekday_override;
        self.watch.override_text = override_text;
        self.sim_face_idx = sim_face_idx;
        self.sim_year = sim_year;
        self.sim_month = sim_month;
        self.sim_day = sim_day;
        self.sim_hour = sim_hour;
        self.sim_minute = sim_minute;
        self.sim_weekday = sim_weekday;

        self.status = "Simulated diagnostics complete; no UART hardware queried".to_string();
    }

    fn poll_probe_worker(&mut self) {
        if let Some(receiver) = &self.probe_progress_rx {
            while let Ok(progress) = receiver.try_recv() {
                self.probe_progress = Some(progress);
            }
        }
        let Some(handle) = self.pending_probe.take() else {
            return;
        };
        if !handle.is_finished() {
            self.pending_probe = Some(handle);
            return;
        }
        self.probe_progress_rx = None;
        match handle.join() {
            Ok(result) => {
                self.probe_report = Some(result.report);
                match result.connection {
                    probe::ConnectionState::Connected => {
                        self.uart = result.transport;
                        self.transport_mode = transport::TransportMode::UartJig;
                        self.status = "Physical probe complete".to_string();
                    }
                    probe::ConnectionState::Disconnected => {
                        self.uart = None;
                        self.transport_mode = transport::TransportMode::Simulated;
                        self.status =
                            "Physical probe complete; UART connection was lost".to_string();
                    }
                }
            }
            Err(_) => {
                self.status = "Physical probe worker panicked".to_string();
                self.log_error("Physical probe worker panicked");
                self.transport_mode = transport::TransportMode::Simulated;
                self.uart = None;
            }
        }
    }

    fn refresh_serial_ports(&mut self) {
        match transport::discover_ports() {
            Ok(ports) => {
                self.serial_ports = ports;
                self.status = format!("Discovered {} serial port(s)", self.serial_ports.len());
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn connect_uart(&mut self) {
        if self.pending_probe.is_some() {
            self.status = "UART is busy with the physical probe".to_string();
            return;
        }
        let Some(name) = self.selected_serial_port.clone() else {
            self.status = transport::TransportError::NoPortSelected.to_string();
            return;
        };
        match transport::SerialTransport::connect(&name, transport::DEFAULT_TIMEOUT) {
            Ok(uart) => {
                self.last_uart_error = None;
                self.uart = Some(uart);
                self.transport_mode = transport::TransportMode::UartJig;
                self.shell_log
                    .log(format!("UART connected: {name} @ 9600 8-N-1"));
                self.status = format!("Connected to UART jig on {name}");
            }
            Err(error) => {
                self.last_uart_error = Some(error.to_string());
                self.uart = None;
                self.transport_mode = transport::TransportMode::Simulated;
                self.status = error.to_string();
                self.shell_log.log(format!("UART connect failed: {error}"));
            }
        }
    }

    fn disconnect_uart(&mut self) {
        if self.pending_probe.is_some() {
            self.status = "UART is busy with the physical probe".to_string();
            return;
        }
        if let Some(uart) = self.uart.take() {
            self.shell_log
                .log(format!("UART disconnected: {}", uart.port_name()));
        }
        self.transport_mode = transport::TransportMode::Simulated;
        self.status = "Using Simulated shell mode".to_string();
    }

    fn send_shell_command(&mut self, cmd: &str) {
        if !self.unsafe_action_allowed() {
            self.status = "UART Send is disabled while a guided tour is active".to_string();
            return;
        }
        if self.pending_probe.is_some() {
            self.status = "UART is busy with the physical probe".to_string();
            return;
        }
        if self.transport_mode == transport::TransportMode::UartJig {
            let Some(uart) = self.uart.as_mut() else {
                self.shell_log.log("UART unavailable; command was not sent");
                self.status = "UART unavailable; command was not sent".to_string();
                return;
            };
            self.shell_log
                .log(format!("[UART {}] > {cmd}", uart.port_name()));
            match uart.command(cmd) {
                Ok(reply) => self.shell_log.log(format!("< {reply}")),
                Err(error) => {
                    let connection_lost = error.is_connection_lost();
                    self.shell_log.log(format!("UART error: {error}"));
                    if connection_lost {
                        self.uart = None;
                        self.transport_mode = transport::TransportMode::Simulated;
                        self.status = "UART disconnected; using Simulated shell mode".to_string();
                    } else {
                        self.status = error.to_string();
                    }
                }
            }
        } else {
            self.run_shell_command(cmd);
        }
    }

    /// The Advanced-only physical probe. USB checks are limited to UF2 metadata;
    /// runtime read-only checks require the explicitly selected UART jig.
    fn probe(&mut self, ui: &mut egui::Ui) {
        ui.heading("Probe / Test");
        ui.colored_label(
            egui::Color32::from_rgb(230, 170, 70),
            "Advanced physical probe - USB is UF2 mass storage only; it cannot expose sensors or runtime hardware.",
        );
        ui.label("Simulated mode is not a physical result. The action below never sends mutation commands.");
        if let Some(progress) = &self.probe_progress {
            ui.add(
                egui::ProgressBar::new(if progress.total == 0 {
                    0.0
                } else {
                    progress.completed as f32 / progress.total as f32
                })
                .text(format!(
                    "{} ({}/{})",
                    progress.message, progress.completed, progress.total
                )),
            );
        }
        ui.horizontal(|ui| {
            let response = ui.add_enabled(
                self.pending_probe.is_none(),
                egui::Button::new("Refresh COM ports"),
            );
            self.register_anchor(Panel::Probe, AnchorId::ProbeRefresh, &response);
            if response
                .on_hover_text("Refresh available UART ports")
                .clicked()
            {
                self.refresh_serial_ports();
            }
            ui.label(format!(
                "{} serial port(s) discovered",
                self.serial_ports.len()
            ));
            if self.uart.is_some() {
                ui.strong(format!(
                    "Connected: {}",
                    self.uart.as_ref().unwrap().port_name()
                ));
            } else {
                ui.weak("No UART connection");
            }
        });
        ui.horizontal(|ui| {
            let enabled = self.advanced_mode && self.pending_probe.is_none();
            let run_response = ui.add_enabled(enabled, egui::Button::new("Run physical probe"));
            self.register_anchor(Panel::Probe, AnchorId::ProbeRun, &run_response);
            if run_response
                .on_hover_text("Run read-only checks; requires a connected UART jig and confirmation")
                .clicked()
                && self.unsafe_action_allowed()
            {
                self.pending_confirm = Some((
                    "Run the physical probe? It will inspect removable drives and send only the read-only commands help, time, events, panic, and optical to the already connected selected UART port.".into(),
                    ConfirmKind::RunPhysicalProbe,
                ));
            }
            let copy_response = ui.button("Copy report");
            self.register_anchor(Panel::Probe, AnchorId::ProbeReport, &copy_response);
            if copy_response.clicked() {
                if let Some(report) = &self.probe_report {
                    match ui_copy_to_clipboard(&report.text()) {
                        Ok(()) => self.status = "Probe report copied".to_string(),
                        Err(error) => self.status = format!("Could not copy probe report: {error}"),
                    }
                } else {
                    self.status = "Run a probe before copying a report".to_string();
                }
            }
            if ui.button("Export report").clicked() {
                if let Some(report) = &self.probe_report {
                    match self.export_text_file("probe-report.txt", report.text()) {
                        Ok(path) => self.status = format!("Probe report exported to {}", path.display()),
                        Err(error) => self.status = format!("Could not export probe report: {error}"),
                    }
                } else {
                    self.status = "Run a probe before exporting a report".to_string();
                }
            }
        });
        ui.separator();
        if let Some(report) = &self.probe_report {
            ui.strong("Results");
            egui::ScrollArea::vertical()
                .id_source("probe_results")
                .show(ui, |ui| {
                    for test in &report.tests {
                        let color = match test.status {
                            probe::TestStatus::Pass => egui::Color32::from_rgb(120, 210, 150),
                            probe::TestStatus::Fail => egui::Color32::from_rgb(220, 90, 90),
                            _ => egui::Color32::from_rgb(220, 180, 80),
                        };
                        ui.colored_label(
                            color,
                            format!("[{}] {} - {}", test.status.label(), test.name, test.reason),
                        );
                    }
                    ui.separator();
                    ui.strong("Bounded probe log (latest entries)");
                    egui::ScrollArea::vertical()
                        .id_source("probe_log")
                        .stick_to_bottom(true)
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for line in &report.log {
                                ui.monospace(line);
                            }
                        });
                });
        } else {
            ui.weak("No physical probe has been run. Select a UART port in Shell Access and connect it explicitly before probing.");
        }
    }

    /// The Shell Access panel: a dedicated terminal for talking to the watch.
    /// Split into two halves: the top is the user-facing command surface (the
    /// app sends a command and the watch replies OK/Done/...), and the bottom
    /// is the low-level "watch brain" view showing the hardware / ISR level of
    /// what the command actually did.
    fn shell(&mut self, ui: &mut egui::Ui) {
        ui.heading("Shell Access");
        ui.label(
            "Send commands to the watch's serial command shell (set-time / drift / help).\n\
             Top: the command surface (what you send + the watch's reply).\n\
             Bottom: the watch's brain (the low-level hardware / register view).",
        );
        ui.add_space(4.0);
        ui.colored_label(
            egui::Color32::from_rgb(220, 180, 80),
            if self.uart.is_some() {
                "UART Jig mode: commands target the selected debug UART. The USB port remains UF2-only."
            } else {
                "Simulated mode: commands target the in-app watch model. Real serial requires the debug UART header; USB CDC is not used."
            },
        );
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Transport:");
            let mode_response = ui.selectable_label(
                self.transport_mode == transport::TransportMode::Simulated,
                "Simulated",
            );
            self.register_anchor(Panel::Shell, AnchorId::ShellMode, &mode_response);
            if mode_response.clicked() {
                self.disconnect_uart();
            }
            if ui
                .selectable_label(
                    self.transport_mode == transport::TransportMode::UartJig,
                    "UART Jig",
                )
                .clicked()
            {
                self.transport_mode = transport::TransportMode::UartJig;
            }
            ui.separator();
            ui.label("Port:");
            let selected = self.selected_serial_port.clone().unwrap_or_default();
            let selected_text = if selected.is_empty() {
                "Select port".to_string()
            } else {
                selected.clone()
            };
            egui::ComboBox::from_id_source("uart_port")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for port in &self.serial_ports {
                        ui.selectable_value(
                            &mut self.selected_serial_port,
                            Some(port.name.clone()),
                            format!("{} - {}", port.name, port.description),
                        );
                    }
                });
            if ui
                .add_enabled(self.pending_probe.is_none(), egui::Button::new("Refresh"))
                .clicked()
            {
                self.refresh_serial_ports();
            }
            if self.uart.is_some() {
                if ui
                    .add_enabled(
                        self.pending_probe.is_none(),
                        egui::Button::new("Disconnect"),
                    )
                    .clicked()
                {
                    self.disconnect_uart();
                }
            } else if ui
                .add_enabled(self.pending_probe.is_none(), egui::Button::new("Connect"))
                .on_hover_text("Connect to the selected UART jig; USB is UF2 storage, not UART")
                .clicked()
            {
                self.connect_uart();
            }
        });
        ui.weak("UART jig: SERCOM3, 9600 8-N-1, A4 TX / A2 RX. USB is UF2 mass storage only; USB CDC is not used.");
        ui.separator();

        // The two halves split vertically.
        egui::TopBottomPanel::top("shell_cmd")
            .resizable(true)
            .default_height(ui.available_height() * 0.45)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Command surface");
                    ui.separator();
                    ui.label("Ticks:");
                    self.tick_filter_ui(ui, "shell_tick_filter");
                    if ui.small_button("Clear comms").clicked() {
                        self.shell_log.clear();
                    }
                    if ui.small_button("Copy").clicked() {
                        let text = self
                            .shell_log
                            .entries()
                            .iter()
                            .map(|e| e.message.clone())
                            .collect::<Vec<_>>()
                            .join("\n");
                        let _ = ui_copy_to_clipboard(&text);
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(">");
                    let resp = ui.text_edit_singleline(&mut self.shell_input);
                    self.register_anchor(Panel::Shell, AnchorId::ShellInput, &resp);
                    let submitted =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let send_response =
                        ui.add_enabled(self.pending_probe.is_none(), egui::Button::new("Send"));
                    self.register_anchor(Panel::Shell, AnchorId::ShellSend, &send_response);
                    if send_response
                        .on_hover_text(
                            "Send the command to the selected simulated or UART transport",
                        )
                        .clicked()
                        && self.unsafe_action_allowed()
                        || (submitted
                            && self.pending_probe.is_none()
                            && self.unsafe_action_allowed())
                    {
                        let cmd = self.shell_input.trim().to_string();
                        self.shell_input.clear();
                        self.send_shell_command(&cmd);
                    }
                });
                ui.add_space(4.0);
                self.tick_log_ui(ui, "shell_tick_log");
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.shell_log.is_empty() {
                            ui.weak("(no commands sent yet)");
                        }
                        for entry in self.shell_log.entries() {
                            if !self.show_main_event(&entry.message) {
                                continue;
                            }
                            let secs = entry.timestamp % 60;
                            let mins = (entry.timestamp / 60) % 60;
                            let hrs = (entry.timestamp / 3600) % 24;
                            ui.monospace(format!(
                                "[{hrs:02}:{mins:02}:{secs:02}] {}",
                                entry.message
                            ));
                        }
                    });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Watch brain (hardware level)");
                ui.separator();
                ui.label("Ticks:");
                self.tick_filter_ui(ui, "shell_hw_tick_filter");
                if ui.small_button("Clear log").clicked() {
                    self.shell_hw_log.clear();
                }
                if ui.small_button("Copy").clicked() {
                    let text = self
                        .shell_hw_log
                        .entries()
                        .iter()
                        .map(|e| e.message.clone())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let _ = ui_copy_to_clipboard(&text);
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if self.shell_hw_log.is_empty() {
                        ui.weak("(hardware events will appear here)");
                    }
                    for entry in self.shell_hw_log.entries() {
                        if !self.show_main_event(&entry.message) {
                            continue;
                        }
                        let secs = entry.timestamp % 60;
                        let mins = (entry.timestamp / 60) % 60;
                        let hrs = (entry.timestamp / 3600) % 24;
                        ui.monospace(format!("[{hrs:02}:{mins:02}:{secs:02}] {}", entry.message));
                    }
                });
            self.tick_log_ui(ui, "shell_hw_tick_log");
        });
    }

    /// Runs a single shell command. Logs the user-facing command/reply to the
    /// command surface and the simulated hardware-level steps to the brain log.
    /// The commands actually mutate/read the simulated watch (`self.watch`).
    fn run_shell_command(&mut self, cmd: &str) {
        self.shell_log.log(format!("> {cmd}"));
        self.shell_hw_log
            .log("SERCOM3 RX: bytes received".to_string());
        let reply = match simulated_shell_command_name(cmd).unwrap_or("") {
            "help" => {
                self.shell_hw_log
                    .log("shell dispatcher: match(\"help\")".to_string());
                "CMDS: time, settime YYMMDDHHMMSS, drift N, optical".to_string()
            }
            "optical" => {
                self.shell_hw_log
                    .log("optical receiver: disabled; external accessory ADC required".to_string());
                "OPTICAL disabled: SensorUnavailable (LIGHT is a button; external ADC required)"
                    .to_string()
            }
            "time" => {
                let (y, mo, d, h, mi, s, _) = self.watch.get_time();
                self.shell_hw_log
                    .log("rtc->get_time(): RTC_CLOCK read".to_string());
                format!("{:02}{:02}{:02}{:02}{:02}{:02}", y % 100, mo, d, h, mi, s)
            }
            "settime" => {
                let payload = &cmd["settime ".len()..];
                match parse_settime(payload) {
                    Some((year, month, day, hour, minute)) => {
                        self.shell_hw_log.log(format!(
                            "RTC_clock <- write {year:04}-{month:02}-{day:02} {hour:02}:{minute:02}"
                        ));
                        self.watch.set_datetime(year, month, day, hour, minute);
                        self.sync_sim_controller_from_watch();
                        self.shell_hw_log
                            .log("freqcorr/settings: persist settings reg".to_string());
                        "OK".to_string()
                    }
                    None => {
                        self.shell_hw_log
                            .log("RTC_clock <- write FAILED: malformed payload".to_string());
                        "ERR".to_string()
                    }
                }
            }
            "drift" => match parse_drift_command(cmd) {
                Some(ppm) => {
                    let sign = if ppm < 0 { "+" } else { "-" };
                    self.shell_hw_log.log(format!(
                        "RTC_FREQCORR <- sign={} value={} step=0.95ppm",
                        sign,
                        ppm.unsigned_abs()
                    ));
                    "OK".to_string()
                }
                None => {
                    self.shell_hw_log.log(
                        "RTC_FREQCORR <- write FAILED: malformed or out-of-range ppm".to_string(),
                    );
                    "ERR".to_string()
                }
            },
            _ => {
                self.shell_hw_log
                    .log("shell dispatcher: no match -> '?'".to_string());
                "?".to_string()
            }
        };
        self.shell_hw_log
            .log("SERCOM3 TX: reply queued".to_string());
        self.shell_log.log(reply.to_string());
    }

    /// The debug panel: show the background activity log.
    fn debug(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(tr(self.language, Key::DebugOutput));
            ui.label("Ticks:");
            self.tick_filter_ui(ui, "debug_tick_filter");
            let log_response = ui.label("Debug log");
            self.register_anchor(Panel::Debug, AnchorId::DebugLog, &log_response);
            if ui.button(tr(self.language, Key::Clear)).clicked() {
                self.log.clear();
            }
            let response = ui.button("Copy all");
            self.register_anchor(Panel::Debug, AnchorId::DebugCopy, &response);
            if response.clicked() {
                let text = self
                    .log
                    .entries()
                    .iter()
                    .map(|e| e.message.clone())
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = ui_copy_to_clipboard(&text);
                self.status = "Debug log copied".to_string();
            }
            if ui.button("Export log").clicked() {
                let text = self
                    .log
                    .entries()
                    .iter()
                    .map(|e| e.message.clone())
                    .collect::<Vec<_>>()
                    .join("\n");
                match self.export_text_file("debug.log", text) {
                    Ok(path) => {
                        self.status = format!("Log exported to {}", path.display());
                        self.log.log(format!("Log exported to {}", path.display()));
                    }
                    Err(error) => {
                        self.status = format!("Log export failed: {error}");
                        self.log_error(&format!("Log export failed: {error}"));
                    }
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.log.is_empty() {
                    ui.label("(empty)");
                    return;
                }
                // Spreadsheet-style: time in one column, message in the other.
                egui::Grid::new("debug_grid")
                    .striped(true)
                    .spacing([16.0, 2.0])
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.strong("Time");
                        ui.strong("Message");
                        ui.end_row();
                        for entry in self.log.entries() {
                            if !self.show_main_event(&entry.message) {
                                continue;
                            }
                            let secs = entry.timestamp % 60;
                            let mins = (entry.timestamp / 60) % 60;
                            let hrs = (entry.timestamp / 3600) % 24;
                            ui.monospace(format!("{hrs:02}:{mins:02}:{secs:02}"));
                            ui.monospace(&entry.message);
                            ui.end_row();
                        }
                    });
            });
        self.tick_log_ui(ui, "debug_tick_log");
    }

    /// Searchable reference table for known errors, faults, and safe recovery.
    fn error_catalog(&mut self, ui: &mut egui::Ui) {
        ui.heading("Error and fault encyclopedia");
        ui.label(
            "Reference only. Recovery is descriptive; no automatic hardware action is performed.",
        );
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.add(
                egui::TextEdit::singleline(&mut self.catalog_error_search)
                    .hint_text("code, area, or symptom")
                    .desired_width(220.0),
            );
            ui.label("Area:");
            egui::ComboBox::from_id_source("error_catalog_area")
                .selected_text(&self.catalog_error_area)
                .show_ui(ui, |ui| {
                    for area in error_catalog::areas() {
                        ui.selectable_value(&mut self.catalog_error_area, area.to_string(), area);
                    }
                });
        });
        let visible = error_catalog::ENTRIES
            .iter()
            .filter(|entry| {
                error_catalog::matches(entry, &self.catalog_error_search, &self.catalog_error_area)
            })
            .count();
        ui.weak(format!(
            "{visible} of {} entries shown. Expand a row for recovery detail.",
            error_catalog::ENTRIES.len()
        ));
        egui::ScrollArea::both()
            .id_source("error_catalog_table")
            .max_height(430.0)
            .show(ui, |ui| {
                egui::Grid::new("error_catalog_grid")
                    .striped(true)
                    .min_col_width(90.0)
                    .show(ui, |ui| {
                        ui.strong("Code");
                        ui.strong("Area");
                        ui.strong("Meaning");
                        ui.strong("Likely cause");
                        ui.strong("Safe action");
                        ui.strong("Do not do");
                        ui.end_row();
                        for (index, entry) in error_catalog::ENTRIES.iter().enumerate() {
                            if !error_catalog::matches(
                                entry,
                                &self.catalog_error_search,
                                &self.catalog_error_area,
                            ) {
                                continue;
                            }
                            ui.monospace(entry.code);
                            ui.label(entry.area);
                            ui.label(entry.meaning);
                            ui.label(entry.likely_cause);
                            ui.label(entry.safe_action);
                            ui.label(entry.do_not_do);
                            ui.end_row();
                            egui::CollapsingHeader::new(format!("Details for {}", entry.code))
                                .id_source(("error_catalog_detail", index))
                                .show(ui, |ui| {
                                    ui.label(format!("Meaning: {}", entry.meaning));
                                    ui.label(format!("Likely cause: {}", entry.likely_cause));
                                    ui.label(format!("Safe action: {}", entry.safe_action));
                                    ui.label(format!("Do not do: {}", entry.do_not_do));
                                });
                            ui.end_row();
                        }
                    });
            });
    }

    /// The bugs panel: show errors/warnings and app state for troubleshooting.
    fn bugs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Bugs & Diagnostics");
            if ui.button("Error encyclopedia").clicked() {
                self.catalog_error_search.clear();
                self.catalog_error_area = String::from("All");
            }
            if ui.button("Clear").clicked() {
                self.error_log.clear();
            }
            if ui.button("Copy all").clicked() {
                let text = self
                    .error_log
                    .entries()
                    .iter()
                    .map(|e| e.message.clone())
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = ui_copy_to_clipboard(&text);
            }
            let report_response = ui.button("Generate bug report");
            self.register_anchor(Panel::Bugs, AnchorId::BugsReport, &report_response);
            if report_response.clicked() && self.guided_action_allowed(AnchorId::BugsReport) {
                let report = self.build_bug_report();
                let _ = ui_copy_to_clipboard(&report);
                self.status = "Bug report copied to clipboard".to_string();
                self.log.log("Bug report generated and copied to clipboard");
            }
        });
        ui.separator();
        ui.strong("Firmware panic fingerprint");
        ui.label("Resolve the Pxxxxxx value printed by the watch's `panic` shell command against the exact ELF/source build.");
        let mut resolve_fingerprint = false;
        ui.horizontal(|ui| {
            ui.label("Fingerprint:");
            let response = ui.text_edit_singleline(&mut self.panic_fingerprint_input);
            self.register_anchor(Panel::Bugs, AnchorId::BugsFingerprint, &response);
            let resolve_response = ui.button("Resolve");
            self.register_anchor(Panel::Bugs, AnchorId::BugsResolve, &resolve_response);
            resolve_fingerprint = (resolve_response.clicked()
                && self.guided_action_allowed(AnchorId::BugsResolve))
                || (response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && self.guided_action_allowed(AnchorId::BugsResolve));
        });
        if resolve_fingerprint {
            let root = build::firmware_dir();
            let elf = root.join(format!("target/{}/release/sensor-watch", build::TARGET));
            self.panic_resolution = match panic_map::resolve_against_elf(
                &self.panic_fingerprint_input,
                &elf,
            ) {
                Ok(matches) if matches.is_empty() => format!(
                    "No source location matched {} under {}. Check that the source tree matches the ELF build.",
                    self.panic_fingerprint_input.trim(),
                    root.display()
                ),
                Ok(matches) => matches
                    .iter()
                    .map(panic_map::SourceLocation::display)
                    .collect::<Vec<_>>()
                    .join("\n"),
                Err(error) => error,
            };
        }
        if !self.panic_resolution.is_empty() {
            ui.monospace(&self.panic_resolution);
        }
        ui.add_space(8.0);
        let response = ui.text_edit_singleline(&mut self.catalog_error_search);
        self.register_anchor(Panel::Bugs, AnchorId::BugsSearch, &response);
        ui.separator();
        self.error_catalog(ui);
        ui.separator();
        ui.label(
            "Errors and warnings encountered by the app. Use this to report bugs or\n\
             troubleshoot issues. The full activity log is in the Debug tab.",
        );
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.error_log.is_empty() {
                    ui.weak("(no errors recorded)");
                }
                for entry in self.error_log.entries() {
                    let secs = entry.timestamp % 60;
                    let mins = (entry.timestamp / 60) % 60;
                    let hrs = (entry.timestamp / 3600) % 24;
                    ui.monospace(format!("[{hrs:02}:{mins:02}:{secs:02}] {}", entry.message));
                }
            });
    }

    /// The read-only workspace reference browser.
    fn file_browser(&mut self, ui: &mut egui::Ui) {
        let (message, anchors) = self.file_browser.ui(ui);
        for hit in anchors {
            self.help_anchors
                .register(Panel::FileBrowser.help_id(), hit.key.key(), hit.rect);
        }
        if let Some(message) = message {
            self.status = message;
        }
    }

    /// The tutorials panel: a beginner-friendly guide to making watch faces.
    fn tutorials(&mut self, ui: &mut egui::Ui) {
        ui.heading("Tutorials");
        ui.separator();
        ui.label(
            "New to watch faces? Start here. This guide walks you through what a\n\
             watch face is, how the buttons work, and how to build your first one.\n\
             Everything is plain language - no prior coding needed.",
        );
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // What is a watch face?
                let section_response = egui::CollapsingHeader::new("What is a watch face?")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(
                            "A watch face is the screen you see on your watch. It is a small\n\
                             program that decides what to draw and how to react when you\n\
                             press a button.\n\n\
                             The watch can hold several faces at once. You switch between\n\
                             them with the Mode button. Each face is a separate file in the\n\
                             firmware project, and this app lets you create, edit, and test\n\
                             them without touching the rest of the code.",
                        );
                    });
                self.register_anchor(
                    Panel::Tutorials,
                    AnchorId::TutorialSections,
                    &section_response.header_response,
                );
                ui.add_space(6.0);

                // The 3 buttons.
                egui::CollapsingHeader::new("The 3 buttons")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(
                            "The watch has three buttons on the side. In code they are\n\
                             called Light, Mode, and Alarm, but you can think of them as\n\
                             L, C, and A.",
                        );
                        ui.add_space(4.0);
                        ui.monospace("L = Light   (Button::Light)");
                        ui.monospace("C = Mode    (Button::Mode)");
                        ui.monospace("A = Alarm   (Button::Alarm)");
                        ui.add_space(4.0);
                        ui.label(
                            "Your face decides what each button does. For example, the\n\
                             Counter template makes the Alarm button add one to a counter.\n\
                             The watch itself handles the basics (like turning on the\n\
                             backlight), so your face only needs to react to the presses\n\
                             that matter to it.",
                        );
                    });
                ui.add_space(6.0);

                // Your first face.
                egui::CollapsingHeader::new("Your first face")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label("Follow these steps to make and simulate your first face:");
                        ui.add_space(4.0);
                        for (n, step) in [
                            "Open the Editor tab.",
                            "Pick the Counter template.",
                            "Type a name in snake_case, like my_counter.",
                            "Click Generate from template to fill in the code.",
                            "Click Save face. This writes the file and registers it.",
                            "Open the Watch Faces tab and add your face to the active preset.",
                            "Try the face in Simulator. Firmware build and flash are unavailable until the build input contract is complete.",
                        ]
                        .iter()
                        .enumerate()
                        {
                            ui.label(format!("{}. {}", n + 1, step));
                        }
                        ui.add_space(4.0);
                        ui.weak(
                            "Tip: you can test your face in the Simulator tab before\n\
                             building. The simulator shows the faces in the active preset.",
                        );
                    });
                ui.add_space(6.0);

                // The WatchFace trait.
                egui::CollapsingHeader::new("The WatchFace trait")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(
                            "Every face implements the WatchFace trait. A trait is just a\n\
                             list of things your face must be able to do. You only need to\n\
                             fill in these four methods:",
                        );
                        ui.add_space(4.0);
                        ui.monospace("setup    - runs once when the watch boots");
                        ui.monospace("activate - runs when your face appears on screen");
                        ui.monospace("loop_    - runs on every event (tick or button)");
                        ui.monospace("resign   - runs when your face leaves the screen");
                        ui.add_space(4.0);
                        ui.label(
                            "Most of your work goes in loop_. It is called once per second\n\
                             for a tick, and again whenever a button is pressed. Here is a\n\
                             tiny example that shows HELLO and counts presses of the Alarm\n\
                             button:",
                        );
                        ui.add_space(4.0);
                        ui.monospace(
                            "fn loop_(&mut self, event: Event, _settings: &mut Settings) {\n\
                             \x20   match event {\n\
                             \x20       Event::Tick => {\n\
                             \x20           watch::slcd::display_string(\"HELLO\", 0);\n\
                             \x20       }\n\
                             \x20       Event::Button(Button::Alarm, ButtonEvent::Up) => {\n\
                             \x20           self.count += 1;\n\
                             \x20       }\n\
                             \x20       _ => {}\n\
                             \x20   }\n\
                             }",
                        );
                    });
                ui.add_space(6.0);

                // Events.
                egui::CollapsingHeader::new("Events")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(
                            "loop_ receives an Event each time something happens. The most\n\
                             common ones are:",
                        );
                        ui.add_space(4.0);
                        ui.monospace("Event::Tick - once per second");
                        ui.monospace("Event::Button(Button, ButtonEvent) - a button changed");
                        ui.add_space(4.0);
                        ui.label(
                            "A button press produces several ButtonEvent values in order:\n\
                             Down, Up, LongPress, LongUp, and ReallyLongPress. You usually\n\
                             react to Up (a normal press) or LongPress (holding it down).\n\
                             Here is how to tell them apart:",
                        );
                        ui.add_space(4.0);
                        ui.monospace(
                            "Event::Button(Button::Mode, ButtonEvent::Up) => {\n\
                             \x20   // a quick press of the Mode button\n\
                             }\n\
                             Event::Button(Button::Mode, ButtonEvent::LongPress) => {\n\
                             \x20   // the Mode button was held down\n\
                             }",
                        );
                    });
                ui.add_space(6.0);

                // Drawing to the display.
                egui::CollapsingHeader::new("Drawing to the display")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(
                            "The screen is a segment LCD with 10 character positions. You\n\
                             draw by turning segments on. The most useful helpers are:",
                        );
                        ui.add_space(4.0);
                        ui.monospace("display_string(\"TEXT\", 0) - draw text from a position");
                        ui.monospace("display_character(b'A', 0) - draw one character");
                        ui.monospace("set_colon() / clear_colon() - the colon between hours");
                        ui.monospace("set_indicator(Indicator::Bell) - small icons");
                        ui.add_space(4.0);
                        ui.label(
                            "The second argument to display_string is the starting position\n\
                             (0 to 9). Positions 4 and 6 are the two middle digits, so a\n\
                             clock usually draws the hour at position 0 and the minute at\n\
                             position 5. Indicators are the little icons around the edge,\n\
                             like the bell for an alarm or PM.",
                        );
                    });
                ui.add_space(6.0);

                // Common pitfalls.
                egui::CollapsingHeader::new("Common pitfalls")
                    .default_open(false)
                    .show(ui, |ui| {
                        for (n, pitfall) in [
                            "Use snake_case names (my_counter, not MyCounter or my-counter).",
                            "After saving, your face must be registered so it shows up in Watch Faces. Saving normally does this for you.",
                            "If the build fails, the error message tells you which file and line. Fix that before flashing.",
                            "Remember to add your face to the active preset, or it will not be built into the firmware.",
                            "Test in the Simulator first - it is much faster than building and flashing every time.",
                        ]
                        .iter()
                        .enumerate()
                        {
                            ui.label(format!("{}. {}", n + 1, pitfall));
                        }
                    });
            });
    }

    /// The wiki panel: a built-in reference browser for project concepts.
    fn wiki(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Wiki");
            if ui.button("Error encyclopedia").clicked() {
                self.current_panel = Panel::Bugs;
            }
            let response = ui.button("Back");
            self.register_anchor(Panel::Wiki, AnchorId::WikiNavigation, &response);
            if response.clicked() {
                self.wiki.back();
                self.log.log("Wiki: back".to_string());
            }
            if ui.button("Home").clicked() {
                self.wiki.history.clear();
                self.wiki.current = String::from("Wiki Home");
                self.log.log("Wiki: home".to_string());
            }
            ui.separator();
            if ui
                .add_enabled(
                    self.wiki.current != "Wiki Home",
                    egui::Button::new("Previous"),
                )
                .clicked()
            {
                self.wiki.previous_page();
                self.log.log(format!("Wiki: opened {}", self.wiki.current));
            }
            if ui
                .add_enabled(
                    self.wiki.current
                        != self
                            .wiki
                            .pages
                            .last()
                            .map(|page| page.title.as_str())
                            .unwrap_or_default(),
                    egui::Button::new("Next"),
                )
                .clicked()
            {
                self.wiki.next_page();
                self.log.log(format!("Wiki: opened {}", self.wiki.current));
            }
        });
        ui.separator();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Browse repos").strong());
            if ui.button("Sensor-Watch").clicked() {
                let _ = webbrowser::open("https://github.com/joeycastillo/Sensor-Watch");
            }
            if ui
                .button("Second Movement")
                .on_hover_text("The second-movement firmware variant")
                .clicked()
            {
                let _ = webbrowser::open(
                    "https://github.com/joeycastillo/Sensor-Watch/tree/main/movement2",
                );
            }
            if ui.button("Author's repo").clicked() {
                let _ = webbrowser::open("https://github.com/kaiiuen/sensor-watch-rs");
            }
        });
        ui.separator();

        // Allocate the complete remaining central-panel area before creating
        // the panes, so both scroll areas can use its full height.
        let available = ui.available_size();
        ui.allocate_ui_with_layout(
            available,
            egui::Layout::left_to_right(egui::Align::TOP),
            |ui| {
                egui::SidePanel::left("wiki_pages")
                    .resizable(true)
                    .default_width((available.x * 0.28).clamp(200.0, 320.0))
                    .width_range(180.0..=available.x * 0.6)
                    .show_inside(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.wiki.search)
                                .hint_text("Search pages...")
                                .desired_width(f32::INFINITY),
                        );
                        self.register_anchor(Panel::Wiki, AnchorId::WikiSearch, &response);
                        ui.add_space(4.0);
                        let query = self.wiki.search.to_lowercase();
                        let mut clicked: Option<String> = None;
                        let mut visible_pages = 0;
                        egui::ScrollArea::vertical()
                            .id_source("wiki_page_list")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height())
                            .show(ui, |ui| {
                                for page in &self.wiki.pages {
                                    if !query.is_empty()
                                        && !page.title.to_lowercase().contains(&query)
                                    {
                                        continue;
                                    }
                                    visible_pages += 1;
                                    if ui
                                        .selectable_label(
                                            self.wiki.current == page.title,
                                            &page.title,
                                        )
                                        .clicked()
                                    {
                                        clicked = Some(page.title.clone());
                                    }
                                }
                                if visible_pages == 0 {
                                    ui.weak("No wiki pages match your search.");
                                }
                            });
                        if let Some(title) = clicked {
                            self.wiki.navigate(&title);
                            self.log.log(format!("Wiki: opened {}", title));
                        }
                    });

                egui::CentralPanel::default().show_inside(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    if let Some(page) = self.wiki.current_page().map(|p| p.title.clone()) {
                        let body = self
                            .wiki
                            .current_page()
                            .map(|p| p.body.clone())
                            .unwrap_or_default();
                        let links = self
                            .wiki
                            .current_page()
                            .map(|p| p.links.clone())
                            .unwrap_or_default();
                        ui.heading(&page);
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_source("wiki_article")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height())
                            .show(ui, |ui| {
                                let mut rest = body.as_str();
                                loop {
                                    let start = rest.find("[[");
                                    let text = match start {
                                        Some(index) => &rest[..index],
                                        None => rest,
                                    };
                                    for line in text.lines() {
                                        ui.add(egui::Label::new(line.trim()).wrap(true));
                                    }
                                    match start {
                                        Some(index) => {
                                            let rest_after = &rest[index + 2..];
                                            if let Some(end) = rest_after.find("]]") {
                                                let target = &rest_after[..end];
                                                if ui.button(target).clicked() {
                                                    self.wiki.navigate(target);
                                                    self.log
                                                        .log(format!("Wiki: opened {}", target));
                                                }
                                                rest = &rest_after[end + 2..];
                                            } else {
                                                ui.add(egui::Label::new(rest_after).wrap(true));
                                                break;
                                            }
                                        }
                                        None => break,
                                    }
                                }
                                if !links.is_empty() {
                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.strong("Related pages");
                                    ui.horizontal_wrapped(|ui| {
                                        for link in &links {
                                            if ui.button(link).clicked() {
                                                self.wiki.navigate(link);
                                                self.log.log(format!("Wiki: opened {}", link));
                                            }
                                        }
                                    });
                                }
                            });
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.add_space(ui.available_height() * 0.35);
                            ui.heading("No wiki page selected");
                            ui.label("Choose a page from the list or clear the search filter.");
                        });
                    }
                });
            },
        );
    }

    /// The simulator panel: render the watch and handle its buttons.
    fn simulator(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(
            "Simulate the watch: press the on-screen buttons, adjust the date/time,\n\
             and watch the active face react. The simulator reflects the faces and\n\
             settings configured in the Watch Faces tab.",
        );
        ui.separator();
        ui.heading("Simulation provenance");
        ui.label(sim_provenance::STATUS);
        ui.weak(sim_provenance::LIMITATIONS);
        ui.separator();

        // The simulator body can be taller than the window (especially with the
        // date controller, debug log, and a large watch rendering all expanded),
        // so wrap it in a scroll area that shows a scrollbar on overflow. The
        // watch buttons inside draw_watch use pointer mapping against the
        // allocated rect, which still works inside the scroll area.
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let watch_response = ui.label("Watch preview");
                self.register_anchor(Panel::Simulator, AnchorId::SimulatorWatch, &watch_response);
                ui.horizontal(|ui| {
                    ui.heading("Simulator");
                    ui.separator();
                    // Adjustable size slider.
                    ui.label("Size:");
                    ui.add(
                        egui::Slider::new(&mut self.sim_scale, 0.5..=2.0)
                            .step_by(0.1)
                            .suffix("x"),
                    );
                    if ui.button("Reset").clicked() {
                        self.sim_scale = 0.5;
                        self.sim_log.log("Sim size reset".to_string());
                    }
                    ui.separator();
                    // Show which preset face is being simulated.
                    let faces = self.presets.active_faces();
                    let idx = self.sim_face_idx.min(faces.len().saturating_sub(1));
                    // Dual counters: what the sim thinks it's on vs the actual face.
                    ui.label(format!(
                        "Sim face: {} / {}",
                        if faces.is_empty() { 0 } else { idx + 1 },
                        faces.len()
                    ));
                    if !faces.is_empty() {
                        ui.monospace(&faces[idx]);
                    }
                    ui.separator();
                    ui.label(format!("Engine face: {}", self.face_engine.face_name));
                    ui.separator();
                    // This is the result of the most recently completed render;
                    // draw_watch updates it only after texture creation succeeds.
                    ui.label(format!(
                        "Current face render path: {}",
                        if self.last_render_used_real {
                            "actual firmware face source via host seam and MockHw"
                        } else {
                            "face_sim approximation fallback"
                        }
                    ));
                    ui.separator();
                    // Fuzz the current face.
                    if ui
                        .button("Fuzz face")
                        .on_hover_text(
                            "Run randomized button/tick sequences through the current face to\n\
                 check it never panics or produces a broken display.",
                        )
                        .clicked()
                    {
                        let name = if faces.is_empty() {
                            "SIMPLE_CLOCK".to_string()
                        } else {
                            faces[idx].clone()
                        };
                        let iters = self.fuzz_iterations;
                        let seed = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(0);
                        match fuzz::fuzz_face(&name, iters, seed) {
                            Ok(n) => {
                                self.status = format!("Fuzz passed: {n} iterations on {name}");
                                self.sim_log
                                    .log(format!("Fuzz passed: {n} iters on {name}"));
                                self.log
                                    .log(format!("Fuzz passed: {n} iterations on {name}"));
                            }
                            Err(e) => {
                                self.status = format!("Fuzz failed: {e}");
                                self.sim_log.log(format!("Fuzz failed: {e}"));
                                self.log_error(&format!("Fuzz failed: {e}"));
                            }
                        }
                    }
                });
                ui.separator();

                // Date/time controller: set the simulated display without tedious
                // button mashing.
                let date_response = ui.collapsing("Date / time controller (PC local)", |ui| {
                    ui.weak("Reset to now uses the host PC's local civil time. Apply date/time stays deterministic and is not timezone-converted.");
                    egui::Grid::new("sim_date_grid")
                        .spacing([12.0, 6.0])
                        .num_columns(4)
                        .show(ui, |ui| {
                            ui.label("Year");
                            ui.add(
                                egui::DragValue::new(&mut self.sim_year).clamp_range(1970..=2100),
                            );
                            ui.label("Month");
                            ui.add(egui::DragValue::new(&mut self.sim_month).clamp_range(1..=12));
                            ui.end_row();
                            ui.label("Day");
                            ui.add(egui::DragValue::new(&mut self.sim_day).clamp_range(1..=31));
                            ui.label("Weekday");
                            egui::ComboBox::from_id_source("sim_weekday")
                                .selected_text(sim_weekday_name(self.sim_weekday))
                                .show_ui(ui, |ui| {
                                    for (i, n) in SIM_WEEKDAY_NAMES.iter().enumerate() {
                                        ui.selectable_value(&mut self.sim_weekday, i, *n);
                                    }
                                });
                            ui.end_row();
                            ui.label("Hour");
                            ui.add(egui::DragValue::new(&mut self.sim_hour).clamp_range(0..=23));
                            ui.label("Minute");
                            ui.add(egui::DragValue::new(&mut self.sim_minute).clamp_range(0..=59));
                            ui.end_row();
                        });
                    ui.horizontal(|ui| {
                        let response = ui.button("Apply date/time");
                        self.register_anchor(Panel::Simulator, AnchorId::SimulatorApply, &response);
                        if response.clicked()
                            && self.guided_action_allowed(AnchorId::SimulatorApply)
                        {
                            self.watch.set_datetime(
                                self.sim_year,
                                self.sim_month,
                                self.sim_day,
                                self.sim_hour,
                                self.sim_minute,
                            );
                            self.sync_sim_controller_from_watch();
                            self.sim_log.log(format!(
                                "Set date/time to {}-{:02}-{:02} {:02}:{:02}",
                                self.sim_year,
                                self.sim_month,
                                self.sim_day,
                                self.sim_hour,
                                self.sim_minute
                            ));
                            self.log.log(format!(
                                "Sim date set to {}-{:02}-{:02} {:02}:{:02}",
                                self.sim_year,
                                self.sim_month,
                                self.sim_day,
                                self.sim_hour,
                                self.sim_minute
                            ));
                        }
                        // Changing the weekday only overrides the weekday; it does not
                        // touch the date/time.
                        if ui.button("Apply weekday").clicked() {
                            self.sim_weekday = clamp_sim_weekday(self.sim_weekday);
                            self.watch.weekday_override = Some(self.sim_weekday as u32);
                            self.sim_log
                                .log(format!("Weekday set to {}", self.sim_weekday));
                            self.log.log(format!(
                                "Weekday set to {}",
                                sim_weekday_name(self.sim_weekday)
                            ));
                        }
                        if ui.button("Reset to now").clicked() {
                            self.watch.reset_to_now();
                            self.sync_sim_controller_from_watch();
                            self.sim_log.log("Reset to host PC local time".to_string());
                            self.log.log("Sim date reset to host PC local time");
                        }
                    });
                    ui.separator();
                });
                self.register_anchor(
                    Panel::Simulator,
                    AnchorId::SimulatorDate,
                    &date_response.header_response,
                );

                // Simulator debug log: under the sim bar / date controller, showing
                // button presses, face switches, and sim actions.
                egui::CollapsingHeader::new("Simulator debug log")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("Clear").clicked() {
                                self.sim_log.clear();
                            }
                            if ui.small_button("Copy").clicked() {
                                let text = self
                                    .sim_log
                                    .entries()
                                    .iter()
                                    .map(|e| e.message.clone())
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                let _ = ui_copy_to_clipboard(&text);
                            }
                        });
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .stick_to_bottom(true)
                            .max_height(100.0)
                            .show(ui, |ui| {
                                if self.sim_log.is_empty() {
                                    ui.weak("(no sim activity yet)");
                                }
                                for entry in self.sim_log.entries() {
                                    let secs = entry.timestamp % 60;
                                    let mins = (entry.timestamp / 60) % 60;
                                    let hrs = (entry.timestamp / 3600) % 24;
                                    ui.monospace(format!(
                                        "[{hrs:02}:{mins:02}:{secs:02}] {}",
                                        entry.message
                                    ));
                                }
                            });
                    });

                self.draw_watch(ui, ctx, self.sim_scale);
            });
    }

    /// Draws the watch SVG at the given scale with clickable F-91W button hotspots.
    fn draw_watch(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, scale: f32) {
        // Track frame time for hold timing.
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.sim_last_tick);
        self.sim_last_tick = now;
        self.sim_dt = dt.as_secs_f32();

        // Update the display state.
        self.watch.update_display();

        // Sync the sim's clock mode with the watch settings (12/24), and keep
        // the face engine (which renders the display) in sync with it.
        let want_24 = self.watch_config.clock_mode_24h;
        let is_24 = self.watch.time_mode == watch_sim::TimeMode::H24;
        if want_24 != is_24 {
            self.watch.time_mode = if want_24 {
                watch_sim::TimeMode::H24
            } else {
                watch_sim::TimeMode::H12
            };
        }
        self.face_engine.time_mode_24 = self.watch.time_mode == watch_sim::TimeMode::H24;

        // Determine the current face and sync the engine's face name.
        let faces = self.presets.active_faces();
        let face_name = if faces.is_empty() {
            "SIMPLE_CLOCK".to_string()
        } else {
            faces[self.sim_face_idx.min(faces.len() - 1)].clone()
        };
        if self.face_engine.face_name != face_name {
            self.cancel_simulator_buttons();
            self.face_engine = face_sim::FaceEngine::new(&face_name);
            // (Re)build the real-face engine for the new face. Faces that have
            // not yet been migrated through the firmware seam stay `None` and
            // the simulator falls back to `face_engine` below.
            if self
                .real_face
                .as_ref()
                .map(|r| faces::face_identity(r.face_name()))
                .unwrap_or_else(|| faces::face_identity(&face_name))
                != faces::face_identity(&face_name)
            {
                // Drop the old seam guard before constructing the replacement;
                // `RealFace::new` must acquire the same global lock.
                self.real_face.take();
                self.real_face = real_face::RealFace::new(&face_name);
                self.active_real_face_name = None;
                self.active_real_mode_24 = None;
            }
        }
        // Advance the face state by one second per real second.
        self.face_tick_accum += self.sim_dt;
        while self.face_tick_accum >= 1.0 {
            self.face_tick_accum -= 1.0;
            self.face_engine.tick();
            // The real firmware face receives the same one-Hz event, but only
            // after activation. `set_time` updates RTC state without creating
            // synthetic ticks for arbitrary edits or frame redraws.
            if let Some(real) = self.real_face.as_mut() {
                if real.is_activated() {
                    real.tick();
                }
            }
            // Route the high-frequency event without touching the main logs when
            // ticks are hidden. This keeps the per-frame path allocation-free.
            self.record_tick();
        }

        // Use the watch's live simulated time so the clock ticks and the date
        // controller (set_datetime / reset to now) takes effect.
        let (t_year, t_month, t_day, t_hour, t_minute, t_second, t_weekday) = self.watch.get_time();
        let sim_time = face_sim::SimTime {
            year: t_year,
            month: t_month,
            day: t_day,
            hour: t_hour,
            minute: t_minute,
            second: t_second,
            weekday: t_weekday,
        };

        // Prefer the REAL face when available; that keeps what the Simulator
        // renders in lockstep with the firmware (no drift from `face_sim`). When
        // a face has not yet been migrated into the seam, fall back to the
        // hand-written engine (and log the fallback).
        let mode_24 = self.watch.time_mode == watch_sim::TimeMode::H24;
        let active_real_face_name = self.active_real_face_name.clone();
        let active_real_mode_24 = self.active_real_mode_24;
        // Pass the selected civil fields through unchanged. The firmware seam
        // validates its supported 2020-2083 RTC range and declines invalid edits.
        let real_result = self.real_face.as_mut().map(|real| {
            let valid_time =
                real.set_time(t_year as u32, t_month, t_day, t_hour, t_minute, t_second);
            let face_name = real.face_name().to_string();
            let face_changed = active_real_face_name.as_deref().map(faces::face_identity)
                != Some(faces::face_identity(&face_name));
            let mode_changed = active_real_mode_24 != Some(mode_24);
            if valid_time && (face_changed || mode_changed) {
                real.activate(mode_24);
            }
            (valid_time, face_name, real.snapshot())
        });
        let (used_real, real_snapshot) =
            if let Some((valid_time, face_name, snapshot)) = real_result {
                if valid_time {
                    self.active_real_face_name = Some(face_name);
                    self.active_real_mode_24 = Some(mode_24);
                }
                (valid_time, Some(snapshot))
            } else {
                self.active_real_face_name = None;
                self.active_real_mode_24 = None;
                (false, None)
            };
        let fd = if used_real {
            let snap = real_snapshot.expect("real face snapshot when real rendering is active");
            face_sim::FaceDisplay {
                chars: snap.chars,
                colon: snap.colon,
                signal: snap.signal,
                bell: snap.bell,
                pm: snap.pm,
                h24: snap.h24,
                lap: snap.lap,
            }
        } else {
            self.face_engine.render(&sim_time)
        };
        let mut svg_display = watch_display::face_display_to_svg(&fd);
        // Log which render path produced this frame so the user can tell when a
        // face is running the REAL firmware seam vs. the face_sim fallback. Use
        // the tail of the log (the message is idempotent) and only append on a
        // path change or spawn so the Simulator debug log does not flood.
        if self.last_render_used_real != used_real {
            self.last_render_used_real = used_real;
            self.sim_log.log(if used_real {
                format!("Render path: REAL face via seam ({face_name})")
            } else {
                format!("Render path: face_sim fallback ({face_name})")
            });
        }
        // Apply the watch's light and CASIO-override state, which the face
        // engine does not model.
        svg_display.light = self.watch.light;
        // Honor the "show seconds" setting: blank the seconds digits when off.
        if !self.watch_config.show_seconds {
            svg_display.second_2 = ' ';
            svg_display.second_1 = ' ';
        }
        if let Some(text) = &self.watch.override_text {
            svg_display.apply_text_override(text);
        }

        // Render the watch SVG at a size based on the scale.
        let base = 740u32;
        let size = [(base as f32 * scale) as u32, (655.0 * scale) as u32];
        let texture =
            watch_display::render_to_texture(&mut self.watch_renderer, &svg_display, size, ctx);
        let aspect = 1480.0 / 1311.0;
        let w = size[0] as f32;
        let h = w / aspect;

        // If rendering failed, skip drawing the watch but keep the rest of the
        // simulator working rather than crashing.
        let Some(texture) = texture else {
            self.cancel_simulator_buttons();
            return;
        };
        // This is the last path that actually produced a rendered frame, rather
        // than merely the path that was selected before rasterization.
        self.last_render_used_real = used_real;

        // Allocate the image rect so we can map clicks to SVG button hotspots.
        let (rect, response) = ui.allocate_exact_size(egui::Vec2::new(w, h), egui::Sense::click());
        self.register_anchor(Panel::Simulator, AnchorId::SimulatorWatch, &response);
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        // Determine which button is being held. Use the GLOBAL pointer state so
        // holding stays active even if the pointer drifts off the small widget.
        // The held button locks on press and only clears when the mouse is
        // fully released.
        // The card owns pointer routing while a guided tour is visible. In
        // particular, do not let a global pointer query turn a card click into
        // L/C/A input, light, CASIO mode, face cycling, or real-face events.
        let tour_owns_input = self.help_owns_input();
        let pointer_down = !tour_owns_input && ui.input(|i| i.pointer.primary_down());
        let pointer_pos = (!tour_owns_input)
            .then(|| ui.input(|i| i.pointer.interact_pos()))
            .flatten();

        // Buttons: mirror the physical F-91W layout (L top-left, C bottom-left,
        // A right). Each press fires exactly once on the press edge.
        ui.add_space(8.0);
        egui::Grid::new("sim_buttons")
            .spacing([16.0, 4.0])
            .num_columns(3)
            .min_col_width(170.0)
            .show(ui, |ui| {
                // Header.
                ui.strong("L (top-left)");
                ui.strong("C (bottom-left)");
                ui.strong("A (right)");
                ui.end_row();
                // Custom-drawn clickable regions (never grey out while held).
                let l = sim_hold_button(ui, "Hold");
                let c = sim_hold_button(ui, "Hold");
                let a = sim_hold_button(ui, "Hold");

                // Determine which button is under the pointer right now.
                let mut under = None;
                if pointer_down {
                    if let Some(pos) = pointer_pos {
                        // On-watch SVG hotspots.
                        if rect.contains(pos) {
                            let svg_x = (pos.x - rect.min.x) / rect.width() * 1480.0;
                            let svg_y = (pos.y - rect.min.y) / rect.height() * 1311.0;
                            let is_hit = |cx: f32, cy: f32, r: f32| {
                                let dx = svg_x - cx;
                                let dy = svg_y - cy;
                                dx * dx + dy * dy <= r * r
                            };
                            if is_hit(1355.0, 811.0, 125.0) {
                                under = Some(ButtonId::A);
                            } else if is_hit(125.0, 813.0, 125.0) {
                                under = Some(ButtonId::C);
                            } else if is_hit(125.0, 511.0, 125.0) {
                                under = Some(ButtonId::L);
                            }
                        }
                        // Hold buttons.
                        if under.is_none() {
                            if l.rect.contains(pos) {
                                under = Some(ButtonId::L);
                            } else if c.rect.contains(pos) {
                                under = Some(ButtonId::C);
                            } else if a.rect.contains(pos) {
                                under = Some(ButtonId::A);
                            }
                        }
                    }
                }

                // Lock onto the first button pressed; keep it held until the
                // mouse is fully released. A visible tour has already cancelled
                // state and skips all simulator input routing below. A cancelled
                // press cannot reacquire a button until a real pointer-up has
                // been observed.
                if tour_owns_input {
                    self.cancel_simulator_buttons();
                } else {
                    update_simulator_pointer_lock(
                        &mut self.blocked_until_pointer_release,
                        &mut self.held_button,
                        pointer_down,
                        under,
                    );
                }
                let l_down = self.held_button == Some(ButtonId::L);
                let c_down = self.held_button == Some(ButtonId::C);
                let a_down = self.held_button == Some(ButtonId::A);

                let l_act = if tour_owns_input {
                    SimAction::None
                } else {
                    handle_sim_button(
                        l_down,
                        &mut self.btn_l_down,
                        &mut self.btn_l_hold,
                        self.sim_dt,
                    )
                };
                let c_act = if tour_owns_input {
                    SimAction::None
                } else {
                    handle_sim_button(
                        c_down,
                        &mut self.btn_c_down,
                        &mut self.btn_c_hold,
                        self.sim_dt,
                    )
                };
                let a_act = if tour_owns_input {
                    SimAction::None
                } else {
                    handle_sim_button(
                        a_down,
                        &mut self.btn_a_down,
                        &mut self.btn_a_hold,
                        self.sim_dt,
                    )
                };
                // Deliver threshold-aware public events to migrated firmware
                // faces. The face_sim path below intentionally keeps its
                // existing short-press semantics.
                if !tour_owns_input {
                    if let Some(event) = self.btn_l_events.update(l_down, self.sim_dt) {
                        if let Some(real) = self.real_face.as_mut() {
                            real.button_event(real_face::RealButton::Light, event);
                        }
                    }
                    if let Some(event) = self.btn_a_events.update(a_down, self.sim_dt) {
                        if let Some(real) = self.real_face.as_mut() {
                            real.button_event(real_face::RealButton::Alarm, event);
                        }
                    }
                }
                // L button: toggle the backlight while held, and act as the
                // face's Light button on press.
                match l_act {
                    SimAction::Press => {
                        self.watch.light = true;
                        self.face_engine.press(face_sim::FaceButton::Light);

                        self.sim_log.log("L: press (Light)".to_string());
                    }
                    SimAction::Release => {
                        self.watch.light = false;
                        self.sim_log.log("L: release".to_string());
                    }
                    SimAction::None => {}
                }
                // C button: cycle through the preset's watch faces on press.
                if c_act == SimAction::Press {
                    let faces = self.presets.active_faces();
                    if !faces.is_empty() {
                        self.sim_face_idx = (self.sim_face_idx + 1) % faces.len();
                        let name = faces[self.sim_face_idx].clone();
                        self.log.log(format!("Simulating face: {name}"));
                        self.sim_log.log(format!("C: cycle -> face {name}"));
                    }
                }
                // A button: toggle 12/24 on a clean press, and act as the face's
                // Alarm button on press.
                if a_act == SimAction::Press {
                    self.watch.toggle_time_mode();
                    // Keep the watch settings in sync so the per-frame config
                    // sync doesn't override the toggle.
                    self.watch_config.clock_mode_24h =
                        self.watch.time_mode == watch_sim::TimeMode::H24;
                    self.face_engine.time_mode_24 =
                        self.watch.time_mode == watch_sim::TimeMode::H24;
                    self.face_engine.press(face_sim::FaceButton::Alarm);

                    self.sim_log
                        .log(if self.watch.time_mode == watch_sim::TimeMode::H24 {
                            "A: press (12/24 -> 24h, Alarm)".to_string()
                        } else {
                            "A: press (12/24 -> 12h, Alarm)".to_string()
                        });
                }
                // A button: holding for ~1s shows the CASIO logo for as long as
                // it's held; releasing returns to the time display.
                if self.btn_a_down {
                    if self.btn_a_hold >= 1.0 {
                        self.watch.set_casio(true);
                    }
                } else {
                    self.watch.set_casio(false);
                }
                ui.end_row();
                // Instructions row (fixed size so text changes don't shift layout).
                ui.add_sized(egui::vec2(170.0, 40.0), egui::Label::new("Backlight"));
                ui.add_sized(
                    egui::vec2(170.0, 40.0),
                    egui::Label::new("Cycle watch face"),
                );
                ui.add_sized(
                    egui::vec2(170.0, 40.0),
                    egui::Label::new("12/24 hour\nHold for CASIO"),
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.label(format!("Time mode: {:?}", self.watch.time_mode));
        ui.label(format!("Light: {}", self.watch.light));
        ui.label(format!(
            "Buzzer: {:.1} V ({})   Board: {}",
            self.watch_config.piezo_voltage,
            ["Click", "Beep", "Double", "Chime"][self.watch_config.buzzer_type as usize % 4],
            self.board.label()
        ));
        ui.label(format!(
            "LED: {}  Gradient: {}",
            self.watch_config.led_color_hex,
            if self.watch_config.led_gradient {
                "on"
            } else {
                "off"
            }
        ));

        // Keep the simulator responsive for button holds and sub-second faces,
        // without repainting the rest of the application at display rate.
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    /// Runs a command typed into the terminal.
    /// Simulates a single button press on the active face (used by the terminal).
    fn press_sim_button(&mut self, btn: ButtonId) {
        match btn {
            ButtonId::L => {
                self.watch.light = true;
                self.face_engine.press(face_sim::FaceButton::Light);
                if let Some(real) = self.real_face.as_mut() {
                    real.button_event(
                        real_face::RealButton::Light,
                        real_face::RealButtonEvent::Down,
                    );
                    real.button_event(real_face::RealButton::Light, real_face::RealButtonEvent::Up);
                }
                self.watch.light = false;
            }
            ButtonId::C => {
                let faces = self.presets.active_faces();
                if !faces.is_empty() {
                    self.sim_face_idx = (self.sim_face_idx + 1) % faces.len();
                }
            }
            ButtonId::A => {
                self.watch.toggle_time_mode();
                self.watch_config.clock_mode_24h = self.watch.time_mode == watch_sim::TimeMode::H24;
                self.face_engine.time_mode_24 = self.watch.time_mode == watch_sim::TimeMode::H24;
                self.face_engine.press(face_sim::FaceButton::Alarm);
                if let Some(real) = self.real_face.as_mut() {
                    real.button_event(
                        real_face::RealButton::Alarm,
                        real_face::RealButtonEvent::Down,
                    );
                    real.button_event(real_face::RealButton::Alarm, real_face::RealButtonEvent::Up);
                }
            }
        }
    }

    fn sync_sim_controller_from_watch(&mut self) {
        let (year, month, day, hour, minute, _, weekday) = self.watch.get_time();
        self.sim_year = year;
        self.sim_month = month;
        self.sim_day = day;
        self.sim_hour = hour;
        self.sim_minute = minute;
        self.sim_weekday = clamp_sim_weekday(weekday as usize);
    }

    fn run_terminal_command(&mut self, cmd: &str) {
        if self.shutting_down || cmd.trim().is_empty() {
            return;
        }
        if cmd.len() > 128 || !cmd.is_ascii() || cmd.bytes().any(|b| b < 0x20 && b != b'\t') {
            self.push_terminal("ERR malformed command (ASCII, <=128 bytes required)");
            return;
        }
        self.push_terminal(format!("> {cmd}"));
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }
        match parts[0].to_lowercase().as_str() {
            "help" => {
                self.push_terminal(
                    "Commands: help, status, faces, board, build, flash, fuzz, time,\
                     settime YYMMDDHHMMSS, clear, modules, errors, bugreport, sim, theme, lang"
                        .to_string(),
                );
            }
            "status" => {
                self.push_terminal(format!(
                    "Panel: {:?}, Board: {:?}",
                    self.current_panel, self.board
                ));
            }
            "faces" => {
                let faces = self.presets.active_faces();
                self.push_terminal(format!("{} faces in active preset", faces.len()));
                for f in faces {
                    self.push_terminal(format!("  {f}"));
                }
            }
            "build" => {
                if self.building || self.pending_build.is_some() || self.flash_busy() {
                    self.push_terminal("Build or flash already running");
                } else {
                    self.start_build();
                }
            }
            "flash" => {
                if self.building || self.pending_build.is_some() {
                    self.push_terminal("Build in progress; flash unavailable");
                } else if let Some(approved) = self.approved_artifact.clone() {
                    self.copy_to_watch(&approved);
                } else {
                    self.push_terminal("No build yet".to_string());
                }
            }
            "fuzz" => {
                let faces = self.presets.active_faces();
                let name = if faces.is_empty() {
                    "SIMPLE_CLOCK".to_string()
                } else {
                    faces[self.sim_face_idx.min(faces.len() - 1)].clone()
                };
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);
                match fuzz::fuzz_face(&name, self.fuzz_iterations, seed) {
                    Ok(n) => self.push_terminal(format!("Fuzz passed: {n} iterations on {name}")),
                    Err(e) => self.push_terminal(format!("Fuzz failed: {e}")),
                }
            }
            "time" => {
                let (_, _, _, h, m, s, _) = self.watch.get_time();
                self.push_terminal(format!("Sim time: {h:02}:{m:02}:{s:02}"));
            }
            "settime" => {
                if parts.len() != 2 {
                    self.push_terminal("Usage: settime YYMMDDHHMMSS");
                    return;
                }
                // Drive the simulated watch from the terminal, mirroring the
                // shell sim's settime so both surfaces stay in sync with the
                // Simulator.
                let payload = parts.get(1).copied().unwrap_or("");
                match parse_settime(payload) {
                    Some((year, month, day, hour, minute)) => {
                        self.watch.set_datetime(year, month, day, hour, minute);
                        self.sync_sim_controller_from_watch();
                        self.push_terminal(format!(
                            "Sim time set to {year:04}-{month:02}-{day:02} {hour:02}:{minute:02}"
                        ));
                    }
                    None => self.push_terminal("Usage: settime YYMMDDHHMMSS"),
                }
            }
            "board" => {
                if let Some(b) = parts.get(1) {
                    let next = match b.to_lowercase().as_str() {
                        "green" => Some(Board::Green),
                        "red" | "lite" => Some(Board::RedLite),
                        "blue" => Some(Board::Blue),
                        "pro" => Some(Board::Pro),
                        _ => None,
                    };
                    if let Some(nb) = next {
                        self.board = nb;
                        self.save_settings_internal();
                        self.push_terminal(format!("Board set to {}", nb.label()));
                    } else {
                        self.push_terminal("Unknown board (green/red/blue/pro)");
                    }
                } else {
                    self.push_terminal(format!("Board: {}", self.board.label()));
                }
            }
            "clear" => self.terminal_history.clear(),
            "modules" => {
                if self.modules.modules.is_empty() {
                    self.push_terminal("No custom modules registered");
                }
                let module_lines: Vec<String> = self
                    .modules
                    .modules
                    .iter()
                    .map(|m| {
                        format!(
                            "{} [{}] {}",
                            if m.enabled { "ON " } else { "OFF" },
                            m.target,
                            m.name
                        )
                    })
                    .collect();
                for line in module_lines {
                    self.push_terminal(line);
                }
            }
            "errors" => {
                let n = self.error_log.entries().len();
                self.push_terminal(format!("{n} errors recorded"));
                let error_lines: Vec<String> = self
                    .error_log
                    .entries()
                    .iter()
                    .rev()
                    .take(10)
                    .map(|e| format!("  {}", e.message))
                    .collect();
                for line in error_lines {
                    self.push_terminal(line);
                }
            }
            "bugreport" => {
                let report = self.build_bug_report();
                let _ = ui_copy_to_clipboard(&report);
                self.push_terminal("Bug report copied to clipboard");
            }
            "sim" => {
                if let Some(b) = parts.get(1) {
                    let btn = match b.to_lowercase().as_str() {
                        "a" => Some(ButtonId::A),
                        "b" | "l" => Some(ButtonId::L),
                        "c" | "m" => Some(ButtonId::C),
                        _ => None,
                    };
                    if let Some(btn) = btn {
                        self.press_sim_button(btn);
                        self.push_terminal(format!("Pressed {:?}", btn));
                    } else {
                        self.push_terminal("Unknown button (a/b/c)");
                    }
                } else {
                    self.push_terminal("Usage: sim <a|b|c>".to_string());
                }
            }
            "theme" => {
                if let Some(t) = parts.get(1) {
                    let next = Theme::ALL
                        .iter()
                        .find(|t2| t2.name().to_lowercase() == t.to_lowercase());
                    if let Some(t2) = next {
                        self.theme = *t2;
                        self.push_terminal(format!("Theme set to {}", t2.name()));
                    } else {
                        self.push_terminal("Unknown theme".to_string());
                    }
                } else {
                    self.push_terminal(format!("Theme: {}", self.theme.name()));
                }
            }
            "lang" => {
                if let Some(l) = parts.get(1) {
                    let next = Language::ALL
                        .iter()
                        .find(|l2| l2.name().to_lowercase() == l.to_lowercase());
                    if let Some(l2) = next {
                        self.language = *l2;
                        self.push_terminal(format!("Language set to {}", l2.name()));
                    } else {
                        self.push_terminal("Unknown language (English/简体中文/繁體中文)");
                    }
                } else {
                    self.push_terminal(format!("Language: {}", self.language.name()));
                }
            }
            _ => self.push_terminal(format!("Unknown command: {cmd} (try 'help')")),
        }
    }

    fn master_clock_executable(&self) -> Option<std::path::PathBuf> {
        if let Some(path) = self.package_status.master_clock.as_ref() {
            return Some(path.clone());
        }
        if self.package_status.mode == distribution::DistributionMode::Developer
            && !self.master_clock_path.trim().is_empty()
        {
            return master_clock::validate_developer_tool(std::path::Path::new(
                self.master_clock_path.trim(),
            ))
            .ok();
        }
        None
    }

    fn poll_master_clock(&mut self) {
        let Some(mut process) = self.master_clock_process.take() else {
            return;
        };
        match process.poll() {
            Ok(true) => self.master_clock_process = Some(process),
            Ok(false) => self.status = "Master Clock exited unsuccessfully".into(),
            Err(error) => {
                self.status = error;
                self.log_error(&self.status.clone());
            }
        }
    }

    /// The settings panel: configure the app and the watch.
    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr(self.language, Key::Settings));
        let theme_response = ui.label("Theme and layout");
        self.register_anchor(Panel::Settings, AnchorId::SettingsTheme, &theme_response);
        ui.separator();

        // The settings panel is long, so wrap everything in a scroll area that
        // shows scrollbars automatically when content overflows.
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.settings_body(ui);
            });
    }

    /// Shows presentation-only overrides. These controls never reach operation
    /// handlers, so they cannot weaken artifact, transport, path, or consent gates.
    fn panel_ux_settings(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.separator();
        ui.heading("Per-panel UX overrides");
        ui.label("Each override changes presentation or guidance for one panel. Hard safeguards are never overrideable.");
        let panels = Panel::ALL;
        let selected = self.ux_override_panel.min(panels.len() - 1);
        self.ux_override_panel = selected;
        egui::ComboBox::from_id_source("panel_ux_panel")
            .selected_text(panels[selected].label(self.language))
            .show_ui(ui, |ui| {
                for (index, panel) in panels.iter().enumerate() {
                    ui.selectable_value(
                        &mut self.ux_override_panel,
                        index,
                        panel.label(self.language),
                    );
                }
            });
        let panel = panels[self.ux_override_panel];
        let key = panel.key().to_string();
        let mut policy = self
            .panel_ux_overrides
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let mut changed = false;
        for (value, label, description) in [
            (&mut policy.tutorial_input_barrier, "Tutorial input barrier", "Controls whether the tutorial guides input around its card. It does not gate hardware actions."),
            (&mut policy.tutorial_spotlight, "Tutorial spotlight", "Highlights the current tutorial target without granting permission to use it."),
            (&mut policy.advanced_tab_visibility, "Advanced-tab visibility", "Controls whether advanced tabs are shown after Developer Mode has already enabled them."),
            (&mut policy.simulated_diagnostics_visibility, "Simulated diagnostics", "Shows or hides host-only diagnostic detail. It never changes a diagnostic result or hardware check."),
            (&mut policy.developer_tool_visibility, "Developer tools", "Shows or hides developer tool affordances. Hidden tools remain unavailable unless all normal gates pass."),
            (&mut policy.confirmation_verbosity, "Confirmation verbosity", "Shows the full or compact explanation. Explicit confirmation is required in either form."),
        ] {
            if ui.checkbox(value, label).changed() {
                changed = true;
            }
            ui.weak(description);
        }
        if changed {
            self.panel_ux_overrides.insert(key, policy);
            self.save_settings_unconditionally();
        }
        ui.weak("Never affected here: signature checks, UF2 and sidecar validation, fail-closed builds, drive revalidation, UART bounds, shell authorization, path safety, read-only browser rules, rollback, or physical consent.");
    }

    /// The scrollable body of the settings panel.
    fn settings_body(&mut self, ui: &mut egui::Ui) {
        ui.label(tr(self.language, Key::ConfigureApp));
        ui.add_space(8.0);

        // Spreadsheet-style layout: label on the left, config on the right.
        egui::Grid::new("settings_grid")
            .striped(true)
            .spacing([24.0, 8.0])
            .num_columns(2)
            .show(ui, |ui| {
                // Language.
                ui.label(tr(self.language, Key::Language));
                ui.horizontal(|ui| {
                    for lang in Language::ALL {
                        if ui
                            .selectable_label(self.language == lang, lang.name())
                            .clicked()
                        {
                            self.language = lang;
                            self.log.log(format!("Language set to {}", lang.name()));
                        }
                    }
                });
                ui.end_row();

                // Theme.
                ui.label(tr(self.language, Key::Theme));
                ui.horizontal(|ui| {
                    for theme in Theme::ALL {
                        let response = ui.selectable_label(self.theme == theme, theme.name());
                        self.register_anchor(Panel::Settings, AnchorId::SettingsTheme, &response);
                        if response.clicked()
                        {
                            self.theme = theme;
                            self.log.log(format!("Theme set to {}", theme.name()));
                        }
                    }
                });
                ui.end_row();

                // Top-level tab layout.
                ui.label(tr(self.language, Key::TabLayout))
                    .on_hover_text("Choose how many rows the top navigation prefers. Auto adapts to window width and mode.");
                let mut tab_settings_changed = false;
                ui.horizontal(|ui| {
                    for (mode, key) in [
                        (settings::TabLayoutMode::Auto, Key::TabLayoutAuto),
                        (settings::TabLayoutMode::OneRow, Key::TabLayoutOneRow),
                        (settings::TabLayoutMode::TwoRows, Key::TabLayoutTwoRows),
                        (settings::TabLayoutMode::ThreeRows, Key::TabLayoutThreeRows),
                    ] {
                        let response = ui.selectable_label(self.tab_layout == mode, tr(self.language, key));
                        self.register_anchor(Panel::Settings, AnchorId::SettingsLayout, &response);
                        if response.clicked()
                        {
                            self.tab_layout = mode;
                            tab_settings_changed = true;
                        }
                    }
                });
                ui.end_row();

                // Tab overflow behavior.
                ui.label(tr(self.language, Key::TabOverflow))
                    .on_hover_text("Wrapping keeps every tab reachable without a second horizontal scrollbar. Horizontal scrolling is opt-in.");
                ui.horizontal(|ui| {
                    for (behavior, key) in [
                        (settings::TabOverflowBehavior::Wrap, Key::TabOverflowWrap),
                        (settings::TabOverflowBehavior::HorizontalScroll, Key::TabOverflowScroll),
                    ] {
                        if ui
                            .selectable_label(self.tab_overflow == behavior, tr(self.language, key))
                            .clicked()
                        {
                            self.tab_overflow = behavior;
                            tab_settings_changed = true;
                        }
                    }
                });
                ui.end_row();
                if tab_settings_changed {
                    self.save_settings_internal();
                }

                // Automatic persistence and compile-session behavior.
                ui.label("Persist user changes")
                    .on_hover_text("Automatically save ordinary changes and your settings when Studio closes. Explicit Save, Export, and Restore actions still work when this is off.");
                let persist_response = ui.checkbox(
                    &mut self.persist_user_changes,
                    "Save changes automatically",
                );
                if persist_response.changed() {
                    self.save_settings_unconditionally();
                    self.save_bootstrap_preferences();
                }
                ui.end_row();

                ui.label("Reset test session on compile")
                    .on_hover_text("After a valid build check passes, clear temporary simulator/test-session state before compiling. Your settings, source, presets, restore points, logs, output files, and Cargo caches are not removed.");
                let reset_response = ui.checkbox(
                    &mut self.reset_test_session_on_compile,
                    "Start each compile with a fresh test session",
                );
                if reset_response.changed() {
                    self.save_settings_unconditionally();
                }
                ui.end_row();

                ui.label("Fresh debug executable profile")
                    .on_hover_text("Beginner-safe isolation: each debug/test Studio executable gets its own settings and restore points. The same binary reuses its profile; a newly compiled binary starts clean. Release Studio paths are unchanged.");
                let profile_response = ui.checkbox(
                    &mut self.fresh_test_executable_profile,
                    "Isolate settings for each debug build",
                );
                if profile_response.changed() {
                    self.save_settings_unconditionally();
                    self.save_bootstrap_preferences();
                }
                ui.end_row();

                if test_runtime::active().isolated_debug {
                    ui.label("Test profile");
                    let workers_active = self.workers_active();
                    let reset = ui.add_enabled(
                        !workers_active,
                        egui::Button::new("Reset test profile now"),
                    );
                    if reset.clicked() {
                        self.pending_confirm = Some((
                            "Reset this isolated debug/test profile to defaults? Restore points, source/editor files, output/UF2/recovery artifacts, and bootstrap preferences will be preserved.".into(),
                            ConfirmKind::ResetTestProfile,
                        ));
                    }
                    if workers_active {
                        ui.weak("Finish background work before resetting the test profile");
                    }
                    ui.end_row();
                }

                // Text size.
                ui.label("Text size");
                ui.horizontal(|ui| {
                    for (v, label) in [(0u8, "Small"), (1, "Normal"), (2, "Big")] {
                        let response = ui.selectable_label(self.text_size == v, label);
                        self.register_anchor(Panel::Settings, AnchorId::SettingsText, &response);
                        if response.clicked() {
                            self.text_size = v;
                            self.log.log(format!("Text size set to {label}"));
                        }
                    }
                });
                ui.end_row();

                // Output line limit (applies to every terminal/debug log).
                ui.label("Log line limit");
                ui.horizontal(|ui| {
                    let mut limit = self.line_limit as i64;
                    let resp = ui.add(
                        egui::DragValue::new(&mut limit)
                            .clamp_range(10..=10000)
                            .speed(10)
                            .suffix(" lines"),
                    );
                    if resp.changed() {
                        self.line_limit = limit.max(1) as usize;
                        self.apply_line_limit();
                        self.log
                            .log(format!("Log line limit set to {}", self.line_limit));
                    }
                    ui.weak("Applies to the terminal, shell, and all debug logs");
                });
                ui.end_row();

                // Studio data root: deliberately pending until an explicit Apply.
                ui.label("Studio data folder");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.pending_data_folder);
                    if ui.button("Apply").clicked() { self.apply_data_folder(); }
                    if ui.button("Reset to default").clicked() {
                        self.pending_data_folder = data_dir::default_path().display().to_string();
                        self.data_folder_status = "Default selected; click Apply to use it after restart".into();
                    }
                });
                if !self.data_folder_status.is_empty() { ui.weak(&self.data_folder_status); }
                ui.end_row();

                // Packaged firmware is immutable; editing uses the active project.
                ui.label("Bundled firmware template");
                ui.monospace(
                    self.package_status
                        .firmware_project_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "unavailable".into()),
                );
                ui.end_row();
                ui.label("Active mutable project");
                ui.monospace(
                    self.package_status
                        .active_project_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "unavailable".into()),
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.weak("All-in-one CLI: run Firmware Studio with --help");
        self.panel_ux_settings(ui);

        if !self.advanced_mode {
            ui.add_space(16.0);
            ui.separator();
            ui.heading("Developer Mode");
            ui.label("Developer Mode is off by default. It exposes development UX only and never disables hard safety checks.");
            if ui.button("Enable Developer Mode...").clicked() {
                self.advanced_mode_confirm = true;
            }
            return;
        }

        ui.add_space(16.0);
        ui.separator();
        ui.heading("Developer Mode tools");
        ui.label("Developer Mode is enabled. It exposes development UX only. Artifact, drive, UART, shell, path, rollback, and physical-action safeguards remain mandatory.");
        ui.heading("NTP Time / Master Clock");
        ui.label("Optional on-demand tool. NTP and any geolocation behavior are external network activity; this does not change Windows time.");
        ui.weak("Studio never starts this tool automatically. Only a validated package-local tools/master-clock.exe is offered.");
        if self.package_status.mode == distribution::DistributionMode::Developer {
            ui.horizontal(|ui| {
                ui.label("Explicit developer path:");
                ui.text_edit_singleline(&mut self.master_clock_path);
                if ui.button("Validate path").clicked() {
                    match master_clock::validate_developer_tool(std::path::Path::new(
                        self.master_clock_path.trim(),
                    )) {
                        Ok(path) => {
                            self.status = format!("Validated Master Clock: {}", path.display())
                        }
                        Err(error) => self.status = error,
                    }
                }
            });
            ui.weak("Developer mode requires an explicit configured path and validation; PATH lookup is never used.");
        }
        let available = master_clock::action_available(
            self.advanced_mode,
            self.master_clock_executable().is_some(),
            self.master_clock_process.is_some(),
        );
        let launch = ui.add_enabled(available, egui::Button::new("NTP Time / Master Clock"));
        if launch.clicked() {
            self.pending_confirm = Some((
                "Launch Master Clock? It may contact NTP/geolocation services (external network activity). It will not change Windows time. Continue?".into(),
                ConfirmKind::LaunchMasterClock,
            ));
        }
        if self.master_clock_process.is_some() {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(100, 210, 130),
                    "Master Clock is running",
                );
                if ui.button("Stop Master Clock").clicked() {
                    if let Some(mut process) = self.master_clock_process.take() {
                        process.terminate();
                    }
                    self.status = "Master Clock stopped".into();
                }
            });
        } else if !available {
            ui.weak("Unavailable: no validated package capability or explicitly validated developer executable.");
        }

        ui.add_space(16.0);
        ui.separator();
        ui.heading("App Resource Usage");
        ui.horizontal(|ui| {
            ui.label("How much of the system this app is using.");
            ui.separator();
            ui.label("Update rate:");
            egui::ComboBox::from_id_source("stats_rate")
                .selected_text(self.stats_rate_label())
                .show_ui(ui, |ui| {
                    for (label, ms) in [
                        ("Real-time (0.25s)", 250u64),
                        ("0.5s", 500),
                        ("1s", 1000),
                        ("2s", 2000),
                    ] {
                        if ui
                            .selectable_label(self.stats_rate_ms == ms, label)
                            .clicked()
                        {
                            self.stats_rate_ms = ms;
                            self.stats_rate_shared
                                .store(ms, std::sync::atomic::Ordering::Relaxed);
                            self.log.log(format!("Stats rate set to {label}"));
                        }
                    }
                });
        });
        ui.add_space(8.0);

        // App-only stats (no system column).
        egui::Grid::new("app_resources_grid")
            .striped(true)
            .spacing([24.0, 6.0])
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("System CPU cores");
                ui.monospace(
                    self.sys_stats
                        .system_cpu_cores
                        .map(|cores| cores.to_string())
                        .unwrap_or_else(|| "unknown".to_owned()),
                );
                ui.end_row();
                ui.label("Process CPU");
                ui.monospace(format!("{:.1}%", self.sys_stats.cpu_percent));
                ui.end_row();
                ui.label("Process threads");
                ui.monospace(sysstats::format_process_threads(self.sys_stats.threads));
                ui.end_row();
                ui.label("Process memory");
                ui.monospace(fmt_bytes(self.sys_stats.mem_bytes));
                ui.end_row();
                ui.label("Process virtual memory");
                ui.monospace(fmt_bytes(self.sys_stats.virtual_mem_bytes));
                ui.end_row();
                ui.label("Disk read");
                ui.monospace(format!(
                    "{}  ({} total)",
                    fmt_bytes(self.sys_stats.disk_read_rate),
                    fmt_bytes(self.sys_stats.disk_read_bytes)
                ));
                ui.end_row();
                ui.label("Disk write");
                ui.monospace(format!(
                    "{}  ({} total)",
                    fmt_bytes(self.sys_stats.disk_write_rate),
                    fmt_bytes(self.sys_stats.disk_write_bytes)
                ));
                ui.end_row();
                ui.label("Run time");
                ui.monospace(format!("{} s", self.sys_stats.run_time_secs));
                ui.end_row();
                ui.label("GPU");
                ui.monospace("N/A (Windows)");
                ui.end_row();
            });

        ui.add_space(16.0);
        ui.separator();
        ui.heading("Output Directory");
        ui.label("Where built .uf2 files are written. Defaults to your Documents folder so it works even when running from a read-only location.");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.output_dir);
            if ui.button("Reset to default").clicked() {
                self.output_dir = settings::default_output_dir();
            }
        });
        ui.weak("The output dir is created automatically if it doesn't exist.");

        ui.add_space(8.0);
        egui::CollapsingHeader::new("Storage & output details")
            .default_open(false)
            .show(ui, |ui| {
                ui.weak("Measured values are read from existing files only; no build or target directories are scanned.");
                let output_path = std::path::Path::new(&self.output_dir);
                ui.label(format!("Output directory: {}", output_path.display()));
                match measure_directory(output_path) {
                    Some((files, bytes)) => {
                        ui.monospace(format!("Measured output files: {files} files, {}", fmt_bytes(bytes)));
                    }
                    None => {
                        ui.weak("Measured output files: unavailable (directory does not exist or could not be read).");
                    }
                }
                match self
                    .approved_artifact
                    .as_ref()
                    .and_then(|artifact| std::fs::metadata(&artifact.path).ok()) {
                    Some(metadata) => ui.monospace(format!("Measured last UF2: {}", fmt_bytes(metadata.len()))),
                    None => ui.weak("Measured last UF2: unavailable (no existing UF2 artifact)."),
                };

                let source_export = std::path::Path::new("export");
                ui.label(format!("Source export: {}", source_export.display()));
                match measure_directory(source_export) {
                    Some((files, bytes)) => {
                        ui.monospace(format!("Measured source export: {files} files, {}", fmt_bytes(bytes)));
                    }
                    None => {
                        ui.weak("Measured source export: unavailable (no existing export artifact).");
                    }
                }

                let settings_path = persist::settings_path();
                ui.label(format!("Settings export: {}", settings_path.display()));
                match std::fs::metadata(&settings_path) {
                    Ok(metadata) => ui.monospace(format!("Measured settings export: {}", fmt_bytes(metadata.len()))),
                    Err(_) => ui.weak("Measured settings export: unavailable (no existing settings file)."),
                };
            });

        ui.add_space(16.0);
        ui.separator();
        ui.heading("Settings Data");
        ui.label("Save, export, or import your settings, presets, and configuration.");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save settings to file").clicked() {
                self.save_settings_to_file();
            }
            if ui.button("Export settings JSON").clicked() {
                self.export_settings();
            }
            let import_response = ui.button("Import settings JSON").on_hover_text(
                "Replace settings from clipboard JSON; create a restore point first",
            );
            self.register_anchor(Panel::Settings, AnchorId::SettingsImport, &import_response);
            if import_response.clicked() && self.unsafe_action_allowed() {
                self.snapshot_before("Before settings import");
                self.import_settings();
            }
        });

        ui.add_space(16.0);
        ui.separator();
        ui.heading("Restore Points");
        ui.label("Snapshots contain configuration and app state only; secrets and tokens are never stored.");
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut self.restore_name);
            if ui.button("Create").clicked() {
                let name = if self.restore_name.trim().is_empty() {
                    "Manual snapshot".to_string()
                } else {
                    self.restore_name.trim().to_string()
                };
                self.snapshot_before(&name);
                self.restore_name.clear();
            }
        });
        let mut restore_index = None;
        let mut delete_index = None;
        let mut rename_index = None;
        let restore_points = self.restore_store.points.clone();
        let restore_allowed = self.unsafe_action_allowed();
        let unsafe_allowed = self.unsafe_action_allowed();
        for (index, point) in restore_points.iter().enumerate() {
            let mut restore_rect = None;
            let mut restore_clicked = false;
            let mut export_requested = false;
            ui.horizontal(|ui| {
                ui.label(format!("{} - {}", point.name, point.timestamp));
                let restore_response = ui
                    .small_button("Restore")
                    .on_hover_text("Replace current settings with this saved restore point");
                restore_rect = Some(restore_response.rect);
                restore_clicked = restore_response.clicked();
                if restore_clicked && restore_allowed {
                    restore_index = Some(index);
                }
                if ui
                    .small_button("Delete")
                    .on_hover_text("Permanently remove this local restore point")
                    .clicked()
                    && unsafe_allowed
                {
                    delete_index = Some(index);
                }
                if ui.small_button("Rename").clicked() {
                    rename_index = Some(index);
                }
                if ui.small_button("Export").clicked() {
                    export_requested = true;
                }
            });
            if let Some(rect) = restore_rect {
                self.help_anchors.register(
                    Panel::Settings.help_id(),
                    AnchorId::SettingsRestore.key(),
                    help::AnchorRect {
                        min: (rect.min.x, rect.min.y),
                        max: (rect.max.x, rect.max.y),
                    },
                );
            }
            if export_requested {
                match self
                    .restore_store
                    .export_json(index)
                    .and_then(|j| ui_copy_to_clipboard(&j))
                {
                    Ok(_) => self.status = "Restore point copied to clipboard".to_string(),
                    Err(e) => self.status = format!("Restore point export failed: {e}"),
                }
            }
        }
        ui.horizontal(|ui| {
            let import_restore_response = ui
                .button("Import")
                .on_hover_text("Import a restore point from clipboard JSON");
            if import_restore_response.clicked() && self.unsafe_action_allowed() {
                match ui_paste_from_clipboard()
                    .and_then(|j| self.restore_store.import_json(&j).map(|_| j))
                {
                    Ok(_) => match self.restore_store.save() {
                        Ok(_) => self.status = "Restore point imported".to_string(),
                        Err(e) => self.status = format!("Restore point save failed: {e}"),
                    },
                    Err(e) => self.status = format!("Restore point import failed: {e}"),
                }
            }
            if self.restore_store.points.is_empty() {
                ui.weak("No restore points yet.");
            }
        });
        if let Some(index) = rename_index {
            let name = if self.restore_name.trim().is_empty() {
                "Renamed restore point".to_string()
            } else {
                self.restore_name.trim().to_string()
            };
            self.restore_store.rename(index, name);
            self.restore_name.clear();
            match self.restore_store.save() {
                Ok(()) => self.status = "Restore point renamed".to_string(),
                Err(e) => {
                    self.status = format!("Restore point save failed: {e}");
                    self.log_error(&format!("Restore point save failed: {e}"));
                }
            }
        }
        if let Some(index) = delete_index {
            self.restore_store.delete(index);
            match self.restore_store.save() {
                Ok(()) => self.status = "Restore point deleted".to_string(),
                Err(e) => {
                    self.status = format!("Restore point save failed: {e}");
                    self.log_error(&format!("Restore point save failed: {e}"));
                }
            }
        }
        if let Some(index) = restore_index {
            self.restore_selected(index);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.heading("Export");
        ui.label("Export the full source code (firmware + app) to a folder.");
        if ui.button("Export source").clicked() {
            self.export_source();
        }

        ui.add_space(16.0);
        ui.separator();
        ui.heading("Integrity");
        ui.label(
            "Verify this app's executable hasn't been modified. The hash covers the\n\
             binary only; your user data (settings, custom NTP servers, watch faces)\n\
             is intentionally excluded since it changes at runtime.",
        )
        .on_hover_text(
            "This computes a SHA-256 checksum of the running .exe so you can confirm\n\
             it matches the official release. User-defined data (settings file, custom\n\
             NTP servers, watch faces) is NOT hashed, because those are expected\n\
             to differ between users and change as you use the app.\n\n\
             A running exe can't prove its own authenticity, so the real check is\n\
             against the SHA-256 published on the GitHub release. If you're offline,\n\
             the app shows that the checksum could not be validated.",
        );
        ui.add_space(8.0);
        if let Some(h) = integrity::exe_sha256() {
            ui.monospace(format!("SHA-256: {h}"));
            if ui.button("Copy hash").clicked() {
                let _ = ui_copy_to_clipboard(&h);
                self.status = "Integrity hash copied".to_string();
            }
        } else {
            ui.weak("Could not read the executable.");
        }

        // Release digest comparison; this does not establish authenticity.
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Compare release digest").clicked() {
                self.fetch_release_checksum();
            }
            if self.checksum_busy {
                ui.spinner();
                ui.label("Checking...");
            }
        });
        if let Some(expected) = &self.release_sha256 {
            let local = integrity::exe_sha256();
            match local {
                Some(local) => {
                    if *expected == local {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 120),
                            "SHA-256 matches the published value (integrity only; not authenticity).",
                        );
                    } else {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 80, 80),
                            "SHA-256 mismatch - this executable differs from the published value.",
                        );
                    }
                }
                None => {
                    ui.weak("Could not read the local executable.");
                }
            }
        } else if !self.checksum_busy {
            ui.weak(
                "No release digest fetched yet. Press 'Compare release digest' (requires internet).\n\
                 This checksum comparison is an integrity check only, not authenticity verification. If offline, the digest cannot be compared.",
            );
        }

        ui.add_space(16.0);
        ui.separator();
        egui::CollapsingHeader::new("Credits")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Community and upstream credits from docs/CREDITS.md:");
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.credits_search)
                            .hint_text("Filter names, projects, or contributions"),
                    );
                    if !self.credits_search.is_empty() && ui.small_button("x").clicked() {
                        self.credits_search.clear();
                    }
                });
                let query = self.credits_search.trim().to_lowercase();
                let mut visible = 0;
                for (group_index, group) in CREDIT_GROUPS.iter().enumerate() {
                    let entries: Vec<&CreditEntry> = group
                        .entries
                        .iter()
                        .filter(|entry| credit_matches(entry, &query))
                        .collect();
                    if entries.is_empty() {
                        continue;
                    }
                    ui.add_space(8.0);
                    ui.strong(group.name);
                    egui::Grid::new(format!("credits_grid_{group_index}"))
                        .striped(true)
                        .spacing([16.0, 6.0])
                        .num_columns(2)
                        .show(ui, |ui| {
                            for entry in entries {
                                ui.label(entry.name);
                                if let Some(url) = entry.url {
                                    ui.hyperlink_to(entry.details, url);
                                } else {
                                    ui.label(entry.details);
                                }
                                ui.end_row();
                                visible += 1;
                            }
                        });
                }
                if visible == 0 {
                    ui.weak("No credits match the current search.");
                }
                ui.add_space(8.0);
                ui.weak("Attribution is intentionally high-level; see docs/CREDITS.md for the full policy and source inventory.");
            });

        // Code statistics.
        ui.add_space(12.0);
        ui.strong("Project statistics");
        ui.separator();
        let (files, lines, chars, bytes) = code_stats();
        egui::Grid::new("code_stats_grid")
            .striped(true)
            .spacing([24.0, 4.0])
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Source files");
                ui.monospace(files.to_string());
                ui.end_row();
                ui.label("Lines of code");
                ui.monospace(lines.to_string());
                ui.end_row();
                ui.label("Characters");
                ui.monospace(chars.to_string());
                ui.end_row();
                ui.label("Total size");
                ui.monospace(fmt_bytes(bytes));
                ui.end_row();
            });

        // License.
        ui.add_space(12.0);
        ui.strong("License");
        ui.separator();
        ui.label(
            "MIT OR Apache-2.0 (this rewrite). The reference C projects have their own licenses.",
        );
    }

    /// Logs an error to both the main log and the dedicated error log.
    fn log_error(&mut self, msg: &str) {
        self.log.log(msg);
        self.error_log.log(msg);
    }

    fn record_tick(&mut self) {
        if self.tick_verbosity == debug::TickVerbosity::Hide {
            return;
        }
        let (_, _, _, h, m, s, _) = self.watch.get_time();
        let line = format!("tick: {h:02}:{m:02}:{s:02}");
        self.tick_log.log(line.clone());
        if self.tick_verbosity == debug::TickVerbosity::Main {
            self.shell_log.log(line.clone());
            self.shell_hw_log.log("RTC tick: 1 Hz interrupt");
            self.log.log(line.clone());
            self.push_terminal(line);
        }
    }

    fn tick_filter_ui(&mut self, ui: &mut egui::Ui, id: &'static str) {
        let mut changed = false;
        egui::ComboBox::from_id_source(id)
            .selected_text(self.tick_verbosity.label())
            .show_ui(ui, |ui| {
                for mode in debug::TickVerbosity::ALL {
                    changed |= ui
                        .selectable_value(&mut self.tick_verbosity, mode, mode.label())
                        .changed();
                }
            });
        if changed {
            self.save_settings_internal();
        }
    }

    fn show_main_event(&self, message: &str) -> bool {
        self.tick_verbosity == debug::TickVerbosity::Main || !debug::is_tick_or_process(message)
    }

    fn tick_log_ui(&self, ui: &mut egui::Ui, id: &'static str) {
        if self.tick_verbosity != debug::TickVerbosity::Dedicated {
            return;
        }
        ui.separator();
        ui.strong("Tick log");
        ui.weak(format!(
            "{} / {} lines - auto-scroll",
            self.tick_log.entries().len(),
            self.line_limit
        ));
        egui::ScrollArea::vertical()
            .id_source(id)
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .max_height(120.0)
            .show(ui, |ui| {
                for entry in self.tick_log.entries() {
                    ui.monospace(&entry.message);
                }
            });
    }

    /// Appends a line to the terminal history, dropping the oldest lines once
    /// it exceeds the configured line limit.
    fn push_terminal(&mut self, line: impl Into<String>) {
        const MAX_ENTRY_CHARS: usize = 4096;
        let mut line = line.into();
        if line.chars().count() > MAX_ENTRY_CHARS {
            line = line.chars().take(MAX_ENTRY_CHARS).collect();
            line.push_str("...");
        }
        self.terminal_history.push(line);
        const MAX_TERMINAL_ENTRIES: usize = 500;
        let limit = self.line_limit.clamp(1, MAX_TERMINAL_ENTRIES);
        if self.terminal_history.len() > limit {
            let excess = self.terminal_history.len() - limit;
            self.terminal_history.drain(..excess);
        }
    }

    /// Builds a structured bug report with app state and recent errors.
    fn build_bug_report(&self) -> String {
        let mut out = String::new();
        out.push_str("Firmware Studio bug report\n");
        out.push_str(&format!("Version: {}\n", env!("CARGO_PKG_VERSION")));
        out.push_str("=========================\n\n");
        out.push_str(&format!("Language: {}\n", self.language.name()));
        out.push_str(&format!("Theme: {}\n", self.theme.name()));
        out.push_str(&format!("Board: {}\n", self.board.label()));
        out.push_str(&format!(
            "Active faces: {}\n",
            self.presets.active_faces().len()
        ));
        out.push_str(&format!("Catalog faces: {}\n", self.face_list.len()));
        out.push_str(&format!("Custom modules: {}\n", self.modules.modules.len()));
        out.push_str(&format!("NTP servers: {}\n", self.ntp_servers.len() + 1));
        out.push_str(&format!("Last build: {}\n", self.build_message));
        out.push_str("\n--- Error log ---\n");
        if self.error_log.is_empty() {
            out.push_str("(no errors recorded)\n");
        }
        for entry in self.error_log.entries() {
            let secs = entry.timestamp % 60;
            let mins = (entry.timestamp / 60) % 60;
            let hrs = (entry.timestamp / 3600) % 24;
            out.push_str(&format!(
                "[{hrs:02}:{mins:02}:{secs:02}] {}\n",
                entry.message
            ));
        }
        out.push_str("\n--- Recent activity ---\n");
        for entry in self.log.entries().iter().rev().take(20) {
            let secs = entry.timestamp % 60;
            let mins = (entry.timestamp / 60) % 60;
            let hrs = (entry.timestamp / 3600) % 24;
            out.push_str(&format!(
                "[{hrs:02}:{mins:02}:{secs:02}] {}\n",
                entry.message
            ));
        }
        out
    }

    /// Saves the current settings to a JSON file in the app data directory.
    fn save_settings_to_file(&mut self) {
        self.log.log("Settings export: saving settings to file");
        self.push_terminal("Settings export: saving settings to file");
        let settings = settings::AppSettings::capture(
            self.language,
            self.theme,
            &self.presets,
            self.ntp_server,
            &self.ntp_servers,
            self.sim_scale,
            &self.watch_config,
            self.text_size,
            self.catalog_width,
            self.preset_height,
            &self.modules,
            self.output_dir.clone(),
            self.first_run,
            self.tour_claims.keys(),
            self.drift_session.ppm,
            &self.rtc_calibration,
            self.line_limit,
            self.tick_verbosity.setting_name().to_string(),
            &self.component_profiles,
            self.component_profile,
            self.tab_layout,
            self.tab_overflow,
            self.persist_user_changes,
            self.reset_test_session_on_compile,
            self.fresh_test_executable_profile,
        )
        .with_board(self.board.label())
        .with_developer_mode(self.advanced_mode)
        .with_panel_ux_overrides(self.panel_ux_overrides.clone())
        .with_data_folder(self.pending_data_folder.clone());
        // Use the same atomic, validated writer as automatic persistence.
        let path = persist::settings_path();
        match persist::save(&settings) {
            Ok(()) => {
                self.status = format!("Settings saved to {}", path.display());
                self.log.log(format!(
                    "Settings export succeeded: saved to {}",
                    path.display()
                ));
                self.push_terminal(format!(
                    "Settings export succeeded: saved to {}",
                    path.display()
                ));
            }
            Err(e) => {
                self.status = format!("Failed to save settings: {e}");
                self.log.log(format!("Settings export failed: {e}"));
                self.push_terminal(format!("Settings export failed: {e}"));
            }
        }
    }

    /// Captures the complete non-secret app state as a restore point.
    fn snapshot_before(&mut self, name: &str) {
        let settings = settings::AppSettings::capture(
            self.language,
            self.theme,
            &self.presets,
            self.ntp_server,
            &self.ntp_servers,
            self.sim_scale,
            &self.watch_config,
            self.text_size,
            self.catalog_width,
            self.preset_height,
            &self.modules,
            self.output_dir.clone(),
            self.first_run,
            self.tour_claims.keys(),
            self.drift_session.ppm,
            &self.rtc_calibration,
            self.line_limit,
            self.tick_verbosity.setting_name().to_string(),
            &self.component_profiles,
            self.component_profile,
            self.tab_layout,
            self.tab_overflow,
            self.persist_user_changes,
            self.reset_test_session_on_compile,
            self.fresh_test_executable_profile,
        )
        .with_board(self.board.label())
        .with_developer_mode(self.advanced_mode)
        .with_panel_ux_overrides(self.panel_ux_overrides.clone())
        .with_data_folder(self.pending_data_folder.clone());
        self.restore_store
            .create(name, settings, self.board.label(), self.presets.active);
        if let Err(e) = self.restore_store.save() {
            let message = format!("Restore point failed: {e}");
            self.status = message.clone();
            self.log_error(&message);
            self.push_terminal(message);
        }
    }

    fn restore_selected(&mut self, index: usize) {
        let Some(point) = self.restore_store.points.get(index).cloned() else {
            return;
        };
        self.apply_settings(point.settings);
        self.board = match point.board.as_str() {
            "Red / Lite" => Board::RedLite,
            "Blue" => Board::Blue,
            "Pro" => Board::Pro,
            _ => Board::Green,
        };
        self.presets.active = point
            .active_preset
            .min(self.presets.presets.len().saturating_sub(1));
        self.save_settings_unconditionally();
        self.status = format!("Restored {}", point.name);
        self.log
            .log(format!("Restored restore point {}", point.name));
    }

    fn workers_active(&self) -> bool {
        self.building
            || self.pending_build.is_some()
            || self.pending_ntp.is_some()
            || self.pending_checksum.is_some()
            || self.pending_update.is_some()
            || self.ntp_busy
            || self.checksum_busy
            || self.update_checking
            || self.pending_probe.is_some()
            || self.pending_detection.is_some()
            || self.pending_flash.is_some()
            || self.flash_busy()
    }

    fn reset_test_profile(&mut self) {
        if self.workers_active() {
            self.status = "Reset unavailable while background work is active".into();
            return;
        }
        if !test_runtime::active().isolated_debug {
            self.status = "Test profile reset is only available for isolated debug profiles".into();
            return;
        }
        let bootstrap = persist::RuntimePreferences {
            fresh_test_executable_profile: self.fresh_test_executable_profile,
            persist_user_changes: self.persist_user_changes,
            data_folder: self.pending_data_folder.clone(),
            output_dir: self.output_dir.clone(),
        };
        let mut defaults = settings::AppSettings::default();
        defaults.language = "English".into();
        defaults.theme = "Dark".into();
        defaults.component_profiles = components::default_profiles();
        defaults.persist_user_changes = bootstrap.persist_user_changes;
        defaults.fresh_test_executable_profile = bootstrap.fresh_test_executable_profile;
        self.apply_settings(defaults);
        self.apply_bootstrap_preferences(&bootstrap);

        self.watch = CasioF91W::new();
        self.face_engine = face_sim::FaceEngine::new("SIMPLE_CLOCK");
        self.real_face = None;
        self.active_real_face_name = None;
        self.active_real_mode_24 = None;
        self.sim_face_idx = 0;
        self.sync_sim_controller_from_watch();
        self.btn_l_events = real_face::ButtonEventState::default();
        self.btn_a_events = real_face::ButtonEventState::default();
        self.cancel_simulator_buttons();
        self.approved_artifact = None;
        self.pending_artifact = None;
        self.pending_artifact_fingerprint = None;
        self.current_progress = None;
        self.build_message.clear();
        self.status = "Isolated test profile reset to defaults".into();
        self.log.clear();
        self.tick_log.clear();
        self.build_log.clear();
        self.flash_log.clear();
        self.error_log.clear();
        self.faces_log.clear();
        self.sim_log.clear();
        self.shell_log.clear();
        self.shell_hw_log.clear();
        self.terminal_history.clear();
        self.transport_mode = transport::TransportMode::Simulated;
        self.uart = None;
        self.last_uart_error = None;
        self.save_settings_unconditionally();
        self.save_bootstrap_preferences();
    }

    /// Persists the current settings to the active profile.
    fn save_settings_internal(&mut self) {
        if !self.persist_user_changes {
            return;
        }
        self.save_settings_unconditionally();
    }

    /// Performs an explicit settings save, including when automatic persistence is off.
    fn save_settings_unconditionally(&mut self) {
        let settings = settings::AppSettings::capture(
            self.language,
            self.theme,
            &self.presets,
            self.ntp_server,
            &self.ntp_servers,
            self.sim_scale,
            &self.watch_config,
            self.text_size,
            self.catalog_width,
            self.preset_height,
            &self.modules,
            self.output_dir.clone(),
            self.first_run,
            self.tour_claims.keys(),
            self.drift_session.ppm,
            &self.rtc_calibration,
            self.line_limit,
            self.tick_verbosity.setting_name().to_string(),
            &self.component_profiles,
            self.component_profile,
            self.tab_layout,
            self.tab_overflow,
            self.persist_user_changes,
            self.reset_test_session_on_compile,
            self.fresh_test_executable_profile,
        )
        .with_board(self.board.label())
        .with_developer_mode(self.advanced_mode)
        .with_panel_ux_overrides(self.panel_ux_overrides.clone())
        .with_data_folder(self.pending_data_folder.clone());
        match persist::save(&settings) {
            Ok(_) => {}
            Err(e) => {
                self.status = format!("Failed to persist settings: {e}");
                self.log_error(&format!("Failed to persist settings: {e}"));
                self.push_terminal(format!("Settings persistence failed: {e}"));
            }
        }
        self.save_bootstrap_preferences();
    }

    fn export_settings(&mut self) {
        self.log.log("Settings export: exporting settings JSON");
        self.push_terminal("Settings export: exporting settings JSON");
        let settings = settings::AppSettings::capture(
            self.language,
            self.theme,
            &self.presets,
            self.ntp_server,
            &self.ntp_servers,
            self.sim_scale,
            &self.watch_config,
            self.text_size,
            self.catalog_width,
            self.preset_height,
            &self.modules,
            self.output_dir.clone(),
            self.first_run,
            self.tour_claims.keys(),
            self.drift_session.ppm,
            &self.rtc_calibration,
            self.line_limit,
            self.tick_verbosity.setting_name().to_string(),
            &self.component_profiles,
            self.component_profile,
            self.tab_layout,
            self.tab_overflow,
            self.persist_user_changes,
            self.reset_test_session_on_compile,
            self.fresh_test_executable_profile,
        )
        .with_board(self.board.label())
        .with_developer_mode(self.advanced_mode)
        .with_panel_ux_overrides(self.panel_ux_overrides.clone())
        .with_data_folder(self.pending_data_folder.clone());
        match settings.to_json() {
            Ok(json) => {
                self.log
                    .log("Settings export succeeded: JSON copied to clipboard");
                match ui_copy_to_clipboard(&json) {
                    Ok(()) => {
                        self.status = "Settings JSON copied to clipboard".to_string();
                        self.push_terminal("Settings export succeeded: JSON copied to clipboard");
                    }
                    Err(error) => {
                        self.status = format!("Settings export clipboard failed: {error}");
                        self.log_error(&format!("Settings export clipboard failed: {error}"));
                        self.push_terminal(format!("Settings export clipboard failed: {error}"));
                    }
                }
            }
            Err(e) => {
                self.status = format!("Failed to serialize settings: {e}");
                self.log
                    .log(format!("Settings export failed to serialize: {e}"));
                self.push_terminal(format!("Settings export failed to serialize: {e}"));
            }
        }
    }

    /// Imports settings JSON from the clipboard, reciprocal to export_settings.
    fn import_settings(&mut self) {
        match ui_paste_from_clipboard() {
            Ok(json) => match settings::AppSettings::from_json(&json) {
                Ok(s) => {
                    self.apply_settings(s);
                    self.status = "Settings imported from clipboard".to_string();
                    self.log.log("Settings imported from clipboard");
                }
                Err(e) => {
                    self.status = format!("Failed to parse settings: {e}");
                    self.log_error(&format!("Failed to parse settings: {e}"));
                }
            },
            Err(e) => {
                self.status = format!("Failed to read settings from clipboard: {e}");
                self.log_error(&format!("Failed to read settings from clipboard: {e}"));
            }
        }
    }

    fn apply_data_folder(&mut self) {
        let candidate = std::path::PathBuf::from(self.pending_data_folder.trim());
        let executable = std::env::current_exe().unwrap_or_default();
        let firmware = build::firmware_dir();
        let output = std::path::PathBuf::from(&self.output_dir);
        let recovery = output.join("recovery");
        let protected = [
            firmware.as_path(),
            output.as_path(),
            recovery.as_path(),
            executable.as_path(),
        ];
        if let Err(error) = data_dir::validate(&candidate, &protected) {
            self.data_folder_status = format!("Invalid Studio data folder: {error}");
            return;
        }
        let old = test_runtime::active().root;
        if let Err(error) = data_dir::migrate(&old, &candidate) {
            self.data_folder_status = format!("Data folder was not changed: {error}");
            return;
        }
        let preferences = persist::RuntimePreferences {
            fresh_test_executable_profile: self.fresh_test_executable_profile,
            persist_user_changes: self.persist_user_changes,
            data_folder: candidate.display().to_string(),
            output_dir: self.output_dir.clone(),
        };
        if let Err(error) = persist::save_runtime_preferences(&preferences) {
            self.data_folder_status = format!("Data folder was not changed: {error}");
            return;
        }
        self.data_folder_status = "Applied safely. Restart Studio to use the new folder; the current session remains on its original root.".into();
    }

    fn apply_bootstrap_preferences(&mut self, preferences: &persist::RuntimePreferences) {
        self.persist_user_changes = preferences.persist_user_changes;
        self.fresh_test_executable_profile = preferences.fresh_test_executable_profile;
        if !preferences.data_folder.is_empty() {
            self.pending_data_folder = preferences.data_folder.clone();
        }
    }

    fn save_bootstrap_preferences(&mut self) {
        if let Err(error) = persist::save_toggle_preferences(
            self.fresh_test_executable_profile,
            self.persist_user_changes,
            self.pending_data_folder.clone(),
            self.output_dir.clone(),
        ) {
            self.log_error(&format!("Failed to save runtime preferences: {error}"));
        }
    }

    /// Applies imported settings to the app state.
    fn apply_settings(&mut self, s: settings::AppSettings) {
        if let Some(lang) = Language::ALL.iter().find(|l| l.name() == s.language) {
            self.language = *lang;
        }
        if let Some(theme) = Theme::ALL.iter().find(|t| t.name() == s.theme) {
            self.theme = *theme;
        }
        self.presets = s.presets;
        self.presets.migrate_face_duplicates();
        self.presets.clamp_active();
        self.ntp_server = s.ntp_server;
        self.ntp_servers = s.ntp_servers;
        self.ntp_edit_index = None;
        self.ntp_edit_name.clear();
        self.ntp_edit_host.clear();
        self.sim_scale = s.sim_scale;
        self.watch_config = s.watch_config;
        self.text_size = s.text_size;
        self.tab_layout = s.tab_layout;
        self.tab_overflow = s.tab_overflow;
        self.catalog_width = s.catalog_width;
        self.preset_height = s.preset_height;
        self.modules = s.modules;
        self.component_profiles = if s.component_profiles.is_empty() {
            components::default_profiles()
        } else {
            s.component_profiles
        };
        self.component_profile = s
            .active_component_profile
            .min(self.component_profiles.len().saturating_sub(1));
        self.component_draft =
            components::selected_config(&self.component_profiles, self.component_profile);
        self.first_run = s.first_run;
        self.welcome_minimized = false;
        self.tour_claims = TourClaims::from_keys(s.tour_claims);
        self.persist_user_changes = s.persist_user_changes;
        self.reset_test_session_on_compile = s.reset_test_session_on_compile;
        self.fresh_test_executable_profile = s.fresh_test_executable_profile;
        self.pending_data_folder = if s.data_folder.is_empty() {
            data_dir::default_path().display().to_string()
        } else {
            s.data_folder
        };
        self.advanced_mode = s.developer_mode;
        self.panel_ux_overrides = s.panel_ux_overrides;
        self.advanced_mode_confirm = false;
        self.drift_session.ppm = s.drift_ppm;
        self.rtc_calibration = s.rtc_calibration;
        self.output_dir = if s.output_dir.is_empty() {
            settings::default_output_dir()
        } else {
            s.output_dir
        };
        self.board = match s.board.as_str() {
            "Red / Lite" | "Red" | "Lite" => Board::RedLite,
            "Blue" => Board::Blue,
            "Pro" => Board::Pro,
            _ => Board::Green,
        };
        let profile = self
            .component_profiles
            .get(self.component_profile)
            .cloned()
            .unwrap_or_else(|| {
                components::BuildProfile::new("draft", self.component_draft.clone())
            });
        self.component_effective =
            components::effective_config(self.board, &profile, &self.component_draft);
        self.line_limit = s.line_limit.max(1);
        self.tick_verbosity = debug::TickVerbosity::from_setting(&s.tick_verbosity);
        self.apply_line_limit();
        self.save_settings_internal();
    }

    /// Applies the configured line limit to every output log so the bound is
    /// enforced uniformly across all panes.
    fn apply_line_limit(&mut self) {
        const MAX_TERMINAL_ENTRIES: usize = 500;
        let limit = self.line_limit.max(1);
        let terminal_limit = limit.min(MAX_TERMINAL_ENTRIES);
        self.log.set_limit(limit);
        self.tick_log.set_limit(limit);
        self.shell_log.set_limit(limit);
        self.shell_hw_log.set_limit(limit);
        self.build_log.set_limit(limit);
        self.flash_log.set_limit(limit);
        self.error_log.set_limit(limit);
        self.faces_log.set_limit(limit);
        self.sim_log.set_limit(limit);
        if self.terminal_history.len() > terminal_limit {
            let excess = self.terminal_history.len() - terminal_limit;
            self.terminal_history.drain(..excess);
        }
    }

    /// Exports the source code to a folder.
    fn export_source(&mut self) {
        // Export the firmware + studio source to an "export" folder.
        let source_dir = build::firmware_dir();
        let export_dir = source_dir.join("export");
        let result = copy_dir(&source_dir, &export_dir);
        match result {
            Ok(_) => {
                self.status = format!("Exported source to {}", export_dir.display());
                self.log
                    .log(format!("Exported source to {}", export_dir.display()));
            }
            Err(e) => {
                self.status = format!("Export failed: {e}");
                self.log.log(format!("Export failed: {e}"));
            }
        }
    }

    fn inspect_artifact_from_input(&mut self) {
        let path = std::path::PathBuf::from(self.artifact_path_input.trim());
        if self.building || self.pending_build.is_some() {
            self.status = ARTIFACT_BUSY_STATUS.to_string();
            self.build_message = ARTIFACT_BUSY_STATUS.to_string();
            return;
        }
        match build::inspect_artifact(&path) {
            Ok(inspection) => {
                set_verified_artifact_state(
                    &mut self.status,
                    &mut self.build_message,
                    &mut self.approved_artifact,
                    &mut self.pending_artifact,
                    inspection,
                    false,
                );
                self.pending_artifact_fingerprint = Some(self.build_configuration_fingerprint());
            }
            Err(error) => {
                // Do not disturb an already approved artifact when a new candidate
                // fails verification; the failed candidate never enters flash state.
                set_failed_artifact_state(
                    &mut self.status,
                    &mut self.build_message,
                    &mut self.approved_artifact,
                    &mut self.pending_artifact,
                    error,
                );
                self.pending_artifact_fingerprint = None;
            }
        }
    }

    /// Starts one worker that owns all artifact and removable-drive filesystem
    /// work. The UI thread only snapshots approval state and schedules it.
    fn copy_to_watch(&mut self, approved: &ApprovedArtifact) {
        if !self.unsafe_action_allowed() {
            self.status = "Flash is disabled while a guided tour is active".to_string();
            return;
        }
        if self.flash_busy() || self.building || self.pending_build.is_some() {
            self.status = "Flash is unavailable while another operation is active.".to_string();
            self.log_error("Flash blocked while another operation is active");
            return;
        }
        let current_inspection = build::inspect_artifact(&approved.path);
        let selected_drive = match flash::validate_copy_guard(
            &build::ArtifactInspection {
                path: approved.path.clone(),
                generation: approved.generation.clone(),
                family_id: approved.family_id.clone(),
                uf2_bytes: approved.uf2_bytes.clone(),
                uf2_blocks: approved.uf2_blocks.clone(),
                payload_bytes: approved.payload_bytes.clone(),
                sha256: approved.sha256.clone(),
                payload_sha256: approved.payload_sha256.clone(),
                manifest_digest: approved.manifest_digest.clone(),
                generated_input_digest: approved.generated_input_digest.clone(),
            },
            current_inspection.as_ref().map_err(|error| error.as_str()),
            &self.cached_watch,
        ) {
            Ok(candidate) => candidate,
            Err(error) => {
                self.status = format!("Copy blocked: {error}");
                self.log_error(&self.status.clone());
                return;
            }
        };
        if !self.flash_worker_state.start_flash() {
            self.status = "Flash is unavailable while another operation is active.".to_string();
            return;
        }
        let operation_id = self.next_operation_id;
        self.next_operation_id = self.next_operation_id.wrapping_add(1).max(1);
        let (progress, receiver) = progress::channel(operation_id);
        let path = approved.path.clone();
        let selected_drive = Some(selected_drive);

        let approved_metadata = build::ArtifactInspection {
            path: path.clone(),
            generation: approved.generation.clone(),
            family_id: approved.family_id.clone(),
            uf2_bytes: approved.uf2_bytes.clone(),
            uf2_blocks: approved.uf2_blocks.clone(),
            payload_bytes: approved.payload_bytes.clone(),
            sha256: approved.sha256.clone(),
            payload_sha256: approved.payload_sha256.clone(),
            manifest_digest: approved.manifest_digest.clone(),
            generated_input_digest: approved.generated_input_digest.clone(),
        };
        self.log
            .log(format!("Attempting to flash {}", path.display()));
        self.flash_log
            .log(format!("Attempting to flash {}", path.display()));
        self.current_progress = None;
        self.pending_flash = Some((
            operation_id,
            std::thread::spawn(move || {
                flash::flash_with_start_progress(
                    FlashRequest {
                        path,
                        approved: approved_metadata,
                        selected_drive,
                    },
                    || {},
                    &progress,
                )
            }),
            receiver,
        ));
        self.status = "Flashing in background…".to_string();
        self.flash_log
            .log("Artifact verification, drive detection, and host copy started in background");
    }

    fn flash_busy(&self) -> bool {
        self.pending_flash.is_some() || self.pending_detection.is_some()
    }

    fn start_watch_detection(&mut self) {
        if self.flash_busy() || self.shutting_down {
            return;
        }
        if !self.flash_worker_state.start_detection() {
            return;
        }
        let operation_id = self.next_operation_id;
        self.next_operation_id = self.next_operation_id.wrapping_add(1).max(1);
        let (progress, receiver) = progress::channel(operation_id);
        self.current_progress = None;
        self.pending_detection = Some((
            operation_id,
            std::thread::spawn(move || {
                flash::select_watch_drive_with_progress(flash::windows_drive_roots(), &progress)
            }),
            receiver,
        ));
        self.status = "Refreshing Sensor Watch detection…".to_string();
    }

    fn drain_progress(&mut self, receiver: &ProgressReceiver) {
        for event in receiver.drain() {
            self.current_progress = Some(event.clone());
            let progress = match (event.current, event.total) {
                (Some(current), Some(total)) if total > 0 => format!(" [{current}/{total}]"),
                _ => String::new(),
            };
            self.flash_log.log(format!(
                "op={} seq={} [{}]{} {}",
                event.operation_id,
                event.sequence,
                event.phase.label(),
                progress,
                event.message
            ));
        }
        let dropped = receiver.take_dropped();
        if dropped > 0 {
            let message =
                format!("Progress channel dropped {dropped} event(s); details may be missing");
            self.flash_log.log(&message);
            self.status = message;
        }
    }

    fn poll_flash_workers(&mut self) {
        if let Some((operation_id, handle, receiver)) = self.pending_detection.take() {
            self.drain_progress(&receiver);
            if handle.is_finished() {
                match handle.join() {
                    Ok(selection) => {
                        self.flash_worker_state.finish();
                        self.clear_completed_progress(operation_id);
                        self.cached_watch = selection;
                        if let WatchDriveSelection::One(candidate) = &self.cached_watch {
                            if let Some(board) = board_from_info(&candidate.info) {
                                if self.board != board {
                                    self.board = board;
                                    self.log.log(format!(
                                        "Auto-selected board {} from watch",
                                        board.label()
                                    ));
                                }
                            }
                        }
                        self.status = "Sensor Watch detection refreshed".to_string();
                    }
                    Err(_) => {
                        self.flash_worker_state.finish();
                        self.clear_completed_progress(operation_id);
                        self.status = "Watch detection worker panicked".to_string();
                    }
                }
            } else {
                self.pending_detection = Some((operation_id, handle, receiver));
            }
        }
        if let Some((operation_id, handle, receiver)) = self.pending_flash.take() {
            self.drain_progress(&receiver);
            if handle.is_finished() {
                match handle.join() {
                    Ok(result) => {
                        self.flash_worker_state.finish();
                        self.clear_completed_progress(operation_id);
                        self.apply_flash_result(result);
                    }
                    Err(_) => {
                        self.flash_worker_state.finish();
                        self.clear_completed_progress(operation_id);
                        self.status = "Flash worker panicked; no completion is assumed".to_string();
                        self.log_error(&self.status.clone());
                    }
                }
            } else {
                self.pending_flash = Some((operation_id, handle, receiver));
            }
        }
    }

    fn clear_completed_progress(&mut self, operation_id: u64) {
        if self
            .current_progress
            .as_ref()
            .is_some_and(|event| event.operation_id == operation_id)
        {
            self.current_progress = None;
        }
    }

    fn apply_flash_result(&mut self, result: FlashResult) {
        self.status = result.message.clone();
        self.flash_log.log(&result.message);
        match result.status {
            FlashStatus::HostCopySucceeded => {
                self.log.log(&result.message);
                self.fetch_ntp();
                self.flash_log.log("Fetching NTP time for sync...");
            }
            FlashStatus::ArtifactInvalid
            | FlashStatus::ArtifactChanged
            | FlashStatus::NoWatch
            | FlashStatus::Ambiguous
            | FlashStatus::DriveDisappeared
            | FlashStatus::Failed => {
                self.log_error(&result.message);
            }
        }
    }

    /// The label for the current stats sampling rate.
    fn stats_rate_label(&self) -> &'static str {
        match self.stats_rate_ms {
            250 => "Real-time (0.25s)",
            500 => "0.5s",
            2000 => "2s",
            _ => "1s",
        }
    }
}

fn board_from_info(info: &str) -> Option<Board> {
    let lower = info.to_ascii_lowercase();
    if lower.contains("pro") {
        Some(Board::Pro)
    } else if lower.contains("blue") {
        Some(Board::Blue)
    } else if lower.contains("red") || lower.contains("lite") {
        Some(Board::RedLite)
    } else if lower.contains("green") {
        Some(Board::Green)
    } else {
        None
    }
}

/// Measures regular files below a directory without entering unrelated paths.
/// Returns `None` when the directory is absent or cannot be read.
fn measure_directory(root: &std::path::Path) -> Option<(usize, u64)> {
    if !root.is_dir() {
        return None;
    }
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(metadata) = entry.metadata() {
                files += 1;
                bytes = bytes.saturating_add(metadata.len());
            }
        }
    }
    Some((files, bytes))
}

/// Counts the source files, lines, characters, and bytes in the project's Rust
/// source (firmware + core + studio).
fn code_stats() -> (usize, usize, usize, u64) {
    let root = build::firmware_dir();
    let mut files = 0usize;
    let mut lines = 0usize;
    let mut chars = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![root.join("src"), root.join("core"), root.join("studio")];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        files += 1;
                        lines += content.lines().count();
                        chars += content.chars().count();
                        bytes += content.len() as u64;
                    }
                }
            }
        }
    }
    (files, lines, chars, bytes)
}

/// Formats a byte count into a human-readable string.
fn fmt_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{b} B")
    }
}

fn print_cli_help() {
    println!(
        "Usage: sensor-watch-studio <COMMAND> [ARGS]\n\nCommands:\n  build\n      Refuse unconfigured firmware builds until Studio inputs are wired\n  uf2 <INPUT> <OUTPUT>\n      Convert a binary image to UF2\n  verify <PATH> [--manifest <PATH>] [--trusted-sha256 <SHA256>]\n      Verify a UF2 artifact and its optional manifest\n  backup <SRC> <DST>\n      Preserve a known-good UF2 and write its manifest\n  rollback <SRC> <DST> <TRUSTED_SHA256>\n      Verify and stage a trusted rollback UF2\n  report <PATH> <TRUSTED_SHA256>\n      Print a recovery report for a trusted UF2\n  flash [ELF]\n      Flash firmware with probe-rs\n  help\n      Show this help\n\nWith no command, Firmware Studio starts its normal GUI."
    );
}

fn required_cli_arg(args: &mut impl Iterator<Item = String>) -> Result<String, String> {
    args.next()
        .ok_or_else(|| "missing required argument (try --help)".to_string())
}

fn ensure_cli_no_extra(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    if let Some(extra) = args.next() {
        return Err(format!("unexpected argument: {extra} (try --help)"));
    }
    Ok(())
}

fn print_manifest_cli(manifest: &sensor_watch_tools::Manifest) -> Result<(), String> {
    let output = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    println!("{output}");
    Ok(())
}

fn run_cli(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let command = args
        .next()
        .ok_or_else(|| "missing command (try --help)".to_string())?;
    match command.as_str() {
        "help" | "--help" | "-h" => {
            print_cli_help();
            Ok(())
        }
        "build" => {
            ensure_cli_no_extra(&mut args)?;
            build::validate_configuration_inputs().map_err(str::to_string)?;
            let result = sensor_watch_tools::build_firmware()?;
            println!("built {}", result.uf2_path.display());
            Ok(())
        }
        "uf2" => {
            let input = std::path::PathBuf::from(required_cli_arg(&mut args)?);
            let output = std::path::PathBuf::from(required_cli_arg(&mut args)?);
            ensure_cli_no_extra(&mut args)?;
            sensor_watch_tools::convert_uf2(&input, &output)?;
            println!("wrote {}", output.display());
            Ok(())
        }
        "verify" => {
            let path = std::path::PathBuf::from(required_cli_arg(&mut args)?);
            let mut manifest = None;
            let mut trusted = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--manifest" if manifest.is_none() => {
                        manifest = Some(std::path::PathBuf::from(required_cli_arg(&mut args)?));
                    }
                    "--trusted-sha256" if trusted.is_none() => {
                        trusted = Some(required_cli_arg(&mut args)?)
                    }
                    _ => return Err(format!("unknown verify option: {arg}")),
                }
            }
            let result =
                sensor_watch_tools::verify_uf2(&path, manifest.as_deref(), trusted.as_deref())?;
            let output = manifest.unwrap_or_else(|| path.with_extension("uf2.json"));
            if !output.exists() {
                sensor_watch_tools::write_manifest(&output, &result)?;
            }
            print_manifest_cli(&result)
        }
        "backup" => {
            let src = std::path::PathBuf::from(required_cli_arg(&mut args)?);
            let dst = std::path::PathBuf::from(required_cli_arg(&mut args)?);
            ensure_cli_no_extra(&mut args)?;
            sensor_watch_tools::backup_uf2(&src, &dst)?;
            println!("preserved known-good UF2 at {}", dst.display());
            Ok(())
        }
        "rollback" => {
            let src = std::path::PathBuf::from(required_cli_arg(&mut args)?);
            let dst = std::path::PathBuf::from(required_cli_arg(&mut args)?);
            let trusted = required_cli_arg(&mut args)?;
            ensure_cli_no_extra(&mut args)?;
            let manifest = sensor_watch_tools::rollback_uf2(&src, &dst, &trusted)?;
            println!(
                "staged rollback UF2 at {}\ngeneration {}\nsha256 {}",
                dst.display(),
                sensor_watch_tools::manifest_value(&manifest, "generation_id"),
                sensor_watch_tools::manifest_value(&manifest, "sha256")
            );
            Ok(())
        }
        "report" => {
            let path = std::path::PathBuf::from(required_cli_arg(&mut args)?);
            let trusted = required_cli_arg(&mut args)?;
            ensure_cli_no_extra(&mut args)?;
            let report = sensor_watch_tools::recovery_report(&path, &trusted)?;
            let output = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
            println!("{output}");
            Ok(())
        }
        "flash" => {
            let elf = args
                .next()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::PathBuf::from("target/thumbv6m-none-eabi/release/sensor-watch")
                });
            ensure_cli_no_extra(&mut args)?;
            sensor_watch_tools::flash_firmware(&elf)
        }
        _ => Err(format!("unknown command: {command} (try --help)")),
    }
}

#[cfg(windows)]
fn ensure_cli_console() {
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn AllocConsole() -> i32;
    }
    // A GUI-subsystem executable has no console when launched from Explorer,
    // but should reuse the caller's console when launched from a terminal.
    unsafe {
        if AttachConsole(u32::MAX) == 0 {
            let _ = AllocConsole();
        }
    }
}

#[cfg(not(windows))]
fn ensure_cli_console() {}

fn main() -> eframe::Result<()> {
    // Launcher bootstrap arguments are consumed before Studio CLI dispatch.
    // They identify one version/attempt and must never be treated as commands.
    let (startup, studio_args) = update::parse_startup_context(std::env::args().skip(1));
    let executable = std::env::current_exe().unwrap_or_default();
    let user_data = startup
        .user_data
        .clone()
        .unwrap_or_else(data_dir::default_path);

    // Recover an interrupted local package activation before any distribution,
    // resource, or project initialization can select the new version.
    let package_root = executable
        .parent()
        .map(|path| path.join("versions"))
        .unwrap_or_else(|| std::path::PathBuf::from("versions"));
    let recovery_manager = update::UpdateManager::new(&package_root, &user_data);
    if let Err(error) = recovery_manager.recover_failed_startup() {
        let message = format!("Studio update recovery failed: {error}");
        eprintln!("{message}");
        update::record_startup_status(message);
    }

    // Bootstrap preferences are deliberately unscoped: they must be read
    // before selecting an executable-hash profile. Distribution discovery is
    // initialized only after failed-startup recovery.
    let developer_mode = distribution::developer_mode_requested();
    let package_status = distribution::initialize(&executable, developer_mode);
    if !package_status.warnings.is_empty() {
        for warning in &package_status.warnings {
            eprintln!("Studio distribution: {warning}");
        }
    }
    let bootstrap = persist::load_runtime_preferences();
    let fresh = bootstrap.fresh_test_executable_profile;
    let requested_root = startup
        .user_data
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(&bootstrap.data_folder));
    let fallback_root = data_dir::default_path();
    let firmware = build::firmware_dir();
    // The bootstrap output directory is the only safe output value available
    // before selecting the executable-scoped profile. Do not load that profile
    // merely to discover its output path: that would make profile selection
    // circular.
    let output_path = std::path::PathBuf::from(&bootstrap.output_dir);
    let recovery = output_path.join("recovery");
    let protected = [
        firmware.as_path(),
        output_path.as_path(),
        recovery.as_path(),
        executable.as_path(),
    ];
    let (root, root_warning) = if data_dir::validate(&requested_root, &protected).is_ok() {
        (requested_root, None)
    } else if data_dir::validate(&fallback_root, &protected).is_ok() {
        (
            fallback_root,
            Some("Configured Studio data folder was rejected; using the default folder"),
        )
    } else {
        // This is deliberately a visible degraded mode. Never enable writes
        // against a root that overlaps firmware, output, recovery, or the exe.
        (
            fallback_root,
            Some(
                "Studio data-folder validation failed; running without changing the selected root",
            ),
        )
    };
    if let Some(warning) = root_warning {
        eprintln!("{warning}: {}", root.display());
    }
    let identity = test_runtime::current_executable_identity();
    let candidate_profile = test_runtime::resolve_from(fresh, identity, root.clone());
    let profile_protected = [
        firmware.as_path(),
        output_path.as_path(),
        recovery.as_path(),
        executable.as_path(),
    ];
    let profile_fresh = if data_dir::validate(&candidate_profile.root, &profile_protected).is_ok() {
        fresh
    } else {
        eprintln!("Studio profile overlaps a protected path; using the non-isolated profile");
        false
    };
    // Migration is intentionally not automatic here. A newly compiled
    // executable-hash profile starts clean; the user-visible Apply action is
    // the explicit, copy-first migration path.
    let profile = test_runtime::initialize_from(profile_fresh, root);
    if let Some(warning) = profile.warning.as_deref() {
        eprintln!("{warning}: {}", profile.root.display());
    }

    if !studio_args.is_empty() {
        ensure_cli_console();
        let exit_code = match run_cli(studio_args.into_iter()) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("error: {error}");
                if error.starts_with("missing ")
                    || error.starts_with("unexpected argument:")
                    || error.starts_with("unknown command:")
                    || error.starts_with("unknown verify option:")
                {
                    2
                } else {
                    1
                }
            }
        };
        std::process::exit(exit_code);
    }

    // Acknowledgement is deliberately last: distribution discovery, mutable
    // project preparation, and profile initialization have all completed.
    if let Err(error) = update::mark_startup_success(&startup) {
        eprintln!("Studio startup acknowledgement: {error}");
    }

    let options = eframe::NativeOptions {
        // Launch at 640x480 (480p, 4:3) so there's ample space by default while
        // remaining adjustable.
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        &format!("Firmware Studio {}", env!("CARGO_PKG_VERSION")),
        options,
        Box::new(|_cc| Box::new(StudioApp::default())),
    )
}

/// Recursively copies a directory, skipping links and generated trees.
fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    let src = src.canonicalize()?;
    let dst_real = match dst.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = dst
                .parent()
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "export destination has no parent",
                    )
                })?
                .canonicalize()?;
            parent.join(dst.file_name().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "export destination has no name",
                )
            })?)
        }
        Err(error) => return Err(error),
    };
    if dst_real == src || dst_real.starts_with(&src) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "export destination is inside the source tree",
        ));
    }
    if let Ok(metadata) = std::fs::symlink_metadata(dst) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "export destination must be a real directory",
            ));
        }
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(&src)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        let name = entry.file_name();
        if name == "target" || name == ".git" || name == "export" || is_secret_export_path(&name) {
            continue;
        }
        if metadata.file_type().is_symlink() {
            continue;
        }
        let dest = dst.join(&name);
        if let Ok(dest_metadata) = std::fs::symlink_metadata(&dest) {
            if dest_metadata.file_type().is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "refusing to overwrite symlink in export destination",
                ));
            }
        }
        if metadata.is_dir() {
            copy_dir(&path, &dest)?;
        } else if metadata.is_file() {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

fn is_secret_export_path(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy().to_ascii_lowercase();
    name == ".env"
        || name.starts_with(".env.")
        || name == ".aws"
        || name == ".ssh"
        || name == "secret"
        || name == "secrets"
        || name == "id_rsa"
        || name == "id_ed25519"
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
}

/// Copies text to the system clipboard (best-effort).
fn ui_copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| e.to_string())
}

/// Reads text from the system clipboard (best-effort).
fn ui_paste_from_clipboard() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.get_text().map_err(|e| e.to_string())
}

/// Fetches the expected SHA-256 for the current release from GitHub.
///
/// The release asset is named `sensor-watch-studio.sha256` and contains the
/// hash of the release executable. Returns an error if offline or unavailable.
fn fetch_release_sha256() -> Result<String, String> {
    let url = "https://raw.githubusercontent.com/kaiiuen/sensor-watch-rs/master/release/sensor-watch-studio.sha256";
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| e.to_string())?;
    let body = resp.into_string().map_err(|e| e.to_string())?;
    let sha = body.trim().to_ascii_lowercase();
    if is_valid_sha256(&sha) {
        Ok(sha)
    } else {
        Err("Unexpected checksum format".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteCommit {
    sha: String,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpdateCheckError {
    RateLimited,
    NotFound,
    Timeout,
    Network(String),
    Http(u16),
    MalformedJson(String),
}

impl std::fmt::Display for UpdateCheckError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited => write!(formatter, "GitHub rate limit reached (HTTP 403)"),
            Self::NotFound => write!(
                formatter,
                "GitHub branch or repository not found (HTTP 404)"
            ),
            Self::Timeout => write!(formatter, "GitHub request timed out"),
            Self::Network(message) => write!(formatter, "network error: {message}"),
            Self::Http(status) => write!(formatter, "GitHub returned HTTP {status}"),
            Self::MalformedJson(message) => write!(formatter, "malformed GitHub JSON: {message}"),
        }
    }
}

#[derive(serde::Deserialize)]
struct GithubCommitResponse {
    sha: String,
    commit: GithubCommitDetails,
}

#[derive(serde::Deserialize)]
struct GithubCommitDetails {
    message: String,
}

fn parse_latest_commit(body: &str) -> Result<RemoteCommit, UpdateCheckError> {
    let parsed: GithubCommitResponse = serde_json::from_str(body)
        .map_err(|error| UpdateCheckError::MalformedJson(error.to_string()))?;
    if parsed.sha.trim().is_empty() || parsed.commit.message.trim().is_empty() {
        return Err(UpdateCheckError::MalformedJson(
            "commit SHA and message are required".to_string(),
        ));
    }
    Ok(RemoteCommit {
        sha: parsed.sha,
        message: parsed.commit.message,
    })
}

fn local_commit_sha() -> Option<&'static str> {
    option_env!("GIT_COMMIT_SHA")
        .or(option_env!("VERGEN_GIT_SHA"))
        .map(str::trim)
        .filter(|sha| !sha.is_empty())
}

fn short_sha(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

fn commits_match(local: Option<&str>, remote: &str) -> Option<bool> {
    local.map(|local| local.eq_ignore_ascii_case(remote))
}

fn classify_http_status(status: u16) -> UpdateCheckError {
    match status {
        403 => UpdateCheckError::RateLimited,
        404 => UpdateCheckError::NotFound,
        status => UpdateCheckError::Http(status),
    }
}

fn classify_transport(
    kind: ureq::ErrorKind,
    message: Option<&str>,
    timed_out: bool,
) -> UpdateCheckError {
    if timed_out || message.is_some_and(|message| message.contains("timed out")) {
        UpdateCheckError::Timeout
    } else {
        UpdateCheckError::Network(format!("{}: {}", kind, message.unwrap_or("request failed")))
    }
}

fn fetch_latest_commit() -> Result<RemoteCommit, UpdateCheckError> {
    let url = "https://api.github.com/repos/kaiiuen/sensor-watch-rs/commits/master";
    let response = ureq::get(url)
        .set("User-Agent", "Firmware-Studio")
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(status, _) => classify_http_status(status),
            ureq::Error::Transport(transport) => {
                let timed_out = transport
                    .source()
                    .and_then(|source| source.downcast_ref::<std::io::Error>())
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::TimedOut);
                classify_transport(transport.kind(), transport.message(), timed_out)
            }
        })?;
    let body = response
        .into_string()
        .map_err(|error| UpdateCheckError::Network(error.to_string()))?;
    parse_latest_commit(&body)
}

/// Returns a cautious, user-facing description of the two firmware volume steps.
fn volume_description(loud: bool) -> &'static str {
    if loud {
        "Loud: stronger knock analogy; up to the configured 9.0 V drive limit. Estimated level only - dB is not measured here."
    } else {
        "Soft: gentle tap analogy; lower drive than Loud. Estimated voltage/level only - dB depends on the hardware and environment."
    }
}

/// Shows a hex field, swatch, and editable RGB picker for a persisted color.
fn color_picker_row(ui: &mut egui::Ui, hex: &mut String, id: &str) {
    ui.horizontal(|ui| {
        ui.text_edit_singleline(hex);
        if let Some(col) = parse_hex_color(hex) {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 16.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, col);
            let mut rgb = [
                col.r() as f32 / 255.0,
                col.g() as f32 / 255.0,
                col.b() as f32 / 255.0,
            ];
            let picker = ui.push_id(id, |ui| ui.color_edit_button_rgb(&mut rgb));
            if picker.inner.changed() {
                *hex = format!(
                    "#{:02x}{:02x}{:02x}",
                    (rgb[0] * 255.0) as u8,
                    (rgb[1] * 255.0) as u8,
                    (rgb[2] * 255.0) as u8
                );
            }
        }
    });
}

/// Parses a hex color string like "#00FF88" into an egui color.
fn parse_hex_color(s: &str) -> Option<egui::Color32> {
    let t = s.trim().trim_start_matches('#');
    if t.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(t, 16).ok()?;
    Some(egui::Color32::from_rgb(
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    ))
}

/// Clears every simulator-side button latch without emitting a release event.
#[allow(clippy::too_many_arguments)]
fn reset_simulator_button_state(
    btn_l_down: &mut bool,
    btn_c_down: &mut bool,
    btn_a_down: &mut bool,
    btn_l_hold: &mut f32,
    btn_c_hold: &mut f32,
    btn_a_hold: &mut f32,
    btn_l_events: &mut real_face::ButtonEventState,
    btn_a_events: &mut real_face::ButtonEventState,
    held_button: &mut Option<ButtonId>,
) {
    *btn_l_down = false;
    *btn_c_down = false;
    *btn_a_down = false;
    *btn_l_hold = 0.0;
    *btn_c_hold = 0.0;
    *btn_a_hold = 0.0;
    *btn_l_events = real_face::ButtonEventState::default();
    *btn_a_events = real_face::ButtonEventState::default();
    *held_button = None;
}

/// Updates the simulator's pointer ownership barrier.
///
/// A pointer-up is the only event that clears a cancellation barrier. While
/// blocked, a still-down pointer cannot acquire any button, including after
/// pointer drift or a tab/face replacement.
fn update_simulator_pointer_lock(
    blocked_until_pointer_release: &mut bool,
    held_button: &mut Option<ButtonId>,
    pointer_down: bool,
    under: Option<ButtonId>,
) {
    if !pointer_down {
        *blocked_until_pointer_release = false;
        *held_button = None;
    } else if !*blocked_until_pointer_release && held_button.is_none() {
        *held_button = under;
    }
}

/// Edge-detects a simulator button and returns the action to apply.
///
/// - Fires `Press` exactly once on the press edge.
/// - Fires `Release` once on the release edge.
/// - While held, `hold` accumulates elapsed seconds (used for hold features
///   like the CASIO display). It does NOT auto-repeat.
fn handle_sim_button(is_down: bool, down: &mut bool, hold: &mut f32, dt: f32) -> SimAction {
    if is_down && !*down {
        // Press edge: fire once.
        *down = true;
        *hold = 0.0;
        SimAction::Press
    } else if !is_down && *down {
        // Release edge.
        *down = false;
        *hold = 0.0;
        SimAction::Release
    } else if is_down {
        // Held: accumulate hold time (no auto-repeat).
        *hold += dt;
        SimAction::None
    } else {
        SimAction::None
    }
}

/// Draws a fixed-style clickable region that never changes appearance while
/// held (unlike `egui::Button`, which greys out / depresses on press).
fn sim_hold_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let desired = egui::vec2(80.0, 40.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = visuals.widgets.inactive.bg_fill;
        let stroke = visuals.widgets.inactive.bg_stroke;
        ui.painter().rect(rect, 4.0, fill, stroke);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(14.0),
            visuals.text_color(),
        );
    }
    response
}

/// Returns the simulated shell command name only for the exact no-argument
/// commands or the two commands that have their own argument parsers.
fn simulated_shell_command_name(command: &str) -> Option<&'static str> {
    match command {
        "help" => Some("help"),
        "optical" => Some("optical"),
        "time" => Some("time"),
        _ if command.starts_with("settime ") => Some("settime"),
        _ if command.starts_with("drift ") => Some("drift"),
        _ => None,
    }
}

/// Parses the simulated/firmware-compatible `drift N` command.
fn parse_drift_command(command: &str) -> Option<i16> {
    let payload = command.strip_prefix("drift ")?;
    if payload.is_empty() || payload.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    let ppm = payload.parse::<i16>().ok()?;
    (-127..=127).contains(&ppm).then_some(ppm)
}

/// Parses a `YYMMDDHHMMSS` settime payload into (year, month, day, hour,
/// minute). Returns `None` if the payload is not exactly 12 digits.
fn parse_settime(payload: &str) -> Option<(i32, u32, u32, u32, u32)> {
    let bytes = payload.as_bytes();
    if bytes.len() != 12 || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let part = |range: std::ops::Range<usize>| -> u32 {
        bytes[range]
            .iter()
            .fold(0u32, |acc, b| acc * 10 + u32::from(b - b'0'))
    };
    let yy = part(0..2);
    let month = part(2..4);
    let day = part(4..6);
    let hour = part(6..8);
    let minute = part(8..10);
    let second = part(10..12);
    let year = 2000 + yy as i32;
    // The firmware RTC stores 2020..2083 in its 6-bit year field.
    if !(20..=83).contains(&yy)
        || second > 59
        || month == 0
        || month > 12
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
    {
        return None;
    }
    Some((year, month, day, hour, minute))
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Re-verify the candidate immediately before flashing using the same shared
/// verifier as explicit inspection. This is local consistency checking only;
/// it does not establish authenticity.
#[cfg(test)]
fn verify_artifact_manifest(
    artifact: &std::path::Path,
    manifest_path: &std::path::Path,
) -> Result<(), String> {
    sensor_watch_tools::verify_uf2(artifact, Some(manifest_path), None)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.is_ascii() && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Returns the selected combined-server index after removing one custom server.
fn selection_after_custom_ntp_removal(selected: usize, removed: usize) -> usize {
    if selected == removed {
        0
    } else if selected > removed {
        selected - 1
    } else {
        selected
    }
}

fn invalidate_ntp_reference(ntp_time: &mut Option<u64>, ntp_ping: &mut f64, ntp_offset: &mut f64) {
    *ntp_time = None;
    *ntp_ping = 0.0;
    *ntp_offset = 0.0;
}

/// Existing output files are for explicit inspection/recovery only. They are
/// never implicitly promoted to this session's flashable artifact.
fn initial_flashable_uf2() -> Option<ApprovedArtifact> {
    None
}

fn flashable_uf2_after_build(result: &build::BuildResult) -> Option<std::path::PathBuf> {
    result.success.then(|| result.uf2_path.clone()).flatten()
}

/// A build is publishable only after its UF2 and both manifest sidecars pass
/// the shared verifier. Verification failure must not affect approved flash
/// state or build accounting.
fn verified_artifact_after_build(
    result: &build::BuildResult,
) -> Result<build::ArtifactInspection, String> {
    let path = flashable_uf2_after_build(result)
        .ok_or_else(|| "successful build produced no UF2 artifact".to_string())?;
    let inspection = build::inspect_artifact(&path)?;
    build::validate_generated_input_digest(&inspection)?;
    Ok(inspection)
}

const ARTIFACT_VERIFIED_PENDING_STATUS: &str = "Artifact verified; approval pending";
const ARTIFACT_APPROVED_STATUS: &str = "Artifact approved for this session";
const ARTIFACT_VERIFICATION_FAILED_STATUS: &str = "Artifact verification failed";
const ARTIFACT_BUSY_STATUS: &str = "Artifact inspection unavailable while build is in progress";

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApprovedArtifact {
    path: std::path::PathBuf,
    generation: String,
    family_id: String,
    uf2_bytes: String,
    uf2_blocks: String,
    payload_bytes: String,
    sha256: String,
    payload_sha256: String,
    manifest_digest: String,
    generated_input_digest: String,
    config_fingerprint: String,
}

impl ApprovedArtifact {
    fn from_inspection(inspection: &build::ArtifactInspection) -> Self {
        Self {
            path: inspection.path.clone(),
            generation: inspection.generation.clone(),
            family_id: inspection.family_id.clone(),
            uf2_bytes: inspection.uf2_bytes.clone(),
            uf2_blocks: inspection.uf2_blocks.clone(),
            payload_bytes: inspection.payload_bytes.clone(),
            sha256: inspection.sha256.clone(),
            payload_sha256: inspection.payload_sha256.clone(),
            manifest_digest: inspection.manifest_digest.clone(),
            generated_input_digest: inspection.generated_input_digest.clone(),
            config_fingerprint: String::new(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn matches(&self, inspection: &build::ArtifactInspection) -> bool {
        self == &Self::from_inspection(inspection)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn configuration_fingerprint(
    board: Board,
    presets: &PresetManager,
    watch_config: &watch_config::WatchConfig,
    modules: &modules::ModuleManager,
    component_profiles: &[components::BuildProfile],
    component_profile: usize,
    component_draft: &components::ComponentsConfig,
    output_dir: &str,
) -> String {
    configuration_fingerprint_with_effective(
        board,
        presets,
        watch_config,
        modules,
        component_profiles,
        component_profile,
        component_draft,
        component_draft,
        output_dir,
    )
}

fn configuration_fingerprint_with_effective(
    board: Board,
    presets: &PresetManager,
    watch_config: &watch_config::WatchConfig,
    modules: &modules::ModuleManager,
    component_profiles: &[components::BuildProfile],
    component_profile: usize,
    component_draft: &components::ComponentsConfig,
    component_effective: &components::ComponentsConfig,
    output_dir: &str,
) -> String {
    build_snapshot::BuildInputSnapshot::from_state(
        board.label(),
        presets,
        watch_config,
        modules,
        component_profiles,
        component_profile,
        component_draft,
        component_effective,
        output_dir,
    )
    .fingerprint()
}

fn invalidate_stale_artifact_state(
    _approved_artifact: &mut Option<ApprovedArtifact>,
    pending_artifact: &mut Option<build::ArtifactInspection>,
    pending_fingerprint: &mut Option<String>,
    current_fingerprint: &str,
) -> bool {
    // A planning fingerprint describes current UI intent, not artifact
    // provenance. An approved existing artifact remains valid until its UF2,
    // sidecars, or compatibility checks fail at copy time. Only an unapproved
    // build candidate is tied to the current planning state.
    let pending_stale =
        pending_artifact.is_some() && pending_fingerprint.as_deref() != Some(current_fingerprint);
    if pending_stale {
        *pending_artifact = None;
        *pending_fingerprint = None;
        true
    } else {
        false
    }
}

fn set_verified_artifact_state(
    status: &mut String,
    build_message: &mut String,
    approved_artifact: &mut Option<ApprovedArtifact>,
    pending_artifact: &mut Option<build::ArtifactInspection>,
    inspection: build::ArtifactInspection,
    artifact_actions_blocked: bool,
) {
    if artifact_actions_blocked {
        *status = ARTIFACT_BUSY_STATUS.to_string();
        *build_message = ARTIFACT_BUSY_STATUS.to_string();
        return;
    }
    // A newly verified candidate supersedes the prior approval. It must be
    // explicitly approved before it can become flashable.
    *approved_artifact = None;
    *status = ARTIFACT_VERIFIED_PENDING_STATUS.to_string();
    *build_message = format!(
        "Artifact verified locally; approve it before flashing.\n{}",
        artifact_metadata(&inspection)
    );
    *pending_artifact = Some(inspection);
}

fn approve_artifact_state(
    status: &mut String,
    build_message: &mut String,
    pending_artifact: &mut Option<build::ArtifactInspection>,
    approved_artifact: &mut Option<ApprovedArtifact>,
    artifact_actions_blocked: bool,
) {
    if artifact_actions_blocked {
        *status = ARTIFACT_BUSY_STATUS.to_string();
        *build_message = ARTIFACT_BUSY_STATUS.to_string();
        return;
    }
    let Some(inspection) = pending_artifact.take() else {
        return;
    };
    if inspection.generated_input_digest.is_empty() {
        *status = ARTIFACT_VERIFICATION_FAILED_STATUS.to_string();
        *build_message = "Artifact rejected: generated-input digest is missing".to_string();
        return;
    }
    *status = ARTIFACT_APPROVED_STATUS.to_string();
    *approved_artifact = Some(ApprovedArtifact::from_inspection(&inspection));
    *build_message = format!("Approved for flashing: {}", inspection.path.display());
}

fn set_failed_artifact_state(
    status: &mut String,
    build_message: &mut String,
    _approved_artifact: &mut Option<ApprovedArtifact>,
    pending_artifact: &mut Option<build::ArtifactInspection>,
    error: String,
) {
    *status = ARTIFACT_VERIFICATION_FAILED_STATUS.to_string();
    *build_message = format!("Artifact rejected: {error}");
    *pending_artifact = None;
}

fn artifact_metadata(inspection: &build::ArtifactInspection) -> String {
    format!(
        "Path: {}\nGeneration: {}\nFamily: {}\nUF2: {} bytes / {} blocks\nPayload: {} bytes\nUF2 SHA-256: {}\nPayload SHA-256: {}\nManifest digest: {}",
        inspection.path.display(),
        inspection.generation,
        inspection.family_id,
        inspection.uf2_bytes,
        inspection.uf2_blocks,
        inspection.payload_bytes,
        inspection.sha256,
        inspection.payload_sha256,
        inspection.manifest_digest,
        inspection.generated_input_digest
    )
}

#[cfg(test)]
mod tests {
    use super::Board;
    use super::{
        approve_artifact_state, clamp_sim_weekday, configuration_fingerprint,
        contextual_help_allowed, credit_matches, flashable_uf2_after_build, initial_flashable_uf2,
        invalidate_stale_artifact_state, preset_name, set_failed_artifact_state,
        set_verified_artifact_state, sim_weekday_name, verified_artifact_after_build,
        verify_artifact_manifest, ApprovedArtifact, CreditEntry, WatchDriveSelection,
        ARTIFACT_APPROVED_STATUS, ARTIFACT_BUSY_STATUS, ARTIFACT_VERIFICATION_FAILED_STATUS,
        ARTIFACT_VERIFIED_PENDING_STATUS, CREDIT_GROUPS, FIRST_RUN_STEPS, HELP_CARD_LAYER_ORDER,
        HELP_DIM_LAYER_ORDER,
    };
    use super::{
        classify_http_status, classify_transport, commits_match, parse_latest_commit,
        UpdateCheckError,
    };
    use super::{
        handle_sim_button, reset_simulator_button_state, update_simulator_pointer_lock, ButtonId,
        SimAction,
    };
    use crate::components;
    use crate::flash::{select_watch_drive, FlashResult, FlashStatus};
    use crate::modules;
    use crate::presets::PresetManager;
    use crate::progress::{self, Phase, ProgressEvent};
    use crate::watch_config;

    fn test_progress_event(operation_id: u64, phase: Phase, message: &str) -> ProgressEvent {
        ProgressEvent {
            operation_id,
            sequence: 0,
            phase,
            message: message.to_string(),
            current: None,
            total: None,
        }
    }

    #[test]
    fn simulated_shell_accepts_exact_commands_only() {
        for (command, name) in [("help", "help"), ("optical", "optical"), ("time", "time")] {
            assert_eq!(super::simulated_shell_command_name(command), Some(name));
        }
        assert_eq!(super::simulated_shell_command_name("help extra"), None);
        assert_eq!(super::simulated_shell_command_name("time 1"), None);
        assert_eq!(super::simulated_shell_command_name("optical now"), None);
        assert_eq!(super::simulated_shell_command_name(" time"), None);
    }

    #[test]
    fn simulated_shell_rejects_extra_arguments_and_malformed_forms() {
        for command in ["drift", "settime"] {
            assert_eq!(
                super::simulated_shell_command_name(command),
                None,
                "{command}"
            );
        }
        assert_eq!(super::parse_drift_command("drift 1 2"), None);
        assert_eq!(super::parse_drift_command("drift 128"), None);
        assert_eq!(super::parse_drift_command("drift -128"), None);
        assert_eq!(super::parse_drift_command("drift 1 "), None);
        assert_eq!(super::parse_settime("260101120000 extra"), None);
        assert_eq!(super::parse_settime("26010112000x"), None);
        assert_eq!(super::parse_settime("260101120000 "), None);
    }

    #[test]
    fn simulated_shell_accepts_firmware_argument_forms() {
        assert_eq!(super::parse_drift_command("drift 0"), Some(0));
        assert_eq!(super::parse_drift_command("drift +127"), Some(127));
        assert_eq!(super::parse_drift_command("drift -127"), Some(-127));
        assert_eq!(
            super::parse_settime("240229235959"),
            Some((2024, 2, 29, 23, 59))
        );
        assert_eq!(
            super::parse_settime("831231235959"),
            Some((2083, 12, 31, 23, 59))
        );
    }

    fn poll_until_flash_workers_finish(app: &mut super::StudioApp) {
        for _ in 0..1000 {
            app.poll_flash_workers();
            if app.pending_detection.is_none() && app.pending_flash.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("test worker did not finish");
    }

    #[test]
    fn parses_github_commit_json_without_substring_assumptions() {
        let remote = parse_latest_commit(
            r#"{"sha":"abc12345","commit":{"message":"Fix \"quoted\" JSON\\ntext"}}"#,
        )
        .expect("valid GitHub response");
        assert_eq!(remote.sha, "abc12345");
        assert_eq!(remote.message, "Fix \"quoted\" JSON\\ntext");
    }

    #[test]
    fn classifies_malformed_json_and_http_failures_distinctly() {
        assert!(matches!(
            parse_latest_commit("not json"),
            Err(UpdateCheckError::MalformedJson(_))
        ));
        assert!(matches!(
            classify_http_status(403),
            UpdateCheckError::RateLimited
        ));
        assert!(matches!(
            classify_http_status(404),
            UpdateCheckError::NotFound
        ));
        assert!(matches!(
            classify_transport(ureq::ErrorKind::Io, Some("timed out"), false),
            UpdateCheckError::Timeout
        ));
        assert!(matches!(
            classify_transport(ureq::ErrorKind::Dns, Some("offline"), false),
            UpdateCheckError::Network(_)
        ));
    }

    #[test]
    fn commit_comparison_never_infers_update_from_message() {
        assert_eq!(commits_match(Some("ABC"), "abc"), Some(true));
        assert_eq!(commits_match(Some("local"), "remote"), Some(false));
        assert_eq!(commits_match(None, "remote"), None);
    }

    #[test]
    fn failed_update_check_clears_stale_state_and_spinner() {
        let mut app = super::StudioApp::default();
        app.latest_commit = Some("old message".into());
        app.latest_sha = Some("old sha".into());
        app.update_time = Some(123);
        app.update_checking = true;
        app.finish_update_check(Ok(Err(UpdateCheckError::NotFound)));
        assert!(!app.update_checking);
        assert!(app.latest_commit.is_none());
        assert!(app.latest_sha.is_none());
        assert!(app.update_time.is_none());
        assert!(app.status.contains("404"));
    }

    #[test]
    fn completed_detection_clears_spinner_but_keeps_progress_history() {
        let mut app = super::StudioApp::default();
        let operation_id = 41;
        let (progress, receiver) = progress::channel(operation_id);
        progress.emit(
            Phase::Selection,
            "No Sensor Watch drive selected",
            Some(0),
            Some(0),
        );
        app.current_progress = Some(test_progress_event(
            operation_id,
            Phase::Selection,
            "No Sensor Watch drive selected",
        ));
        assert!(app.flash_worker_state.start_detection());
        app.pending_detection = Some((
            operation_id,
            std::thread::spawn(|| WatchDriveSelection::None),
            receiver,
        ));

        poll_until_flash_workers_finish(&mut app);

        assert!(app.current_progress.is_none());
        assert_eq!(app.flash_worker_state, super::flash::WorkerState::Idle);
        assert!(app
            .flash_log
            .entries()
            .iter()
            .any(|entry| entry.message.contains("op=41")
                && entry.message.contains("No Sensor Watch drive selected")));
    }

    #[test]
    fn failed_flash_clears_spinner_and_uses_final_result_authority() {
        let mut app = super::StudioApp::default();
        let operation_id = 42;
        let (progress, receiver) = progress::channel(operation_id);
        progress.emit(Phase::Failure, "transfer failed", None, None);
        app.current_progress = Some(test_progress_event(
            operation_id,
            Phase::Failure,
            "transfer failed",
        ));
        assert!(app.flash_worker_state.start_flash());
        app.pending_flash = Some((
            operation_id,
            std::thread::spawn(|| FlashResult {
                status: FlashStatus::Failed,
                message: "final flash failure".to_string(),
            }),
            receiver,
        ));

        poll_until_flash_workers_finish(&mut app);

        assert!(app.current_progress.is_none());
        assert_eq!(app.status, "final flash failure");
        assert_eq!(app.flash_worker_state, super::flash::WorkerState::Idle);
        assert!(app
            .flash_log
            .entries()
            .iter()
            .any(|entry| entry.message.contains("op=42")
                && entry.message.contains("transfer failed")));
        assert!(app
            .flash_log
            .entries()
            .iter()
            .any(|entry| entry.message == "final flash failure"));
    }

    #[test]
    fn stale_progress_from_another_operation_is_not_cleared_by_late_completion() {
        let mut app = super::StudioApp::default();
        let (progress, receiver) = progress::channel(43);
        app.current_progress = Some(test_progress_event(44, Phase::Transfer, "new operation"));
        assert!(app.flash_worker_state.start_detection());
        app.pending_detection = Some((
            43,
            std::thread::spawn(|| WatchDriveSelection::None),
            receiver,
        ));
        drop(progress);

        poll_until_flash_workers_finish(&mut app);

        assert_eq!(
            app.current_progress
                .as_ref()
                .map(|event| event.operation_id),
            Some(44)
        );
    }

    #[test]
    fn help_layers_keep_painter_below_the_single_interactive_card() {
        assert_eq!(HELP_DIM_LAYER_ORDER, egui::Order::Middle);
        assert_eq!(HELP_CARD_LAYER_ORDER, egui::Order::Foreground);
        let painter_layers = [HELP_DIM_LAYER_ORDER];
        let interactive_layers = [HELP_CARD_LAYER_ORDER];
        assert_eq!(painter_layers.len(), 1);
        assert_eq!(interactive_layers.len(), 1);
    }

    #[test]
    fn active_tour_owns_input_and_protects_unsafe_actions() {
        let mut app = super::StudioApp::default();
        // This fixture exercises the built-in safe policy, not persisted UX overrides.
        app.panel_ux_overrides.remove(super::Panel::Simulator.key());
        app.current_panel = super::Panel::Simulator;
        app.help_open = Some(crate::help::HelpId::Simulator);
        app.help_anchors.begin_frame(1);
        app.help_anchors.register(
            crate::help::HelpId::Simulator,
            crate::help::AnchorId::SimulatorWatch.key(),
            crate::help::AnchorRect {
                min: (10.0, 10.0),
                max: (100.0, 80.0),
            },
        );
        assert!(app.help_owns_input());
        assert!(!app.unsafe_action_allowed());
    }

    #[test]
    fn paused_tour_releases_input_but_keeps_unsafe_actions_blocked() {
        let mut app = super::StudioApp::default();
        app.current_panel = super::Panel::Simulator;
        app.help_open = Some(crate::help::HelpId::Simulator);
        app.help_anchors.begin_frame(1);
        app.help_anchors.register(
            crate::help::HelpId::Simulator,
            crate::help::AnchorId::SimulatorWatch.key(),
            crate::help::AnchorRect {
                min: (10.0, 10.0),
                max: (100.0, 80.0),
            },
        );
        app.help_step = 1;
        app.minimize_help();
        assert!(!app.help_owns_input());
        assert!(!app.unsafe_action_allowed());
        assert_eq!(app.help_open, Some(crate::help::HelpId::Simulator));
        assert_eq!(app.help_step, 1);
    }

    #[test]
    fn pausing_clears_simulator_latches_once() {
        let mut app = super::StudioApp::default();
        app.help_open = Some(crate::help::HelpId::Simulator);
        app.btn_l_down = true;
        app.btn_l_hold = 2.0;
        app.held_button = Some(ButtonId::L);
        app.minimize_help();
        assert!(!app.btn_l_down && app.btn_l_hold == 0.0 && app.held_button.is_none());

        // A paused tour must not keep clearing state, or simulator testing is impossible.
        app.btn_l_down = true;
        app.btn_l_hold = 1.0;
        app.held_button = Some(ButtonId::L);
        app.minimize_help();
        assert!(app.btn_l_down && app.btn_l_hold == 1.0 && app.held_button == Some(ButtonId::L));
    }

    #[test]
    fn paused_simulator_state_can_be_used_normally() {
        let mut app = super::StudioApp::default();
        app.help_open = Some(crate::help::HelpId::Simulator);
        app.minimize_help();
        app.held_button = Some(ButtonId::C);
        app.btn_c_down = true;
        assert!(!app.help_owns_input());
        assert_eq!(app.held_button, Some(ButtonId::C));
        assert!(app.btn_c_down);
    }

    #[test]
    fn resuming_restores_tour_input_ownership() {
        let mut app = super::StudioApp::default();
        // This fixture exercises the built-in safe policy, not persisted UX overrides.
        app.panel_ux_overrides.remove(super::Panel::Simulator.key());
        app.current_panel = super::Panel::Simulator;
        app.help_open = Some(crate::help::HelpId::Simulator);
        app.help_anchors.begin_frame(1);
        app.help_anchors.register(
            crate::help::HelpId::Simulator,
            crate::help::AnchorId::SimulatorWatch.key(),
            crate::help::AnchorRect {
                min: (10.0, 10.0),
                max: (100.0, 80.0),
            },
        );
        app.minimize_help();
        app.help_minimized = false;
        assert!(app.help_owns_input());
        assert!(!app.unsafe_action_allowed());
    }

    #[test]
    fn closing_tour_clears_simulator_state_and_claims_tour() {
        let mut app = super::StudioApp::default();
        app.first_run = true;
        app.help_open = Some(crate::help::HelpId::Simulator);
        app.btn_a_down = true;
        app.btn_a_hold = 2.0;
        app.held_button = Some(ButtonId::A);
        app.close_help(true);
        assert!(app.help_open.is_none());
        assert!(!app.btn_a_down && app.btn_a_hold == 0.0 && app.held_button.is_none());
        assert!(app.tour_claims.contains(crate::help::HelpId::Simulator));
    }

    #[test]
    fn pending_tour_transition_remains_protected() {
        let mut app = super::StudioApp::default();
        app.help_open = Some(crate::help::HelpId::Editor);
        app.help_pending_panel = Some(super::Panel::Simulator);
        assert!(app.help_owns_input());
        assert!(!app.unsafe_action_allowed());
    }

    #[test]
    fn visible_tour_cancels_all_simulator_input_and_unsafe_actions_stay_closed() {
        let mut app = super::StudioApp::default();
        app.current_panel = super::Panel::Simulator;
        app.help_open = Some(crate::help::HelpId::Simulator);
        app.help_anchors.begin_frame(1);
        app.help_anchors.register(
            crate::help::HelpId::Simulator,
            crate::help::AnchorId::SimulatorWatch.key(),
            crate::help::AnchorRect {
                min: (10.0, 10.0),
                max: (100.0, 80.0),
            },
        );
        app.btn_l_down = true;
        app.btn_c_down = true;
        app.btn_a_down = true;
        app.btn_l_hold = 2.0;
        app.btn_c_hold = 2.0;
        app.btn_a_hold = 2.0;
        app.held_button = Some(ButtonId::A);
        app.watch.light = true;
        app.watch.set_casio(true);
        app.cancel_simulator_buttons();
        assert!(!app.btn_l_down && !app.btn_c_down && !app.btn_a_down);
        assert_eq!(app.btn_l_hold, 0.0);
        assert_eq!(app.btn_c_hold, 0.0);
        assert_eq!(app.btn_a_hold, 0.0);
        assert!(app.held_button.is_none());
        assert!(!app.watch.light);
        assert!(app.watch.override_text.is_none());
        assert!(!app.unsafe_action_allowed());
    }

    #[test]
    fn invalid_simulator_weekday_clamps_without_panicking() {
        let invalid_weekday = 7;

        assert_eq!(clamp_sim_weekday(invalid_weekday), 6);
        assert_eq!(sim_weekday_name(invalid_weekday), "Sat");
        assert_eq!(sim_weekday_name(0), "Sun");
        assert_eq!(sim_weekday_name(6), "Sat");
    }

    #[test]
    fn unchanged_configuration_retains_approved_artifact() {
        let presets = PresetManager::new();
        let watch_config = watch_config::WatchConfig::default();
        let modules = modules::ModuleManager::default();
        let profiles = components::default_profiles();
        let draft = components::selected_config(&profiles, 0);
        let fingerprint = configuration_fingerprint(
            Board::Green,
            &presets,
            &watch_config,
            &modules,
            &profiles,
            0,
            &draft,
            "build",
        );
        let mut approved = Some(ApprovedArtifact {
            config_fingerprint: fingerprint.clone(),
            ..ApprovedArtifact::from_inspection(&test_inspection("approved.uf2"))
        });
        let mut pending = Some(test_inspection("pending.uf2"));
        let mut pending_fingerprint = Some(fingerprint.clone());

        assert!(!invalidate_stale_artifact_state(
            &mut approved,
            &mut pending,
            &mut pending_fingerprint,
            &fingerprint,
        ));
        assert!(approved.is_some());
        assert!(pending.is_some());
    }

    #[test]
    fn planning_changes_clear_only_unapproved_candidate() {
        let base_presets = PresetManager::new();
        let base_watch_config = watch_config::WatchConfig::default();
        let base_modules = modules::ModuleManager::default();
        let base_profiles = components::default_profiles();
        let base_draft = components::selected_config(&base_profiles, 0);
        let base = (
            Board::Green,
            base_presets,
            base_watch_config,
            base_modules,
            base_profiles,
            0,
            base_draft,
            "build".to_string(),
        );
        let base_fingerprint = configuration_fingerprint(
            base.0, &base.1, &base.2, &base.3, &base.4, base.5, &base.6, &base.7,
        );

        let changed = |value: (
            Board,
            PresetManager,
            watch_config::WatchConfig,
            modules::ModuleManager,
            Vec<components::BuildProfile>,
            usize,
            components::ComponentsConfig,
            String,
        )| {
            let fingerprint = configuration_fingerprint(
                value.0, &value.1, &value.2, &value.3, &value.4, value.5, &value.6, &value.7,
            );
            assert_ne!(fingerprint, base_fingerprint);
            let mut approved = Some(ApprovedArtifact {
                config_fingerprint: base_fingerprint.clone(),
                ..ApprovedArtifact::from_inspection(&test_inspection("approved.uf2"))
            });
            let mut pending = Some(test_inspection("pending.uf2"));
            let mut pending_fingerprint = Some(base_fingerprint.clone());
            assert!(invalidate_stale_artifact_state(
                &mut approved,
                &mut pending,
                &mut pending_fingerprint,
                &fingerprint,
            ));
            assert!(approved.is_some());
            assert!(pending.is_none());
            assert!(pending_fingerprint.is_none());
        };

        let mut value = base.clone();
        value.0 = Board::Blue;
        changed(value);
        let mut value = base.clone();
        value.1.presets[0].faces.swap(0, 1);
        changed(value);
        let mut value = base.clone();
        value.2.show_seconds = !value.2.show_seconds;
        changed(value);
        let mut value = base.clone();
        value.3.modules.push(modules::Module {
            name: "custom".into(),
            target: "custom.rs".into(),
            description: "test".into(),
            enabled: true,
        });
        changed(value);
        let mut value = base.clone();
        value.4[0].name.push_str(" changed");
        changed(value);
        let mut value = base.clone();
        value.5 = 1;
        changed(value);
        let mut value = base.clone();
        value.6.buzzer = !value.6.buzzer;
        changed(value);
        let mut value = base;
        value.7 = "other-build".into();
        changed(value);
    }

    #[cfg(feature = "real-faces")]
    #[test]
    fn terminal_short_press_matches_gui_down_up_and_keeps_fallback() {
        // STOPWATCH changes its running state on Alarm Down. An Up-only
        // terminal press (the old compatibility path) therefore leaves it
        // stopped, while the GUI's Down -> Up sequence starts it.
        let expected = {
            let mut real = super::real_face::RealFace::new("STOPWATCH")
                .expect("STOPWATCH is a migrated real face");
            real.set_time(2023, 1, 6, 15, 4, 0);
            real.activate(true);
            real.button_event(
                super::real_face::RealButton::Alarm,
                super::real_face::RealButtonEvent::Down,
            );
            real.button_event(
                super::real_face::RealButton::Alarm,
                super::real_face::RealButtonEvent::Up,
            );
            let snapshot = real.snapshot();
            // Reset the stateful face before dropping it so the next isolated
            // real-face instance starts from the same deterministic state.
            real.button_event(
                super::real_face::RealButton::Alarm,
                super::real_face::RealButtonEvent::Down,
            );
            snapshot
        };
        {
            let mut app = super::StudioApp::default();
            let mut real = super::real_face::RealFace::new("STOPWATCH")
                .expect("STOPWATCH is a migrated real face");
            real.set_time(2023, 1, 6, 15, 4, 0);
            real.activate(true);
            app.real_face = Some(real);

            app.press_sim_button(super::ButtonId::A);
            let actual = app.real_face.as_ref().unwrap().snapshot();
            assert_eq!(actual.chars, expected.chars);
            assert_eq!(actual.colon, expected.colon);
            assert_eq!(actual.lap, expected.lap);
        }

        // A face without a migrated real-face implementation still uses the
        // stateful face_sim engine and must retain the terminal action.
        let mut fallback = super::StudioApp::default();
        fallback.face_engine = super::face_sim::FaceEngine::new("STOPWATCH");
        fallback.press_sim_button(super::ButtonId::A);
        assert!(fallback.face_engine.sw_running);
    }

    #[test]
    fn cancellation_blocks_a_held_c_press_until_pointer_release() {
        let mut app = super::StudioApp::default();
        app.sim_pointer_primary_down = true;
        app.btn_c_down = true;
        app.btn_c_hold = 0.4;
        app.held_button = Some(ButtonId::C);

        app.cancel_simulator_buttons();
        assert!(app.blocked_until_pointer_release);
        assert!(!app.btn_c_down);
        assert_eq!(app.held_button, None);

        // Face/tab replacement can render several frames while the physical
        // pointer remains down; none of those frames may reacquire C.
        let mut held = app.held_button;
        update_simulator_pointer_lock(
            &mut app.blocked_until_pointer_release,
            &mut held,
            true,
            Some(ButtonId::C),
        );
        assert_eq!(held, None);
        assert_eq!(
            handle_sim_button(false, &mut app.btn_c_down, &mut app.btn_c_hold, 0.0),
            SimAction::None
        );

        // A real up clears the barrier, and only the next fresh down presses C.
        update_simulator_pointer_lock(
            &mut app.blocked_until_pointer_release,
            &mut held,
            false,
            None,
        );
        assert!(!app.blocked_until_pointer_release);
        update_simulator_pointer_lock(
            &mut app.blocked_until_pointer_release,
            &mut held,
            true,
            Some(ButtonId::C),
        );
        assert_eq!(held, Some(ButtonId::C));
        assert_eq!(
            handle_sim_button(true, &mut app.btn_c_down, &mut app.btn_c_hold, 0.0),
            SimAction::Press
        );
    }

    #[test]
    fn cancellation_preserves_l_and_a_short_long_event_parity() {
        for button in [
            super::real_face::RealButton::Light,
            super::real_face::RealButton::Alarm,
        ] {
            let mut events = super::real_face::ButtonEventState::default();
            assert_eq!(
                events.update(true, 0.0),
                Some(super::real_face::RealButtonEvent::Down)
            );
            assert_eq!(
                events.update(false, 0.0),
                Some(super::real_face::RealButtonEvent::Up)
            );

            assert_eq!(
                events.update(true, 0.0),
                Some(super::real_face::RealButtonEvent::Down)
            );
            assert_eq!(
                events.update(true, super::real_face::ButtonEventState::LONG_PRESS_SECONDS),
                Some(super::real_face::RealButtonEvent::LongPress)
            );
            assert_eq!(
                events.update(false, 0.0),
                Some(super::real_face::RealButtonEvent::LongUp)
            );
            let _ = button;
        }
    }

    #[test]
    fn face_switch_while_held_starts_the_next_press_with_down() {
        let mut l_down = false;
        let mut c_down = true;
        let mut a_down = true;
        let mut l_hold = 0.0;
        let mut c_hold = 0.5;
        let mut a_hold = 0.5;
        let mut l_events = super::real_face::ButtonEventState::default();
        let mut a_events = super::real_face::ButtonEventState::default();
        let mut held = Some(ButtonId::L);

        assert!(matches!(
            handle_sim_button(true, &mut l_down, &mut l_hold, 0.0),
            SimAction::Press
        ));
        assert_eq!(
            l_events.update(true, 0.0),
            Some(super::real_face::RealButtonEvent::Down)
        );
        assert_eq!(
            l_events.update(true, 1.1),
            Some(super::real_face::RealButtonEvent::LongPress)
        );

        // C face cycling replaces the face while the pointer is still held.
        reset_simulator_button_state(
            &mut l_down,
            &mut c_down,
            &mut a_down,
            &mut l_hold,
            &mut c_hold,
            &mut a_hold,
            &mut l_events,
            &mut a_events,
            &mut held,
        );

        assert!(!l_down && !c_down && !a_down);
        assert_eq!((l_hold, c_hold, a_hold), (0.0, 0.0, 0.0));
        assert_eq!(held, None);
        assert_eq!(l_events, super::real_face::ButtonEventState::default());
        assert_eq!(a_events, super::real_face::ButtonEventState::default());
        assert_eq!(
            l_events.update(true, 0.0),
            Some(super::real_face::RealButtonEvent::Down)
        );
        assert_eq!(
            l_events.update(false, 0.0),
            Some(super::real_face::RealButtonEvent::Up)
        );
    }

    #[test]
    fn leaving_and_reentering_simulator_while_held_does_not_release_into_new_face() {
        let mut l_down = true;
        let mut c_down = false;
        let mut a_down = true;
        let mut l_hold = 1.2;
        let mut c_hold = 0.0;
        let mut a_hold = 1.2;
        let mut l_events = super::real_face::ButtonEventState::default();
        let mut a_events = super::real_face::ButtonEventState::default();
        let mut held = Some(ButtonId::A);
        l_events.update(true, 1.2);
        a_events.update(true, 1.2);

        // Leaving the tab cancels ownership; re-entering must not synthesize Up.
        reset_simulator_button_state(
            &mut l_down,
            &mut c_down,
            &mut a_down,
            &mut l_hold,
            &mut c_hold,
            &mut a_hold,
            &mut l_events,
            &mut a_events,
            &mut held,
        );

        assert_eq!(l_events.update(false, 0.0), None);
        assert_eq!(a_events.update(false, 0.0), None);
        assert!(matches!(
            handle_sim_button(true, &mut a_down, &mut a_hold, 0.0),
            SimAction::Press
        ));
        assert_eq!(
            a_events.update(true, 0.0),
            Some(super::real_face::RealButtonEvent::Down)
        );
    }

    fn shared_manifest_fixture(
        name: &str,
        configured: bool,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("sensor-watch-studio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let artifact = root.join("candidate.uf2");
        std::fs::write(
            &artifact,
            sensor_watch_core::uf2::convert_to_uf2(b"shared-tool-compatible"),
        )
        .unwrap();
        let manifest_path = artifact.with_extension("uf2.json");
        let (generated_digest, inputs) = if configured {
            let inputs = artifact.with_extension("uf2.inputs");
            std::fs::create_dir_all(&inputs).unwrap();
            let mut files = std::collections::BTreeMap::new();
            files.insert("firmware_inputs.json".to_string(), "{}".to_string());
            files.insert("firmware_inputs.rs".to_string(), "marker".to_string());
            files.insert("Cargo.config.toml".to_string(), "layer".to_string());
            files.insert("PROVENANCE.json".to_string(), "provenance".to_string());
            for (name, contents) in &files {
                std::fs::write(inputs.join(name), contents).unwrap();
            }
            let digest = crate::firmware_inputs::digest_generated_files(&files);
            std::fs::write(
                inputs.join("SHA256"),
                format!("{digest}  firmware_inputs.json\n"),
            )
            .unwrap();
            (Some(digest), Some(inputs))
        } else {
            (None, None)
        };
        let mut manifest = sensor_watch_tools::create_manifest(
            &artifact,
            Some("studio-test".into()),
            Some(&artifact),
        )
        .unwrap();
        if let Some(generated_digest) = generated_digest {
            manifest.insert(
                "generated_input_digest".into(),
                serde_json::Value::String(generated_digest),
            );
            let manifest_digest = sensor_watch_tools::manifest_digest(&manifest);
            manifest.insert(
                "manifest_digest".into(),
                serde_json::Value::String(manifest_digest.clone()),
            );
            manifest.insert(
                "signature".into(),
                serde_json::Value::String(manifest_digest),
            );
        }
        sensor_watch_tools::write_manifest(&manifest_path, &manifest).unwrap();
        let _ = inputs;
        (root, artifact)
    }

    fn drive_fixture(name: &str, info: Option<&str>) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sensor-watch-studio-drive-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        if let Some(info) = info {
            std::fs::write(root.join("INFO_UF2.TXT"), info).unwrap();
        }
        root
    }

    #[test]
    fn drive_selection_distinguishes_zero_matching_drives() {
        let root = drive_fixture("zero", None);
        assert_eq!(
            select_watch_drive([root.clone()]),
            WatchDriveSelection::None
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn drive_selection_returns_one_matching_drive() {
        let root = drive_fixture("one", Some("UF2 Bootloader; Board-ID: Sensor Watch Green"));
        match select_watch_drive([root.clone()]) {
            WatchDriveSelection::One(candidate) => assert_eq!(candidate.root, root),
            selection => panic!("expected one drive, got {selection:?}"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn drive_selection_refuses_multiple_matching_drives() {
        let first = drive_fixture(
            "multiple-first",
            Some("UF2 Bootloader; Board-ID: Sensor Watch"),
        );
        let second = drive_fixture(
            "multiple-second",
            Some("UF2 Bootloader; Family ID: 0x2C29472F"),
        );
        assert_eq!(
            select_watch_drive([first.clone(), second.clone()]),
            WatchDriveSelection::Multiple(2)
        );
        std::fs::remove_dir_all(first).unwrap();
        std::fs::remove_dir_all(second).unwrap();
    }

    #[test]
    fn drive_selection_ignores_non_watch_candidates() {
        let root = drive_fixture("non-watch", Some("UF2 Bootloader; Board-ID: Generic UF2"));
        assert_eq!(
            select_watch_drive([root.clone()]),
            WatchDriveSelection::None
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn flash_revalidation_accepts_shared_tool_manifest_and_sidecar() {
        let (root, artifact) = shared_manifest_fixture("compatible", false);
        let manifest = artifact.with_extension("uf2.json");
        assert!(verify_artifact_manifest(&artifact, &manifest).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn flash_revalidation_rejects_missing_or_tampered_sidecar() {
        let (root, artifact) = shared_manifest_fixture("sidecar", false);
        let manifest = artifact.with_extension("uf2.json");
        let sidecar = manifest.with_extension("json.sig");
        std::fs::remove_file(&sidecar).unwrap();
        let missing_error = verify_artifact_manifest(&artifact, &manifest).unwrap_err();
        assert!(missing_error.contains("json.sig") || missing_error.contains("No such file"));

        let (root_tampered, artifact_tampered) = shared_manifest_fixture("tampered-sidecar", false);
        let manifest_tampered = artifact_tampered.with_extension("uf2.json");
        let sidecar_tampered = manifest_tampered.with_extension("json.sig");
        std::fs::write(sidecar_tampered, "sha256:tampered").unwrap();
        let tampered_error =
            verify_artifact_manifest(&artifact_tampered, &manifest_tampered).unwrap_err();
        assert!(tampered_error.contains("sidecar"));

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(root_tampered).unwrap();
    }

    #[test]
    fn startup_does_not_adopt_an_existing_uf2_as_flashable() {
        // An existing output path remains available to explicit inspection via
        // build::last_uf2, but cannot initialize this session's flash state.
        assert!(initial_flashable_uf2().is_none());
    }

    #[test]
    fn successful_build_promotes_its_uf2() {
        let path = std::path::PathBuf::from("session-build/sensor-watch.uf2");
        let result = super::build::BuildResult {
            success: true,
            message: String::new(),
            uf2_path: Some(path.clone()),
        };

        assert_eq!(flashable_uf2_after_build(&result), Some(path));
    }

    #[test]
    fn successful_build_without_verified_artifact_is_rejected() {
        let result = super::build::BuildResult {
            success: true,
            message: String::new(),
            uf2_path: Some(std::path::PathBuf::from("missing.uf2")),
        };

        let error = verified_artifact_after_build(&result).unwrap_err();
        assert!(error.contains("No such file") || error.contains("missing.uf2"));
    }

    #[test]
    fn successful_build_with_verified_artifact_is_accepted() {
        let (root, artifact) = shared_manifest_fixture("build-success", true);
        let result = super::build::BuildResult {
            success: true,
            message: String::new(),
            uf2_path: Some(artifact.clone()),
        };

        let inspection = verified_artifact_after_build(&result).unwrap();
        assert_eq!(inspection.path, artifact);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_build_does_not_promote_an_artifact() {
        let result = super::build::BuildResult {
            success: false,
            message: String::from("build failed"),
            uf2_path: Some(std::path::PathBuf::from("stale.uf2")),
        };

        assert_eq!(flashable_uf2_after_build(&result), None);
    }

    fn test_inspection(path: &str) -> super::build::ArtifactInspection {
        super::build::ArtifactInspection {
            path: std::path::PathBuf::from(path),
            generation: "test-generation".to_string(),
            family_id: "test-family".to_string(),
            uf2_bytes: "1".to_string(),
            uf2_blocks: "1".to_string(),
            payload_bytes: "1".to_string(),
            sha256: "uf2-sha".to_string(),
            payload_sha256: "payload-sha".to_string(),
            manifest_digest: "manifest-sha".to_string(),
            generated_input_digest: "inputs-sha".to_string(),
        }
    }

    #[test]
    fn successful_inspection_sets_distinct_verified_pending_state() {
        let mut status = String::new();
        let mut message = String::new();
        let mut approved = Some(ApprovedArtifact::from_inspection(&test_inspection(
            "old-approved.uf2",
        )));
        let mut pending = None;

        set_verified_artifact_state(
            &mut status,
            &mut message,
            &mut approved,
            &mut pending,
            test_inspection("candidate.uf2"),
            false,
        );

        assert!(approved.is_none());

        assert_eq!(status, ARTIFACT_VERIFIED_PENDING_STATUS);
        assert!(message.contains("approve it before flashing"));
        assert_eq!(
            pending.as_ref().unwrap().path,
            std::path::PathBuf::from("candidate.uf2")
        );
    }

    #[test]
    fn explicit_approval_sets_session_approved_state() {
        let mut status = ARTIFACT_VERIFIED_PENDING_STATUS.to_string();
        let mut message = String::new();
        let mut pending = Some(test_inspection("candidate.uf2"));
        let mut approved = Some(ApprovedArtifact::from_inspection(&test_inspection(
            "old-approved.uf2",
        )));

        approve_artifact_state(
            &mut status,
            &mut message,
            &mut pending,
            &mut approved,
            false,
        );

        assert_eq!(status, ARTIFACT_APPROVED_STATUS);
        assert_eq!(
            approved.as_ref().map(|artifact| &artifact.path),
            Some(&std::path::PathBuf::from("candidate.uf2"))
        );
        assert!(pending.is_none());
        assert!(message.contains("Approved for flashing"));
    }

    #[test]
    fn build_completion_cannot_overwrite_newer_approved_artifact_while_busy() {
        let newer = test_inspection("newer-approved.uf2");
        let mut status = ARTIFACT_APPROVED_STATUS.to_string();
        let mut message = String::new();
        let mut approved = Some(ApprovedArtifact::from_inspection(&newer));
        let mut pending = None;

        // This models an older worker result arriving after a newer candidate
        // was approved. The busy guard is the same policy used by the UI and
        // explicit inspection entry point while that worker is still active.
        set_verified_artifact_state(
            &mut status,
            &mut message,
            &mut approved,
            &mut pending,
            test_inspection("older-build.uf2"),
            true,
        );

        assert_eq!(status, ARTIFACT_BUSY_STATUS);
        assert_eq!(message, ARTIFACT_BUSY_STATUS);
        assert!(pending.is_none());
        assert_eq!(approved, Some(ApprovedArtifact::from_inspection(&newer)));
    }

    #[test]
    fn approved_metadata_accepts_unchanged_artifact() {
        let inspection = test_inspection("candidate.uf2");
        let approved = ApprovedArtifact::from_inspection(&inspection);

        assert!(approved.matches(&inspection));
    }

    #[test]
    fn approved_metadata_rejects_a_different_valid_replacement() {
        let approved_inspection = test_inspection("candidate.uf2");
        let approved = ApprovedArtifact::from_inspection(&approved_inspection);
        let mut replacement = test_inspection("candidate.uf2");
        replacement.sha256 = "different-valid-uf2-sha".to_string();
        replacement.payload_sha256 = "different-valid-payload-sha".to_string();
        replacement.manifest_digest = "different-valid-manifest-digest".to_string();
        replacement.generation = "replacement-generation".to_string();

        assert!(!approved.matches(&replacement));
    }

    #[test]
    fn failed_inspection_preserves_approved_artifact_and_reports_failure() {
        let mut status = ARTIFACT_APPROVED_STATUS.to_string();
        let mut message = String::new();
        let mut pending = Some(test_inspection("old-candidate.uf2"));
        let mut approved = Some(ApprovedArtifact::from_inspection(&test_inspection(
            "approved.uf2",
        )));

        set_failed_artifact_state(
            &mut status,
            &mut message,
            &mut approved,
            &mut pending,
            "invalid manifest".to_string(),
        );

        assert_eq!(status, ARTIFACT_VERIFICATION_FAILED_STATUS);
        assert!(message.contains("invalid manifest"));
        assert!(pending.is_none());
        assert_eq!(
            approved.as_ref().map(|artifact| &artifact.path),
            Some(&std::path::PathBuf::from("approved.uf2"))
        );
    }

    #[test]
    fn empty_preset_names_are_rejected_before_mutation() {
        assert_eq!(preset_name(""), None);
        assert_eq!(preset_name("   \t\n"), None);
        assert_eq!(preset_name("  travel  "), Some("travel"));
    }

    #[test]
    fn removing_a_custom_ntp_server_preserves_or_resets_selection() {
        assert_eq!(super::selection_after_custom_ntp_removal(2, 2), 0);
        assert_eq!(super::selection_after_custom_ntp_removal(4, 2), 3);
        assert_eq!(super::selection_after_custom_ntp_removal(1, 2), 1);
    }

    #[test]
    fn failed_ntp_refresh_cannot_leave_a_stale_reference() {
        let mut time = Some(1_700_000_000);
        let mut ping = 12.5;
        let mut offset = -0.25;

        super::invalidate_ntp_reference(&mut time, &mut ping, &mut offset);

        assert_eq!(time, None);
        assert_eq!(ping, 0.0);
        assert_eq!(offset, 0.0);
    }

    #[test]
    fn first_run_steps_describe_a_non_build_beginner_path() {
        let steps = FIRST_RUN_STEPS.join(" ");
        assert!(steps.contains("Normal mode"));
        assert!(steps.contains("Simulator"));
        assert!(steps.contains("Build & Flash"));
        assert!(!steps.contains("Build UF2"));
        assert!(!steps.contains("Copy to watch"));
    }

    #[test]
    fn build_contract_is_fail_closed_and_ui_does_not_promise_artifacts() {
        assert!(super::build::validate_configuration_inputs().is_err());
        assert!(
            super::build::CONFIGURATION_BUILD_BLOCKED.contains("no configured UF2 was generated")
        );
        assert!(super::build::missing_configuration_inputs().len() >= 5);
    }

    #[test]
    fn first_run_starts_with_the_beginner_tour() {
        let app = super::StudioApp::default();
        assert_eq!(
            super::help::route(super::HelpId::Startup, 0).panel,
            super::HelpId::Dashboard
        );
        assert!(app.first_run);
        assert!(app.block_editor.is_blocks_mode());
    }

    #[test]
    fn first_run_editor_route_has_an_anchor_for_each_step() {
        let mut registry = super::AnchorRegistry::default();
        registry.begin_frame(1);
        for anchor in [
            super::AnchorId::EditorName,
            super::AnchorId::BlocksGenerate,
            super::AnchorId::LoadIntoRust,
            super::AnchorId::EditorSave,
        ] {
            registry.register(
                super::HelpId::Editor,
                anchor.key(),
                super::AnchorRect {
                    min: (0.0, 0.0),
                    max: (10.0, 10.0),
                },
            );
        }
        for index in 0..super::help::tutorial(super::HelpId::Editor).steps.len() {
            let anchor = super::help::anchor_for_step(super::HelpId::Editor, index).unwrap();
            assert!(registry.get(super::HelpId::Editor, anchor.key()).is_some());
        }
    }

    #[test]
    fn contextual_help_is_ignored_while_welcome_is_active() {
        assert!(!contextual_help_allowed(true));
        assert!(contextual_help_allowed(false));
    }

    #[test]
    fn welcome_skip_claims_first_run_tours_without_opening_one() {
        let mut app = super::StudioApp::default();
        app.first_run = false;
        app.tour_claims.claim_all(super::help::FIRST_RUN_SEQUENCE);
        assert!(super::help::FIRST_RUN_SEQUENCE
            .into_iter()
            .all(|id| app.tour_claims.contains(id)));
        assert!(app.help_open.is_none());
    }

    #[test]
    fn pause_and_resume_keep_the_same_tour_step() {
        let mut app = super::StudioApp::default();
        app.open_help_for(super::Panel::Editor, false);
        app.help_step = 2;
        app.minimize_help();
        assert_eq!(app.help_open, Some(super::HelpId::Editor));
        assert_eq!(app.help_step, 2);
        assert!(app.help_minimized);
        app.help_minimized = false;
        assert_eq!(app.help_step, 2);
    }

    #[test]
    fn manual_help_reopens_after_an_auto_claim() {
        let mut app = super::StudioApp::default();
        app.tour_claims.claim(super::HelpId::Settings);
        app.open_help_for(super::Panel::Settings, false);
        assert_eq!(app.help_open, Some(super::HelpId::Settings));
        assert_eq!(app.help_step, 0);
    }

    #[test]
    fn compile_session_reset_preserves_user_and_recovery_state() {
        let mut app = super::StudioApp::default();
        app.language = super::Language::ChineseSimplified;
        app.editor_source = "unsaved source".to_string();
        app.output_dir = "user-output".to_string();
        app.restore_store.points.push(super::restore::RestorePoint {
            name: "keep me".to_string(),
            timestamp: 7,
            settings: super::settings::AppSettings::default(),
            board: app.board.label().to_string(),
            active_preset: 0,
        });
        let restore_metadata: Vec<(String, u64)> = app
            .restore_store
            .points
            .iter()
            .map(|point| (point.name.clone(), point.timestamp))
            .collect();
        let source = app.editor_source.clone();
        let output_dir = app.output_dir.clone();
        let language = app.language;

        app.reset_compile_session_state("new-fingerprint".to_string());

        assert_eq!(app.editor_source, source);
        assert_eq!(app.output_dir, output_dir);
        assert_eq!(app.language, language);
        let current_restore_metadata: Vec<(String, u64)> = app
            .restore_store
            .points
            .iter()
            .map(|point| (point.name.clone(), point.timestamp))
            .collect();
        assert_eq!(current_restore_metadata, restore_metadata);
    }

    #[test]
    fn compile_session_reset_invalidates_prior_approval_and_simulator_latches() {
        let mut app = super::StudioApp::default();
        app.approved_artifact = Some(ApprovedArtifact::from_inspection(&test_inspection(
            "approved.uf2",
        )));
        app.pending_artifact = Some(test_inspection("pending.uf2"));
        app.pending_artifact_fingerprint = Some("old-fingerprint".to_string());
        app.pending_build_fingerprint = Some("stale-build".to_string());
        app.btn_l_down = true;
        app.btn_c_hold = 2.0;
        app.btn_a_events = super::real_face::ButtonEventState::default();
        app.held_button = Some(super::ButtonId::A);
        app.watch.light = true;

        app.reset_compile_session_state("new-fingerprint".to_string());

        assert!(app.approved_artifact.is_none());
        assert!(app.pending_artifact.is_none());
        assert!(app.pending_artifact_fingerprint.is_none());
        assert_eq!(
            app.pending_build_fingerprint.as_deref(),
            Some("new-fingerprint")
        );
        assert!(!app.btn_l_down);
        assert_eq!(app.btn_c_hold, 0.0);
        assert_eq!(
            app.btn_a_events,
            super::real_face::ButtonEventState::default()
        );
        assert!(app.held_button.is_none());
        assert!(!app.watch.light);
        assert!(app.building);
    }

    #[test]
    fn compile_session_reset_can_be_disabled() {
        let mut app = super::StudioApp::default();
        app.reset_test_session_on_compile = false;
        app.approved_artifact = Some(ApprovedArtifact::from_inspection(&test_inspection(
            "approved.uf2",
        )));
        app.btn_l_down = true;
        app.held_button = Some(super::ButtonId::L);
        app.watch.light = true;

        app.begin_compile_session("new-fingerprint".to_string());

        assert!(app.approved_artifact.is_some());
        assert!(app.btn_l_down);
        assert_eq!(app.held_button, Some(super::ButtonId::L));
        assert!(app.watch.light);
        assert!(app.building);
        assert_eq!(
            app.pending_build_fingerprint.as_deref(),
            Some("new-fingerprint")
        );
    }

    #[test]
    fn automatic_persistence_is_gated_by_user_preference() {
        let mut app = super::StudioApp::default();
        app.persist_user_changes = false;
        app.status = "unchanged".to_string();

        app.save_settings_internal();

        assert_eq!(app.status, "unchanged");
    }

    #[test]
    fn startup_does_not_launch_master_clock() {
        let app = super::StudioApp::default();
        assert!(app.master_clock_process.is_none());
    }

    #[test]
    fn developer_and_panel_ux_toggles_apply_without_opening_hard_actions() {
        let mut app = super::StudioApp::default();
        let mut settings = super::settings::AppSettings::default();
        settings.developer_mode = true;
        settings.persist_user_changes = false;
        let mut ux = settings.panel_ux("simulator");
        ux.tutorial_input_barrier = false;
        settings.set_panel_ux("simulator", ux);

        app.apply_settings(settings);

        assert!(app.advanced_mode);
        assert!(!app.panel_ux(super::Panel::Simulator).tutorial_input_barrier);
        app.help_open = Some(super::HelpId::Simulator);
        assert!(!app.unsafe_action_allowed());
    }

    #[test]
    fn apply_settings_updates_both_studio_preferences() {
        let mut app = super::StudioApp::default();
        let mut settings = super::settings::AppSettings::default();
        settings.persist_user_changes = false;
        settings.reset_test_session_on_compile = false;

        app.apply_settings(settings);

        assert!(!app.persist_user_changes);
        assert!(!app.reset_test_session_on_compile);
    }

    #[test]
    fn reset_is_guarded_for_normal_profiles_and_does_not_destroy_state() {
        let mut app = super::StudioApp::default();
        app.pending_ntp.take();
        app.pending_update.take();
        app.ntp_busy = false;
        app.update_checking = false;
        app.presets.add_preset("preserve me");
        let before = app.presets.presets.len();
        app.reset_test_profile();
        assert_eq!(app.presets.presets.len(), before);
        assert!(app
            .status
            .contains("only available for isolated debug profiles"));
    }

    #[test]
    fn reset_worker_guard_preserves_state_for_all_explicit_worker_flags() {
        let mut app = super::StudioApp::default();
        app.language = super::Language::ChineseSimplified;
        app.watch.light = true;
        app.status = "before reset".into();
        app.log.log("preserve log");
        app.approved_artifact = Some(ApprovedArtifact::from_inspection(&test_inspection(
            "approved.uf2",
        )));
        app.building = true;
        app.ntp_busy = true;
        app.checksum_busy = true;
        app.update_checking = true;

        assert!(app.workers_active());
        let language = app.language;
        let light = app.watch.light;
        let log_len = app.log.entries().len();
        let approved_artifact = app.approved_artifact.clone();

        app.reset_test_profile();

        assert_eq!(app.language, language);
        assert_eq!(app.watch.light, light);
        assert_eq!(app.log.entries().len(), log_len);
        assert_eq!(app.approved_artifact, approved_artifact);
        assert_eq!(
            app.status,
            "Reset unavailable while background work is active"
        );
    }

    #[test]
    fn blocked_build_does_not_reset_compile_session_state() {
        let mut app = super::StudioApp::default();
        let approved = ApprovedArtifact::from_inspection(&test_inspection("approved.uf2"));
        app.approved_artifact = Some(approved.clone());
        app.pending_artifact = Some(test_inspection("pending.uf2"));
        app.pending_artifact_fingerprint = Some("old-fingerprint".to_string());
        app.pending_build_fingerprint = Some("stale-build".to_string());
        app.btn_l_down = true;
        app.held_button = Some(super::ButtonId::L);
        app.watch.light = true;
        let output_dir = app.output_dir.clone();

        app.start_build();

        assert_eq!(app.approved_artifact, Some(approved));
        assert!(app.pending_artifact.is_some());
        assert_eq!(
            app.pending_artifact_fingerprint.as_deref(),
            Some("old-fingerprint")
        );
        assert_eq!(
            app.pending_build_fingerprint.as_deref(),
            Some("stale-build")
        );
        assert!(app.btn_l_down);
        assert_eq!(app.held_button, Some(super::ButtonId::L));
        assert!(app.watch.light);
        assert_eq!(app.output_dir, output_dir);
        assert!(app.pending_build.is_none());
    }

    #[test]
    fn credits_search_matches_names_and_details_case_insensitively() {
        let entry = CreditEntry {
            name: "Example Contributor",
            details: "Optical flashing tools",
            url: None,
        };
        assert!(credit_matches(&entry, "contributor"));
        assert!(credit_matches(&entry, "OPTICAL"));
        assert!(!credit_matches(&entry, "unrelated"));
        assert!(credit_matches(&entry, ""));
    }

    #[test]
    fn credits_include_all_documented_groups() {
        let groups: Vec<&str> = CREDIT_GROUPS.iter().map(|group| group.name).collect();
        assert_eq!(
            groups,
            vec![
                "Upstream projects and maintainers",
                "Named community contributors",
                "Community tools and integrations",
            ]
        );
        assert!(CREDIT_GROUPS
            .iter()
            .flat_map(|group| group.entries)
            .any(|entry| entry.name.contains("Matheus Moreira")));
        assert!(CREDIT_GROUPS
            .iter()
            .flat_map(|group| group.entries)
            .any(|entry| entry.name.contains("UltraPatch")));
    }
}
