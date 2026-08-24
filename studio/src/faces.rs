//! Watch-face discovery.
//!
//! Scans the firmware source to enumerate the registered watch faces. This
//! gives the Watch Faces panel a live list of faces to enable/disable/reorder.

/// Identifies whether a catalog entry comes from firmware or the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceKind {
    Registered,
    Editor,
}

pub struct FaceInfo {
    pub index: usize,
    pub name: String,
    /// A short description from the face's source file (the first `//!` doc comment line), if available.
    pub description: String,
    /// A category label derived from the face name.
    pub category: &'static str,
    pub kind: FaceKind,
}

/// Returns the one ASCII identity used for face names throughout Studio.
///
/// Face names are Rust identifiers, so ASCII folding is intentional and avoids
/// Unicode case-folding differences between source lookup and persisted data.
pub fn face_identity(name: &str) -> String {
    name.bytes()
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect()
}

fn valid_face_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Movement modules that support the firmware but are not watch faces.
const INFRASTRUCTURE_MODULES: &[&str] = &[
    "battery",
    "board",
    "debounce",
    "fault",
    "persist",
    "rtc_calibration_store",
    "shell_auth",
    "stats",
    "types",
];

fn is_infrastructure_module(name: &str) -> bool {
    INFRASTRUCTURE_MODULES
        .iter()
        .any(|module| face_identity(module) == face_identity(name))
}

fn make_face(index: usize, name: String, kind: FaceKind) -> FaceInfo {
    let category = match kind {
        FaceKind::Registered => face_category(&name),
        FaceKind::Editor => "Custom",
    };
    FaceInfo {
        index,
        description: face_description(&name),
        category,
        name,
        kind,
    }
}

fn discover_faces_from_content(content: &str) -> Vec<FaceInfo> {
    let mut faces = Vec::new();

    // Parse by occurrence rather than physical line: generated firmware may
    // split a WATCH_FACES assignment across several lines.
    let mut cursor = 0;
    while let Some(found) = content[cursor..].find("WATCH_FACES[") {
        let start = cursor + found;
        let rest = &content[start + "WATCH_FACES[".len()..];
        let Some(idx_end) = rest.find(']') else { break };
        let Ok(index) = rest[..idx_end].trim().parse::<usize>() else {
            cursor = start + 1;
            continue;
        };
        let end = content[start + 1..]
            .find("WATCH_FACES[")
            .map_or(content.len(), |next| start + 1 + next);
        let assignment = &content[start..end];
        if let Some(name_start) = assignment.find("addr_of_mut!(") {
            let after = &assignment[name_start + "addr_of_mut!(".len()..];
            if let Some(name_end) = after.find(')') {
                let name = after[..name_end].trim().to_string();
                if valid_face_identifier(&name) && !is_infrastructure_module(&name) {
                    faces.push(make_face(index, name, FaceKind::Registered));
                }
            }
        }
        cursor = start + "WATCH_FACES[".len();
    }

    // A declaration without a registry entry is an editor-created face. The
    // infrastructure allowlist prevents support modules from entering the
    // catalog while keeping arbitrary custom faces visible.
    let anchor_pos = content
        .find("use crate::movement::types::*;")
        .unwrap_or(content.len());
    let mut next_index = faces.iter().map(|f| f.index).max().map_or(0, |m| m + 1);
    for line in content[..anchor_pos].lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("pub mod ") {
            if let Some(name) = rest.strip_suffix(';') {
                let name = name.trim().to_string();
                if valid_face_identifier(&name)
                    && !is_infrastructure_module(&name)
                    && !faces
                        .iter()
                        .any(|f| face_identity(&f.name) == face_identity(&name))
                {
                    faces.push(make_face(next_index, name, FaceKind::Editor));
                    next_index += 1;
                }
            }
        }
    }
    faces.sort_by_key(|f| f.index);
    faces
}

/// Returns all registered firmware faces and safe editor-only face entries.
pub fn discover_faces() -> Vec<FaceInfo> {
    let path = crate::build::firmware_dir().join("src/movement/mod.rs");
    std::fs::read_to_string(&path)
        .map(|content| discover_faces_from_content(&content))
        .unwrap_or_default()
}

