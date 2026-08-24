//! Native file and folder pickers used by path-based Studio inputs.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilePickerKind {
    Uf2,
    MasterClock,
    MasterClockNtpCatalog,
}

impl FilePickerKind {
    pub const fn filter(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Uf2 => ("UF2 firmware", &["uf2"]),
            Self::MasterClock => ("Master Clock executable", &["exe"]),
            Self::MasterClockNtpCatalog => ("Master Clock NTP catalog", &["json"]),
        }
    }

    fn accepts(self, path: &Path) -> bool {
        let name = path.file_name().and_then(|name| name.to_str());
        match self {
            Self::Uf2 => path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("uf2")),
            Self::MasterClock => name == Some("master-clock.exe"),
            Self::MasterClockNtpCatalog => path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json")),
        }
    }
}

pub fn start_directory(current: &str, fallback: &Path) -> PathBuf {
    let trimmed = current.trim();
    let current = Path::new(trimmed);
    if !trimmed.is_empty() && current.is_dir() {
        return current.to_path_buf();
    }
    if let Some(parent) = current
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if parent.is_dir() {
            return parent.to_path_buf();
        }
    }
    if fallback.is_dir() {
        return fallback.to_path_buf();
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn pick_file(kind: FilePickerKind, current: &str, fallback: &Path) -> Option<PathBuf> {
    let (label, extensions) = kind.filter();
    rfd::FileDialog::new()
        .set_directory(start_directory(current, fallback))
        .add_filter(label, extensions)
        .pick_file()
        .filter(|path| selected_file_is_allowed(kind, path))
}

pub fn pick_folder(current: &str, fallback: &Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_directory(start_directory(current, fallback))
        .pick_folder()
        .filter(|path| selected_folder_is_allowed(path))
}

pub fn selected_file_is_allowed(kind: FilePickerKind, path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    metadata.is_file() && !metadata.file_type().is_symlink() && kind.accepts(path)
}

pub fn selected_folder_is_allowed(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    metadata.is_dir() && !metadata.file_type().is_symlink()
}

/// The exact executable and argument vector used to open a validated path in
/// Windows Explorer. The path is never interpolated into a shell command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

pub fn explorer_command(path: &Path) -> Option<ExplorerCommand> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }
    let canonical = path.canonicalize().ok()?;
    let canonical_metadata = std::fs::symlink_metadata(&canonical).ok()?;
    if canonical_metadata.file_type().is_symlink() {
        return None;
    }
    Some(ExplorerCommand {
        program: PathBuf::from("explorer.exe"),
        args: vec![OsString::from(format!("/select,{}", canonical.display()))],
    })
}

pub fn copy_path(path: &Path) -> Option<String> {
    Some(
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(path.display().to_string())) {
            Ok(()) => "Copied path to clipboard".into(),
            Err(error) => format!("Clipboard error: {error}"),
        },
    )
}

pub fn open_in_explorer(path: &Path) -> Result<(), String> {
    let command = explorer_command(path).ok_or_else(|| "path is not safe to open".to_string())?;
    Command::new(&command.program)
        .args(&command.args)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open Windows Explorer: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_current_directory_then_parent_then_fallback() {
        let fallback = std::env::temp_dir();
        assert_eq!(start_directory("", &fallback), fallback);
        assert_eq!(start_directory("missing/file.uf2", &fallback), fallback);
        assert_eq!(
            start_directory(
                &fallback.join("missing/file.uf2").display().to_string(),
                &fallback
            ),
            fallback
        );
    }

    #[test]
    fn filters_match_picker_kind() {
        assert_eq!(FilePickerKind::Uf2.filter().1, &["uf2"]);
        assert_eq!(FilePickerKind::MasterClock.filter().1, &["exe"]);
        assert_eq!(FilePickerKind::MasterClockNtpCatalog.filter().1, &["json"]);
    }

    #[test]
    fn explorer_command_uses_validated_argument_vector() {
        let root = std::env::temp_dir().join("studio-explorer-command-tests");
        std::fs::create_dir_all(&root).unwrap();
        let selected = root.join("name with spaces & punctuation.txt");
        std::fs::write(&selected, b"x").unwrap();
        let command = explorer_command(&selected).unwrap();
        assert_eq!(command.program, PathBuf::from("explorer.exe"));
        assert_eq!(command.args.len(), 1);
        assert_eq!(
            command.args[0],
            OsString::from(format!(
                "/select,{}",
                selected.canonicalize().unwrap().display()
            ))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn explorer_command_rejects_missing_paths() {
        assert!(explorer_command(Path::new("missing-explorer-path")).is_none());
    }

    #[test]
    fn rejects_invalid_file_selections() {
        let root = std::env::temp_dir().join("studio-picker-tests");
        std::fs::create_dir_all(&root).unwrap();
        let uf2 = root.join("firmware.uf2");
        let exe = root.join("master-clock.exe");
        let other = root.join("other.exe");
        std::fs::write(&uf2, b"x").unwrap();
        std::fs::write(&exe, b"x").unwrap();
        std::fs::write(&other, b"x").unwrap();
        assert!(selected_file_is_allowed(FilePickerKind::Uf2, &uf2));
        assert!(!selected_file_is_allowed(FilePickerKind::Uf2, &exe));
        assert!(selected_file_is_allowed(FilePickerKind::MasterClock, &exe));
        assert!(!selected_file_is_allowed(
            FilePickerKind::MasterClock,
            &other
        ));
        let _ = std::fs::remove_dir_all(root);
    }
}
