//! Firmware Studio — a GUI companion app for the Sensor-Watch firmware.
//!
//! This is the end-goal product: an editor, debugger, and assembler that
//! produces the final `.uf2` firmware file.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod build;
mod debug;
mod drift;
mod editor;
mod face_sim;
mod faces;
mod fuzz;
mod i18n;
mod integrity;
mod ntp;
mod persist;
mod presets;
mod settings;
mod sysstats;
mod theme;
mod watch_config;
mod watch_display;
mod watch_sim;

use eframe::egui;
use i18n::{tr, Key, Language};
use presets::PresetManager;
use theme::Theme;
use watch_sim::CasioF91W;

/// The main application state.
struct StudioApp {
    /// The currently selected panel.
    current_panel: Panel,
    /// The last status message shown in the status bar.
    status: String,
    /// The discovered watch faces.
    face_list: Vec<faces::FaceInfo>,
    /// Whether a build is currently running.
    building: bool,
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
    /// The debug log.
    log: debug::DebugLog,
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
    /// The name for a new preset.
    new_preset_name: String,
    /// The editor's current face name.
    editor_name: String,
    /// The editor's current source.
    editor_source: String,
    /// The selected editor template.
    editor_template: usize,
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
    /// The latest system resource snapshot for the footer.
    sys_stats: sysstats::SysStats,
    /// The receiver for background system resource samples.
    sys_rx: std::sync::mpsc::Receiver<sysstats::SysStats>,
    /// The catalog search query.
    catalog_search: String,
    /// The target board revision (green, red/lite, blue, pro).
    board: Board,
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
    /// Accumulator for advancing the face state once per second.
    face_tick_accum: f32,
    /// The drift calibration session.
    drift_session: drift::DriftSession,
    /// The number of fuzz iterations to run.
    fuzz_iterations: usize,
    /// The stats sampling rate in milliseconds.
    stats_rate_ms: u64,
    /// Shared atomic for the sampler thread to read the live rate.
    stats_rate_shared: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// The UI text size (0=small, 1=normal, 2=big).
    text_size: u8,
    /// The timestamp of the last successful build.
    last_build_time: Option<u64>,
    /// The number of builds performed this session.
    build_count: u32,
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
    fn label(self) -> &'static str {
        match self {
            Board::Green => "Green",
            Board::RedLite => "Red / Lite",
            Board::Blue => "Blue",
            Board::Pro => "Pro",
        }
    }
}

/// The navigation panels.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Dashboard,
    Faces,
    Editor,
    Simulator,
    Build,
    Flash,
    Debug,
    Settings,
}

/// The action a simulator button edge produced.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SimAction {
    None,
    Press,
    Release,
}

/// Which simulator button is being handled.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonId {
    L,
    C,
    A,
}

impl Panel {
    fn label(self, lang: Language) -> &'static str {
        match self {
            Panel::Dashboard => tr(lang, Key::Dashboard),
            Panel::Faces => tr(lang, Key::WatchFaces),
            Panel::Editor => "Editor",
            Panel::Simulator => "Simulator",
            Panel::Build => tr(lang, Key::Build),
            Panel::Flash => tr(lang, Key::Flash),
            Panel::Debug => tr(lang, Key::DebugOutput),
            Panel::Settings => tr(lang, Key::Settings),
        }
    }
}

