//! Read-only workspace file browser for Firmware Studio.

use eframe::egui;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_PREVIEW_BYTES: u64 = 512 * 1024;

#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    is_dir: bool,
    size: Option<u64>,
}

/// State and UI for the project's read-only reference browser.
pub struct FileBrowser {
    root: PathBuf,
    current_dir: PathBuf,
    entries: Vec<Entry>,
    filter: String,
    selected: Option<PathBuf>,
    preview: String,
    preview_allowed: bool,
    preview_size: Option<u64>,
    message: String,
}

impl Default for FileBrowser {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            current_dir: PathBuf::new(),
            entries: Vec::new(),
            filter: String::new(),
            selected: None,
            preview: String::new(),
            preview_allowed: false,
            preview_size: None,
            message: String::new(),
        }
    }
}

impl FileBrowser {
    pub fn new() -> Self {
        let mut browser = Self::default();
        browser.refresh_workspace();
        browser
    }

    fn refresh_workspace(&mut self) {
        let candidate = crate::build::firmware_dir();
        match candidate.canonicalize() {
            Ok(root) if root.is_dir() => {
                self.root = root.clone();
                self.current_dir = root;
                self.message.clear();
                self.load_entries();
            }
            _ => {
                self.root.clear();
                self.current_dir.clear();
                self.entries.clear();
                self.message = format!("Workspace not found at {}", candidate.display());
            }
        }
    }

    fn load_entries(&mut self) {
        self.entries.clear();
        let read_dir = match fs::read_dir(&self.current_dir) {
            Ok(read_dir) => read_dir,
            Err(error) => {
                self.message = format!("Cannot read {}: {error}", self.current_dir.display());
                return;
            }
        };
        for item in read_dir.flatten() {
            let path = item.path();
            if is_excluded(&path, &self.root) {
                continue;
            }
            let metadata = match item.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            self.entries.push(Entry {
                path,
                is_dir: metadata.is_dir(),
                size: (!metadata.is_dir()).then_some(metadata.len()),
            });
        }
        self.entries.sort_by_key(|entry| {
            (
                !entry.is_dir,
                entry
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase()),
            )
        });
    }

    fn select(&mut self, path: PathBuf) {
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.message = format!("Cannot inspect {}: {error}", path.display());
                return;
            }
        };
        self.selected = Some(path.clone());
        self.preview_size = Some(metadata.len());
        if metadata.is_file() && is_previewable(&path, metadata.len()) {
            match fs::read_to_string(&path) {
                Ok(contents) => {
                    self.preview = contents;
                    self.preview_allowed = true;
                    self.message.clear();
                }
                Err(error) => {
                    self.preview = format!("Preview unavailable: {error}");
                    self.preview_allowed = false;
                }
            }
        } else {
            self.preview.clear();
            self.preview_allowed = false;
            self.message = if metadata.is_dir() {
                "Directory selected; open it to browse its contents.".to_string()
            } else {
                "Content preview is disabled for credentials, secrets, and binary files."
                    .to_string()
            };
        }
    }

    fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .map(|relative| {
                if relative.as_os_str().is_empty() {
                    "/".to_string()
                } else {
                    relative.display().to_string()
                }
            })
            .unwrap_or_else(|_| path.display().to_string())
    }

    /// Render the read-only browser. Returns a status message when the user copies data.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut copied = None;
        ui.heading("File Browser");
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(220, 180, 80),
                "READ-ONLY REFERENCE BROWSER",
            );
            ui.weak("No writes or deletes are available");
            if ui.button("Refresh").clicked() {
                self.refresh_workspace();
            }
        });
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter files and directories"),
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Workspace:");
            if ui.link("/").clicked() {
                self.current_dir = self.root.clone();
                self.load_entries();
            }
            let relative = self.relative_path(&self.current_dir);
            for component in Path::new(&relative).components() {
                let name = component.as_os_str().to_string_lossy().to_string();
                if name == "/" {
                    continue;
                }
                ui.label("/");
                ui.monospace(name);
            }
            ui.weak(format!("({})", self.current_dir.display()));
        });

        let available = ui.available_height().max(180.0);
        ui.columns(2, |columns| {
            columns[0].set_min_width(240.0);
            egui::ScrollArea::vertical()
                .max_height(available)
                .show(&mut columns[0], |ui| {
                    for entry in self.entries.clone() {
                        let name = entry
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default();
                        if !self.filter.trim().is_empty()
                            && !name
                                .to_lowercase()
                                .contains(&self.filter.trim().to_lowercase())
                        {
                            continue;
                        }
                        let label = format!("{} {}", if entry.is_dir { "📁" } else { "📄" }, name);
                        if ui
                            .selectable_label(self.selected.as_ref() == Some(&entry.path), label)
                            .clicked()
                        {
                            if entry.is_dir {
                                self.current_dir = entry.path.clone();
                                self.load_entries();
                            } else {
                                self.select(entry.path.clone());
                            }
                        }
                    }
                });
            columns[1].vertical(|ui| {
                let selected = self.selected.clone();
                ui.horizontal(|ui| {
                    ui.heading("Selection");
                    if let Some(path) = &selected {
                        if ui.button("Copy path").clicked() {
                            copied = copy_to_clipboard(&path.display().to_string());
                        }
                        if self.preview_allowed && ui.button("Copy contents").clicked() {
                            copied = copy_to_clipboard(&self.preview);
                        }
                    }
                });
                if let Some(path) = selected {
                    ui.label(format!("Path: {}", self.relative_path(&path)));
                    ui.label(format!(
                        "Size: {}",
                        format_size(self.preview_size.unwrap_or(0))
                    ));
                    ui.separator();
                    if self.preview_allowed {
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.preview)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        });
                    } else {
                        ui.weak("No content preview for this selection.");
                    }
                } else {
                    ui.weak("Select a file to view its contents.");
                }
            });
        });
        copied
    }
}

