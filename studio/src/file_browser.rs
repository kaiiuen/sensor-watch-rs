//! Bounded Studio File Browser backed by the central filesystem policy.

use crate::fs_policy::{Item, Policy, PolicyError, RootKind, Roots};
use crate::help::{AnchorId, AnchorRect};
use eframe::egui;
use std::path::{Path, PathBuf};

pub struct AnchorHit {
    pub key: AnchorId,
    pub rect: AnchorRect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortMode {
    Name,
    Size,
}

pub struct FileBrowser {
    policy: Policy,
    root_kind: RootKind,
    current_dir: PathBuf,
    entries: Vec<Item>,
    filter: String,
    selected: Option<PathBuf>,
    message: String,
    sort: SortMode,
    open_request: Option<PathBuf>,
    new_name: String,
    rename_name: String,
    pending_delete: Option<PathBuf>,
    pending_rename: Option<(PathBuf, PathBuf)>,
}

impl Default for FileBrowser {
    fn default() -> Self {
        Self {
            policy: Policy::new(Roots::empty()),
            root_kind: RootKind::ActiveProject,
            current_dir: PathBuf::new(),
            entries: Vec::new(),
            filter: String::new(),
            selected: None,
            message: String::new(),
            sort: SortMode::Name,
            open_request: None,
            new_name: String::new(),
            rename_name: String::new(),
            pending_delete: None,
            pending_rename: None,
        }
    }
}

impl FileBrowser {
    pub fn new() -> Self {
        let mut browser = Self {
            policy: Policy::new(Roots::from_distribution(&crate::distribution::active())),
            ..Self::default()
        };
        browser.refresh();
        browser
    }

    #[cfg(test)]
    fn from_roots(data: PathBuf, project: PathBuf) -> Self {
        let mut browser = Self {
            policy: Policy::new(Roots::test(data, Some(project))),
            root_kind: RootKind::AppData,
            ..Self::default()
        };
        browser.refresh();
        browser
    }

    fn refresh(&mut self) {
        self.selected = None;
        self.entries.clear();
        if let Some(path) = self.pending_delete.take() {
            if let Ok(root) = self.policy.root(self.root_kind) {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(Path::new(""))
                    .to_path_buf();
                self.action(self.policy.remove(self.root_kind, &relative));
            }
        }
        if let Some((from, to)) = self.pending_rename.take() {
            if let Ok(root) = self.policy.root(self.root_kind) {
                let from = from
                    .strip_prefix(&root)
                    .unwrap_or(Path::new(""))
                    .to_path_buf();
                let to = to
                    .strip_prefix(&root)
                    .unwrap_or(Path::new(""))
                    .to_path_buf();
                self.action(self.policy.rename(self.root_kind, &from, &to));
            }
        }
        match self.policy.root(self.root_kind) {
            Ok(root) => {
                if self.current_dir.as_os_str().is_empty() || !self.current_dir.starts_with(&root) {
                    self.current_dir = root;
                }
                let relative = self
                    .current_dir
                    .strip_prefix(self.policy.root(self.root_kind).unwrap())
                    .unwrap_or(Path::new(""));
                match self.policy.list(self.root_kind, relative) {
                    Ok(entries) => {
                        self.entries = entries;
                        self.sort_entries();
                    }
                    Err(error) => self.message = error.to_string(),
                }
            }
            Err(error) => {
                self.current_dir.clear();
                self.message = error.to_string();
            }
        }
    }

    fn sort_entries(&mut self) {
        match self.sort {
            SortMode::Name => self.entries.sort_by_key(|item| {
                (
                    !item.is_dir,
                    item.relative.to_string_lossy().to_ascii_lowercase(),
                )
            }),
            SortMode::Size => self.entries.sort_by_key(|item| (!item.is_dir, item.size)),
        }
    }

