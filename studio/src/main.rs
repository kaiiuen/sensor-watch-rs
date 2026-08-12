//! Firmware Studio - a GUI companion app for the Sensor-Watch firmware.
//!
//! This is the end-goal product: an editor, debugger, and assembler that
//! produces the final `.uf2` firmware file.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod block_editor;
mod build;
mod components;
mod debug;
mod diagnostics;
mod drift;
mod editor;
mod error_catalog;
mod face_sim;
mod faces;
mod file_browser;
mod fonts;
mod fuzz;
mod i18n;
mod integrity;
mod modules;
mod ntp;
mod optical;
mod panic_map;
mod persist;
mod presets;
mod probe;
mod real_face;
mod restore;
mod settings;
mod sysstats;
mod theme;
mod transport;
mod watch_config;
mod watch_display;
mod watch_sim;
mod wiki;

use eframe::egui;
use i18n::{tr, Key, Language};
use presets::PresetManager;
use sha2::{Digest, Sha256};
use std::io::Read;
use theme::Theme;
use watch_sim::CasioF91W;

/// The main application state.
struct StudioApp {
    /// Whether the CJK font has been installed yet.
    fonts_installed: bool,
    /// The currently selected panel.
    current_panel: Panel,
    /// The last status message shown in the status bar.
    status: String,
    /// The discovered watch faces.
    face_list: Vec<faces::FaceInfo>,
    /// Whether a build is currently running.
    building: bool,
    /// Whether the window is closing; no new work is accepted after this point.
    shutting_down: bool,
    /// The handle to the background build thread.
    pending_build: Option<std::thread::JoinHandle<build::BuildResult>>,
    /// The last build result message.
    build_message: String,
    /// The path to the last-built .uf2.
    last_uf2: Option<std::path::PathBuf>,
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
    /// The editor's module name/target/description inputs.
    module_name: String,
    module_target: String,
    module_description: String,
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
    /// The time delta (seconds) since the last frame, for hold timing.
    sim_dt: f32,
    /// The button currently being held (via the on-watch SVG hotspot or a Hold
    /// button). Persists even if the pointer drifts off the widget while held;
    /// only clears when the mouse is fully released.
    held_button: Option<ButtonId>,
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
    /// The latest commit message from GitHub (for update notifications).
    latest_commit: Option<String>,
    /// The timestamp (unix seconds) when the update notification was received.
    update_time: Option<u64>,
    /// Whether the update check is in flight.
    update_checking: bool,
    /// The handle to the background update check.
    pending_update: Option<std::thread::JoinHandle<Result<String, String>>>,
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
    /// Whether Advanced-only tools are visible.
    advanced_mode: bool,
    /// Whether the advanced-mode warning is awaiting confirmation.
    advanced_mode_confirm: bool,
    /// Most recent physical probe report.
    probe_report: Option<probe::ProbeReport>,
}

/// The supported Sensor Watch board revisions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Board {
    Green,
    RedLite,
    Blue,
    Pro,
}

impl Board {
    /// All supported board revisions, in display order.
    const ALL: [Board; 4] = [Board::Green, Board::RedLite, Board::Blue, Board::Pro];

    fn label(self) -> &'static str {
        match self {
            Board::Green => "Green",
            Board::RedLite => "Red / Lite",
            Board::Blue => "Blue",
            Board::Pro => "Pro",
        }
    }
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

/// The navigation panels.
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
#[derive(Clone, Copy, PartialEq, Eq)]
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

/// A destructive action awaiting confirmation in a modal dialog.
#[derive(Clone, PartialEq, Eq, Debug)]
enum ConfirmKind {
    DeletePreset(String),
    DeleteFaceFromPreset(usize),
    DeleteFaceFile(String),
    RemoveModule(String),
    RunPhysicalProbe,
}