impl Default for StudioApp {
    fn default() -> Self {
        // Shared atomic for the stats sampler's live rate.
        let stats_rate_shared = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1000));
        let mut app = StudioApp {
            current_panel: Panel::Dashboard,
            status: String::new(),
            face_list: Vec::new(),
            building: false,
            pending_build: None,
            build_message: String::new(),
            last_uf2: build::last_uf2(),
            // Default to English and Dark.
            language: Language::English,
            theme: Theme::Dark,
            log: debug::DebugLog::new(),
            watch: CasioF91W::new(),
            sim_last_tick: std::time::Instant::now(),
            watch_renderer: watch_display::WatchRenderer::new(),
            sim_scale: 0.5,
            presets: PresetManager::new(),
            selected_face: None,
            selected_preset_face: None,
            new_preset_name: String::new(),
            editor_name: String::new(),
            editor_source: String::new(),
            editor_template: 0,
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
            sys_stats: sysstats::SysStats::default(),
            stats_rate_ms: 1000,
            stats_rate_shared: stats_rate_shared.clone(),
            sys_rx: sysstats::spawn_sampler(stats_rate_shared),
            text_size: 1,
            last_build_time: None,
            build_count: 0,
            catalog_search: String::new(),
            board: Board::Green,
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
            face_tick_accum: 0.0,
            drift_session: drift::DriftSession::new(),
            fuzz_iterations: 5000,
        };
        app.log.log("Firmware Studio starting");
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
        app.status = tr(app.language, Key::Ready).to_string();
        app
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply the theme.
        self.theme.apply(ctx);

        // Apply the text size (small/normal/big).
        let scale = match self.text_size {
            0 => 0.85,
            2 => 1.2,
            _ => 1.0,
        };
        ctx.set_pixels_per_point(ctx.pixels_per_point() * 1.0);
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

        // Keep the UI animating even when the cursor leaves the window, so the
        // clock and sim keep running instead of freezing.
        ctx.request_repaint();

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
                        if result.success {
                            self.last_uf2 = result.uf2_path;
                            self.last_build_time = Some(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0),
                            );
                            self.build_count += 1;
                            if let Some(p) = &self.last_uf2 {
                                self.log.log(format!("UF2 written to {}", p.display()));
                            }
                        }
                    }
                    Err(_) => {
                        self.building = false;
                        self.build_message =
                            tr(self.language, Key::BuildThreadPanicked).to_string();
                        self.log.log("Build thread panicked");
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
                        self.log.log(format!("NTP error: {e}"));
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
                        self.release_sha256 = Some(sha);
                        self.status = "Release checksum fetched".to_string();
                    }
                    Ok(Err(e)) => {
                        self.status = format!("Checksum unavailable: {e}");
                        self.log.log(format!("Checksum unavailable: {e}"));
                    }
                    Err(_) => {
                        self.status = "Checksum thread panicked".to_string();
                    }
                }
            } else {
                self.pending_checksum = Some(handle);
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
                // The tab bar scrolls horizontally so it never clips when the
                // window is narrow.
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for panel in [
                            Panel::Dashboard,
                            Panel::Faces,
                            Panel::Editor,
                            Panel::Simulator,
                            Panel::Build,
                            Panel::Flash,
                            Panel::Debug,
                            Panel::Settings,
                        ] {
                            if ui
                                .selectable_label(
                                    self.current_panel == panel,
                                    panel.label(self.language),
                                )
                                .clicked()
                            {
                                self.current_panel = panel;
                            }
                        }
                    });
                });
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
                ui.monospace(format!("~{} KB flash", self.estimate_flash_kb(selected)));
                ui.separator();
                ui.monospace(format!("~{} KB RAM", self.estimate_ram_kb(selected)));
                ui.separator();
                ui.monospace(format!(
                    "~{} KB compiled",
                    self.estimate_compiled_kb(selected)
                ));
                ui.separator();
                // Window size.
                let size = ctx.screen_rect().size();
                ui.monospace(format!("Window: {:.0}x{:.0}", size.x, size.y));
                ui.separator();
                ui.label(&self.status);
            });
        });

        // The central panel.
        egui::CentralPanel::default().show(ctx, |ui| match self.current_panel {
            Panel::Dashboard => self.dashboard(ui),
            Panel::Faces => self.faces(ui, ctx),
            Panel::Editor => self.editor(ui),
            Panel::Simulator => self.simulator(ui, ctx),
            Panel::Build => self.build(ui),
            Panel::Flash => self.flash(ui),
            Panel::Debug => self.debug(ui),
            Panel::Settings => self.settings(ui),
        });
    }
}

