use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_INFO_UF2_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WatchDriveCandidate {
    pub(crate) root: PathBuf,
    pub(crate) info: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WatchDriveSelection {
    None,
    One(WatchDriveCandidate),
    Multiple(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FlashRequest {
    pub(crate) path: PathBuf,
    pub(crate) approved: crate::build::ArtifactInspection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlashStatus {
    HostCopySucceeded,
    ArtifactInvalid,
    ArtifactChanged,
    NoWatch,
    Ambiguous,
    DriveDisappeared,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FlashResult {
    pub(crate) status: FlashStatus,
    pub(crate) message: String,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkerState {
    Idle,
    Detecting,
    Flashing,
}

#[cfg_attr(not(test), allow(dead_code))]
impl WorkerState {
    pub(crate) fn start_detection(&mut self) -> bool {
        if *self != Self::Idle {
            return false;
        }
        *self = Self::Detecting;
        true
    }

    pub(crate) fn start_flash(&mut self) -> bool {
        if *self != Self::Idle {
            return false;
        }
        *self = Self::Flashing;
        true
    }

    pub(crate) fn finish(&mut self) {
        *self = Self::Idle;
    }
}

pub(crate) fn windows_drive_roots() -> impl Iterator<Item = PathBuf> {
    ('A'..='Z').map(|drive| PathBuf::from(format!("{drive}:\\")))
}

/// Finds Sensor Watch UF2 drives using one identity predicate for both display
/// detection and flashing. The roots are injected to keep selection testable.
pub(crate) fn select_watch_drive<I>(roots: I) -> WatchDriveSelection
where
    I: IntoIterator<Item = PathBuf>,
{
    let candidates = roots
        .into_iter()
        .filter_map(|root| {
            let entries = std::fs::read_dir(&root).ok()?;
            entries.flatten().find_map(|entry| {
                if !entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("info_uf2.txt")
                {
                    return None;
                }
                let info = read_info_uf2(entry.path())?;
                crate::probe::is_watch_info(&info).then_some(WatchDriveCandidate {
                    root: root.clone(),
                    info,
                })
            })
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => WatchDriveSelection::None,
        [candidate] => WatchDriveSelection::One(candidate.clone()),
        _ => WatchDriveSelection::Multiple(candidates.len()),
    }
}

pub(crate) fn flash(request: FlashRequest) -> FlashResult {
    flash_with_start(request, || {})
}

fn flash_with_start<F>(request: FlashRequest, on_start: F) -> FlashResult
where
    F: FnOnce(),
{
    // This function intentionally owns every artifact and removable-drive
    // filesystem operation. A blocked OS syscall cannot be forcibly cancelled
    // from a Rust thread, but it cannot block eframe's UI thread either.
    on_start();
    let inspection = match crate::build::inspect_artifact(&request.path) {
        Ok(inspection) => inspection,
        Err(error) => {
            return FlashResult {
                status: FlashStatus::ArtifactInvalid,
                message: format!("Refusing to copy invalid artifact: {error}"),
            };
        }
    };
    if inspection != request.approved {
        return FlashResult {
            status: FlashStatus::ArtifactChanged,
            message: "Refusing to copy: approved artifact changed since approval".to_string(),
        };
    }
    let data = match std::fs::read(&request.path) {
        Ok(data) => data,
        Err(error) => {
            return FlashResult {
                status: FlashStatus::ArtifactInvalid,
                message: format!("Refusing to copy artifact: {error}"),
            };
        }
    };
    match select_watch_drive(windows_drive_roots()) {
        WatchDriveSelection::Multiple(count) => FlashResult {
            status: FlashStatus::Ambiguous,
            message: format!(
                "Refusing to flash: {count} Sensor Watch bootloader drives are present; disconnect all but one"
            ),
        },
        WatchDriveSelection::None => FlashResult {
            status: FlashStatus::NoWatch,
            message: "Watch not found (is it in bootloader mode?)".to_string(),
        },
        WatchDriveSelection::One(candidate) => write_to_drive(&candidate.root, &data),
    }
}

fn write_to_drive(root: &Path, data: &[u8]) -> FlashResult {
    write_to_drive_with_read(root, data, |path| std::fs::read(path))
}

fn write_to_drive_with_read<F>(root: &Path, data: &[u8], mut read: F) -> FlashResult
where
    F: FnMut(&Path) -> std::io::Result<Vec<u8>>,
{
    let dest_path = root.join("CURRENT.UF2");
    let temp_path = root.join(".current.uf2.tmp");
    let backup_path = root.join(".current.uf2.previous");
    let dest = dest_path.display().to_string();

    let result = (|| -> Result<(), FlashResult> {
        regular_or_absent(&dest_path)?;
        regular_or_absent(&temp_path)?;
        regular_or_absent(&backup_path)?;
        remove_if_present(&temp_path)?;
        std::fs::write(&temp_path, data).map_err(classify_io)?;
        let written = read(&temp_path).map_err(classify_io)?;
        if written != data || sensor_watch_core::uf2::validate(&written).is_err() {
            return Err(FlashResult {
                status: FlashStatus::Failed,
                message: format!("Failed to verify temporary UF2 at {dest}"),
            });
        }

        let had_old = dest_path.is_file();
        if had_old {
            if backup_path.exists() {
                std::fs::remove_file(&backup_path).map_err(classify_io)?;
            }
            std::fs::rename(&dest_path, &backup_path).map_err(classify_io)?;
        }
        if let Err(error) = std::fs::rename(&temp_path, &dest_path) {
            if had_old {
                let _ = std::fs::rename(&backup_path, &dest_path);
            }
            return Err(classify_io(error));
        }

        let published = match read(&dest_path) {
            Ok(published) => published,
            Err(error) => {
                let read_result = classify_io(error);
                let rollback_result = rollback_published(&dest_path, &backup_path, had_old);
                return Err(match rollback_result {
                    Ok(()) => read_result,
                    Err(rollback_error) => FlashResult {
                        status: FlashStatus::Failed,
                        message: format!(
                            "{}; rollback failed: {}",
                            read_result.message, rollback_error.message
                        ),
                    },
                });
            }
        };
        if published != data || sensor_watch_core::uf2::validate(&published).is_err() {
            let _ = std::fs::remove_file(&dest_path);
            if had_old {
                let _ = std::fs::rename(&backup_path, &dest_path);
            }
            return Err(FlashResult {
                status: FlashStatus::Failed,
                message: format!("Failed to verify published UF2 at {dest}"),
            });
        }
        if had_old {
            let _ = std::fs::remove_file(&backup_path);
        }
        Ok(())
    })();

    match result {
        Ok(()) => FlashResult {
            status: FlashStatus::HostCopySucceeded,
            message: format!(
                "Flashed to {dest} (host copy complete; the drive may disconnect next)"
            ),
        },
        Err(result) => {
            let _ = std::fs::remove_file(&temp_path);
            result
        }
    }
}

fn rollback_published(
    dest_path: &Path,
    backup_path: &Path,
    had_old: bool,
) -> Result<(), FlashResult> {
    remove_if_present(dest_path)?;
    if had_old {
        std::fs::rename(backup_path, dest_path).map_err(classify_io)?;
    }
    Ok(())
}

fn classify_io(error: std::io::Error) -> FlashResult {
    let status = if error.kind() == std::io::ErrorKind::NotFound {
        FlashStatus::DriveDisappeared
    } else {
        FlashStatus::Failed
    };
    FlashResult {
        status,
        message: format!("Flash filesystem operation failed: {error}"),
    }
}

fn remove_if_present(path: &Path) -> Result<(), FlashResult> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(classify_io(error)),
    }
}

fn regular_or_absent(path: &Path) -> Result<(), FlashResult> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(FlashResult {
            status: FlashStatus::Failed,
            message: format!("Refusing symlinked path: {}", path.display()),
        }),
        Ok(metadata) if !metadata.is_file() => Err(FlashResult {
            status: FlashStatus::Failed,
            message: format!("Path is not a regular file: {}", path.display()),
        }),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(classify_io(error)),
    }
}

fn read_info_uf2(path: PathBuf) -> Option<String> {
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_INFO_UF2_BYTES
    {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_INFO_UF2_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_INFO_UF2_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_selection_is_deterministic_and_ambiguous_is_refused() {
        let roots = [temp_drive("one"), temp_drive("two")];
        for (root, info) in roots.iter().zip(["UF2 Sensor Watch", "UF2 Sensor Watch"]) {
            std::fs::create_dir_all(root).unwrap();
            std::fs::write(root.join("INFO_UF2.TXT"), info).unwrap();
        }
        assert_eq!(
            select_watch_drive(roots.clone()),
            WatchDriveSelection::Multiple(2)
        );
        for root in roots {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn operation_state_refuses_overlapping_detection_and_flash() {
        let mut state = WorkerState::Idle;
        assert!(state.start_detection());
        assert!(!state.start_flash());
        state.finish();
        assert!(state.start_flash());
        assert!(!state.start_detection());
    }

    #[test]
    fn error_status_preserves_missing_drive_semantics() {
        let result = classify_io(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(result.status, FlashStatus::DriveDisappeared);
        assert!(result.message.contains("filesystem operation failed"));
    }

    #[test]
    fn published_read_failure_restores_old_artifact_and_preserves_sidecars() {
        let root = temp_drive("published-read-failure-old");
        std::fs::create_dir_all(&root).unwrap();
        let old = b"old UF2";
        let data = sensor_watch_core::uf2::convert_to_uf2(&[1, 2, 3]);
        std::fs::write(root.join("CURRENT.UF2"), old).unwrap();
        std::fs::write(root.join("CURRENT.UF2.json"), b"old manifest").unwrap();
        std::fs::write(root.join("CURRENT.json.sig"), b"old signature").unwrap();

        let result = write_to_drive_with_read(&root, &data, |path| {
            if path.file_name().and_then(|name| name.to_str()) == Some("CURRENT.UF2") {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            } else {
                std::fs::read(path)
            }
        });

        assert_eq!(result.status, FlashStatus::Failed);
        assert_eq!(std::fs::read(root.join("CURRENT.UF2")).unwrap(), old);
        assert_eq!(
            std::fs::read(root.join("CURRENT.UF2.json")).unwrap(),
            b"old manifest"
        );
        assert_eq!(
            std::fs::read(root.join("CURRENT.json.sig")).unwrap(),
            b"old signature"
        );
        assert!(!root.join(".current.uf2.previous").exists());
        assert!(!root.join(".current.uf2.tmp").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn published_read_failure_removes_new_artifact_without_old_file() {
        let root = temp_drive("published-read-failure-new");
        std::fs::create_dir_all(&root).unwrap();
        let data = sensor_watch_core::uf2::convert_to_uf2(&[4, 5, 6]);

        let result = write_to_drive_with_read(&root, &data, |path| {
            if path.file_name().and_then(|name| name.to_str()) == Some("CURRENT.UF2") {
                Err(std::io::Error::from(std::io::ErrorKind::NotFound))
            } else {
                std::fs::read(path)
            }
        });

        assert_eq!(result.status, FlashStatus::DriveDisappeared);
        assert!(!root.join("CURRENT.UF2").exists());
        assert!(!root.join(".current.uf2.previous").exists());
        assert!(!root.join(".current.uf2.tmp").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn worker_starts_before_artifact_filesystem_access_and_keeps_caller_responsive() {
        use std::sync::mpsc;
        use std::time::Duration;

        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let path = PathBuf::from("missing-worker-order.uf2");
        let approved = crate::build::ArtifactInspection {
            path: path.clone(),
            generation: String::new(),
            family_id: String::new(),
            uf2_bytes: String::new(),
            uf2_blocks: String::new(),
            payload_bytes: String::new(),
            sha256: String::new(),
            payload_sha256: String::new(),
            manifest_digest: String::new(),
        };
        let handle = std::thread::spawn(move || {
            flash_with_start(FlashRequest { path, approved }, || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        });

        // The worker has entered before it attempts to read the missing path.
        // The test thread remains immediately usable while that worker is held.
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!handle.is_finished());
        release_tx.send(()).unwrap();
        assert_eq!(handle.join().unwrap().status, FlashStatus::ArtifactInvalid);
    }

    fn temp_drive(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sensor-watch-flash-{name}-{}", std::process::id()))
    }
}
