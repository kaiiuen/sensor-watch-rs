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
                    if let Some(name_start) = rest.find("addr_of_mut!(") {
                        let after = &rest[name_start + "addr_of_mut!(".len()..];
                        if let Some(name_end) = after.find(')') {
                            let name = after[..name_end].to_string();
                            faces.push(FaceInfo {
                                index,
                                description: face_description(&name),
                                name: name.clone(),
                            });
                        }
                    }
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