impl StudioApp {
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
                let s = secs % 60;
                ui.monospace(format!(
                    "Last build: {h:02}:{m:02}:{s:02}  ({} builds this session)",
                    self.build_count
                ));
            }
        } else {
            ui.label(tr(self.language, Key::NoBuildYet));
        }

        ui.add_space(16.0);
        ui.separator();
        ui.heading("NTP Time");
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

        // Clock calibration: compute the next-minute-boundary timestamp from the
        // NTP time and generate a `settime` command for the serial shell.
        ui.add_space(16.0);
        ui.separator();
        ui.heading("Clock Calibration");
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
            let yy = (year % 100) as u32;
            let cmd = format!(
                "settime {:02}{:02}{:02}{:02}{:02}{:02}",
                yy, month, day, h, m, s
            );
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

        // Drift calibration: measure the watch's drift against NTP over time.
        ui.add_space(16.0);
        ui.separator();
        ui.heading("Drift Calibration");
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
                    let (_, _, _, h, m, s, _) = self.watch.get_time();
                    let watch_secs = (h as u64) * 3600 + (m as u64) * 60 + s as u64;
                    self.drift_session.record(watch_secs, ts);
                    let n = if self.drift_session.start.is_some() {
                        if self.drift_session.end.is_some() {
                            "end".to_string()
                        } else {
                            "start".to_string()
                        }
                    } else {
                        "start".to_string()
                    };
                    self.log.log(format!("Drift sample recorded ({n})"));
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

        // Fuzz testing: run randomized input through the face engine.
        ui.add_space(16.0);
        ui.separator();
        ui.heading("Fuzz Testing");
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
            ui.add(egui::DragValue::new(&mut self.fuzz_iterations).clamp_range(100..=100_000));
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
    }

    /// Fetches the current time from the selected NTP server on a background thread.
    fn fetch_ntp(&mut self) {
        if self.ntp_busy {
            return;
        }
        self.ntp_busy = true;
        let server = ntp::SERVERS[self.ntp_server].1.to_string();
        let handle = std::thread::spawn(move || ntp::query_ntp(&server));
        self.pending_ntp = Some(handle);
    }

    /// Fetches the expected SHA-256 from the GitHub release on a background thread.
    fn fetch_release_checksum(&mut self) {
        if self.checksum_busy {
            return;
        }
        self.checksum_busy = true;
        let handle = std::thread::spawn(|| fetch_release_sha256());
        self.pending_checksum = Some(handle);
    }

    /// The watch-faces panel: catalog (left) and active preset (right), both
    /// as spreadsheets, plus preset management sub-tabs.
    fn faces(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.heading(tr(self.language, Key::WatchFaces));
        ui.separator();

        // Preset management sub-tabs along the top.
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
            if ui.button("+").clicked() {
                self.presets.add_preset(&self.new_preset_name);
                self.new_preset_name.clear();
            }
            if ui.button("Rename").clicked() {
                let name = self.new_preset_name.clone();
                if !name.is_empty() {
                    self.presets.rename_active(&name);
                    self.new_preset_name.clear();
                }
            }
            if ui.button("Delete").clicked() {
                self.presets.delete_active();
            }
        });
        ui.horizontal(|ui| {
            ui.label("New preset name:");
            ui.text_edit_singleline(&mut self.new_preset_name);
        });
        ui.separator();

        // Left column: catalog (top) and active preset (bottom), stacked.
        egui::SidePanel::left("catalog")
            .resizable(true)
            .default_width(ui.available_width() * 0.45)
            .width_range(180.0..=f32::INFINITY)
            .show_inside(ui, |ui| {
                // Catalog on top.
                egui::TopBottomPanel::top("catalog_top")
                    .resizable(true)
                    .show_inside(ui, |ui| {
                        ui.heading("Catalog");
                        // Search box.
                        ui.horizontal(|ui| {
                            ui.label("Search:");
                            ui.text_edit_singleline(&mut self.catalog_search);
                            if !self.catalog_search.is_empty() {
                                if ui.small_button("x").clicked() {
                                    self.catalog_search.clear();
                                }
                            }
                        });
                        ui.separator();
                        let query = self.catalog_search.trim().to_lowercase();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                // Spreadsheet-style grid: # | Face | Add.
                                egui::Grid::new("catalog_grid")
                                    .striped(true)
                                    .spacing([12.0, 4.0])
                                    .show(ui, |ui| {
                                        ui.strong("#");
                                        ui.strong("Face");
                                        ui.strong("Add");
                                        ui.end_row();
                                        for (i, face) in self.face_list.iter().enumerate() {
                                            // Filter by search query.
                                            if !query.is_empty()
                                                && !face.name.to_lowercase().contains(&query)
                                                && !face.index.to_string().contains(&query)
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
                                            ui.label(&face.name);
                                            if ui.small_button("+").clicked() {
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
                                    for (i, face) in faces.iter().enumerate() {
                                        let selected = self.selected_preset_face == Some(i);
                                        if ui
                                            .selectable_label(selected, (i + 1).to_string())
                                            .clicked()
                                        {
                                            self.selected_preset_face = Some(i);
                                        }
                                        ui.label(face);
                                        if ui.small_button("Up").clicked() {
                                            self.presets.move_face_up(i);
                                        }
                                        if ui.small_button("Dn").clicked() {
                                            self.presets.move_face_down(i);
                                        }
                                        if ui.small_button("Del").clicked() {
                                            self.presets.remove_face(i);
                                        }
                                        ui.end_row();
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
                            ui.end_row();

                            // Imperial units.
                            ui.label("Imperial units");
                            ui.checkbox(&mut self.watch_config.use_imperial_units, "");
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
                                    .clicked()
                                {
                                    self.watch_config.button_volume = false;
                                }
                                if ui
                                    .selectable_label(self.watch_config.button_volume, "Loud")
                                    .clicked()
                                {
                                    self.watch_config.button_volume = true;
                                }
                            });
                            ui.end_row();

                            // Signal volume.
                            ui.label("Signal volume");
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(!self.watch_config.signal_volume, "Soft")
                                    .clicked()
                                {
                                    self.watch_config.signal_volume = false;
                                }
                                if ui
                                    .selectable_label(self.watch_config.signal_volume, "Loud")
                                    .clicked()
                                {
                                    self.watch_config.signal_volume = true;
                                }
                            });
                            ui.end_row();

                            // Alarm volume.
                            ui.label("Alarm volume");
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(!self.watch_config.alarm_volume, "Soft")
                                    .clicked()
                                {
                                    self.watch_config.alarm_volume = false;
                                }
                                if ui
                                    .selectable_label(self.watch_config.alarm_volume, "Loud")
                                    .clicked()
                                {
                                    self.watch_config.alarm_volume = true;
                                }
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

                            // LED gradient hex.
                            ui.label("Gradient color (hex)");
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut self.watch_config.led_gradient_hex);
                                if let Some(col) =
                                    parse_hex_color(&self.watch_config.led_gradient_hex)
                                {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(24.0, 16.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect, 2.0, col);
                                }
                            });
                            ui.end_row();

                            // Night light red.
                            ui.label("Night light");
                            ui.checkbox(
                                &mut self.watch_config.night_light_red,
                                "Use red at night instead of day color",
                            );
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
        ui.heading("Editor");
        ui.separator();
        ui.label("Create, edit, or delete watch faces.");
        ui.add_space(8.0);

        // Template selection.
        ui.label("Template:");
        for (i, t) in editor::TEMPLATES.iter().enumerate() {
            if ui
                .selectable_label(
                    self.editor_template == i,
                    format!("{} — {}", t.name, t.description),
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
            if ui.button("Generate from template").clicked() {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() {
                    let source =
                        editor::generate_face(&name, &editor::TEMPLATES[self.editor_template]);
                    self.editor_source = source;
                    self.log.log(format!("Generated {name} from template"));
                }
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save face").clicked() {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() && !self.editor_source.is_empty() {
                    match editor::write_face(&name, &self.editor_source) {
                        Ok(_) => {
                            self.log.log(format!("Saved face {name}"));
                            self.face_list = faces::discover_faces();
                        }
                        Err(e) => self.log.log(format!("Save failed: {e}")),
                    }
                }
            }
            if ui.button("Load face").clicked() {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() {
                    match editor::read_face(&name) {
                        Ok(src) => self.editor_source = src,
                        Err(e) => self.log.log(format!("Load failed: {e}")),
                    }
                }
            }
            if ui.button("Delete face").clicked() {
                let name = self.editor_name.trim().to_string();
                if !name.is_empty() {
                    match editor::delete_face(&name) {
                        Ok(_) => {
                            self.log.log(format!("Deleted face {name}"));
                            self.face_list = faces::discover_faces();
                        }
                        Err(e) => self.log.log(format!("Delete failed: {e}")),
                    }
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

    /// The build panel: assemble the firmware into a .uf2.
    fn build(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr(self.language, Key::Build));
        ui.separator();
        ui.label(tr(self.language, Key::AssembleFirmware));
        ui.add_space(8.0);

        if self.building {
            ui.spinner();
            ui.label(tr(self.language, Key::Building));
        } else {
            if ui.button(tr(self.language, Key::BuildUf2)).clicked() {
                self.building = true;
                self.build_message = tr(self.language, Key::Building).to_string();
                self.log.log("Starting firmware build");
                let handle = std::thread::spawn(build::build_firmware);
                self.pending_build = Some(handle);
            }
        }

        if !self.build_message.is_empty() {
            ui.add_space(8.0);
            ui.label(&self.build_message);
        }
        if let Some(uf2) = &self.last_uf2 {
            ui.add_space(8.0);
            ui.label(tr(self.language, Key::Output).replace("{path}", &uf2.display().to_string()));
        }
    }

    /// The flash panel: flash the firmware to the watch.
    fn flash(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr(self.language, Key::Flash));
        ui.separator();
        ui.label(tr(self.language, Key::FlashFirmware));
        ui.add_space(8.0);
        if let Some(uf2) = &self.last_uf2 {
            let uf2 = uf2.clone();
            ui.label(
                tr(self.language, Key::Firmware).replace("{path}", &uf2.display().to_string()),
            );
            if ui.button(tr(self.language, Key::CopyToWatch)).clicked() {
                self.copy_to_watch(&uf2);
            }
        } else {
            ui.label(tr(self.language, Key::NoBuildYet));
        }
    }

    /// The debug panel: show the background activity log.
    fn debug(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(tr(self.language, Key::DebugOutput));
            if ui.button(tr(self.language, Key::Clear)).clicked() {
                self.log.clear();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
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
                            let secs = entry.timestamp % 60;
                            let mins = (entry.timestamp / 60) % 60;
                            let hrs = (entry.timestamp / 3600) % 24;
                            ui.monospace(format!("{hrs:02}:{mins:02}:{secs:02}"));
                            ui.monospace(&entry.message);
                            ui.end_row();
                        }
                    });
            });
    }

    /// The simulator panel: render the watch and handle its buttons.
    fn simulator(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Simulator");
            ui.separator();
            // Adjustable size slider.
            ui.label("Size:");
            ui.add(
                egui::Slider::new(&mut self.sim_scale, 0.4..=2.0)
                    .step_by(0.1)
                    .suffix("x"),
            );
            if ui.button("Reset").clicked() {
                self.sim_scale = 0.5;
            }
            ui.separator();
            // Show which preset face is being simulated.
            let faces = self.presets.active_faces();
            let idx = self.sim_face_idx.min(faces.len().saturating_sub(1));
            ui.label(format!(
                "Face: {} / {}",
                if faces.is_empty() { 0 } else { idx + 1 },
                faces.len()
            ));
            if !faces.is_empty() {
                ui.monospace(&faces[idx]);
            }
            ui.separator();
            // Show which preset face is being simulated.
            let faces = self.presets.active_faces();
            let idx = self.sim_face_idx.min(faces.len().saturating_sub(1));
            ui.label(format!(
                "Face: {} / {}",
                if faces.is_empty() { 0 } else { idx + 1 },
                faces.len()
            ));
            if !faces.is_empty() {
                ui.monospace(&faces[idx]);
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
                    ui.add(egui::DragValue::new(&mut self.sim_year).clamp_range(1970..=2100));
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
                    self.log.log(format!(
                        "Sim date set to {}-{:02}-{:02} {:02}:{:02}",
                        self.sim_year, self.sim_month, self.sim_day, self.sim_hour, self.sim_minute
                    ));
                }
                // Changing the weekday only overrides the weekday; it does not
                // touch the date/time.
                if ui.button("Apply weekday").clicked() {
                    self.watch.weekday_override = Some(self.sim_weekday as u32);
                    self.log.log(format!(
                        "Weekday set to {}",
                        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][self.sim_weekday]
                    ));
                }
                if ui.button("Reset to now").clicked() {
                    self.watch.time_offset = 0;
                    self.watch.weekday_override = None;
                    self.log.log("Sim date reset to now");
                }
            });
            ui.separator();
        });

        self.draw_watch(ui, ctx, self.sim_scale);
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

        // Determine the current face and sync the engine's face name.
        let faces = self.presets.active_faces();
        let face_name = if faces.is_empty() {
            "SIMPLE_CLOCK".to_string()
        } else {
            faces[self.sim_face_idx.min(faces.len() - 1)].clone()
        };
        if self.face_engine.face_name != face_name {
            self.face_engine = face_sim::FaceEngine::new(&face_name);
        }
        // Advance the face state by one second per real second.
        self.face_tick_accum += self.sim_dt;
        if self.face_tick_accum >= 1.0 {
            self.face_tick_accum -= 1.0;
            self.face_engine.tick();
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
        let fd = self.face_engine.render(&sim_time);
        let mut svg_display = watch_display::face_display_to_svg(&fd);
        // Apply the watch's light and CASIO-override state, which the face
        // engine does not model.
        svg_display.light = self.watch.light;
        if let Some(text) = &self.watch.override_text {
            let chars: Vec<char> = text.chars().collect();
            let slot = |i: usize| -> char { chars.get(i).copied().unwrap_or(' ') };
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
                    }
                    SimAction::Release => self.watch.light = false,
                    SimAction::None => {}
                }
                // C button: cycle through the preset's watch faces on press.
                if c_act == SimAction::Press {
                    let faces = self.presets.active_faces();
                    if !faces.is_empty() {
                        self.sim_face_idx = (self.sim_face_idx + 1) % faces.len();
                        let name = faces[self.sim_face_idx].clone();
                        self.log.log(format!("Simulating face: {name}"));
                    }
                }
                // A button: toggle 12/24 on a clean press, and act as the face's
                // Alarm button on press.
                if a_act == SimAction::Press {
                    self.watch.toggle_time_mode();
                    self.face_engine.time_mode_24 =
                        self.watch.time_mode == watch_sim::TimeMode::H24;
                    self.face_engine.press(face_sim::FaceButton::Alarm);
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

        // Request a repaint so the clock ticks.
        ctx.request_repaint();
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

                // Firmware project path.
                ui.label(tr(self.language, Key::FirmwareProject));
                ui.monospace(build::firmware_dir().display().to_string());
                ui.end_row();
            });

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
                ui.label("CPU");
                ui.monospace(format!("{:.1}%", self.sys_stats.cpu_percent));
                ui.end_row();
                ui.label("CPU speed");
                ui.monospace(format!("{} MHz", self.sys_stats.cpu_freq_mhz));
                ui.end_row();
                ui.label("Threads");
                ui.monospace(format!("{}", self.sys_stats.threads));
                ui.end_row();
                ui.label("Memory");
                ui.monospace(fmt_bytes(self.sys_stats.mem_bytes));
                ui.end_row();
                ui.label("Virtual memory");
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
                self.import_settings();
            }
        });

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
                            "Checksum MISMATCH — this executable differs from the official release.",
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

    /// Saves the current settings to a JSON file in the app data directory.
    fn save_settings_to_file(&mut self) {
        let settings = settings::AppSettings::capture(
            self.language,
            self.theme,
            &self.presets,
            self.ntp_server,
            &self.ntp_servers,
            self.sim_scale,
            &self.watch_config,
            self.text_size,
        );
        match settings.to_json() {
            Ok(json) => {
                let path = std::path::Path::new("settings.json");
                match std::fs::write(path, json) {
                    Ok(_) => {
                        self.status = format!("Settings saved to {}", path.display());
                        self.log
                            .log(format!("Settings saved to {}", path.display()));
                    }
                    Err(e) => {
                        self.status = format!("Failed to save settings: {e}");
                        self.log.log(format!("Failed to save settings: {e}"));
                    }
                }
            }
            Err(e) => {
                self.status = format!("Failed to serialize settings: {e}");
                self.log.log(format!("Failed to serialize settings: {e}"));
            }
        }
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
        );
        match persist::save(&settings) {
            Ok(_) => {}
            Err(e) => self.log.log(format!("Failed to persist settings: {e}")),
        }
    }

    /// Exports the settings JSON to the clipboard.
    fn export_settings(&mut self) {
        let settings = settings::AppSettings::capture(
            self.language,
            self.theme,
            &self.presets,
            self.ntp_server,
            &self.ntp_servers,
            self.sim_scale,
            &self.watch_config,
            self.text_size,
        );
        match settings.to_json() {
            Ok(json) => {
                self.status = "Settings JSON copied to clipboard".to_string();
                self.log.log("Settings JSON copied to clipboard");
                let _ = ui_copy_to_clipboard(&json);
            }
            Err(e) => {
                self.status = format!("Failed to serialize settings: {e}");
                self.log.log(format!("Failed to serialize settings: {e}"));
            }
        }
    }

    /// Imports settings from a JSON file in the app data directory.
    fn import_settings(&mut self) {
        let path = std::path::Path::new("settings.json");
        match std::fs::read_to_string(path) {
            Ok(json) => match settings::AppSettings::from_json(&json) {
                Ok(s) => {
                    self.apply_settings(s);
                    self.status = format!("Settings imported from {}", path.display());
                    self.log
                        .log(format!("Settings imported from {}", path.display()));
                }
                Err(e) => {
                    self.status = format!("Failed to parse settings: {e}");
                    self.log.log(format!("Failed to parse settings: {e}"));
                }
            },
            Err(e) => {
                self.status = format!("Failed to read settings: {e}");
                self.log.log(format!("Failed to read settings: {e}"));
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
        self.ntp_server = s.ntp_server;
        self.ntp_servers = s.ntp_servers;
        self.sim_scale = s.sim_scale;
        self.watch_config = s.watch_config;
        self.text_size = s.text_size;
        self.save_settings_internal();
    }

    /// Exports the source code to a folder.
    fn export_source(&mut self) {
        // Export the firmware + studio source to an "export" folder.
        let export_dir = std::path::Path::new("export");
        let _ = std::fs::create_dir_all(export_dir);
        let result = copy_dir(std::path::Path::new("."), export_dir);
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
        self.log
            .log(format!("Attempting to flash {}", uf2.display()));
        let data = match std::fs::read(uf2) {
            Ok(d) => d,
            Err(e) => {
                self.status = format!("Failed to read uf2: {e}");
                self.log.log(format!("Failed to read uf2: {e}"));
                return;
            }
        };
        for drive in 'A'..='Z' {
            let root = format!("{drive}:\\");
            if let Ok(entries) = std::fs::read_dir(&root) {
                let is_watch = entries.flatten().any(|e| {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    name == "info_uf2.txt" || name == "current.uf2"
                });
                if is_watch {
                    let dest = format!("{root}CURRENT.UF2");
                    if std::fs::write(&dest, &data).is_ok() {
                        self.status = format!("Flashed to {dest}");
                        self.log.log(format!("Flashed to {dest}"));
                        return;
                    }
                }
            }
        }
        self.status = "Watch not found (is it in bootloader mode?)".to_string();
        self.log.log("Watch not found (is it in bootloader mode?)");
    }

    /// Rough estimate of the firmware flash size in KB for the selected faces.
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

    /// Rough estimate of the firmware RAM usage in KB for the selected faces.
    /// The OS baseline is ~4 KB; each face adds ~0.4 KB.
    fn estimate_ram_kb(&self, selected: usize) -> u32 {
        4 + (selected as u32) / 2
    }

    /// Rough estimate of the compiled .uf2 size in KB for the selected faces.
    fn estimate_compiled_kb(&self, selected: usize) -> u32 {
        // UF2 adds ~512-byte headers; estimate flash + 10% overhead.
        self.estimate_flash_kb(selected) + self.estimate_flash_kb(selected) / 10
    }
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

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        // Launch at 640x480 (480p, 4:3) so there's ample space by default while
        // remaining adjustable.
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Firmware Studio",
        options,
        Box::new(|_cc| Box::new(StudioApp::default())),
    )
}

/// Recursively copies a directory, skipping `target` and `.git`.
fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name == "target" || name == ".git" {
            continue;
        }
        let dest = dst.join(&name);
        if path.is_dir() {
            copy_dir(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
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
    let sha = body.trim().to_string();
    if sha.len() == 64 {
        Ok(sha)
    } else {
        Err("Unexpected checksum format".to_string())
    }
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