fn copy_to_clipboard(text: &str) -> Option<String> {
    match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text.to_string())) {
        Ok(()) => Some("Copied to clipboard".to_string()),
        Err(error) => Some(format!("Clipboard error: {error}")),
    }
}

fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KiB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", size as f64 / 1_048_576.0)
    }
}

fn is_excluded(path: &Path, root: &Path) -> bool {
    let relative = match path.strip_prefix(root) {
        Ok(relative) => relative,
        Err(_) => return true,
    };
    relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        name == ".git" || name == "target" || is_secret_name(&name)
    })
}

fn is_secret_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("credential")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("token")
        || lower == ".env"
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
}

fn is_previewable(path: &Path, size: u64) -> bool {
    if size > MAX_PREVIEW_BYTES
        || path
            .file_name()
            .map(|n| is_secret_name(&n.to_string_lossy()))
            .unwrap_or(true)
    {
        return false;
    }
    let binary_extensions = [
        "bin", "uf2", "elf", "exe", "dll", "so", "dylib", "png", "jpg", "jpeg", "gif", "bmp",
        "ico", "pdf", "zip", "gz", "wasm",
    ];
    !path
        .extension()
        .map(|ext| binary_extensions.contains(&ext.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_sensitive_components() {
        let root = Path::new("workspace");
        assert!(is_excluded(&root.join("target/debug/a"), root));
        assert!(is_excluded(&root.join("src/credentials.toml"), root));
        assert!(!is_excluded(&root.join("src/main.rs"), root));
    }

    #[test]
    fn rejects_binary_and_large_previews() {
        assert!(!is_previewable(Path::new("firmware.uf2"), 10));
        assert!(!is_previewable(
            Path::new("notes.txt"),
            MAX_PREVIEW_BYTES + 1
        ));
        assert!(is_previewable(Path::new("notes.txt"), 10));
    }
}
