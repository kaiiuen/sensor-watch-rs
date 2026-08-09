//! Watch-face discovery.
//!
//! Scans the firmware source to enumerate the registered watch faces. This
//! gives the Watch Faces panel a live list of faces to enable/disable/reorder.

/// A discovered watch face.
pub struct FaceInfo {
    pub index: usize,
    pub name: String,
    /// A short description from the face's module doc comment, if available.
    pub description: String,
    /// A category label derived from the face name.
    pub category: &'static str,
}

/// Scans the firmware's `app_setup()` for registered faces.
///
/// This parses the `WATCH_FACES[N] = Some(&mut *core::ptr::addr_of_mut!(NAME));`
/// lines in `src/movement/mod.rs` to build the face list.
pub fn discover_faces() -> Vec<FaceInfo> {
    let mut faces = Vec::new();
    let path = crate::build::firmware_dir().join("src/movement/mod.rs");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return faces,
    };

    for line in content.lines() {
        let line = line.trim();
        // WATCH_FACES[0] = Some(&mut *core::ptr::addr_of_mut!(SIMPLE_CLOCK));
        if let Some(rest) = line.strip_prefix("WATCH_FACES[") {
            if let Some(idx_end) = rest.find(']') {
                if let Ok(index) = rest[..idx_end].parse::<usize>() {
                    if let Some(name_start) = rest.find("addr_of_mut!") {
                        let after = &rest[name_start + "addr_of_mut!(".len()..];
                        if let Some(name_end) = after.find(')') {
                            let name = after[..name_end].to_string();
                            faces.push(FaceInfo {
                                index,
                                description: face_description(&name),
                                category: face_category(&name),
                                name: name.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    // Also surface faces that only have a `pub mod <name>;` declaration but no
    // `WATCH_FACES[]` entry yet (e.g. a face the editor just registered). This
    // lets a freshly saved face appear in the catalog so it can be added to a
    // preset, even though it isn't wired into the firmware's arrays.
    let anchor_pos = content
        .find("use crate::movement::types::*;")
        .unwrap_or(content.len());
    let mut next_index = faces.iter().map(|f| f.index).max().map_or(0, |m| m + 1);
    // Scan only the `pub mod` block (everything before the `use ...` anchor) to
    // avoid picking up unrelated `pub mod` lines further down.
    for line in content[..anchor_pos].lines() {
        let line = line.trim();
        // pub mod simple_clock;
        if let Some(rest) = line.strip_prefix("pub mod ") {
            if let Some(name) = rest.strip_suffix(';') {
                let name = name.trim().to_string();
                if !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !faces.iter().any(|f| f.name == name)
                {
                    faces.push(FaceInfo {
                        index: next_index,
                        description: face_description(&name),
                        category: face_category(&name),
                        name: name.clone(),
                    });
                    next_index += 1;
                }
            }
        }
    }
    faces.sort_by_key(|f| f.index);
    faces
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
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("WATCH_FACES[") {
                if let Some(idx_end) = rest.find(']') {
                    if let Ok(index) = rest[..idx_end].parse::<usize>() {
                        if let Some(name_start) = rest.find("addr_of_mut!(") {
                            let after = &rest[name_start + "addr_of_mut!(".len()..];
                            if let Some(name_end) = after.find(')') {
                                let name = after[..name_end].to_string();
                                faces.push(FaceInfo {
                                    index,
                                    description: String::new(),
                                    category: "Other",
                                    name: name.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(faces.len(), 3);
        assert_eq!(faces[0].index, 0);
        assert_eq!(faces[0].name, "SIMPLE_CLOCK");
        assert_eq!(faces[2].index, 110);
        assert_eq!(faces[2].name, "SQUASH");
    }
}
