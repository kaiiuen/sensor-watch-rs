//! Markdown notes stored below the validated Studio app-data root.

use crate::editor::DocumentTab;
use crate::fs_policy::{Policy, PolicyError, RootKind};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_NOTE_BYTES: usize = 512 * 1024;
const NOTE_SCHEMA_VERSION: u32 = 1;
const NOTES_DIR: &str = "notes";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum NoteCategory {
    Project,
    Hardware,
    Firmware,
    Build,
    Testing,
    Ideas,
    Archive,
}

impl NoteCategory {
    pub const ALL: [Self; 7] = [
        Self::Project,
        Self::Hardware,
        Self::Firmware,
        Self::Build,
        Self::Testing,
        Self::Ideas,
        Self::Archive,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Hardware => "Hardware",
            Self::Firmware => "Firmware",
            Self::Build => "Build",
            Self::Testing => "Testing",
            Self::Ideas => "Ideas",
            Self::Archive => "Archive",
        }
    }

    pub fn from_label(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|category| category.label().eq_ignore_ascii_case(value))
            .unwrap_or(Self::Archive)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NoteHeader {
    pub schema_version: u32,
    pub title: String,
    pub category: NoteCategory,
}

impl Default for NoteHeader {
    fn default() -> Self {
        Self {
            schema_version: NOTE_SCHEMA_VERSION,
            title: "Untitled note".into(),
            category: NoteCategory::Project,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSummary {
    pub path: PathBuf,
    pub title: String,
    pub category: NoteCategory,
}

#[derive(Clone, Debug)]
pub struct Notepad {
    policy: Policy,
    pub notes: Vec<NoteSummary>,
    pub tabs: Vec<DocumentTab>,
    pub active: Option<usize>,
    pub category_filter: Option<NoteCategory>,
    pub search: String,
    pub preview: bool,
    pub title_input: String,
    pub category_input: NoteCategory,
    pub status: String,
    pub delete_request: Option<usize>,
    pub rename_request: Option<String>,
}

impl Default for Notepad {
    fn default() -> Self {
        Self {
            policy: Policy::new(crate::fs_policy::Roots::empty()),
            notes: Vec::new(),
            tabs: Vec::new(),
            active: None,
            category_filter: None,
            search: String::new(),
            preview: false,
            title_input: String::new(),
            category_input: NoteCategory::Project,
            status: String::new(),
            delete_request: None,
            rename_request: None,
        }
    }
}

impl Notepad {
    #[cfg(test)]
    fn from_roots(data: PathBuf) -> Self {
        let mut notepad = Self {
            policy: Policy::new(crate::fs_policy::Roots::test(data, None)),
            ..Self::default()
        };
        notepad.refresh();
        notepad
    }

    pub fn new() -> Self {
        let mut notepad = Self {
            policy: Policy::new(crate::fs_policy::Roots::from_distribution(
                &crate::distribution::active(),
            )),
            ..Self::default()
        };
        notepad.refresh();
        notepad
    }

    fn ensure_note_dirs(&self) -> Result<(), PolicyError> {
        let root = self.policy.root(RootKind::AppData)?;
        let notes = root.join(NOTES_DIR);
        if !notes.exists() {
            self.policy
                .create_dir(RootKind::AppData, Path::new(NOTES_DIR))?;
        }
        let _ = root;
        Ok(())
    }

    fn note_relative(&self, path: &Path) -> Result<PathBuf, String> {
        let root = self
            .policy
            .root(RootKind::AppData)
            .map_err(|e| e.to_string())?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "note is outside app data".to_string())?;
        if !relative.starts_with(NOTES_DIR) {
            return Err("note is outside notes root".into());
        }
        Ok(relative.to_path_buf())
    }

    pub fn refresh(&mut self) {
        self.notes.clear();
        let result = self.ensure_note_dirs().and_then(|_| {
            let mut found = Vec::new();
            let dir = Path::new(NOTES_DIR);
            for item in self.policy.list(RootKind::AppData, dir)? {
                if !item.is_dir && item.path.extension().is_some_and(|ext| ext == "md") {
                    let source = self
                        .policy
                        .read_markdown(RootKind::AppData, &item.relative)?;
                    let (header, _) = parse_note(&source)?;
                    found.push(NoteSummary {
                        path: item.path,
                        title: header.title,
                        category: header.category,
                    });
                }
            }
            Ok::<_, PolicyError>(found)
        });
        match result {
            Ok(notes) => self.notes = notes,
            Err(error) => self.status = error.to_string(),
        }
    }

    fn filtered_notes(&self) -> Vec<NoteSummary> {
        let query = self.search.trim().to_ascii_lowercase();
        self.notes
            .iter()
            .filter(|note| self.category_filter.is_none_or(|c| c == note.category))
            .filter(|note| {
                query.is_empty()
                    || note.title.to_ascii_lowercase().contains(&query)
                    || note
                        .path
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .contains(&query)
            })
            .cloned()
            .collect()
    }

    pub fn create(&mut self, title: &str, category: NoteCategory) -> Result<(), String> {
        let slug = safe_slug(title)?;
        self.ensure_note_dirs().map_err(|e| e.to_string())?;
        let relative = Path::new(NOTES_DIR).join(format!("{slug}.md"));
        let header = NoteHeader {
            schema_version: NOTE_SCHEMA_VERSION,
            title: title.trim().to_string(),
            category,
        };
        let body = format!("# {}\n\n", title.trim());
        let source = serialize_note(&header, &body);
        self.policy
            .create_file(RootKind::AppData, &relative)
            .map_err(|e| e.to_string())?;
        self.policy
            .write_markdown(RootKind::AppData, &relative, &source, Some(""))
            .map_err(|e| e.to_string())?;
        self.open_relative(&relative)?;
        self.refresh();
        Ok(())
    }

    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let relative = self.note_relative(path)?;
        self.open_relative(&relative)
    }

    fn open_relative(&mut self, relative: &Path) -> Result<(), String> {
        let contents = self
            .policy
            .read_markdown(RootKind::AppData, relative)
            .map_err(|e| e.to_string())?;
        if contents.len() > MAX_NOTE_BYTES {
            return Err("note exceeds the Markdown size limit".into());
        }
        let path = self
            .policy
            .root(RootKind::AppData)
            .map_err(|e| e.to_string())?
            .join(relative);
        if let Some(index) = self.tabs.iter().position(|tab| tab.path == path) {
            self.active = Some(index);
        } else {
            self.tabs.push(DocumentTab::new(path, contents));
            self.active = Some(self.tabs.len() - 1);
        }
        Ok(())
    }

    pub fn save_active(&mut self) -> Result<(), String> {
        let index = self.active.ok_or_else(|| "no note is open".to_string())?;
        let tab = self
            .tabs
            .get_mut(index)
            .ok_or_else(|| "note tab is missing".to_string())?;
        if tab.contents.len() > MAX_NOTE_BYTES {
            return Err("note exceeds the Markdown size limit".into());
        }
        let source = tab.contents.clone();
        parse_note(&source).map_err(|error| error.to_string())?;
        let path = tab.path.clone();
        let original = tab.original_contents().to_string();
        let result = self.policy.write_markdown(
            RootKind::AppData,
            path.strip_prefix(
                self.policy
                    .root(RootKind::AppData)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|_| "note is outside notes root")?,
            &source,
            Some(&original),
        );
        result.map_err(|e| e.to_string())?;
        tab.mark_saved();
        self.refresh();
        Ok(())
    }

    pub fn check_conflict(&mut self) {
        if let Some(index) = self.active {
            if let Some(tab) = self.tabs.get_mut(index) {
                let changed = self
                    .policy
                    .read_text(
                        RootKind::AppData,
                        tab.path
                            .strip_prefix(self.policy.root(RootKind::AppData).unwrap_or_default())
                            .unwrap_or(Path::new("")),
                    )
                    .map(|source| editor_hash(&source) != tab.original_hash)
                    .unwrap_or(true);
                tab.external_conflict = changed;
            }
        }
    }

    pub fn rename_active(&mut self, title: &str) -> Result<(), String> {
        let index = self.active.ok_or_else(|| "no note is open".to_string())?;
        let tab = self
            .tabs
            .get(index)
            .ok_or_else(|| "note tab is missing".to_string())?;
        if tab.dirty || tab.external_conflict {
            return Err("save or resolve the note conflict before renaming".into());
        }
        let slug = safe_slug(title)?;
        let old_relative = tab
            .path
            .strip_prefix(
                self.policy
                    .root(RootKind::AppData)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|_| "note is outside notes root")?;
        let new_relative = Path::new(NOTES_DIR).join(format!("{slug}.md"));
        self.policy
            .rename(RootKind::AppData, old_relative, &new_relative)
            .map_err(|e| e.to_string())?;
        self.tabs[index].path = self
            .policy
            .root(RootKind::AppData)
            .map_err(|e| e.to_string())?
            .join(new_relative);
        self.refresh();
        Ok(())
    }

    pub fn confirm_rename(&mut self, title: &str) -> Result<(), String> {
        self.rename_active(title)
    }

    pub fn delete(&mut self, index: usize) -> Result<(), String> {
        let path = self
            .notes
            .get(index)
            .ok_or_else(|| "note is missing".to_string())?
            .path
            .clone();
        self.delete_path(&path)
    }

    fn delete_tab(&mut self, index: usize) -> Result<(), String> {
        let path = self
            .tabs
            .get(index)
            .ok_or_else(|| "note tab is missing".to_string())?
            .path
            .clone();
        self.delete_path(&path)
    }

    fn delete_path(&mut self, path: &Path) -> Result<(), String> {
        let note = self
            .notes
            .iter()
            .find(|note| note.path == path)
            .ok_or_else(|| "note is missing".to_string())?;
        let relative = note
            .path
            .strip_prefix(
                self.policy
                    .root(RootKind::AppData)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|_| "note is outside notes root")?;
        self.policy
            .remove(RootKind::AppData, relative)
            .map_err(|e| e.to_string())?;
        self.tabs.retain(|tab| tab.path != path);
        self.active = self.active.filter(|active| *active < self.tabs.len());
        self.refresh();
        Ok(())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.check_conflict();
        ui.heading("Notepad");
        ui.label("Markdown notes are stored only below the Studio app-local data root.");
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.text_edit_singleline(&mut self.search);
            egui::ComboBox::from_id_source("note-category-filter")
                .selected_text(self.category_filter.map_or("All", NoteCategory::label))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.category_filter, None, "All");
                    for category in NoteCategory::ALL {
                        ui.selectable_value(
                            &mut self.category_filter,
                            Some(category),
                            category.label(),
                        );
                    }
                });
            ui.checkbox(&mut self.preview, "Preview");
        });
        ui.horizontal(|ui| {
            ui.label("New title:");
            ui.text_edit_singleline(&mut self.title_input);
            egui::ComboBox::from_id_source("new-note-category")
                .selected_text(self.category_input.label())
                .show_ui(ui, |ui| {
                    for category in NoteCategory::ALL {
                        ui.selectable_value(&mut self.category_input, category, category.label());
                    }
                });
            if ui.button("Create").clicked() {
                let title = self.title_input.trim().to_string();
                match self.create(&title, self.category_input) {
                    Ok(()) => self.title_input.clear(),
                    Err(error) => self.status = error,
                }
            }
        });
        if !self.status.is_empty() {
            ui.colored_label(egui::Color32::YELLOW, &self.status);
        }
        ui.separator();
        ui.horizontal(|ui| {
            for index in 0..self.tabs.len() {
                let name = self.tabs[index]
                    .path
                    .file_stem()
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_default();
                ui.selectable_value(
                    &mut self.active,
                    Some(index),
                    format!("{}{}", name, if self.tabs[index].dirty { " *" } else { "" }),
                );
            }
        });
        let filtered = self.filtered_notes();
        ui.columns(2, |columns| {
            egui::ScrollArea::vertical().show(&mut columns[0], |ui| {
                for note in filtered {
                    if ui
                        .selectable_label(
                            self.active
                                .is_some_and(|index| self.tabs[index].path == note.path),
                            format!("{} [{}]", note.title, note.category.label()),
                        )
                        .clicked()
                    {
                        if let Err(error) = self.open(&note.path) {
                            self.status = error;
                        }
                    }
                }
            });
            if let Some(index) = self.active {
                if let Some(tab) = self.tabs.get_mut(index) {
                    if tab.external_conflict {
                        columns[1].colored_label(
                            egui::Color32::RED,
                            "External change detected. Reload or save after review.",
                        );
                    }
                    if self.preview {
                        columns[1].label(&tab.contents);
                    } else {
                        let before = tab.contents.clone();
                        columns[1].add(
                            egui::TextEdit::multiline(&mut tab.contents)
                                .desired_rows(24)
                                .code_editor(),
                        );
                        if tab.contents != before {
                            tab.set_contents(tab.contents.clone());
                        }
                    }
                    columns[1].horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            let _ = self.save_active();
                        }
                        if ui.button("Rename").clicked() {
                            let title = self.title_input.trim().to_string();
                            self.rename_request = Some(title);
                        }
                        if ui.button("Delete").clicked() {
                            self.delete_request = Some(index);
                        }
                    });
                }
            } else {
                columns[1].weak("Select or create a Markdown note.");
            }
        });
        if let Some(title) = self.rename_request.clone() {
            ui.separator();
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("Rename this note to {title:?}?"),
            );
            if ui.button("Confirm rename").clicked() {
                if let Err(error) = self.confirm_rename(&title) {
                    self.status = error;
                }
                self.rename_request = None;
            }
            if ui.button("Cancel rename").clicked() {
                self.rename_request = None;
            }
        }
        if let Some(index) = self.delete_request {
            ui.separator();
            ui.colored_label(
                egui::Color32::RED,
                "Delete this note? This cannot be undone.",
            );
            if ui.button("Confirm delete").clicked() {
                if let Err(error) = self.delete_tab(index) {
                    self.status = error;
                }
                self.delete_request = None;
            }
            if ui.button("Cancel").clicked() {
                self.delete_request = None;
            }
        }
    }
}