    fn navigate(&mut self, relative: PathBuf) {
        match self.policy.root(self.root_kind).and_then(|root| {
            self.policy
                .list(self.root_kind, &relative)
                .map(|_| root.join(&relative))
        }) {
            Ok(path) => {
                self.current_dir = path;
                self.selected = None;
                self.refresh();
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    pub fn take_open_request(&mut self) -> Option<PathBuf> {
        self.open_request.take()
    }

    pub fn read_text_path(&self, path: &Path) -> Result<String, PolicyError> {
        for kind in [RootKind::ActiveProject, RootKind::AppData] {
            if let Ok(root) = self.policy.root(kind) {
                if let Ok(relative) = path.strip_prefix(&root) {
                    return self.policy.read_text(kind, relative);
                }
            }
        }
        Err(PolicyError::OutsideRoot)
    }

    pub fn write_text_path(
        &self,
        path: &Path,
        contents: &str,
        expected: Option<&str>,
    ) -> Result<(), PolicyError> {
        for kind in [RootKind::ActiveProject, RootKind::AppData] {
            if let Ok(root) = self.policy.root(kind) {
                if let Ok(relative) = path.strip_prefix(&root) {
                    return self.policy.write_text(kind, relative, contents, expected);
                }
            }
        }
        Err(PolicyError::OutsideRoot)
    }

    fn action(&mut self, result: Result<(), PolicyError>) {
        self.message = match result {
            Ok(()) => {
                self.refresh();
                "Operation completed".into()
            }
            Err(error) => error.to_string(),
        };
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> (Option<String>, Vec<AnchorHit>) {
        let mut copied = None;
        let mut anchors = Vec::new();
        ui.heading("File Browser");
        ui.horizontal(|ui| {
            ui.label("Root:");
            for kind in [RootKind::AppData, RootKind::ActiveProject] {
                if ui
                    .selectable_label(self.root_kind == kind, kind.label())
                    .clicked()
                {
                    self.root_kind = kind;
                    self.current_dir.clear();
                    self.refresh();
                }
            }
            let response = ui.button("Refresh");
            anchors.push(anchor(AnchorId::FileRefresh, &response));
            if response.clicked() {
                self.refresh();
            }
            ui.label("Writes are limited to app data and the active project");
        });
        if !self.message.is_empty() {
            ui.colored_label(egui::Color32::YELLOW, &self.message);
        }
        ui.horizontal(|ui| {
            ui.label("Search:");
            let response = ui.text_edit_singleline(&mut self.filter);
            anchors.push(anchor(AnchorId::FileFilter, &response));
            egui::ComboBox::from_id_source("file-sort")
                .selected_text(match self.sort {
                    SortMode::Name => "Name",
                    SortMode::Size => "Size",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sort, SortMode::Name, "Name");
                    ui.selectable_value(&mut self.sort, SortMode::Size, "Size");
                });
            if ui.button("Search recursively").clicked() {
                match self.policy.search(self.root_kind, self.filter.trim()) {
                    Ok(entries) => {
                        self.entries = entries;
                        self.sort_entries();
                        self.message = format!("{} entries shown", self.entries.len());
                    }
                    Err(error) => self.message = error.to_string(),
                }
            }
        });
        let root = self.policy.root(self.root_kind).ok();
        let relative = root
            .as_ref()
            .and_then(|root| self.current_dir.strip_prefix(root).ok())
            .unwrap_or(Path::new(""))
            .to_path_buf();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Root").clicked() {
                self.navigate(PathBuf::new());
            }
            let mut crumb = PathBuf::new();
            for component in relative.components() {
                let name = component.as_os_str().to_owned();
                crumb.push(&name);
                if ui.button(name.to_string_lossy()).clicked() {
                    self.navigate(crumb.clone());
                }
            }
            if !relative.as_os_str().is_empty() && ui.button("Parent").clicked() {
                self.navigate(relative.parent().unwrap_or(Path::new("")).to_path_buf());
            }
        });
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.new_name);
            if ui.button("New folder").clicked() && !self.new_name.trim().is_empty() {
                let path = relative.join(self.new_name.trim());
                self.action(self.policy.create_dir(self.root_kind, &path));
                self.new_name.clear();
            }
            if ui.button("New file").clicked() && !self.new_name.trim().is_empty() {
                let path = relative.join(self.new_name.trim());
                self.action(self.policy.create_file(self.root_kind, &path));
                self.new_name.clear();
            }
        });
        let filter = self.filter.to_ascii_lowercase();
        let mut selected = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for item in &self.entries {
                let name = item
                    .relative
                    .file_name()
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_default();
                if !filter.is_empty()
                    && !item
                        .relative
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .contains(&filter)
                {
                    continue;
                }
                let response = ui.selectable_label(
                    self.selected.as_ref() == Some(&item.path),
                    format!(
                        "{} {} ({})",
                        if item.is_dir { "DIR" } else { "FILE" },
                        name,
                        item.size
                    ),
                );
                anchors.push(anchor(AnchorId::FileList, &response));
                if response.clicked() {
                    selected = Some(item.clone());
                }
            }
        });
        if let Some(item) = selected {
            self.selected = Some(item.path.clone());
            ui.separator();
            ui.label(format!("Selected: {}", item.relative.display()));
            ui.horizontal(|ui| {
                if !item.is_dir && ui.button("Open in editor").clicked() {
                    self.open_request = Some(item.path.clone());
                }
                if ui.button("Copy path").clicked() {
                    copied = copy_to_clipboard(&item.path.display().to_string());
                }
                if ui.button("Delete").clicked() {
                    self.pending_delete = Some(item.path.clone());
                    self.message = "Confirm delete below".into();
                }
            });
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.rename_name);
                if ui.button("Rename or move").clicked() && !self.rename_name.trim().is_empty() {
                    if let Ok(root) = self.policy.root(self.root_kind) {
                        let from = item
                            .path
                            .strip_prefix(&root)
                            .unwrap_or(Path::new(""))
                            .to_path_buf();
                        let to = relative.join(self.rename_name.trim());
                        self.pending_rename = Some((item.path.clone(), root.join(to)));
                        self.rename_name.clear();
                        self.message =
                            format!("Confirm rename or move of {} below", from.display());
                    }
                }
            });
            if self.pending_delete.as_ref() == Some(&item.path) {
                ui.colored_label(egui::Color32::YELLOW, "Delete this entry and its contents?");
                if ui.button("Confirm delete").clicked() {
                    self.refresh();
                }
                if ui.button("Cancel delete").clicked() {
                    self.pending_delete = None;
                }
            }
            if self
                .pending_rename
                .as_ref()
                .is_some_and(|(from, _)| from == &item.path)
            {
                ui.colored_label(egui::Color32::YELLOW, "Confirm rename or move?");
                if ui.button("Confirm rename or move").clicked() {
                    self.refresh();
                }
                if ui.button("Cancel rename or move").clicked() {
                    self.pending_rename = None;
                }
            }
        }
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
fn copy_to_clipboard(text: &str) -> Option<String> {
    Some(
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_owned())) {
            Ok(()) => "Copied to clipboard".into(),
            Err(error) => format!("Clipboard error: {error}"),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roots_and_refresh_clear_stale_selection() {
        let root = std::env::temp_dir().join(format!("studio-browser-{}", std::process::id()));
        let data = root.join("data");
        let project = root.join("project");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(data.join("a.txt"), b"a").unwrap();
        let mut browser = FileBrowser::from_roots(data.clone(), project);
        browser.selected = Some(data.join("a.txt"));
        browser.refresh();
        assert!(browser.selected.is_none());
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn parent_navigation_is_root_bounded() {
        let root =
            std::env::temp_dir().join(format!("studio-browser-parent-{}", std::process::id()));
        let data = root.join("data");
        let project = root.join("project");
        std::fs::create_dir_all(data.join("one/two")).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        let mut browser = FileBrowser::from_roots(data, project);
        browser.navigate(PathBuf::from("one/two"));
        browser.navigate(PathBuf::from("one"));
        browser.navigate(PathBuf::new());
        assert!(browser.current_dir.ends_with("data"));
        let _ = std::fs::remove_dir_all(root);
    }
}