/// Reads a short description from a face's source file (the first `//!` doc
/// comment line), falling back to an empty string.
fn face_description(name: &str) -> String {
    let path = crate::build::firmware_dir()
        .join("src/movement")
        .join(format!("{name}.rs"));
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("//!") {
            let rest = rest.trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    String::new()
}

/// Classifies a face into a category based on its name.
fn face_category(name: &str) -> &'static str {
    let n = name.to_uppercase();
    if n.contains("CLOCK")
        || n.contains("TIME")
        || n.contains("WORLD")
        || n.contains("SOLAR")
        || n.contains("MARS")
        || n.contains("WEEK")
        || n.contains("BEATS")
    {
        "Time"
    } else if n.contains("ALARM")
        || n.contains("TIMER")
        || n.contains("STOPWATCH")
        || n.contains("COUNTDOWN")
        || n.contains("METRONOME")
    {
        "Timers & Alarms"
    } else if n.contains("GAME")
        || n.contains("SIMON")
        || n.contains("INVADERS")
        || n.contains("BLACKJACK")
        || n.contains("LANDER")
        || n.contains("TAROT")
        || n.contains("WORDLE")
        || n.contains("TOSS")
        || n.contains("COIN")
        || n.contains("HIGHER")
        || n.contains("ENDLESS")
        || n.contains("BUTTERFLY")
    {
        "Games"
    } else if n.contains("CALC")
        || n.contains("CONVERSION")
        || n.contains("MORSE")
        || n.contains("TOTP")
        || n.contains("DATABANK")
    {
        "Tools"
    } else if n.contains("THERM")
        || n.contains("TEMP")
        || n.contains("LIGHT")
        || n.contains("ACCEL")
        || n.contains("LIS2DW")
        || n.contains("BATTERY")
        || n.contains("VOLTAGE")
    {
        "Sensors"
    } else if n.contains("ASTRONOMY")
        || n.contains("MOON")
        || n.contains("SUNRISE")
        || n.contains("SOLSTICE")
        || n.contains("ORRERY")
        || n.contains("TIDE")
        || n.contains("PLANET")
    {
        "Astronomy"
    } else if n.contains("DIAGNOSTIC")
        || n.contains("SETTINGS")
        || n.contains("PREFERENCE")
        || n.contains("FINETUNE")
        || n.contains("FREQUENCY")
        || n.contains("SAVE")
        || n.contains("SET_TIME")
    {
        "System"
    } else {
        "Other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_face_lines() {
        let content = "\n        if WATCH_FACES[0].is_none() {\n            WATCH_FACES[0] = Some(&mut *core::ptr::addr_of_mut!(SIMPLE_CLOCK));\n            WATCH_FACES[1] = Some(&mut *core::ptr::addr_of_mut!(COUNTDOWN));\n            WATCH_FACES[110] = Some(&mut *core::ptr::addr_of_mut!(SQUASH));\n        }\n";
        let mut faces = Vec::new();
        let mut cursor = 0;
        while let Some(start) = content[cursor..].find("WATCH_FACES[") {
            let start = cursor + start;
            let rest = &content[start + "WATCH_FACES[".len()..];
            let idx_end = rest.find(']').unwrap();
            let index = rest[..idx_end].trim().parse::<usize>().unwrap();
            let end = content[start + 1..]
                .find("WATCH_FACES[")
                .map_or(content.len(), |next| start + 1 + next);
            let assignment = &content[start..end];
            if let Some(name_start) = assignment.find("addr_of_mut!(") {
                let after = &assignment[name_start + "addr_of_mut!(".len()..];
                if let Some(name_end) = after.find(')') {
                    faces.push(FaceInfo {
                        index,
                        description: String::new(),
                        category: "Other",
                        name: after[..name_end].trim().to_string(),
                        kind: FaceKind::Registered,
                    });
                }
            }
            cursor = start + "WATCH_FACES[".len();
        }
        assert_eq!(faces.len(), 3);
        assert_eq!(faces[0].index, 0);
        assert_eq!(faces[0].name, "SIMPLE_CLOCK");
        assert_eq!(faces[2].index, 110);
        assert_eq!(faces[2].name, "SQUASH");
    }

    #[test]
    fn current_catalog_has_111_registered_faces() {
        let catalog = discover_faces();
        let registered: Vec<_> = catalog
            .iter()
            .filter(|face| face.kind == FaceKind::Registered)
            .collect();
        assert_eq!(registered.len(), 111);
        assert_eq!(catalog.len(), 111);
        assert!(registered
            .iter()
            .any(|face| face.name == "ACCELEROMETER_DATA_ACQUISITION"));
        let identities: std::collections::HashSet<_> = registered
            .iter()
            .map(|face| face_identity(&face.name))
            .collect();
        assert_eq!(identities.len(), registered.len());
    }

    #[test]
    fn infrastructure_modules_are_not_faces() {
        let content = "pub mod battery;\npub mod board;\npub mod debounce;\npub mod fault;\npub mod persist;\npub mod rtc_calibration_store;\npub mod shell_auth;\npub mod stats;\npub mod types;\npub mod editor_face;\nWATCH_FACES[0] = Some(&mut *core::ptr::addr_of_mut!(battery));\nuse crate::movement::types::*;";
        let faces = discover_faces_from_content(content);
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].name, "editor_face");
        assert_eq!(faces[0].kind, FaceKind::Editor);
        assert_eq!(faces[0].category, "Custom");
    }

    #[test]
    fn editor_modules_are_classified_separately() {
        let content = "pub mod custom_editor_face;\nuse crate::movement::types::*;";
        let faces = discover_faces_from_content(content);
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].kind, FaceKind::Editor);
        assert_eq!(faces[0].category, "Custom");
    }

    #[test]
    fn multiline_registry_merges_case_only_module_duplicate() {
        let content = "pub mod simple_clock;\npub mod ACCELEROMETER_DATA_ACQUISITION;\n\nWATCH_FACES[0] = Some(\n    &mut *core::ptr::addr_of_mut!(SIMPLE_CLOCK)\n);\nWATCH_FACES[1] = Some(&mut *core::ptr::addr_of_mut!(ACCELEROMETER_DATA_ACQUISITION));";
        let mut names = Vec::new();
        let mut cursor = 0;
        while let Some(start) = content[cursor..].find("WATCH_FACES[") {
            let start = cursor + start;
            let end = content[start + 1..]
                .find("WATCH_FACES[")
                .map_or(content.len(), |n| start + 1 + n);
            let assignment = &content[start..end];
            let name_start = assignment.find("addr_of_mut!(").unwrap();
            let after = &assignment[name_start + "addr_of_mut!(".len()..];
            names.push(after[..after.find(')').unwrap()].trim().to_string());
            cursor = start + 1;
        }
        assert_eq!(names, ["SIMPLE_CLOCK", "ACCELEROMETER_DATA_ACQUISITION"]);
        assert_eq!(face_identity("Simple_Clock"), face_identity("SIMPLE_CLOCK"));
    }
}
