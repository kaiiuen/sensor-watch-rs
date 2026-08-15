//! Local restore points for Studio configuration and app state.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::settings::AppSettings;
use super::test_runtime;

pub const MAX_RESTORE_POINTS: usize = 12;

const MAX_RESTORE_JSON_BYTES: u64 = 1024 * 1024;
const MAX_RESTORE_TEXT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestorePoint {
    pub name: String,
    pub timestamp: u64,
    pub settings: AppSettings,
    pub board: String,
    pub active_preset: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RestoreStore {
    pub points: Vec<RestorePoint>,
}

pub fn path() -> PathBuf {
    test_runtime::active().restore
}

fn replace_existing(tmp: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    ensure_regular_or_absent(tmp)?;
    ensure_regular_or_absent(target)?;
    let backup = target.with_extension("json.previous");
    ensure_regular_or_absent(&backup)?;
    let had_old = target.is_file();
    if had_old {
        if backup.exists() {
            std::fs::remove_file(&backup)
                .map_err(|e| format!("cannot remove old restore backup: {e}"))?;
        }
        if let Err(error) = std::fs::rename(target, &backup) {
            return Err(format!("cannot stage existing restore file: {error}"));
        }
    }
    if let Err(error) = std::fs::rename(tmp, target) {
        if had_old {
            let _ = std::fs::rename(&backup, target);
        }
        let _ = std::fs::remove_file(tmp);
        return Err(format!("cannot install restore file: {error}"));
    }
    if had_old {
        std::fs::remove_file(&backup)
            .map_err(|e| format!("restore saved, but old backup could not be removed: {e}"))?;
    }
    Ok(())
}

fn ensure_regular_or_absent(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "refusing symlinked restore path: {}",
            path.display()
        )),
        Ok(metadata) if !metadata.is_file() => Err(format!(
            "restore path is not a regular file: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect restore path: {error}")),
    }
}

fn read_bounded(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "restore file is too large",
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "restore file is not UTF-8")
    })
}

fn validate_point(point: &RestorePoint) -> Result<(), String> {
    if point.name.len() > MAX_RESTORE_TEXT_BYTES || point.board.len() > MAX_RESTORE_TEXT_BYTES {
        return Err("restore point text is too long".into());
    }
    point.settings.validate()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl RestoreStore {
    pub fn load() -> Self {
        let Ok(json) = read_bounded(&path(), MAX_RESTORE_JSON_BYTES) else {
            return Self::default();
        };
        let Ok(mut store) = serde_json::from_str::<Self>(&json) else {
            return Self::default();
        };
        store.normalize();
        store
    }

    pub fn save(&mut self) -> Result<(), String> {
        self.normalize();
        let target = path();
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = target.with_extension("json.tmp");
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        replace_existing(&tmp, &target)
    }

    pub fn create(
        &mut self,
        name: impl Into<String>,
        settings: AppSettings,
        board: impl Into<String>,
        active_preset: usize,
    ) {
        self.points.push(RestorePoint {
            name: name.into(),
            timestamp: now(),
            settings,
            board: board.into(),
            active_preset,
        });
        self.normalize();
    }

    pub fn delete(&mut self, index: usize) {
        if index < self.points.len() {
            self.points.remove(index);
        }
    }

    pub fn rename(&mut self, index: usize, name: impl Into<String>) {
        if let Some(point) = self.points.get_mut(index) {
            point.name = name.into();
        }
    }

    pub fn normalize(&mut self) {
        self.points.retain(|point| validate_point(point).is_ok());
        self.points.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        self.points.truncate(MAX_RESTORE_POINTS);
    }

    pub fn export_json(&self, index: usize) -> Result<String, String> {
        self.points
            .get(index)
            .ok_or_else(|| "Restore point not found".to_string())
            .and_then(|p| serde_json::to_string_pretty(p).map_err(|e| e.to_string()))
    }

    pub fn import_json(&mut self, json: &str) -> Result<(), String> {
        if json.len() > MAX_RESTORE_JSON_BYTES as usize {
            return Err("restore point JSON is too large".into());
        }
        let point: RestorePoint = serde_json::from_str(json).map_err(|e| e.to_string())?;
        validate_point(&point)?;
        self.points.push(point);
        self.normalize();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(ts: u64) -> RestorePoint {
        RestorePoint {
            name: ts.to_string(),
            timestamp: ts,
            settings: AppSettings::default(),
            board: "Green".into(),
            active_preset: 0,
        }
    }

    #[test]
    fn keeps_newest_twelve_in_newest_first_order() {
        let mut store = RestoreStore {
            points: (0..20).map(point).collect(),
        };
        store.normalize();
        assert_eq!(store.points.len(), MAX_RESTORE_POINTS);
        assert_eq!(store.points.first().unwrap().timestamp, 19);
        assert_eq!(store.points.last().unwrap().timestamp, 8);
    }

    #[test]
    fn serializes_and_round_trips() {
        let original = point(42);
        let json = serde_json::to_string(&original).unwrap();
        let decoded: RestorePoint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.timestamp, 42);
        assert_eq!(decoded.board, "Green");
    }
}
