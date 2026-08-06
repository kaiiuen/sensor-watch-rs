//! Firmware Studio — a GUI companion app for the Sensor-Watch firmware.
//!
//! This is the end-goal product: an editor, debugger, and assembler that
//! produces the final `.uf2` firmware file.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod build;
mod debug;
mod editor;
mod faces;
mod i18n;
mod ntp;
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
    /// The watch configuration (mirrors the firmware Settings register).
    watch_config: watch_config::WatchConfig,
    /// The latest system resource snapshot for the footer.
    sys_stats: sysstats::SysStats,
    /// The receiver for background system resource samples.
    sys_rx: std::sync::mpsc::Receiver<sysstats::SysStats>,
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
            ntp_time: None,
            ntp_ping: 0.0,
            ntp_offset: 0.0,
            ntp_busy: false,
            pending_ntp: None,
            watch_config: watch_config::WatchConfig::default(),
            sys_stats: sysstats::SysStats::default(),
            sys_rx: sysstats::spawn_sampler(),
        };
        app.log.log("Firmware Studio starting");
        app.face_list = faces::discover_faces();
        app.log
            .log(format!("Discovered {} watch faces", app.face_list.len()));
        app.status = tr(app.language, Key::Ready).to_string();
        app
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply the theme.
        self.theme.apply(ctx);

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

        // Top navigation bar.
        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(tr(self.language, Key::AppTitle));
                ui.separator();
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
                        .selectable_label(self.current_panel == panel, panel.label(self.language))
                        .clicked()
                    {
                        self.current_panel = panel;
                    }
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
                ui.monospace(format!("~{} KB flash", self.estimate_flash_kb(selected)));
                ui.separator();
                ui.monospace(format!("~{} KB RAM", self.estimate_ram_kb(selected)));
                ui.separator();
                ui.monospace(format!(
                    "~{} KB compiled",
                    self.estimate_compiled_kb(selected)
                ));
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

        ui.label(tr(self.language, Key::Target));
        ui.label(
            tr(self.language, Key::FlashRam).replace("{faces}", &self.face_list.len().to_string()),
        );
        ui.add_space(8.0);
        if let Some(uf2) = &self.last_uf2 {
            ui.label(
                tr(self.language, Key::LastBuild).replace("{path}", &uf2.display().to_string()),
            );
        } else {
            ui.label(tr(self.language, Key::NoBuildYet));
        }

        ui.add_space(16.0);
        ui.separator();
        ui.heading("NTP Time");
        ui.label("Select a server and fetch the current time.");
        ui.add_space(4.0);

        // Server selection.
        ui.horizontal(|ui| {
            ui.label("Server:");
            egui::ComboBox::from_id_source("ntp_server")
                .selected_text(ntp::SERVERS[self.ntp_server].0)
                .show_ui(ui, |ui| {
                    for (i, (name, _)) in ntp::SERVERS.iter().enumerate() {
                        ui.selectable_value(&mut self.ntp_server, i, *name);
                    }
                });
            if ui.button("Fetch time").clicked() {
                self.fetch_ntp();
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

        // Catalog (left) and active preset (right), both filling space.
        egui::SidePanel::left("catalog")
            .resizable(true)
            .default_width(ui.available_width() * 0.45)
            .width_range(180.0..=f32::INFINITY)
            .show_inside(ui, |ui| {
                ui.heading("Catalog");
                ui.separator();
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
                                        self.log.log(format!("Added {} to preset", face.name));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });

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
                                if ui.selectable_label(selected, (i + 1).to_string()).clicked() {
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
        });
        ui.separator();
        self.draw_watch(ui, ctx, self.sim_scale);
    }

    /// Draws the watch SVG at the given scale with clickable F-91W button hotspots.
    fn draw_watch(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, scale: f32) {
        // Advance the stopwatch and button-A hold timer based on elapsed time.
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.sim_last_tick);
        self.sim_last_tick = now;
        let cs = dt.as_millis() as u64 / 10;
        self.watch.tick_stopwatch(cs);
        self.watch.tick_button_a();

        // Update the display state.
        self.watch.update_display();

        // Render the watch SVG at a size based on the scale.
        let base = 740u32;
        let size = [(base as f32 * scale) as u32, (655.0 * scale) as u32];
        let texture = watch_display::render_to_texture(
            &mut self.watch_renderer,
            &self.watch.display,
            size,
            ctx,
        );
        let aspect = 1480.0 / 1311.0;
        let w = size[0] as f32;
        let h = w / aspect;

        // Allocate the image rect so we can map clicks to SVG button hotspots.
        let (rect, response) = ui.allocate_exact_size(egui::Vec2::new(w, h), egui::Sense::click());
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        // Map a click position (in image space) to the F-91W buttons.
        // SVG viewBox is 1480x1311; buttons are circles at known centers.
        if let Some(pos) = response.interact_pointer_pos() {
            if response.clicked() {
                let svg_x = (pos.x - rect.min.x) / rect.width() * 1480.0;
                let svg_y = (pos.y - rect.min.y) / rect.height() * 1311.0;
                let hit = |cx: f32, cy: f32, r: f32| {
                    let dx = svg_x - cx;
                    let dy = svg_y - cy;
                    dx * dx + dy * dy <= r * r
                };
                if hit(1355.0, 811.0, 125.0) {
                    self.watch.button_a(true);
                } else if hit(125.0, 813.0, 125.0) {
                    self.watch.button_c(true);
                } else if hit(125.0, 511.0, 125.0) {
                    self.watch.button_l(true);
                }
            }
        }

        // Release held buttons when the pointer leaves the image.
        if !response.hovered() {
            self.watch.button_l(false);
            self.watch.button_a(false);
        }

        // Buttons: arranged to mirror the physical F-91W layout (L top-left,
        // C bottom-left, A right). Hold the button to keep it pressed; release
        // when the mouse is released. Each shows its current action.
        let (menu_desc, l_desc, c_desc, a_desc) = self.watch.instructions();
        ui.add_space(8.0);
        ui.label(format!("Mode: {menu_desc}"));
        ui.add_space(4.0);
        egui::Grid::new("sim_buttons")
            .spacing([16.0, 4.0])
            .num_columns(3)
            .show(ui, |ui| {
                // Header.
                ui.strong("L (top-left)");
                ui.strong("C (bottom-left)");
                ui.strong("A (right)");
                ui.end_row();
                // Hold-to-press buttons.
                let l = ui.add(egui::Button::new("Hold").min_size(egui::vec2(80.0, 40.0)));
                let c = ui.add(egui::Button::new("Hold").min_size(egui::vec2(80.0, 40.0)));
                let a = ui.add(egui::Button::new("Hold").min_size(egui::vec2(80.0, 40.0)));
                // Press while held, release on release.
                self.watch.button_l(l.is_pointer_button_down_on());
                self.watch.button_c(c.is_pointer_button_down_on());
                self.watch.button_a(a.is_pointer_button_down_on());
                ui.end_row();
                // Instructions row.
                ui.label(l_desc);
                ui.label(c_desc);
                ui.label(a_desc);
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.label(format!(
            "Menu: {:?}  Action: {:?}",
            self.watch.active_menu, self.watch.active_action
        ));
        ui.label(format!(
            "Time mode: {:?}  Alarm: {}  Signal: {}",
            self.watch.time_mode, self.watch.alarm_on_mark, self.watch.time_signal_on_mark
        ));

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

                // Firmware project path.
                ui.label(tr(self.language, Key::FirmwareProject));
                ui.monospace(build::FIRMWARE_DIR);
                ui.end_row();
            });

        ui.add_space(16.0);
        ui.separator();
        ui.heading("Watch Settings");
        ui.label("Configure the watch firmware (mirrors the on-watch preferences face).");
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
                        egui::Slider::new(&mut self.watch_config.piezo_voltage, 0.0..=9.0)
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
                    if let Some(col) = parse_hex_color(&self.watch_config.led_color_hex) {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(24.0, 16.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, col);
                    }
                });
                ui.end_row();

                // LED gradient toggle.
                ui.label("LED gradient");
                ui.checkbox(&mut self.watch_config.led_gradient, "Use a color gradient");
                ui.end_row();

                // LED gradient hex.
                ui.label("Gradient color (hex)");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.watch_config.led_gradient_hex);
                    if let Some(col) = parse_hex_color(&self.watch_config.led_gradient_hex) {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(24.0, 16.0), egui::Sense::hover());
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
                    ui.add(egui::Slider::new(&mut self.watch_config.to_interval, 0..=3));
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
                    ui.add(egui::Slider::new(&mut self.watch_config.le_interval, 0..=7));
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
                let _ = ui_copy_to_clipboard(&format!("0x{:08X}", self.watch_config.to_reg()));
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

        ui.add_space(16.0);
        ui.separator();
        ui.heading("App Resource Usage");
        ui.label("How much of the system this app is currently using (updates every second).");
        ui.add_space(8.0);

        // Side-by-side: App on the left, System on the right, as a plain grid.
        egui::Grid::new("app_resources_grid")
            .striped(true)
            .spacing([40.0, 6.0])
            .num_columns(4)
            .show(ui, |ui| {
                // Headers.
                ui.strong("App");
                ui.label("");
                ui.strong("System");
                ui.label("");
                ui.end_row();

                // CPU.
                ui.label("CPU");
                ui.monospace(format!("{:.1}%", self.sys_stats.cpu_percent));
                ui.label("CPU");
                ui.monospace(format!("{:.1}%", self.sys_stats.sys_cpu_percent));
                ui.end_row();

                // CPU speed / cores.
                ui.label("CPU speed");
                ui.monospace(format!("{} MHz", self.sys_stats.cpu_freq_mhz));
                ui.label("Cores");
                ui.monospace(format!("{}", self.sys_stats.physical_cores));
                ui.end_row();

                // Threads.
                ui.label("Threads");
                ui.monospace(format!("{}", self.sys_stats.threads));
                ui.label("");
                ui.label("");
                ui.end_row();

                // Memory.
                ui.label("Memory");
                ui.monospace(fmt_bytes(self.sys_stats.mem_bytes));
                ui.label("Memory");
                let total = self.sys_stats.total_mem_bytes.max(1);
                let pct = self.sys_stats.sys_mem_used_bytes as f64 / total as f64 * 100.0;
                ui.monospace(format!(
                    "{} / {} ({pct:.1}%)",
                    fmt_bytes(self.sys_stats.sys_mem_used_bytes),
                    fmt_bytes(self.sys_stats.total_mem_bytes)
                ));
                ui.end_row();

                // Virtual memory.
                ui.label("Virtual memory");
                ui.monospace(fmt_bytes(self.sys_stats.virtual_mem_bytes));
                ui.label("");
                ui.label("");
                ui.end_row();

                // Disk read.
                ui.label("Disk read");
                ui.monospace(format!(
                    "{}  ({} total)",
                    fmt_bytes(self.sys_stats.disk_read_rate),
                    fmt_bytes(self.sys_stats.disk_read_bytes)
                ));
                ui.label("");
                ui.label("");
                ui.end_row();

                // Disk write.
                ui.label("Disk write");
                ui.monospace(format!(
                    "{}  ({} total)",
                    fmt_bytes(self.sys_stats.disk_write_rate),
                    fmt_bytes(self.sys_stats.disk_write_bytes)
                ));
                ui.label("");
                ui.label("");
                ui.end_row();

                // Network.
                ui.label("");
                ui.label("");
                ui.label("Network");
                ui.monospace(format!(
                    "↓ {}  ↑ {}",
                    fmt_bytes(self.sys_stats.sys_net_rx_rate),
                    fmt_bytes(self.sys_stats.sys_net_tx_rate)
                ));
                ui.end_row();

                // Run time.
                ui.label("Run time");
                ui.monospace(format!("{} s", self.sys_stats.run_time_secs));
                ui.label("");
                ui.label("");
                ui.end_row();

                // GPU.
                ui.label("GPU");
                ui.monospace("N/A (Windows)");
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
    }

    /// Saves the current settings to a JSON file in the app data directory.
    fn save_settings_to_file(&mut self) {
        let settings = settings::AppSettings::capture(
            self.language,
            self.theme,
            &self.presets,
            self.ntp_server,
            self.sim_scale,
            &self.watch_config,
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

    /// Exports the settings JSON to the clipboard.
    fn export_settings(&mut self) {
        let settings = settings::AppSettings::capture(
            self.language,
            self.theme,
            &self.presets,
            self.ntp_server,
            self.sim_scale,
            &self.watch_config,
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
        self.sim_scale = s.sim_scale;
        self.watch_config = s.watch_config;
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
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 600.0]),
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
