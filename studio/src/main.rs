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
mod presets;
mod theme;
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
    /// The simulator scale used in the Watch Faces panel (defaults smaller).
    faces_sim_scale: f32,
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
            sim_scale: 1.0,
            faces_sim_scale: 0.5,
            presets: PresetManager::new(),
            selected_face: None,
            selected_preset_face: None,
            new_preset_name: String::new(),
            editor_name: String::new(),
            editor_source: String::new(),
            editor_template: 0,
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
            ui.horizontal(|ui| {
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
    /// The dashboard: an overview of the project and its health.
    fn dashboard(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr(self.language, Key::Dashboard));
        ui.separator();
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
    }

    /// The watch-faces panel: split layout with the simulator, catalog, and
    /// the active preset, plus preset management sub-tabs.
    fn faces(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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

        // Split horizontally: simulator on the bottom (resizable), catalog+preset on top.
        egui::TopBottomPanel::bottom("sim")
            .resizable(true)
            .default_height(ui.available_height() * 0.35)
            .min_height(100.0)
            .show_inside(ui, |ui| {
                self.faces_simulator(ui, ctx);
            });

        // Top half: catalog (left) and active preset (right), both filling space.
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
                        for (i, face) in self.face_list.iter().enumerate() {
                            let selected = self.selected_face == Some(i);
                            if ui
                                .selectable_label(
                                    selected,
                                    format!("{} — {}", face.index, face.name),
                                )
                                .clicked()
                            {
                                self.selected_face = Some(i);
                            }
                        }
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

    /// The simulator used inside the Watch Faces panel (smaller default scale).
    fn faces_simulator(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Simulator");
            ui.separator();
            ui.label("Size:");
            ui.add(
                egui::Slider::new(&mut self.faces_sim_scale, 0.3..=1.5)
                    .step_by(0.05)
                    .suffix("x"),
            );
            if ui.button("Reset").clicked() {
                self.faces_sim_scale = 0.5;
            }
        });
        ui.separator();
        self.draw_watch(ui, ctx, self.faces_sim_scale);
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
                }
                for entry in self.log.entries() {
                    let secs = entry.timestamp % 60;
                    let mins = (entry.timestamp / 60) % 60;
                    let hrs = (entry.timestamp / 3600) % 24;
                    ui.monospace(format!(
                        "[{:02}:{:02}:{:02}] {}",
                        hrs, mins, secs, entry.message
                    ));
                }
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
                self.sim_scale = 1.0;
            }
        });
        ui.separator();
        self.draw_watch(ui, ctx, self.sim_scale);
    }

    /// Draws the watch SVG at the given scale and the control buttons.
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
        ui.image((texture.id(), egui::Vec2::new(w, h)));

        // Buttons.
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("L (light)").clicked() {
                self.watch.button_l(true);
            }
            if ui.button("C (mode)").clicked() {
                self.watch.button_c(true);
            }
            if ui.button("A (adjust)").clicked() {
                self.watch.button_a(true);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Release L").clicked() {
                self.watch.button_l(false);
            }
            if ui.button("Release A").clicked() {
                self.watch.button_a(false);
            }
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
        ui.label(tr(self.language, Key::ConfigureApp));
        ui.add_space(8.0);

        // Language selector.
        ui.label(tr(self.language, Key::Language));
        for lang in Language::ALL {
            if ui
                .selectable_label(self.language == lang, lang.name())
                .clicked()
            {
                self.language = lang;
                self.log.log(format!("Language set to {}", lang.name()));
            }
        }

        ui.add_space(8.0);

        // Theme selector.
        ui.label(tr(self.language, Key::Theme));
        for theme in Theme::ALL {
            if ui
                .selectable_label(self.theme == theme, theme.name())
                .clicked()
            {
                self.theme = theme;
                self.log.log(format!("Theme set to {}", theme.name()));
            }
        }

        ui.add_space(8.0);
        ui.label(tr(self.language, Key::FirmwareProject));
        ui.label(build::FIRMWARE_DIR);

        ui.add_space(16.0);
        ui.separator();
        ui.heading("Export");
        ui.label("Export the full source code (firmware + app) to a folder.");
        if ui.button("Export source").clicked() {
            self.export_source();
        }
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