fn safe_slug(title: &str) -> Result<String, String> {
    let trimmed = title.trim();
    if trimmed.is_empty() || trimmed.len() > 80 || !trimmed.is_ascii() {
        return Err("note title must be 1 to 80 ASCII characters".into());
    }
    let slug: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        Err("note title must contain an ASCII letter or number".into())
    } else {
        Ok(slug)
    }
}

fn serialize_note(header: &NoteHeader, body: &str) -> String {
    let json = serde_json::to_string(header).expect("note header is serializable");
    format!("<!-- studio-note: {json} -->\n{body}")
}

fn parse_note(source: &str) -> Result<(NoteHeader, String), PolicyError> {
    let first = source.lines().next().unwrap_or_default();
    if let Some(json) = first
        .strip_prefix("<!-- studio-note: ")
        .and_then(|line| line.strip_suffix(" -->"))
    {
        let mut header: NoteHeader =
            serde_json::from_str(json).map_err(|e| PolicyError::Io(e.to_string()))?;
        if header.schema_version == 0 {
            header.schema_version = NOTE_SCHEMA_VERSION;
        }
        if header.schema_version > NOTE_SCHEMA_VERSION {
            return Err(PolicyError::Io("unsupported note schema version".into()));
        }
        return Ok((
            header,
            source[first.len()..].trim_start_matches('\n').to_string(),
        ));
    }
    Ok((
        NoteHeader {
            title: "Migrated note".into(),
            category: NoteCategory::Archive,
            ..Default::default()
        },
        source.to_string(),
    ))
}

