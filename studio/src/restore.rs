//! Local restore points for Studio configuration and app state.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::settings::AppSettings;

pub const MAX_RESTORE_POINTS: usize = 12;
const FILE_NAME: &str = "studio-restore-points.json";

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

fn config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("FirmwareStudio");
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("firmware-studio");
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(".config").join("firmware-studio");
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn path() -> PathBuf {
    config_dir().join(FILE_NAME)
}

fn replace_existing(tmp: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    let backup = target.with_extension("json.previous");
    let had_old = target.exists();
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

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl RestoreStore {
    pub fn load() -> Self {
        let Ok(json) = std::fs::read_to_string(path()) else {
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
        let point: RestorePoint = serde_json::from_str(json).map_err(|e| e.to_string())?;
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
