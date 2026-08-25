//! Read-only source comparison for Studio.

use crate::faces::face_identity;
use crate::file_browser::FileBrowser;
use eframe::egui;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const MAX_COMPARE_LINES: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrangement {
    SideBySide,
    TopBottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineMarker {
    Unchanged,
    Added,
    Removed,
    Changed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunk {
    pub left_start: usize,
    pub right_start: usize,
    pub left_lines: Vec<String>,
    pub right_lines: Vec<String>,
    pub marker: LineMarker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareDocument {
    pub path: PathBuf,
    pub role: String,
    pub face_id: Option<String>,
    pub contents: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacePair {
    pub identity: String,
    pub left: PathBuf,
    pub right: PathBuf,
}

pub fn pair_face_sources<I, J>(left: I, right: J) -> Vec<FacePair>
where
    I: IntoIterator<Item = (String, PathBuf)>,
    J: IntoIterator<Item = (String, PathBuf)>,
{
    let right: Vec<_> = right.into_iter().collect();
    left.into_iter()
        .filter_map(|(name, left_path)| {
            let identity = face_identity(&name);
            right
                .iter()
                .find(|(other, _)| face_identity(other) == identity)
                .map(|(_, path)| FacePair {
                    identity,
                    left: left_path,
                    right: path.clone(),
                })
        })
        .collect()
}

pub fn diff_hunks(left: &str, right: &str) -> Vec<DiffHunk> {
    let a: Vec<_> = left
        .lines()
        .take(MAX_COMPARE_LINES)
        .map(str::to_owned)
        .collect();
    let b: Vec<_> = right
        .lines()
        .take(MAX_COMPARE_LINES)
        .map(str::to_owned)
        .collect();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut hunks = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
            continue;
        }
        let (ls, rs) = (i, j);
        let mut l = Vec::new();
        let mut r = Vec::new();
        while i < a.len() || j < b.len() {
            if i < a.len() && j < b.len() && a[i] == b[j] {
                break;
            }
            if j == b.len() || (i < a.len() && dp[i + 1][j] >= dp[i][j + 1]) {
                l.push(a[i].clone());
                i += 1;
            } else {
                r.push(b[j].clone());
                j += 1;
            }
        }
        let marker = match (l.is_empty(), r.is_empty()) {
            (true, false) => LineMarker::Added,
            (false, true) => LineMarker::Removed,
            _ => LineMarker::Changed,
        };
        hunks.push(DiffHunk {
            left_start: ls + 1,
            right_start: rs + 1,
            left_lines: l,
            right_lines: r,
            marker,
        });
    }
    hunks
}

fn sha256(contents: &str) -> String {
    Sha256::digest(contents.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Clone, Debug)]
pub struct CompareView {
    pub left: Option<CompareDocument>,
    pub right: Option<CompareDocument>,
    pub hunks: Vec<DiffHunk>,
    pub arrangement: Arrangement,
    pub sync_scroll: bool,
    pub left_offset: f32,
    pub right_offset: f32,
    pub left_max_offset: f32,
    pub right_max_offset: f32,
    pub selected_hunk: usize,
    pub allow_unrelated: bool,
    pub message: String,
}
impl Default for CompareView {
    fn default() -> Self {
        Self {
            left: None,
            right: None,
            hunks: Vec::new(),
            arrangement: Arrangement::SideBySide,
            sync_scroll: true,
            left_offset: 0.0,
            right_offset: 0.0,
            left_max_offset: 0.0,
            right_max_offset: 0.0,
            selected_hunk: 0,
            allow_unrelated: false,
            message: String::new(),
        }
    }
}
impl CompareView {
    pub fn open_paths(
        &mut self,
        browser: &FileBrowser,
        left: (&Path, &str, Option<&str>),
        right: (&Path, &str, Option<&str>),
    ) -> Result<(), String> {
        if !self.allow_unrelated
            && (left.2 != right.2 || (left.2.is_none() && right.2.is_none() && left.0 != right.0))
        {
            return Err("comparison requires matching face IDs; enable unrelated file comparison explicitly".into());
        }
        let load = |item: (&Path, &str, Option<&str>)| -> Result<CompareDocument, String> {
            let contents = browser
                .read_comparison_path(item.0)
                .map_err(|e| e.to_string())?;
            Ok(CompareDocument {
                path: item.0.to_path_buf(),
                role: item.1.into(),
                face_id: item.2.map(face_identity),
                sha256: sha256(&contents),
                contents,
            })
        };
        self.left = Some(load(left)?);
        self.right = Some(load(right)?);
        self.recompute();
        Ok(())
    }
    pub fn open_contents(
        &mut self,
        left: CompareDocument,
        right: CompareDocument,
    ) -> Result<(), String> {
        if !self.allow_unrelated
            && (left.face_id != right.face_id
                || (left.face_id.is_none() && right.face_id.is_none() && left.path != right.path))
        {
            return Err("comparison requires matching face IDs; enable unrelated file comparison explicitly".into());
        }
        self.left = Some(left);
        self.right = Some(right);
        self.recompute();
        Ok(())
    }
    pub fn recompute(&mut self) {
        self.hunks = match (&self.left, &self.right) {
            (Some(l), Some(r)) => diff_hunks(&l.contents, &r.contents),
            _ => Vec::new(),
        };
        self.selected_hunk = self.selected_hunk.min(self.hunks.len().saturating_sub(1));
    }
    pub fn next_change(&mut self) {
        if !self.hunks.is_empty() {
            self.selected_hunk = (self.selected_hunk + 1).min(self.hunks.len() - 1);
        }
    }
    pub fn previous_change(&mut self) {
        self.selected_hunk = self.selected_hunk.saturating_sub(1);
    }
    pub fn set_offsets(&mut self, left: f32, right: f32) {
        self.left_offset = left.clamp(0.0, self.left_max_offset);
        self.right_offset = right.clamp(0.0, self.right_max_offset);
        if self.sync_scroll {
            let position = self.left_offset.max(self.right_offset);
            self.left_offset = position.min(self.left_max_offset);
            self.right_offset = position.min(self.right_max_offset);
        }
    }
    pub fn set_max_offsets(&mut self, left: f32, right: f32) {
        self.left_max_offset = left.max(0.0);
        self.right_max_offset = right.max(0.0);
        self.set_offsets(self.left_offset, self.right_offset);
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Compare");
        ui.label("Read-only source comparison. No save, apply, or mutation actions are available.");
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.arrangement,
                Arrangement::SideBySide,
                "Side by side",
            );
            ui.selectable_value(&mut self.arrangement, Arrangement::TopBottom, "Top-bottom");
            ui.checkbox(&mut self.sync_scroll, "Synchronize scrolling");
            ui.checkbox(&mut self.allow_unrelated, "Allow unrelated files");
            if ui.button("Previous change").clicked() {
                self.previous_change();
            }
            if ui.button("Next change").clicked() {
                self.next_change();
            }
        });
        if let (Some(left), Some(right)) = (&self.left, &self.right) {
            self.header(ui, left, "LEFT");
            self.header(ui, right, "RIGHT");
            ui.label(format!("{} change hunks", self.hunks.len()));
            for (index, hunk) in self.hunks.iter().enumerate() {
                let marker = match hunk.marker {
                    LineMarker::Added => "ADDED",
                    LineMarker::Removed => "REMOVED",
                    LineMarker::Changed => "CHANGED",
                    LineMarker::Unchanged => "UNCHANGED",
                };
                ui.label(format!(
                    "Hunk {}: {marker} (left line {}, right line {})",
                    index + 1,
                    hunk.left_start,
                    hunk.right_start
                ));
            }
            match self.arrangement {
                Arrangement::SideBySide => {
                    ui.columns(2, |cols| {
                        egui::ScrollArea::vertical().show(&mut cols[0], |ui| {
                            ui.monospace(&left.contents);
                        });
                        egui::ScrollArea::vertical().show(&mut cols[1], |ui| {
                            ui.monospace(&right.contents);
                        });
                    });
                }
                Arrangement::TopBottom => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.monospace(&left.contents);
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.monospace(&right.contents);
                    });
                }
            }
        } else {
            ui.weak("Choose two safe text files from the Editor or File Browser.");
        }
    }
    fn header(&self, ui: &mut egui::Ui, d: &CompareDocument, side: &str) {
        ui.label(format!(
            "{side} - {} - role: {} - face ID: {} - SHA-256: {}",
            d.path.display(),
            d.role,
            d.face_id.as_deref().unwrap_or("none"),
            d.sha256
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    #[test]
    fn identity_pairing_is_case_insensitive() {
        let pairs = pair_face_sources(
            [(String::from("Simple_Clock"), PathBuf::from("a"))],
            [(String::from("SIMPLE_CLOCK"), PathBuf::from("b"))],
        );
        assert_eq!(pairs[0].identity, "simple_clock");
    }
    #[test]
    fn missing_sources_are_not_paired() {
        assert!(pair_face_sources(
            [(String::from("a"), PathBuf::from("a"))],
            Vec::<(String, PathBuf)>::new()
        )
        .is_empty());
    }
    #[test]
    fn hunks_mark_added_removed_changed() {
        assert_eq!(diff_hunks("a\nb", "a\nx\nb")[0].marker, LineMarker::Added);
        assert_eq!(diff_hunks("a\nb", "a")[0].marker, LineMarker::Removed);
        assert_eq!(diff_hunks("a", "b")[0].marker, LineMarker::Changed);
    }
    #[test]
    fn unequal_scroll_limits_stop_shorter_pane() {
        let mut v = CompareView::default();
        v.set_max_offsets(10.0, 100.0);
        v.set_offsets(80.0, 80.0);
        assert_eq!((v.left_offset, v.right_offset), (10.0, 80.0));
    }
    #[test]
    fn unrelated_sources_require_explicit_opt_in() {
        let mut v = CompareView::default();
        let left = CompareDocument {
            path: PathBuf::from("a"),
            role: "file".into(),
            face_id: None,
            contents: "a".into(),
            sha256: sha256("a"),
        };
        let right = CompareDocument {
            path: PathBuf::from("b"),
            role: "file".into(),
            face_id: None,
            contents: "b".into(),
            sha256: sha256("b"),
        };
        assert!(v.open_contents(left, right).is_err());
        v.allow_unrelated = true;
        assert!(v
            .open_contents(
                CompareDocument {
                    path: PathBuf::from("a"),
                    role: "file".into(),
                    face_id: None,
                    contents: "a".into(),
                    sha256: sha256("a")
                },
                CompareDocument {
                    path: PathBuf::from("b"),
                    role: "file".into(),
                    face_id: None,
                    contents: "b".into(),
                    sha256: sha256("b")
                }
            )
            .is_ok());
    }
    #[test]
    fn comparison_does_not_mutate_documents() {
        let mut v = CompareView::default();
        let left = CompareDocument {
            path: PathBuf::from("a"),
            role: "left".into(),
            face_id: Some("face".into()),
            contents: "a".into(),
            sha256: sha256("a"),
        };
        let right = CompareDocument {
            path: PathBuf::from("b"),
            role: "right".into(),
            face_id: Some("face".into()),
            contents: "b".into(),
            sha256: sha256("b"),
        };
        v.open_contents(left.clone(), right.clone()).unwrap();
        assert_eq!(v.left.unwrap().contents, left.contents);
        assert_eq!(v.right.unwrap().contents, right.contents);
    }
    #[test]
    fn sync_off_leaves_offsets_independent() {
        let mut v = CompareView {
            sync_scroll: false,
            ..Default::default()
        };
        v.set_max_offsets(100.0, 100.0);
        v.set_offsets(12.0, 88.0);
        assert_eq!((v.left_offset, v.right_offset), (12.0, 88.0));
    }
    #[test]
    fn arrangement_is_selectable() {
        let mut v = CompareView::default();
        v.arrangement = Arrangement::TopBottom;
        assert_eq!(v.arrangement, Arrangement::TopBottom);
    }
}
