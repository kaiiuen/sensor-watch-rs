//! Firmware Studio — a GUI companion app for the Sensor-Watch firmware.
//!
//! This is the end-goal product: an editor, debugger, and assembler that
//! produces the final `.uf2` firmware file.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod build;
mod faces;

use eframe::egui;

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
}

/// The navigation panels.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Dashboard,
    Faces,
    Build,
    Flash,
    Settings,
}

impl Panel {
    fn label(self) -> &'static str {
        match self {
            Panel::Dashboard => "Dashboard",
            Panel::Faces => "Watch Faces",
            Panel::Build => "Build",
            Panel::Flash => "Flash",
            Panel::Settings => "Settings",
        }
    }
}

impl Default for StudioApp {
    fn default() -> Self {
        StudioApp {
            current_panel: Panel::Dashboard,
            status: "Ready".to_string(),
            face_list: faces::discover_faces(),
            building: false,
            pending_build: None,
            build_message: String::new(),
            last_uf2: build::last_uf2(),
        }
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // If a build finished, collect its result.
        if let Some(handle) = self.pending_build.take() {
            if handle.is_finished() {
                match handle.join() {
                    Ok(result) => {
                        self.building = false;
                        self.build_message = result.message.clone();
                        self.status = if result.success {
                            "Build complete".to_string()
                        } else {
                            "Build failed".to_string()
                        };
                        if result.success {
                            self.last_uf2 = result.uf2_path;
                        }
                    }
                    Err(_) => {
                        self.building = false;
                        self.build_message = "Build thread panicked".to_string();
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
                ui.heading("Firmware Studio");
                ui.separator();
                for panel in [
                    Panel::Dashboard,
                    Panel::Faces,
                    Panel::Build,
                    Panel::Flash,
                    Panel::Settings,
                ] {
                    if ui
                        .selectable_label(self.current_panel == panel, panel.label())
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
            Panel::Build => self.build(ui),
            Panel::Flash => self.flash(ui),
            Panel::Settings => self.settings(ui),
        });
    }
}

impl StudioApp {
    /// The dashboard: an overview of the project and its health.
    fn dashboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("Dashboard");
        ui.separator();
        ui.label("Firmware Studio — Sensor-Watch companion app.");
        ui.add_space(8.0);
        ui.label("Target: Microchip SAM L22J18A (ARM Cortex-M0+)");
        ui.label(format!(
            "Flash: 256 KB  |  RAM: 32 KB  |  Faces: {}",
            self.face_list.len()
        ));
        ui.add_space(8.0);
        if let Some(uf2) = &self.last_uf2 {
            ui.label(format!("Last build: {}", uf2.display()));
        } else {
            ui.label("No build yet. Go to the Build panel.");
        }
    }

    /// The watch-faces panel: list and manage watch faces.
    fn faces(&mut self, ui: &mut egui::Ui) {
        ui.heading("Watch Faces");
        ui.separator();
        ui.label(format!(
            "{} faces registered in the firmware.",
            self.face_list.len()
        ));
        ui.add_space(4.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("faces_grid").striped(true).show(ui, |ui| {
                ui.label("Index");
                ui.label("Face");
                ui.label("File");
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
        ui.heading("Build");
        ui.separator();
        ui.label("Assemble the firmware and produce a .uf2 file.");
        ui.add_space(8.0);

        if self.building {
            ui.spinner();
            ui.label("Building...");
        } else {
            if ui.button("Build .uf2").clicked() {
                self.building = true;
                self.build_message = "Building...".to_string();
                // Run the build on a background thread so the UI stays responsive.
                let handle = std::thread::spawn(build::build_firmware);
                // Poll the thread result in a later frame.
                self.pending_build = Some(handle);
            }
        }

        if !self.build_message.is_empty() {
            ui.add_space(8.0);
            ui.label(&self.build_message);
        }
        if let Some(uf2) = &self.last_uf2 {
            ui.add_space(8.0);
            ui.label(format!("Output: {}", uf2.display()));
        }
    }

    /// The flash panel: flash the firmware to the watch.
    fn flash(&mut self, ui: &mut egui::Ui) {
        ui.heading("Flash");
        ui.separator();
        ui.label("Flash the firmware to the watch over USB.");
        ui.add_space(8.0);
        if let Some(uf2) = &self.last_uf2 {
            let uf2 = uf2.clone();
            ui.label(format!("Firmware: {}", uf2.display()));
            if ui.button("Copy to watch").clicked() {
                self.copy_to_watch(&uf2);
            }
        } else {
            ui.label("Build the firmware first.");
        }
    }

    /// The settings panel: configure the app and the watch.
    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.separator();
        ui.label("Configure the app and the watch.");
        ui.add_space(8.0);
        ui.label("Firmware project:");
        ui.label(build::FIRMWARE_DIR);
    }

    /// Copies the built .uf2 to the watch's USB drive (if mounted).
    fn copy_to_watch(&mut self, uf2: &std::path::Path) {
        // The Sensor Watch mounts as a USB mass-storage drive. Look for a
        // removable drive containing the firmware (e.g. a drive with a
        // `INFO_UF2.TXT` or `CURRENT.UF2` marker).
        let data = match std::fs::read(uf2) {
            Ok(d) => d,
            Err(e) => {
                self.status = format!("Failed to read uf2: {e}");
                return;
            }
        };
        for drive in 'A'..='Z' {
            let root = format!("{drive}:\\");
            if let Ok(entries) = std::fs::read_dir(&root) {
                // Look for the UF2 bootloader marker.
                let is_watch = entries.flatten().any(|e| {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    name == "info_uf2.txt" || name == "current.uf2"
                });
                if is_watch {
                    let dest = format!("{root}CURRENT.UF2");
                    if std::fs::write(&dest, &data).is_ok() {
                        self.status = format!("Flashed to {dest}");
                        return;
                    }
                }
            }
        }
        self.status = "Watch not found (is it in bootloader mode?)".to_string();
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