impl Panel {
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

impl Default for StudioApp {
    fn default() -> Self {
        // Shared atomic for the stats sampler's live rate.
        let stats_rate_shared = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1000));
        let mut app = StudioApp {
            fonts_installed: false,
            current_panel: Panel::Dashboard,
            status: String::new(),
            face_list: Vec::new(),
            building: false,
            shutting_down: false,
            pending_build: None,
            build_message: String::new(),
            last_uf2: None,
            // Default to English and Dark.
            language: Language::English,
            theme: Theme::Dark,
            applied_theme: None,
            applied_text_size: None,
            log: debug::DebugLog::new(),
            tick_verbosity: debug::TickVerbosity::Hide,
            tick_log: debug::DebugLog::new(),
            watch: CasioF91W::new(),
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
            module_name: String::new(),
            module_target: String::new(),
            module_description: String::new(),
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
            sim_face_idx: 0,
            sim_year: 2026,
            sim_month: 1,
            sim_day: 1,
            sim_hour: 12,
            sim_minute: 0,
            sim_weekday: 4,
            btn_l_down: false,
            btn_c_down: false,
            btn_a_down: false,
            btn_l_hold: 0.0,
            btn_c_hold: 0.0,
            btn_a_hold: 0.0,
            sim_dt: 0.0,
            held_button: None,
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
            update_time: None,
            update_checking: false,
            pending_update: None,
            beep_armed: false,
            beep_target: 0,
            pending_confirm: None,
            code_view: None,
            fuzz_test_result: None,
            first_run: true,
            saved_on_exit: false,
            wiki: wiki::Wiki::new(),
            line_limit: settings::default_line_limit(),
            restore_store: restore::RestoreStore::load(),
            restore_name: String::new(),
            advanced_mode: false,
            advanced_mode_confirm: false,
            probe_report: None,
        };
        app.log.log("Firmware Studio starting");
        app.last_uf2 = build::last_uf2(std::path::Path::new(&app.output_dir));
        app.face_list = faces::discover_faces();
        app.log
            .log(format!("Discovered {} watch faces", app.face_list.len()));
        // Load persisted settings (language, theme, presets, NTP servers, etc.).
        if let Some(saved) = persist::load() {
            app.apply_settings(saved);
            app.log.log("Loaded persisted settings");
        }
        // Auto-fetch the time from the default NTP server (Cloudflare) on launch.
        app.fetch_ntp();
        // Check for updates on launch.
        app.check_for_updates();
        app.status = tr(app.language, Key::Ready).to_string();
        app
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                || self.pending_update.is_some();
            if active_workers {
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
            }
        }

        // If a build finished, collect its result.
        if let Some(handle) = self.pending_build.take() {
            if handle.is_finished() {
                match handle.join() {
                    Ok(result) => {
                        self.building = false;
                        self.build_message = result.message.clone();
                        self.status = if result.success {
                            tr(self.language, Key::BuildComplete).to_string()
                        } else {
                            tr(self.language, Key::BuildFailed).to_string()
                        };
                        self.log.log(&result.message);
                        self.build_log.log(&result.message);
                        if result.success {
                            self.last_uf2 = result.uf2_path;
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
                            if let Some(p) = &self.last_uf2 {
                                self.log.log(format!("UF2 written to {}", p.display()));
                                self.build_log
                                    .log(format!("UF2 written to {}", p.display()));
                                self.push_terminal(format!(
                                    "Output write succeeded: UF2 written to {}",
                                    p.display()
                                ));
                            } else {
                                self.push_terminal("Output write finished: no UF2 produced");
                            }
                        } else {
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
                        self.status = format!("NTP error: {e}");
                        self.log_error(&format!("NTP error: {e}"));
                    }
                    Err(_) => {
                        self.ntp_busy = false;
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
                self.update_checking = false;
                match handle.join() {
                    Ok(Ok(msg)) => {
                        self.latest_commit = Some(msg.clone());
                        self.update_time = Some(
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0),
                        );
                        self.log.log(format!("Latest commit: {msg}"));
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
            } else {
                self.pending_update = Some(handle);
            }
        }

        // Top navigation bar.
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
                self.tab_bar(ui);
                // Update notification.
                if let Some(commit) = &self.latest_commit {
                    ui.separator();
                    let ts = self.update_time.map(|t| {
                        let secs = (t as i64).rem_euclid(86400);
                        let h = (secs / 3600) % 24;
                        let m = (secs / 60) % 60;
                        format!("{h:02}:{m:02}")
                    });
                    let label = match &ts {
                        Some(t) => format!("New update ({t}): {commit}"),
                        None => format!("New update: {commit}"),
                    };
                    ui.colored_label(egui::Color32::from_rgb(120, 180, 240), label)
                        .on_hover_text(
                            "A new commit was pushed to the repo. Click the title to\n\
                             open GitHub and download the latest release.",
                        );
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
                ui.monospace(format!(
                    "Estimate: ~{} KB flash",
                    self.estimate_flash_kb(selected)
                ));
                ui.separator();
                ui.monospace(format!(
                    "Estimate: ~{} KB RAM",
                    self.estimate_ram_kb(selected)
                ));
                ui.separator();
                ui.monospace(format!(
                    "Estimate: ~{} KB compiled",
                    self.estimate_compiled_kb(selected)
                ));
                ui.separator();
                // Window size.
                let size = ctx.screen_rect().size();
                ui.monospace(format!("Window: {:.0}x{:.0}", size.x, size.y));
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
        if self.advanced_mode {
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

        // The central panel.
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

        if self.advanced_mode_confirm {
            egui::Window::new("Enable Advanced mode?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Advanced controls can affect firmware configuration and hardware.");
                    ui.label("Simulated actions remain simulated; this mode does not make hardware claims.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Enable Advanced mode").clicked() {
                            self.advanced_mode = true;
                            self.advanced_mode_confirm = false;
                            self.save_settings_internal();
                        }
                        if ui.button("Keep Normal mode").clicked() {
                            self.advanced_mode_confirm = false;
                        }
                    });
                });
        }

        // One-time first-run welcome overlay.
        if self.first_run {
            egui::Window::new("Welcome")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.heading("Welcome to Firmware Studio 👋");
                    ui.add_space(6.0);
                    ui.label("Here's how to get started:");
                    ui.add_space(6.0);
                    for step in [
                        "1. Add watch faces in the Watch Faces tab",
                        "2. Try them in the Simulator tab",
                        "3. Click Build & Flash, then Build UF2",
                        "4. Plug the watch into USB in bootloader mode and click Copy to watch",
                    ] {
                        ui.label(step);
                    }
                    ui.add_space(10.0);
                    if ui.button("Got it - Start using").clicked() {
                        self.first_run = false;
                        self.save_settings_internal();
                    }
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
                    ui.label(&message);
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
                        self.faces_log
                            .log(format!("Removed {face} from active preset"));
                    }
                    ConfirmKind::DeleteFaceFile(name) => {
                        self.snapshot_before("Before deleting face");
                        match editor::delete_face(&name) {
                            Ok(_) => {
                                self.log.log(format!("Deleted face {name}"));
                                self.face_list = faces::discover_faces();
                            }
                            Err(e) => self.log.log(format!("Delete failed: {e}")),
                        }
                    }
                    ConfirmKind::RemoveModule(name) => {
                        self.snapshot_before("Before removing module");
                        self.modules.remove(&name);
                        self.log.log(format!("Removed module {name}"));
                        self.save_settings_internal();
                    }
                    ConfirmKind::RunPhysicalProbe => {
                        if self.advanced_mode {
                            self.probe_report = Some(probe::run(
                                self.last_uf2.as_deref(),
                                &self.serial_ports,
                                self.last_uart_error.as_deref(),
                                self.uart.as_mut(),
                            ));
                            self.status = "Physical probe complete".to_string();
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
                self.advanced_mode
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

        // A one-row preference still falls back to wrapping when required;
        // clipping is never preferable to honoring the selected tab.
        let row_count = preferred_rows.max(natural_rows).max(1);
        let mut rows: Vec<Vec<Panel>> = vec![Vec::new(); row_count];
        let mut row_widths = vec![0.0; row_count];
        let mut row = 0usize;
        for (index, panel) in visible.iter().enumerate() {
            if row_widths[row] > 0.0 && row_widths[row] + widths[index] > available {
                row += 1;
                if row == rows.len() {
                    rows.push(Vec::new());
                    row_widths.push(0.0);
                }
            }
            rows[row].push(*panel);
            row_widths[row] += widths[index];
        }
        for panels in rows.into_iter().filter(|panels| !panels.is_empty()) {
            ui.horizontal(|ui| self.draw_tab_buttons(ui, &panels));
        }
    }

    fn draw_tab_buttons(&mut self, ui: &mut egui::Ui, panels: &[Panel]) {
        for panel in panels {
            self.draw_tab_button(ui, *panel);
        }
    }

    fn draw_tab_button(&mut self, ui: &mut egui::Ui, panel: Panel) {
        if ui
            .selectable_label(self.current_panel == panel, panel.label(self.language))
            .clicked()
        {
            if self.current_panel != panel {
                self.log
                    .log(format!("Switched to panel {}", panel.label(self.language)));
            }
            self.current_panel = panel;
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
        ui.horizontal(|ui| {
            ui.label("Target board:");
            for b in [Board::Green, Board::RedLite, Board::Blue, Board::Pro] {
                if ui.selectable_label(self.board == b, b.label()).clicked() {
                    self.board = b;
                    self.log.log(format!("Target board set to {}", b.label()));
                }
            }
        })
        .response
        .on_hover_text(
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
        if let Some(uf2) = &self.last_uf2 {
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
                    if ui.button("Fetch time").clicked() {
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
                        if ui.button("Add").clicked() {
                            let name = self.ntp_edit_name.trim().to_string();
                            let host = self.ntp_edit_host.trim().to_string();
                            if !name.is_empty() && !host.is_empty() {
                                self.ntp_servers.push((name, host));
                                self.ntp_edit_name.clear();
                                self.ntp_edit_host.clear();
                                self.log.log("Added custom NTP server");
                                self.save_settings_internal();
                            }
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
                                to_delete = Some(i); // reuse: remove then re-add on Add
                            }
                            if ui.small_button("Del").clicked() {
                                to_delete = Some(i);
                            }
                        });
                    }
                    if let Some(i) = to_delete {
                        if i < self.ntp_servers.len() {
                            self.ntp_servers.remove(i);
                            if self.ntp_server >= ntp::SERVERS.len() + self.ntp_servers.len() {
                                self.ntp_server = 0;
                            }
                            self.log.log("Removed custom NTP server");
                            self.save_settings_internal();
                        }
                    }
                });

                // Show the fetched time.
                if let Some(ts) = self.ntp_time {
                    let secs = ts as i64;
                    let days = secs.div_euclid(86400);
                    let rem = secs.rem_euclid(86400);
                    let h = (rem / 3600) % 24;
                    let m = (rem / 60) % 60;
                    let s = rem % 60;
                    // Day of week: 1970-01-01 was Thursday.
                    let dow = ((days + 4).rem_euclid(7)) as usize;
                    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][dow];
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
        if self.shutting_down || self.building || self.pending_build.is_some() {
            self.push_terminal("Build already running");
            return;
        }
        self.snapshot_before("Before build");
        self.building = true;
        self.last_uf2 = None;
        self.build_message = tr(self.language, Key::Building).to_string();
        self.log.log("Starting firmware build");
        self.push_terminal("Output write: starting firmware build");
        let out = std::path::PathBuf::from(self.output_dir.clone());
        self.pending_build = Some(std::thread::spawn(move || build::build_firmware(&out)));
    }

    /// Fetches the current time from the selected NTP server on a background thread.
    fn fetch_ntp(&mut self) {
        if self.ntp_busy {
            return;
        }
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

    /// Fetches the latest commit message from GitHub for update notifications.
    fn check_for_updates(&mut self) {
        if self.update_checking {
            return;
        }
        self.update_checking = true;
        let handle = std::thread::spawn(fetch_latest_commit);
        self.pending_update = Some(handle);
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

        // Preset management: tab selectors, name field, and action buttons on
        // one row to save vertical space.
        ui.horizontal(|ui| {
            ui.label("Presets:");
            for (i, preset) in self.presets.presets.iter().enumerate() {
                if ui
                    .selectable_label(self.presets.active == i, &preset.name)
                    .clicked()
                {
                    self.presets.active = i;
                }
            }
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.new_preset_name).desired_width(150.0));
            if ui
                .button("+")
                .on_hover_text("Add a new preset with the typed name")
                .clicked()
            {
                self.presets.add_preset(&self.new_preset_name);
                self.faces_log
                    .log(format!("Added preset {}", self.new_preset_name));
                self.new_preset_name.clear();
            }
            if ui
                .button("Rename")
                .on_hover_text("Rename the active preset to the typed name")
                .clicked()
            {
                let name = self.new_preset_name.clone();
                if !name.is_empty() {
                    self.presets.rename_active(&name);
                    self.new_preset_name.clear();
                }
            }
            if ui
                .button("Delete")
                .on_hover_text("Delete the active preset and its face list")
                .clicked()
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
                            ui.text_edit_singleline(&mut self.catalog_search);
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
                        for face in &self.face_list {
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
                                        for (i, face) in self.face_list.iter().enumerate() {
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
                                                if ui.button("Add to preset").clicked() {
                                                    self.presets.add_face(&face_name);
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
                                                        if *f == face_name {
                                                            idx = Some(j);
                                                            break;
                                                        }
                                                    }
                                                    if idx.is_none() {
                                                        self.presets.add_face(&face_name);
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
                            self.drag_catalog_face = None;
                            self.log.log(format!("Added {name} to preset (drag)"));
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.heading("Active Preset");
                        ui.separator();
                        // Add selected catalog face to the preset.
                        if let Some(i) = self.selected_face {
                            let face = self.face_list[i].name.clone();
                            if ui.button(format!("Add {face}")).clicked() {
                                self.presets.add_face(&face);
                                self.log.log(format!("Added {face} to preset"));
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
                                                ui.close_menu();
                                            }
                                            if ui.button("Move down").clicked() {
                                                self.presets.move_face_down(i);
                                                ui.close_menu();
                                            }
                                            if ui.button("Remove").clicked() {
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
                                        }
                                        if ui
                                            .small_button("Dn")
                                            .on_hover_text("Move this face down in the preset")
                                            .clicked()
                                        {
                                            self.presets.move_face_down(i);
                                        }
                                        if ui
                                            .small_button("Del")
                                            .on_hover_text(
                                                "Remove this face from the active preset",
                                            )
                                            .clicked()
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
                                        self.drag_preset_from = None;
                                    }
                                });
                        });
                });
            });

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

    /// The editor panel: create, edit, or delete watch faces.
    fn editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Editor");
            ui.separator();
            if ui
                .selectable_label(self.block_editor.is_blocks_mode(), "Blocks")
                .clicked()
            {
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
            self.block_editor.show_blocks(ui, &mut self.editor_source);
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
                     6. Click \"Save face\" to write it to src/movement/.\n\
                     7. Add it to the active preset in Watch Faces, then Build & Flash.\n\
                     The simulator reflects faces that are in the active preset.",
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
            ui.text_edit_singleline(&mut self.editor_name);
            if ui
                .button("Generate from template")
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
        ui.horizontal(|ui| {
            ui.label("Description (shown in catalog):");
            ui.text_edit_singleline(&mut self.editor_description);
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .button("Save face")
                .on_hover_text("Save the editor source to the firmware project")
                .clicked()
            {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() && !self.editor_source.is_empty() {
                    match editor::write_face(&name, &self.editor_source) {
                        Ok(_) => {
                            // Best-effort visibility: add a `pub mod <name>;`
                            // declaration so the face shows up in Watch Faces. A
                            // registration failure is non-fatal - the file is
                            // saved, just not wired up yet.
                            match editor::register_face(&name) {
                                Ok(_) => {
                                    self.log.log(format!("Face saved and registered"));
                                }
                                Err(e) => {
                                    let path = editor::face_path(&name)
                                        .display()
                                        .to_string();
                                    self.log.log(format!(
                                        "Face saved to {path} but not yet registered \
                                         (manual step needed): {e}"
                                    ));
                                }
                            }
                            self.face_list = faces::discover_faces();
                        }
                        Err(e) => self.log.log(format!("Save failed: {e}")),
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
                        Ok(src) => self.editor_source = src,
                        Err(e) => self.log.log(format!("Load failed: {e}")),
                    }
                }
            }
            if ui
                .button("Delete face")
                .on_hover_text("Delete the face file from the firmware project")
                .clicked()
            {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() {
                    self.pending_confirm = Some((
                        format!("Delete face '{name}'? This deletes the file from the firmware project."),
                        ConfirmKind::DeleteFaceFile(name),
                    ));
                }
            }
        });

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

    /// The combined build & flash panel.
    fn build_flash(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading("Build & Flash");
                ui.label(
                    "Build the firmware into a .uf2, then flash it to the watch.\n\
                     Build compiles the firmware; Flash copies the .uf2 to the\n\
                     watch's USB drive (bootloader mode).",
                );
                ui.add_space(8.0);

                // Board selection (which revision the .uf2 targets).
                ui.horizontal(|ui| {
                    ui.label("Target board:");
                    for b in Board::ALL {
                        if ui.selectable_label(self.board == b, b.label()).clicked() {
                            self.board = b;
                            self.log.log(format!("Target board set to {}", b.label()));
                            self.save_settings_internal();
                        }
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
                components::show_configurator(
                    ui,
                    &mut self.component_profiles,
                    &mut self.component_profile,
                    &mut self.component_draft,
                );
                ui.add_space(8.0);

                // Estimated times.
                let selected = self.presets.active_faces().len();
                let est_compile = 30 + (selected as u32) * 2;
                ui.monospace(format!(
                    "Estimated compile: ~{est_compile} s   Estimated flash: ~2 s"
                ));
                ui.add_space(8.0);

                // Build.
                ui.strong("Build");
                if self.building {
                    ui.spinner();
                    ui.label(tr(self.language, Key::Building));
                } else if !self.shutting_down
                    && self.pending_build.is_none()
                    && ui
                        .button(tr(self.language, Key::BuildUf2))
                        .on_hover_text("Compile the firmware into a .uf2 file for the watch")
                        .clicked()
                {
                    self.start_build();
                }
                if !self.build_message.is_empty() {
                    ui.label(&self.build_message);
                }
                if let Some(uf2) = &self.last_uf2 {
                    ui.label(
                        tr(self.language, Key::Output)
                            .replace("{path}", &uf2.display().to_string()),
                    );
                }

                ui.add_space(12.0);
                ui.separator();
                // Flash.
                ui.strong("Flash");
                match self.detect_watch() {
                    Some(root) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 120),
                            format!("Watch detected at {root}"),
                        );
                    }
                    None => {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 160, 80),
                            "No watch detected. Put it in bootloader mode (USB connected).",
                        );
                    }
                }
                if self.building || self.pending_build.is_some() {
                    ui.weak("Build in progress; flashing is disabled until it finishes.");
                } else if let Some(uf2) = &self.last_uf2 {
                    let uf2 = uf2.clone();
                    if !self.shutting_down
                        && ui
                            .button(tr(self.language, Key::CopyToWatch))
                            .on_hover_text(
                                "Write the firmware to the watch's USB drive (bootloader mode)",
                            )
                            .clicked()
                    {
                        self.snapshot_before("Before flash");
                        self.copy_to_watch(&uf2);
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
                    if ui.button("Fetch NTP time").clicked() {
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
                    if ui.button("Record sample").clicked() {
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
                            if ui.button("Copy correction command").clicked() {
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
                if ui
                    .button("Register module")
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
                for name in &names {
                    let Some(m) = self.modules.modules.iter().find(|m| &m.name == name) else {
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
                            .on_hover_text("Unregister this module")
                            .clicked()
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
                if self.last_uf2.is_some() {
                    "available"
                } else {
                    "not built"
                }
            ),
        );
        self.diagnostics.log(format!(
            "board/UF2 -> {} / {}",
            self.board.label(),
            if self.last_uf2.is_some() {
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
        self.status = "Simulated diagnostics complete; no UART hardware queried".to_string();
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
        if let Some(uart) = self.uart.take() {
            self.shell_log
                .log(format!("UART disconnected: {}", uart.port_name()));
        }
        self.transport_mode = transport::TransportMode::Simulated;
        self.status = "Using Simulated shell mode".to_string();
    }

    fn send_shell_command(&mut self, cmd: &str) {
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
                    self.shell_log.log(format!("UART error: {error}"));
                    self.status = error.to_string();
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
        ui.horizontal(|ui| {
            if ui.button("Refresh COM ports").clicked() {
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
            let enabled = self.advanced_mode;
            if ui.add_enabled(enabled, egui::Button::new("Run physical probe")).clicked() {
                self.pending_confirm = Some((
                    "Run the physical probe? It will inspect removable drives and send only the read-only commands help, time, events, panic, and optical to the already connected selected UART port.".into(),
                    ConfirmKind::RunPhysicalProbe,
                ));
            }
            if ui.button("Copy report").clicked() {
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
            if ui
                .selectable_label(
                    self.transport_mode == transport::TransportMode::Simulated,
                    "Simulated",
                )
                .clicked()
            {
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
            if ui.button("Refresh").clicked() {
                self.refresh_serial_ports();
            }
            if self.uart.is_some() {
                if ui.button("Disconnect").clicked() {
                    self.disconnect_uart();
                }
            } else if ui.button("Connect").clicked() {
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
                    let submitted =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Send").clicked() || submitted {
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
        let reply = match cmd
            .trim()
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
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
                let payload = cmd.trim().split_whitespace().nth(1).unwrap_or("");
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
            "drift" => {
                let ppm: i16 = cmd
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0);
                let sign = if ppm < 0 { "+" } else { "-" };
                self.shell_hw_log.log(format!(
                    "RTC_FREQCORR <- sign={} value={} step=0.95ppm",
                    sign,
                    ppm.unsigned_abs()
                ));
                "OK".to_string()
            }
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
            if ui.button(tr(self.language, Key::Clear)).clicked() {
                self.log.clear();
            }
            if ui.button("Copy all").clicked() {
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
            if ui.button("Generate bug report").clicked() {
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
            resolve_fingerprint = ui.button("Resolve").clicked()
                || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
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
                    self.panic_fingerprint_input.trim(), root.display()
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
        if let Some(message) = self.file_browser.ui(ui) {
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
                egui::CollapsingHeader::new("What is a watch face?")
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
                        ui.label("Follow these steps to make and install your first face:");
                        ui.add_space(4.0);
                        for (n, step) in [
                            "Open the Editor tab.",
                            "Pick the Counter template.",
                            "Type a name in snake_case, like my_counter.",
                            "Click Generate from template to fill in the code.",
                            "Click Save face. This writes the file and registers it.",
                            "Open the Watch Faces tab and add your face to the active preset.",
                            "Open Build & Flash, click Build .uf2, then Copy to watch.",
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
            if ui.button("Back").clicked() {
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
                        ui.add(
                            egui::TextEdit::singleline(&mut self.wiki.search)
                                .hint_text("Search pages...")
                                .desired_width(f32::INFINITY),
                        );
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

        // The simulator body can be taller than the window (especially with the
        // date controller, debug log, and a large watch rendering all expanded),
        // so wrap it in a scroll area that shows a scrollbar on overflow. The
        // watch buttons inside draw_watch use pointer mapping against the
        // allocated rect, which still works inside the scroll area.
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
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
                    ui.label(if self.last_render_used_real {
                        "Last render: real face (firmware seam)"
                    } else {
                        "Last render: face_sim fallback"
                    });
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
                ui.collapsing("Date / time controller", |ui| {
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
                            let names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
                            egui::ComboBox::from_id_source("sim_weekday")
                                .selected_text(names[self.sim_weekday])
                                .show_ui(ui, |ui| {
                                    for (i, n) in names.iter().enumerate() {
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
                        if ui.button("Apply date/time").clicked() {
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
                            self.watch.weekday_override = Some(self.sim_weekday as u32);
                            self.sim_log
                                .log(format!("Weekday set to {}", self.sim_weekday));
                            self.log.log(format!(
                                "Weekday set to {}",
                                ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][self.sim_weekday]
                            ));
                        }
                        if ui.button("Reset to now").clicked() {
                            self.watch.time_offset = 0;
                            self.watch.weekday_override = None;
                            self.sim_log.log("Reset to now".to_string());
                            self.log.log("Sim date reset to now");
                        }
                    });
                    ui.separator();
                });

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
            self.face_engine = face_sim::FaceEngine::new(&face_name);
            // (Re)build the real-face engine for the new face. Faces that have
            // not yet been migrated through the firmware seam stay `None` and
            // the simulator falls back to `face_engine` below.
            if self
                .real_face
                .as_ref()
                .map(|r| r.face_name())
                .unwrap_or(&face_name)
                != face_name
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
        // `t_year` is signed from the watch; clamp to a sane wall-clock year
        // for the firmware's 2020-2083 range before handing it to the seam.
        let real_result = self.real_face.as_mut().map(|real| {
            let valid_time = real.set_time(
                t_year.clamp(2020, 2083) as u32,
                t_month,
                t_day,
                t_hour,
                t_minute,
                t_second,
            );
            let face_name = real.face_name().to_string();
            let face_changed = active_real_face_name.as_deref() != Some(face_name.as_str());
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
            let slot = |i: usize| -> char { text.chars().nth(i).unwrap_or(' ') };
            svg_display.mode_2 = ' ';
            svg_display.mode_1 = ' ';
            svg_display.day_2 = ' ';
            svg_display.day_1 = ' ';
            svg_display.hour_2 = slot(0);
            svg_display.hour_1 = slot(1);
            svg_display.minute_2 = slot(2);
            svg_display.minute_1 = slot(3);
            svg_display.second_2 = slot(4);
            svg_display.second_1 = slot(5);
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
            return;
        };
        // This is the last path that actually produced a rendered frame, rather
        // than merely the path that was selected before rasterization.
        self.last_render_used_real = used_real;

        // Allocate the image rect so we can map clicks to SVG button hotspots.
        let (rect, _response) = ui.allocate_exact_size(egui::Vec2::new(w, h), egui::Sense::click());
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
        let pointer_down = ui.input(|i| i.pointer.primary_down());
        let pointer_pos = ui.input(|i| i.pointer.interact_pos());

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
                // mouse is fully released.
                if pointer_down {
                    if self.held_button.is_none() {
                        self.held_button = under;
                    }
                } else {
                    self.held_button = None;
                }
                let l_down = self.held_button == Some(ButtonId::L);
                let c_down = self.held_button == Some(ButtonId::C);
                let a_down = self.held_button == Some(ButtonId::A);

                let l_act = handle_sim_button(
                    l_down,
                    &mut self.btn_l_down,
                    &mut self.btn_l_hold,
                    self.sim_dt,
                );
                let c_act = handle_sim_button(
                    c_down,
                    &mut self.btn_c_down,
                    &mut self.btn_c_hold,
                    self.sim_dt,
                );
                let a_act = handle_sim_button(
                    a_down,
                    &mut self.btn_a_down,
                    &mut self.btn_a_hold,
                    self.sim_dt,
                );
                // L button: toggle the backlight while held, and act as the
                // face's Light button on press.
                match l_act {
                    SimAction::Press => {
                        self.watch.light = true;
                        self.face_engine.press(face_sim::FaceButton::Light);
                        if let Some(real) = self.real_face.as_mut() {
                            real.press(true, false);
                        }
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
                    if let Some(real) = self.real_face.as_mut() {
                        real.press(false, true);
                    }
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
                    real.press(true, false);
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
                    real.press(false, true);
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
        self.sim_weekday = (weekday as usize).min(6);
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
                if self.building || self.pending_build.is_some() {
                    self.push_terminal("Build already running");
                } else {
                    self.start_build();
                }
            }
            "flash" => {
                if self.building || self.pending_build.is_some() {
                    self.push_terminal("Build in progress; flash unavailable");
                } else if let Some(uf2) = self.last_uf2.clone() {
                    self.copy_to_watch(&uf2);
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

    /// The settings panel: configure the app and the watch.
    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr(self.language, Key::Settings));
        ui.separator();

        // The settings panel is long, so wrap everything in a scroll area that
        // shows scrollbars automatically when content overflows.
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.settings_body(ui);
            });
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
                        if ui
                            .selectable_label(self.theme == theme, theme.name())
                            .clicked()
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
                        if ui
                            .selectable_label(self.tab_layout == mode, tr(self.language, key))
                            .clicked()
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

                // Text size.
                ui.label("Text size");
                ui.horizontal(|ui| {
                    for (v, label) in [(0u8, "Small"), (1, "Normal"), (2, "Big")] {
                        if ui.selectable_label(self.text_size == v, label).clicked() {
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

                // Firmware project path.
                ui.label(tr(self.language, Key::FirmwareProject));
                ui.monospace(build::firmware_dir().display().to_string());
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.weak("All-in-one CLI: run Firmware Studio with --help");

        if !self.advanced_mode {
            ui.add_space(16.0);
            ui.separator();
            ui.heading("Developer / Advanced mode");
            ui.label("Normal mode keeps protocol, register, diagnostics, and developer tools out of the way.");
            if ui.button("Enable Advanced mode...").clicked() {
                self.advanced_mode_confirm = true;
            }
            return;
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
                match self.last_uf2.as_ref().and_then(|path| std::fs::metadata(path).ok()) {
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
            if ui.button("Import settings JSON").clicked() {
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
        for (index, point) in self.restore_store.points.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{} - {}", point.name, point.timestamp));
                if ui.small_button("Restore").clicked() {
                    restore_index = Some(index);
                }
                if ui.small_button("Delete").clicked() {
                    delete_index = Some(index);
                }
                if ui.small_button("Rename").clicked() {
                    rename_index = Some(index);
                }
                if ui.small_button("Export").clicked() {
                    match self
                        .restore_store
                        .export_json(index)
                        .and_then(|j| ui_copy_to_clipboard(&j))
                    {
                        Ok(_) => self.status = "Restore point copied to clipboard".to_string(),
                        Err(e) => self.status = format!("Restore point export failed: {e}"),
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.button("Import").clicked() {
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

        // Release checksum verification.
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Verify against release").clicked() {
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
                            "Checksum matches the official release.",
                        );
                    } else {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 80, 80),
                            "Checksum MISMATCH - this executable differs from the official release.",
                        );
                    }
                }
                None => {
                    ui.weak("Could not read the local executable.");
                }
            }
        } else if !self.checksum_busy {
            ui.weak(
                "No release checksum fetched yet. Press 'Verify against release' (requires internet).\n\
                 If offline, the checksum cannot be validated.",
            );
        }

        ui.add_space(16.0);
        ui.separator();
        ui.heading("Credits");
        ui.label("This project builds on the work of several open-source projects:");
        ui.add_space(8.0);
        egui::Grid::new("credits_grid")
            .striped(true)
            .spacing([16.0, 6.0])
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Author");
                ui.hyperlink_to("kaiiuen", "https://github.com/kaiiuen");
                ui.end_row();
                ui.label("Sensor Watch");
                ui.hyperlink_to(
                    "Original C firmware + hardware by Joey Castillo",
                    "https://github.com/joeycastillo/Sensor-Watch",
                );
                ui.end_row();
                ui.label("Movement");
                ui.hyperlink_to(
                    "Original watch-face framework (part of Sensor Watch)",
                    "https://github.com/joeycastillo/Sensor-Watch/tree/main/movement",
                );
                ui.end_row();
                ui.label("Second Movement");
                ui.hyperlink_to(
                    "Rewritten C framework with persistent settings and wear-leveling",
                    "https://github.com/joeycastillo/Sensor-Watch/tree/main/movement2",
                );
                ui.end_row();
                ui.label("Casio F-91W simulator");
                ui.hyperlink_to(
                    "Online F-91W replica by Alexis Philip, used for the SVG",
                    "https://github.com/alexisphilip/Casio-F-91W",
                );
                ui.end_row();
                ui.label("egui / eframe");
                ui.hyperlink_to(
                    "Rust GUI framework used for this app",
                    "https://github.com/emilk/egui",
                );
                ui.end_row();
                ui.label("resvg / usvg");
                ui.hyperlink_to(
                    "SVG rendering libraries used to draw the watch face",
                    "https://github.com/RazrFalcon/resvg",
                );
                ui.end_row();
                ui.label("sysinfo");
                ui.hyperlink_to(
                    "System resource usage library",
                    "https://github.com/GuillaumeGomez/sysinfo",
                );
                ui.end_row();
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
            self.drift_session.ppm,
            &self.rtc_calibration,
            self.line_limit,
            self.tick_verbosity.setting_name().to_string(),
            &self.component_profiles,
            self.component_profile,
            self.tab_layout,
            self.tab_overflow,
        )
        .with_board(self.board.label())
        .with_advanced_mode(self.advanced_mode);
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
            self.drift_session.ppm,
            &self.rtc_calibration,
            self.line_limit,
            self.tick_verbosity.setting_name().to_string(),
            &self.component_profiles,
            self.component_profile,
            self.tab_layout,
            self.tab_overflow,
        )
        .with_board(self.board.label())
        .with_advanced_mode(self.advanced_mode);
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
        self.status = format!("Restored {}", point.name);
        self.log
            .log(format!("Restored restore point {}", point.name));
    }

    /// Persists the current settings to the file next to the executable.
    fn save_settings_internal(&mut self) {
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
            self.drift_session.ppm,
            &self.rtc_calibration,
            self.line_limit,
            self.tick_verbosity.setting_name().to_string(),
            &self.component_profiles,
            self.component_profile,
            self.tab_layout,
            self.tab_overflow,
        )
        .with_board(self.board.label())
        .with_advanced_mode(self.advanced_mode);
        match persist::save(&settings) {
            Ok(_) => {}
            Err(e) => {
                self.status = format!("Failed to persist settings: {e}");
                self.log_error(&format!("Failed to persist settings: {e}"));
                self.push_terminal(format!("Settings persistence failed: {e}"));
            }
        }
    }

    /// Exports the settings JSON to the clipboard.
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
            self.drift_session.ppm,
            &self.rtc_calibration,
            self.line_limit,
            self.tick_verbosity.setting_name().to_string(),
            &self.component_profiles,
            self.component_profile,
            self.tab_layout,
            self.tab_overflow,
        )
        .with_board(self.board.label())
        .with_advanced_mode(self.advanced_mode);
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

    /// Applies imported settings to the app state.
    fn apply_settings(&mut self, s: settings::AppSettings) {
        if let Some(lang) = Language::ALL.iter().find(|l| l.name() == s.language) {
            self.language = *lang;
        }
        if let Some(theme) = Theme::ALL.iter().find(|t| t.name() == s.theme) {
            self.theme = *theme;
        }
        self.presets = s.presets;
        self.presets.clamp_active();
        self.ntp_server = s.ntp_server;
        self.ntp_servers = s.ntp_servers;
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
        self.advanced_mode = s.advanced_mode;
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

    /// Copies the built .uf2 to the watch's USB drive (if mounted).
    fn copy_to_watch(&mut self, uf2: &std::path::Path) {
        if self.building || self.pending_build.is_some() {
            self.status = "Build in progress; flashing is disabled until it finishes.".to_string();
            self.log_error("Flash blocked while a build is in progress");
            return;
        }
        self.log
            .log(format!("Attempting to flash {}", uf2.display()));
        self.flash_log
            .log(format!("Attempting to flash {}", uf2.display()));
        let data = match std::fs::read(uf2) {
            Ok(d) => d,
            Err(e) => {
                self.status = format!("Failed to read uf2: {e}");
                self.log.log(format!("Failed to read uf2: {e}"));
                self.flash_log.log(format!("Failed to read uf2: {e}"));
                return;
            }
        };
        if let Err(error) = sensor_watch_core::uf2::validate(&data) {
            self.status = format!("Refusing to copy invalid UF2: {error}");
            self.log_error(&format!("Refusing to copy invalid UF2: {error}"));
            self.flash_log
                .log(format!("UF2 validation failed before copy: {error}"));
            return;
        }
        let manifest = uf2.with_extension("uf2.json");
        if !manifest.exists() {
            self.status =
                "Refusing to copy: artifact manifest is missing (offline verification unavailable)"
                    .to_string();
            self.log_error("Refusing to copy: artifact manifest is missing");
            self.flash_log.log(
                "UF2 copy blocked: artifact manifest missing; checksum status offline/unverified",
            );
            return;
        }
        if let Err(error) = verify_artifact_manifest(uf2, &manifest) {
            self.status =
                format!("Refusing to copy: offline checksum verification failed ({error})");
            self.log_error(&format!("Offline checksum verification failed: {error}"));
            self.flash_log.log(format!(
                "UF2 copy blocked: offline checksum verification failed: {error}"
            ));
            return;
        }
        self.flash_log.log(format!(
            "UF2 validated offline; checksum verified from {}",
            manifest.display()
        ));
        for drive in 'A'..='Z' {
            let root = format!("{drive}:\\");
            if let Ok(entries) = std::fs::read_dir(&root) {
                // CURRENT.UF2 is not a sufficient identity check: any removable
                // drive can contain a file with that name. Require the UF2
                // bootloader's information file before allowing a write.
                let is_watch = entries.flatten().any(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("info_uf2.txt")
                        && read_info_uf2(e.path()).is_some()
                });
                if is_watch {
                    let dest = format!("{root}CURRENT.UF2");
                    // Write to a temp file first, then rename into place so a
                    // crash or full-drive mid-write doesn't corrupt the existing
                    // CURRENT.UF2. Then verify the size on disk matches.
                    let tmp = format!("{root}.current.uf2.tmp");
                    let destination = std::path::Path::new(&dest);
                    let temp = std::path::Path::new(&tmp);
                    let backup_name = format!("{root}.current.uf2.previous");
                    let backup = std::path::Path::new(&backup_name);
                    let mut write_ok = regular_or_absent(destination).is_ok()
                        && regular_or_absent(temp).is_ok()
                        && regular_or_absent(backup).is_ok()
                        && std::fs::remove_file(temp)
                            .or_else(|e| {
                                (e.kind() == std::io::ErrorKind::NotFound)
                                    .then_some(())
                                    .ok_or(e)
                            })
                            .is_ok()
                        && std::fs::write(temp, &data).is_ok()
                        && std::fs::read(temp)
                            .map(|written| {
                                written.len() == data.len()
                                    && written == data
                                    && sensor_watch_core::uf2::validate(&written).is_ok()
                            })
                            .unwrap_or(false);
                    if write_ok {
                        // std::fs::rename does not replace an existing file on
                        // Windows. Stage the old artifact only after the new
                        // temp file has been fully validated, and restore it if
                        // publication or the post-write verification fails.
                        let had_old = destination.is_file();
                        let staged_old = if had_old {
                            (!backup.exists() || std::fs::remove_file(backup).is_ok())
                                && std::fs::rename(destination, backup).is_ok()
                        } else {
                            true
                        };
                        write_ok = staged_old && std::fs::rename(temp, destination).is_ok();
                        if !write_ok && had_old {
                            let _ = std::fs::rename(backup, destination);
                        }
                        if write_ok {
                            write_ok = std::fs::read(destination)
                                .map(|published| {
                                    published == data
                                        && sensor_watch_core::uf2::validate(&published).is_ok()
                                })
                                .unwrap_or(false);
                            if !write_ok && had_old {
                                let _ = std::fs::remove_file(destination);
                                let _ = std::fs::rename(backup, destination);
                            }
                        }
                        if had_old {
                            let _ = std::fs::remove_file(backup);
                        }
                    }
                    if write_ok {
                        self.status = format!("Flashed to {dest}");
                        self.log.log(format!("Flashed to {dest}"));
                        self.flash_log.log(format!("Flashed to {dest}"));
                        // Auto-fetch NTP time after flashing for sync.
                        self.fetch_ntp();
                        self.flash_log.log("Fetching NTP time for sync...");
                        return;
                    } else {
                        // Best-effort cleanup of any leftover temp file.
                        let _ = std::fs::remove_file(&tmp);
                        self.status = format!("Failed to write to {dest}");
                        self.log_error(&format!("Failed to write to {dest}"));
                        self.flash_log.log(format!("Failed to write to {dest}"));
                        return;
                    }
                }
            }
        }
        self.status = "Watch not found (is it in bootloader mode?)".to_string();
        self.log_error("Watch not found (is it in bootloader mode?)");
        self.flash_log
            .log("Watch not found (is it in bootloader mode?)");
    }

    /// Detects whether a Sensor Watch is mounted as a USB drive, and if so
    /// auto-selects the matching board revision from its INFO_UF2.TXT.
    fn detect_watch(&mut self) -> Option<String> {
        for drive in 'A'..='Z' {
            let root = format!("{drive}:\\");
            if let Ok(entries) = std::fs::read_dir(&root) {
                let mut is_watch = false;
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    if name == "info_uf2.txt" {
                        // Require a bounded, regular INFO_UF2.TXT read so a
                        // symlink or unexpectedly large file cannot influence
                        // detection or consume unbounded memory.
                        let Some(text) = read_info_uf2(e.path()) else {
                            continue;
                        };
                        is_watch = true;
                        // Try to auto-select the board from the info file.
                        {
                            let lower = text.to_lowercase();
                            let board = if lower.contains("pro") {
                                Some(Board::Pro)
                            } else if lower.contains("blue") {
                                Some(Board::Blue)
                            } else if lower.contains("red") || lower.contains("lite") {
                                Some(Board::RedLite)
                            } else if lower.contains("green") {
                                Some(Board::Green)
                            } else {
                                None
                            };
                            if let Some(b) = board {
                                if self.board != b {
                                    self.board = b;
                                    self.log.log(format!(
                                        "Auto-selected board {} from watch",
                                        b.label()
                                    ));
                                }
                            }
                        }
                    }
                }
                if is_watch {
                    return Some(root);
                }
            }
        }
        None
    }

    /// Estimate of the firmware flash size in KB for the selected faces.
    /// The watch OS/framework baseline is ~40 KB; each face adds ~2 KB.
    fn estimate_flash_kb(&self, selected: usize) -> u32 {
        40 + (selected as u32) * 2
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

    /// Estimate of the firmware RAM usage in KB for the selected faces.
    /// The OS baseline is ~4 KB; each face adds ~0.4 KB.
    fn estimate_ram_kb(&self, selected: usize) -> u32 {
        4 + (selected as u32) / 2
    }

    /// Estimate of the compiled .uf2 size in KB for the selected faces.
    fn estimate_compiled_kb(&self, selected: usize) -> u32 {
        // UF2 adds ~512-byte headers; estimate flash + 10% overhead.
        self.estimate_flash_kb(selected) + self.estimate_flash_kb(selected) / 10
    }
}

const MAX_INFO_UF2_BYTES: u64 = 16 * 1024;

fn read_info_uf2(path: std::path::PathBuf) -> Option<String> {
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_INFO_UF2_BYTES
    {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_INFO_UF2_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_INFO_UF2_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn regular_or_absent(path: &std::path::Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("refusing symlinked path: {}", path.display()))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(format!("path is not a regular file: {}", path.display()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect path: {error}")),
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
        "Usage: sensor-watch-studio <COMMAND> [ARGS]\n\nCommands:\n  build\n      Build firmware and write the UF2 artifact\n  uf2 <INPUT> <OUTPUT>\n      Convert a binary image to UF2\n  verify <PATH> [--manifest <PATH>] [--trusted-sha256 <SHA256>]\n      Verify a UF2 artifact and its optional manifest\n  backup <SRC> <DST>\n      Preserve a known-good UF2 and write its manifest\n  rollback <SRC> <DST> <TRUSTED_SHA256>\n      Verify and stage a trusted rollback UF2\n  report <PATH> <TRUSTED_SHA256>\n      Print a recovery report for a trusted UF2\n  flash [ELF]\n      Flash firmware with probe-rs\n  help\n      Show this help\n\nWith no command, Firmware Studio starts its normal GUI."
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
    let mut args = std::env::args();
    let _executable = args.next();
    if args.next().is_some() {
        ensure_cli_console();
        let exit_code = match run_cli(std::env::args().skip(1)) {
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

/// Fetches the latest commit message from the GitHub API for update notifications.
fn fetch_latest_commit() -> Result<String, String> {
    let url = "https://api.github.com/repos/kaiiuen/sensor-watch-rs/commits/master";
    let resp = ureq::get(url)
        .set("User-Agent", "Firmware-Studio")
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| e.to_string())?;
    let body = resp.into_string().map_err(|e| e.to_string())?;
    // Extract the commit message from the JSON (best-effort).
    if let Some(idx) = body.find("\"message\":\"") {
        let rest = &body[idx + "\"message\":\"".len()..];
        if let Some(end) = rest.find('"') {
            return Ok(rest[..end].to_string());
        }
    }
    Err("Could not parse commit".to_string())
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

fn verify_artifact_manifest(
    artifact: &std::path::Path,
    manifest_path: &std::path::Path,
) -> Result<(), String> {
    let bytes = std::fs::read(artifact).map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(manifest_path).map_err(|e| e.to_string())?;
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let expected = value
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "manifest has no SHA-256".to_string())?;
    let parsed = sensor_watch_core::uf2::validate(&bytes)
        .map_err(|error| format!("UF2 validation failed: {error}"))?;
    let expected_crc = value
        .get("crc32_ieee")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "manifest has no CRC-32".to_string())?;
    let actual_crc = format!("0x{:08X}", sensor_watch_core::uf2::crc32(&parsed.image));
    if expected_crc != actual_crc {
        return Err(format!(
            "payload CRC-32 mismatch (expected {expected_crc}, got {actual_crc})"
        ));
    }
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if expected != actual {
        return Err(format!(
            "artifact SHA-256 mismatch (expected {expected}, got {actual})"
        ));
    }
    let signature = value
        .get("signature")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "manifest has no signature".to_string())?
        .to_string();
    let object = value
        .as_object_mut()
        .ok_or_else(|| "manifest is not an object".to_string())?;
    object.remove("signature");
    let canonical = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    let digest = Sha256::digest(canonical)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if signature != format!("sha256:{digest}") {
        return Err("manifest signature mismatch".to_string());
    }
    Ok(())
}

fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.is_ascii() && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
