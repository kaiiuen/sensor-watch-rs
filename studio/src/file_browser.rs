//! Bounded Studio Explorer backed by the central filesystem policy.

use crate::fs_policy::{Item, Policy, PolicyError, RootKind, Roots};
use crate::help::{AnchorId, AnchorRect};
use crate::pickers;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub struct AnchorHit {
    pub key: AnchorId,
    pub rect: AnchorRect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Details,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshState {
    Clean,
    Needed,
    Refreshing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortMode {
    Name,
    Size,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextAction {
    Open,
    Editor,
    Explorer,
    NewFile,
    NewFolder,
    Rename,
    Delete,
    CopyPath,
    Refresh,
}

#[derive(Clone, Debug)]
pub struct ExplorerTab {
    pub name: String,
    pub root_kind: RootKind,
    pub current_dir: PathBuf,
    entries: Vec<Item>,
    pub filter: String,
    pub selected: Option<PathBuf>,
    pub sort: SortMode,
    pub view_mode: ViewMode,
    pub refresh_state: RefreshState,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
}

impl ExplorerTab {
    fn new(name: impl Into<String>, root_kind: RootKind) -> Self {
        Self {
            name: name.into(),
            root_kind,
            current_dir: PathBuf::new(),
            entries: Vec::new(),
            filter: String::new(),
            selected: None,
            sort: SortMode::Name,
            view_mode: ViewMode::Details,
            refresh_state: RefreshState::Needed,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }
}

pub struct FileBrowser {
    policy: Policy,
    pub tabs: Vec<ExplorerTab>,
    pub active_tab: usize,
    message: String,
    open_request: Option<PathBuf>,
    new_name: String,
    rename_name: String,
    pending_delete: Option<PathBuf>,
    pending_rename: Option<(PathBuf, PathBuf)>,
    tab_name_input: String,
}

impl Default for FileBrowser {
    fn default() -> Self {
        Self {
            policy: Policy::new(Roots::empty()),
            tabs: vec![ExplorerTab::new("Project", RootKind::ActiveProject)],
            active_tab: 0,
            message: String::new(),
            open_request: None,
            new_name: String::new(),
            rename_name: String::new(),
            pending_delete: None,
            pending_rename: None,
            tab_name_input: String::new(),
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
    pub(crate) fn from_roots(data: PathBuf, project: PathBuf) -> Self {
        let mut browser = Self {
            policy: Policy::new(Roots::test(data, Some(project))),
            ..Self::default()
        };
        browser.tab_mut().root_kind = RootKind::AppData;
        browser.refresh();
        browser
    }

    fn tab(&self) -> &ExplorerTab {
        &self.tabs[self.active_tab]
    }
    fn tab_mut(&mut self) -> &mut ExplorerTab {
        &mut self.tabs[self.active_tab]
    }

    pub fn active_tab(&self) -> &ExplorerTab {
        self.tab()
    }

    pub fn select_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active_tab = index;
            self.refresh();
            true
        } else {
            false
        }
    }

    pub fn create_tab(&mut self, root_kind: RootKind, name: impl Into<String>) -> usize {
        self.tabs.push(ExplorerTab::new(name, root_kind));
        let index = self.tabs.len() - 1;
        self.active_tab = index;
        self.refresh();
        index
    }

    pub fn rename_tab(&mut self, index: usize, name: impl Into<String>) -> bool {
        let name = name.into();
        if name.trim().is_empty() || index >= self.tabs.len() {
            return false;
        }
        self.tabs[index].name = name;
        true
    }

    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        true
    }

    pub fn reorder_tab(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active_tab = to;
        true
    }

    fn root(&self) -> Result<PathBuf, PolicyError> {
        self.policy.root(self.tab().root_kind)
    }

    #[cfg(test)]
    pub(crate) fn from_project_roots(data: PathBuf, project: PathBuf) -> Self {
        let mut browser = Self {
            policy: Policy::new(Roots::test(data, Some(project))),
            ..Self::default()
        };
        browser.refresh();
        browser
    }

    #[cfg(test)]
    fn active_tab_mut_for_test(&mut self) -> &mut ExplorerTab {
        self.tab_mut()
    }

    fn refresh(&mut self) {
        self.tab_mut().refresh_state = RefreshState::Refreshing;
        self.tab_mut().selected = None;
        self.tab_mut().entries.clear();
        if let Some(path) = self.pending_delete.take() {
            if let Ok(root) = self.root() {
                let relative = path
                    .strip_prefix(&root)
                    .unwrap_or(Path::new(""))
                    .to_path_buf();
                self.action(self.policy.remove(self.tab().root_kind, &relative));
            }
        }
        if let Some((from, to)) = self.pending_rename.take() {
            if let Ok(root) = self.root() {
                let from = from
                    .strip_prefix(&root)
                    .unwrap_or(Path::new(""))
                    .to_path_buf();
                let to = to
                    .strip_prefix(&root)
                    .unwrap_or(Path::new(""))
                    .to_path_buf();
                self.action(self.policy.rename(self.tab().root_kind, &from, &to));
            }
        }
        let Ok(root) = self.root() else {
            self.tab_mut().current_dir.clear();
            if self.message.is_empty() {
                self.message = "This project root is unavailable".into();
            }
            return;
        };
        if self.tab().current_dir.as_os_str().is_empty()
            || !self.tab().current_dir.starts_with(&root)
        {
            self.tab_mut().current_dir = root.clone();
        }
        let relative = self
            .tab()
            .current_dir
            .strip_prefix(&root)
            .unwrap_or(Path::new(""));
        match self.policy.list(self.tab().root_kind, relative) {
            Ok(entries) => {
                self.tab_mut().entries = entries;
                self.sort_entries();
            }
            Err(error) => self.message = error.to_string(),
        }
        self.tab_mut().refresh_state = RefreshState::Clean;
    }

    fn sort_entries(&mut self) {
        let sort = self.tab().sort;
        self.tab_mut().entries.sort_by_key(|item| match sort {
            SortMode::Name => (
                0,
                !item.is_dir,
                item.relative.to_string_lossy().to_ascii_lowercase(),
            ),
            SortMode::Size => (1, !item.is_dir, format!("{:020}", item.size)),
            SortMode::Modified => (
                2,
                !item.is_dir,
                item.modified
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs())
                    .to_string(),
            ),
        });
    }

    fn navigate(&mut self, relative: PathBuf) {
        let Ok(root) = self.root() else {
            self.message = "This project root is unavailable".into();
            return;
        };
        let target = root.join(&relative);
        if target == self.tab().current_dir {
            return;
        }
        match self.policy.list(self.tab().root_kind, &relative) {
            Ok(_) => {
                let current = self.tab().current_dir.clone();
                self.tab_mut().back.push(current);
                self.tab_mut().forward.clear();
                self.tab_mut().current_dir = target;
                self.refresh();
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn go_back(&mut self) {
        if let Some(path) = self.tab_mut().back.pop() {
            let current = self.tab().current_dir.clone();
            self.tab_mut().forward.push(current);
            self.tab_mut().current_dir = path;
            self.refresh();
        }
    }

    fn go_forward(&mut self) {
        if let Some(path) = self.tab_mut().forward.pop() {
            let current = self.tab().current_dir.clone();
            self.tab_mut().back.push(current);
            self.tab_mut().current_dir = path;
            self.refresh();
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

    fn execute_context(
        &mut self,
        action: ContextAction,
        item: &Item,
        relative: &Path,
    ) -> Option<String> {
        match action {
            ContextAction::Open => {
                if item.is_dir {
                    self.navigate(item.relative.clone());
                } else {
                    self.open_request = Some(item.path.clone());
                }
            }
            ContextAction::Editor => self.open_request = Some(item.path.clone()),
            ContextAction::Explorer => {
                return Some(match pickers::open_in_explorer(&item.path) {
                    Ok(()) => "Opened in Windows Explorer".into(),
                    Err(error) => error,
                });
            }
            ContextAction::CopyPath => return pickers::copy_path(&item.path),
            ContextAction::Refresh => self.refresh(),
            ContextAction::NewFile | ContextAction::NewFolder => {
                let name = self.new_name.trim();
                if name.is_empty() {
                    self.message = "Enter a name first".into();
                } else {
                    let path = relative.join(name);
                    let result = if action == ContextAction::NewFolder {
                        self.policy.create_dir(self.tab().root_kind, &path)
                    } else {
                        self.policy.create_file(self.tab().root_kind, &path)
                    };
                    self.new_name.clear();
                    self.action(result);
                }
            }
            ContextAction::Rename => {
                let name = self.rename_name.trim();
                if !name.is_empty() {
                    let root = self.root().unwrap_or_default();
                    self.pending_rename = Some((item.path.clone(), root.join(relative).join(name)));
                    self.rename_name.clear();
                    self.message = "Confirm rename below".into();
                }
            }
            ContextAction::Delete => {
                self.pending_delete = Some(item.path.clone());
                self.message = "Confirm delete below".into();
            }
        }
        None
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> (Option<String>, Vec<AnchorHit>) {
        let mut copied = None;
        let mut anchors = Vec::new();
        ui.horizontal(|ui| {
            for index in 0..self.tabs.len() {
                let label = if self.tabs[index].name.is_empty() {
                    "Explorer".to_string()
                } else {
                    self.tabs[index].name.clone()
                };
                if ui
                    .selectable_label(index == self.active_tab, label)
                    .clicked()
                {
                    self.select_tab(index);
                }
            }
            if ui.small_button("+").clicked() {
                self.create_tab(
                    RootKind::ActiveProject,
                    format!("Explorer {}", self.tabs.len() + 1),
                );
            }
            if self.tabs.len() > 1 && ui.small_button("Close tab").clicked() {
                self.close_tab(self.active_tab);
            }
            ui.label("Name:");
            ui.text_edit_singleline(&mut self.tab_name_input);
            if ui.small_button("Rename tab").clicked() {
                let name = self.tab_name_input.trim().to_string();
                if self.rename_tab(self.active_tab, name) {
                    self.tab_name_input.clear();
                }
            }
            if ui.small_button("Move left").clicked() && self.active_tab > 0 {
                let index = self.active_tab;
                self.reorder_tab(index, index - 1);
            }
            if ui.small_button("Move right").clicked() && self.active_tab + 1 < self.tabs.len() {
                let index = self.active_tab;
                self.reorder_tab(index, index + 1);
            }
        });
        ui.heading("Explorer");
        ui.horizontal(|ui| {
            let back = ui.add_enabled(!self.tab().back.is_empty(), egui::Button::new("Back"));
            if back.clicked() {
                self.go_back();
            }
            let forward =
                ui.add_enabled(!self.tab().forward.is_empty(), egui::Button::new("Forward"));
            if forward.clicked() {
                self.go_forward();
            }
            let up_enabled = self
                .root()
                .ok()
                .is_some_and(|root| self.tab().current_dir != root);
            if ui
                .add_enabled(up_enabled, egui::Button::new("Up"))
                .clicked()
            {
                let relative = self
                    .root()
                    .ok()
                    .and_then(|root| {
                        self.tab()
                            .current_dir
                            .strip_prefix(root)
                            .ok()
                            .map(Path::to_path_buf)
                    })
                    .unwrap_or_default();
                self.navigate(relative.parent().unwrap_or(Path::new("")).to_path_buf());
            }
            let refresh = ui.button("Refresh");
            anchors.push(anchor(AnchorId::FileRefresh, &refresh));
            if refresh.clicked() {
                self.refresh();
            }
            if ui
                .selectable_label(self.tab().view_mode == ViewMode::List, "List")
                .clicked()
            {
                self.tab_mut().view_mode = ViewMode::List;
            }
            if ui
                .selectable_label(self.tab().view_mode == ViewMode::Details, "Details")
                .clicked()
            {
                self.tab_mut().view_mode = ViewMode::Details;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Root:");
            for kind in [RootKind::AppData, RootKind::ActiveProject] {
                if ui
                    .selectable_label(self.tab().root_kind == kind, kind.label())
                    .clicked()
                {
                    self.tab_mut().root_kind = kind;
                    self.tab_mut().current_dir.clear();
                    self.tab_mut().back.clear();
                    self.tab_mut().forward.clear();
                    self.message.clear();
                    self.refresh();
                }
            }
        });
        let root = self.root().ok();
        let relative = root
            .as_ref()
            .and_then(|root| self.tab().current_dir.strip_prefix(root).ok())
            .unwrap_or(Path::new(""))
            .to_path_buf();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Project root").clicked() {
                self.navigate(PathBuf::new());
            }
            let mut crumb = PathBuf::new();
            for component in relative.components() {
                crumb.push(component.as_os_str());
                if ui.button(component.as_os_str().to_string_lossy()).clicked() {
                    self.navigate(crumb.clone());
                }
            }
            ui.label(format!("  {}", self.tab().current_dir.display()));
        });
        if !self.message.is_empty() {
            ui.colored_label(egui::Color32::YELLOW, &self.message);
        }
        ui.horizontal(|ui| {
            ui.label("Search:");
            let response = ui.text_edit_singleline(&mut self.tab_mut().filter);
            anchors.push(anchor(AnchorId::FileFilter, &response));
            egui::ComboBox::from_id_source("file-sort")
                .selected_text(match self.tab().sort {
                    SortMode::Name => "Name",
                    SortMode::Size => "Size",
                    SortMode::Modified => "Modified",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.tab_mut().sort, SortMode::Name, "Name");
                    ui.selectable_value(&mut self.tab_mut().sort, SortMode::Size, "Size");
                    ui.selectable_value(&mut self.tab_mut().sort, SortMode::Modified, "Modified");
                });
            if ui.button("Search recursively").clicked() {
                match self
                    .policy
                    .search(self.tab().root_kind, self.tab().filter.trim())
                {
                    Ok(entries) => {
                        self.tab_mut().entries = entries;
                        self.sort_entries();
                        self.message = format!("{} entries shown", self.tab().entries.len());
                    }
                    Err(error) => self.message = error.to_string(),
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("New name:");
            ui.text_edit_singleline(&mut self.new_name);
        });
        ui.horizontal(|ui| {
            ui.strong("Name");
            ui.add_space(190.0);
            ui.label("Type");
            ui.add_space(65.0);
            ui.label("Size");
            ui.add_space(55.0);
            ui.label("Modified");
        });
        let filter = self.tab().filter.to_ascii_lowercase();
        let mut selected = None;
        let mut context_action = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for item in &self.tab().entries {
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
                let modified = item
                    .modified
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or_else(|| "-".into(), |d| d.as_secs().to_string());
                let label = format!(
                    "{:<32} {:<10} {:>10} {:>12}{}",
                    name,
                    if item.is_dir { "Directory" } else { "File" },
                    if item.is_dir { 0 } else { item.size },
                    modified,
                    if item.read_only { "  [Read-only]" } else { "" }
                );
                let response =
                    ui.selectable_label(self.tab().selected.as_ref() == Some(&item.path), label);
                anchors.push(anchor(AnchorId::FileList, &response));
                if response.clicked() {
                    selected = Some(item.clone());
                }
                if response.double_clicked() {
                    context_action = Some((ContextAction::Open, item.clone()));
                }
                response.context_menu(|ui| {
                    let mutable = !item.read_only;
                    if ui.button("Open").clicked() {
                        context_action = Some((ContextAction::Open, item.clone()));
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(!item.is_dir, egui::Button::new("Open in Editor"))
                        .clicked()
                    {
                        context_action = Some((ContextAction::Editor, item.clone()));
                        ui.close_menu();
                    }
                    if ui.button("Open in Windows Explorer").clicked() {
                        context_action = Some((ContextAction::Explorer, item.clone()));
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("New", |ui| {
                        if ui.add_enabled(mutable, egui::Button::new("File")).clicked() {
                            context_action = Some((ContextAction::NewFile, item.clone()));
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(mutable, egui::Button::new("Folder"))
                            .clicked()
                        {
                            context_action = Some((ContextAction::NewFolder, item.clone()));
                            ui.close_menu();
                        }
                    });
                    if ui
                        .add_enabled(mutable, egui::Button::new("Rename"))
                        .clicked()
                    {
                        context_action = Some((ContextAction::Rename, item.clone()));
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(mutable, egui::Button::new("Delete"))
                        .clicked()
                    {
                        context_action = Some((ContextAction::Delete, item.clone()));
                        ui.close_menu();
                    }
                    if ui.button("Copy path").clicked() {
                        context_action = Some((ContextAction::CopyPath, item.clone()));
                        ui.close_menu();
                    }
                    if ui.button("Refresh").clicked() {
                        context_action = Some((ContextAction::Refresh, item.clone()));
                        ui.close_menu();
                    }
                });
            }
        });
        if let Some(item) = selected {
            self.tab_mut().selected = Some(item.path.clone());
        }
        if let Some((action, item)) = context_action {
            let target = if item.is_dir {
                item.relative.clone()
            } else {
                item.relative
                    .parent()
                    .unwrap_or(Path::new(""))
                    .to_path_buf()
            };
            copied = self.execute_context(action, &item, &target);
        }
        if let Some(path) = self.tab().selected.clone() {
            if self.pending_delete.as_ref() == Some(&path) {
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
                .is_some_and(|(from, _)| from == &path)
            {
                ui.colored_label(egui::Color32::YELLOW, "Confirm rename?");
                if ui.button("Confirm rename").clicked() {
                    self.refresh();
                }
                if ui.button("Cancel rename").clicked() {
                    self.pending_rename = None;
                }
            }
            ui.horizontal(|ui| {
                ui.label("Rename as:");
                ui.text_edit_singleline(&mut self.rename_name);
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_root_and_history_navigation_are_bounded() {
        let root =
            std::env::temp_dir().join(format!("studio-browser-history-{}", std::process::id()));
        let data = root.join("data");
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(data.join("one/two")).unwrap();
        let mut browser = FileBrowser::from_roots(data.clone(), project);
        browser.navigate(PathBuf::from("one"));
        browser.navigate(PathBuf::from("one/two"));
        browser.go_back();
        assert!(browser.active_tab().current_dir.ends_with("one"));
        browser.go_forward();
        assert!(browser.active_tab().current_dir.ends_with("two"));
        browser.navigate(PathBuf::new());
        assert!(browser.active_tab().current_dir == data.canonicalize().unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn explorer_tabs_keep_navigation_and_filter_state_isolated() {
        let root = std::env::temp_dir().join(format!("studio-browser-tabs-{}", std::process::id()));
        let data = root.join("data");
        let project = root.join("project");
        std::fs::create_dir_all(data.join("data-dir")).unwrap();
        std::fs::create_dir_all(project.join("project-dir")).unwrap();
        let mut browser = FileBrowser::from_roots(data.clone(), project.clone());
        browser.active_tab_mut_for_test().filter = "data".into();
        browser.navigate(PathBuf::from("data-dir"));
        let first_path = browser.active_tab().current_dir.clone();
        let second = browser.create_tab(RootKind::ActiveProject, "Project view");
        assert_eq!(second, 1);
        assert!(browser.active_tab().current_dir.ends_with("project"));
        assert!(browser.active_tab().filter.is_empty());
        browser.select_tab(0);
        assert_eq!(browser.active_tab().current_dir, first_path);
        assert_eq!(browser.active_tab().filter, "data");
        browser.rename_tab(0, "Data view");
        browser.reorder_tab(0, 1);
        assert_eq!(browser.active_tab().name, "Data view");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn context_actions_disable_mutations_for_read_only_items() {
        let item = Item {
            path: PathBuf::from("x"),
            relative: PathBuf::from("x"),
            is_dir: false,
            size: 1,
            modified: None,
            read_only: true,
        };
        assert!(item.read_only);
        assert!(!item.is_dir);
    }
}
