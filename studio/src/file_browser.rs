//! Read-only workspace file browser for Firmware Studio.

use crate::help::{AnchorId, AnchorRect};
use eframe::egui;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_PREVIEW_BYTES: u64 = 512 * 1024;

pub struct AnchorHit {
    pub key: AnchorId,
    pub rect: AnchorRect,
}

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
        let candidate = match crate::distribution::active().active_project_dir() {
            Some(path) => path,
            None => {
                self.root.clear();
                self.current_dir.clear();
                self.entries.clear();
                self.message = "Mutable project unavailable; bundled firmware is read-only".into();
                return;
            }
        };
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

    #[cfg(test)]
    fn from_root(root: PathBuf) -> Self {
        let root = root.canonicalize().unwrap();
        let mut browser = Self {
            root: root.clone(),
            current_dir: root,
            ..Self::default()
        };
        browser.load_entries();
        browser
    }

    fn load_entries(&mut self) {
        self.entries.clear();
        if !is_within_root(&self.current_dir, &self.root) {
            self.current_dir = self.root.clone();
            self.message = "Refusing to browse outside the workspace".to_string();
            return;
        }
        let read_dir = match fs::read_dir(&self.current_dir) {
            Ok(read_dir) => read_dir,
            Err(error) => {
                self.message = format!("Cannot read {}: {error}", self.current_dir.display());
                return;
            }
        };
        for item in read_dir.flatten() {
            let path = item.path();
            let file_type = match item.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            // Do not follow links: a link can escape the workspace or create a cycle.
            if file_type.is_symlink() || is_excluded(&path, &self.root) {
                continue;
            }
            let canonical = match path.canonicalize() {
                Ok(canonical) if is_within_root(&canonical, &self.root) => canonical,
                _ => continue,
            };
            let metadata = match fs::metadata(&canonical) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            self.entries.push(Entry {
                path: canonical,
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
        let path = match path.canonicalize() {
            Ok(path) if is_within_root(&path, &self.root) => path,
            _ => {
                self.message = "Refusing to inspect a path outside the workspace".to_string();
                return;
            }
        };
        // Re-resolve immediately before opening: the directory listing is not a
        // capability, and a path can be swapped after it was displayed.
        let reopened = match path.canonicalize() {
            Ok(reopened) if reopened == path && is_within_root(&reopened, &self.root) => reopened,
            _ => {
                self.message =
                    "Refusing to inspect a path changed outside the workspace".to_string();
                return;
            }
        };
        let metadata = match fs::symlink_metadata(&reopened) {
            Ok(metadata) if metadata.file_type().is_file() => metadata,
            Ok(metadata) => metadata,
            Err(error) => {
                self.message = format!("Cannot inspect {}: {error}", reopened.display());
                return;
            }
        };
        self.selected = Some(reopened.clone());
        self.preview_size = Some(metadata.len());
        if metadata.is_file() && is_previewable(&reopened, metadata.len()) {
            match read_preview(&reopened) {
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
    pub fn ui(&mut self, ui: &mut egui::Ui) -> (Option<String>, Vec<AnchorHit>) {
        let mut copied = None;
        let mut anchors = Vec::new();
        ui.heading("File Browser");
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(220, 180, 80),
                "READ-ONLY REFERENCE BROWSER",
            );
            ui.weak("No writes or deletes are available");
            let response = ui.button("Refresh");
            anchors.push(anchor(AnchorId::FileRefresh, &response));
            if response.clicked() {
                self.refresh_workspace();
            }
        });
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Search:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter files and directories"),
            );
            anchors.push(anchor(AnchorId::FileFilter, &response));
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
        let filter = self.filter.trim().to_lowercase();
        let mut open_path = None;
        let mut select_path = None;
        ui.columns(2, |columns| {
            columns[0].set_min_width(240.0);
            egui::ScrollArea::vertical()
                .max_height(available)
                .show(&mut columns[0], |ui| {
                    for entry in &self.entries {
                        let name = entry
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default();
                        if !filter.is_empty() && !name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        let size = entry
                            .size
                            .map(|size| format!(" ({})", format_size(size)))
                            .unwrap_or_default();
                        let label = format!(
                            "{} {}{}",
                            if entry.is_dir { "📁" } else { "📄" },
                            name,
                            size
                        );
                        let response =
                            ui.selectable_label(self.selected.as_ref() == Some(&entry.path), label);
                        anchors.push(anchor(AnchorId::FileList, &response));
                        if response.clicked() {
                            if entry.is_dir {
                                open_path = Some(entry.path.clone());
                            } else {
                                select_path = Some(entry.path.clone());
                            }
                        }
                    }
                });
            if let Some(path) = open_path.take() {
                self.current_dir = path;
                self.load_entries();
            } else if let Some(path) = select_path.take() {
                self.select(path);
            }
            columns[1].vertical(|ui| {
                let selected = self.selected.clone();
                ui.horizontal(|ui| {
                    let preview_response = ui.heading("Selection");
                    anchors.push(anchor(AnchorId::FilePreview, &preview_response));
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
        (copied, anchors)
    }
}

fn anchor(key: AnchorId, response: &egui::Response) -> AnchorHit {
    AnchorHit {
        key,
        rect: AnchorRect {
            min: (response.rect.min.x, response.rect.min.y),
            max: (response.rect.max.x, response.rect.max.y),
        },
    }
}

fn read_preview(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_PREVIEW_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PREVIEW_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file changed and is too large to preview",
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "file is not valid UTF-8")
    })
}

fn copy_to_clipboard(text: &str) -> Option<String> {
    match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text.to_string())) {
        Ok(()) => Some("Copied to clipboard".to_string()),
        Err(error) => Some(format!("Clipboard error: {error}")),
    }
}

#[cfg(test)]
mod project_root_tests {
    use super::*;

    #[test]
    fn browser_uses_active_project_as_root() {
        let root = std::env::temp_dir().join(format!("studio-browser-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/editable.rs"), b"x").unwrap();
        let browser = FileBrowser::from_root(root.join("."));
        assert_eq!(browser.root, root.canonicalize().unwrap());
        assert!(browser
            .entries
            .iter()
            .any(|entry| entry.path.ends_with("src")));
        let _ = std::fs::remove_dir_all(root);
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

fn is_within_root(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
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