fn editor_hash(source: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(source.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn all_categories_are_serializable_and_unknowns_migrate_to_archive() {
        for category in NoteCategory::ALL {
            let json = serde_json::to_string(&category).unwrap();
            assert_eq!(NoteCategory::from_label(&json.replace('"', "")), category);
        }
        let (header, body) = parse_note("# old note").unwrap();
        assert_eq!(header.category, NoteCategory::Archive);
        assert_eq!(body, "# old note");
    }

    #[test]
    fn note_header_round_trips_as_markdown() {
        let source = serialize_note(
            &NoteHeader {
                title: "Build".into(),
                category: NoteCategory::Build,
                ..Default::default()
            },
            "# Build\n",
        );
        let (header, body) = parse_note(&source).unwrap();
        assert_eq!(header.category, NoteCategory::Build);
        assert_eq!(body, "# Build\n");
    }

    #[test]
    fn note_titles_cannot_escape_the_notes_root() {
        assert!(safe_slug("../outside").is_ok());
        assert!(safe_slug("..\\outside").is_ok());
        assert!(safe_slug("\u{2603}").is_err());
        assert!(safe_slug("").is_err());
    }

    #[test]
    fn note_size_is_bounded() {
        assert!(MAX_NOTE_BYTES > 0);
        assert!(MAX_NOTE_BYTES <= crate::fs_policy::MAX_TEXT_BYTES as usize);
    }

    #[test]
    fn category_create_and_external_conflict_are_safe() {
        let executable = std::env::current_exe().unwrap();
        let executable_id = sha2::Sha256::digest(executable.to_string_lossy().as_bytes());
        let root = std::env::temp_dir().join(format!(
            "studio-notepad-{}-{executable_id:x}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut notepad = Notepad::from_roots(root.clone());
        notepad.create("Build plan", NoteCategory::Project).unwrap();
        assert_eq!(notepad.notes[0].category, NoteCategory::Project);
        let path = notepad.tabs[0].path.clone();
        notepad.tabs[0].set_contents("local edit".into());
        std::fs::write(&path, "external edit").unwrap();
        notepad.check_conflict();
        assert!(notepad.tabs[0].external_conflict);
        assert!(notepad.save_active().is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
