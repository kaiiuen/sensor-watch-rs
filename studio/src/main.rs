//! Firmware Studio — a GUI companion app for the Sensor-Watch firmware.
//!
//! This is the end-goal product: an editor, debugger, and assembler that
//! produces the final `.uf2` firmware file.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod build;
mod debug;
mod faces;
mod i18n;
mod theme;
mod watch_display;
mod watch_sim;

use eframe::egui;
use i18n::{tr, Key, Language};
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
}

/// The navigation panels.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Dashboard,
    Faces,
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
            Panel::Faces => self.faces(ui),
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

    /// The watch-faces panel: list and manage watch faces.
    fn faces(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr(self.language, Key::WatchFaces));
        ui.separator();
        ui.label(
            tr(self.language, Key::FacesRegistered)
                .replace("{count}", &self.face_list.len().to_string()),
        );
        ui.add_space(4.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("faces_grid").striped(true).show(ui, |ui| {
                ui.label(tr(self.language, Key::Index));
                ui.label(tr(self.language, Key::Face));
                ui.label(tr(self.language, Key::File));
                ui.end_row();
                for face in &self.face_list {
                    ui.label(face.index.to_string());
                    ui.label(&face.name);
                    ui.label(&face.file);
                    ui.end_row();
                }
            });
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
        ui.heading("Simulator");
        ui.separator();
        ui.label("Casio F-91W simulator — try the watch before flashing.");
        ui.add_space(8.0);

        // Advance the stopwatch and button-A hold timer based on elapsed time.
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.sim_last_tick);
        self.sim_last_tick = now;
        let cs = dt.as_millis() as u64 / 10;
        self.watch.tick_stopwatch(cs);
        self.watch.tick_button_a();

        // Draw the watch body and display.
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(360.0, 300.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        // Watch body (dark rounded rectangle).
        painter.rect_filled(rect, 16.0, egui::Color32::from_rgb(0x1a, 0x1a, 0x1a));
        // Display area (light LCD).
        let lcd = egui::Rect::from_min_size(
            rect.min + egui::Vec2::new(20.0, 20.0),
            egui::Vec2::new(rect.width() - 40.0, rect.height() - 60.0),
        );
        painter.rect_filled(lcd, 8.0, egui::Color32::from_rgb(0x2a, 0x30, 0x32));

        // Update and draw the display.
        self.watch.update_display();
        watch_display::draw_display(&painter, lcd, &self.watch.display);

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
