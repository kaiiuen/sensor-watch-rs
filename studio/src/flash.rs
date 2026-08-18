use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::progress::{Phase, ProgressSink};

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

/// Copy-time guard used before a flash worker is scheduled. The worker repeats
/// all checks, but the UI must not queue a copy from stale approval or drive
/// state either.
pub(crate) fn validate_copy_guard(
    approved: &crate::build::ArtifactInspection,
    current: Result<&crate::build::ArtifactInspection, &str>,
    selection: &WatchDriveSelection,
) -> Result<WatchDriveCandidate, String> {
    let current = current.map_err(|error| format!("artifact is no longer available: {error}"))?;
    if current != approved {
        return Err(format!(
            "approved artifact digest changed (approved {}, current {})",
            approved.sha256, current.sha256
        ));
    }
    match selection {
        WatchDriveSelection::One(candidate) => Ok(candidate.clone()),
        WatchDriveSelection::None => Err("one expected Sensor Watch drive is required".into()),
        WatchDriveSelection::Multiple(count) => Err(format!(
            "exactly one expected Sensor Watch drive is required; found {count}"
        )),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FlashRequest {
    pub(crate) path: PathBuf,
    pub(crate) approved: crate::build::ArtifactInspection,
    /// Identity observed by the UI when Flash was clicked, if detection was unique.
    pub(crate) selected_drive: Option<WatchDriveCandidate>,
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
    let roots = ('A'..='Z').map(|drive| PathBuf::from(format!("{drive}:\\")));
    #[cfg(windows)]
    {
        return removable_drive_roots(roots, crate::probe::is_removable_drive).into_iter();
    }
    #[cfg(not(windows))]
    {
        roots
    }
}

fn removable_drive_roots<I, F>(roots: I, is_removable: F) -> Vec<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
    F: Fn(&Path) -> bool,
{
    roots
        .into_iter()
        .filter(|root| is_removable(root))
        .collect()
}

/// Finds Sensor Watch UF2 drives using one identity predicate for both display
/// detection and flashing. The roots are injected to keep selection testable.
pub(crate) fn select_watch_drive<I>(roots: I) -> WatchDriveSelection
where
    I: IntoIterator<Item = PathBuf>,
{
    select_watch_drive_with_progress(roots, &ProgressSink::disabled())
}

pub(crate) fn select_watch_drive_with_progress<I>(
    roots: I,
    progress: &ProgressSink,
) -> WatchDriveSelection
where
    I: IntoIterator<Item = PathBuf>,
{
    let roots = roots.into_iter().collect::<Vec<_>>();
    let total = roots.len() as u64;
    progress.emit(
        Phase::DriveEnumeration,
        format!("Enumerating {} drive roots", roots.len()),
        Some(0),
        Some(total),
    );
    let candidates = roots
        .into_iter()
        .enumerate()
        .filter_map(|(index, root)| {
            progress.emit(
                Phase::DriveEnumeration,
                format!(
                    "Checking drive root {} of {}: {}",
                    index + 1,
                    total,
                    root.display()
                ),
                Some(index as u64 + 1),
                Some(total),
            );
            let entries = std::fs::read_dir(&root).ok()?;
            entries.flatten().find_map(|entry| {
                if !entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("info_uf2.txt")
                {
                    return None;
                }
                progress.emit(
                    Phase::Identity,
                    format!("Reading INFO_UF2 at {}", entry.path().display()),
                    None,
                    None,
                );
                let info = read_info_uf2(entry.path())?;
                progress.emit(
                    Phase::Identity,
                    "INFO_UF2 identity read and revalidated",
                    None,
                    None,
                );
                crate::probe::is_watch_info(&info).then_some(WatchDriveCandidate {
                    root: root.clone(),
                    info,
                })
            })
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => {
            progress.emit(
                Phase::Selection,
                "No Sensor Watch drive selected",
                Some(0),
                Some(0),
            );
            WatchDriveSelection::None
        }
        [candidate] => {
            progress.emit(
                Phase::Selection,
                format!(
                    "One Sensor Watch drive selected: {}",
                    candidate.root.display()
                ),
                Some(1),
                Some(1),
            );
            WatchDriveSelection::One(candidate.clone())
        }
        _ => {
            progress.emit(
                Phase::Selection,
                format!("Multiple Sensor Watch drives found: {}", candidates.len()),
                Some(candidates.len() as u64),
                Some(candidates.len() as u64),
            );
            WatchDriveSelection::Multiple(candidates.len())
        }
    }
}

pub(crate) fn flash(request: FlashRequest) -> FlashResult {
    flash_with_start_progress(request, || {}, &ProgressSink::disabled())
}

fn flash_with_start<F>(request: FlashRequest, on_start: F) -> FlashResult
where
    F: FnOnce(),
{
    flash_with_start_progress(request, on_start, &ProgressSink::disabled())
}

pub(crate) fn flash_with_start_progress<F>(
    request: FlashRequest,
    on_start: F,
    progress: &ProgressSink,
) -> FlashResult
where
    F: FnOnce(),
{
    // This function intentionally owns every artifact and removable-drive
    // filesystem operation. A blocked OS syscall cannot be forcibly cancelled
    // from a Rust thread, but it cannot block eframe's UI thread either.
    on_start();
    progress.emit(
        Phase::Artifact,
        format!("Artifact selected: {}", request.path.display()),
        None,
        None,
    );
    let data = match read_verified_artifact(&request, progress, read_file, || {}) {
        Ok(data) => data,
        Err(result) => return result,
    };
    match select_watch_drive_with_progress(windows_drive_roots(), progress) {
        WatchDriveSelection::Multiple(count) => {
            progress.emit(
                Phase::Failure,
                format!("Flash refused: multiple drives ({count})"),
                None,
                None,
            );
            FlashResult {
            status: FlashStatus::Ambiguous,
            message: format!(
                "Refusing to flash: {count} Sensor Watch bootloader drives are present. Disconnect all but one"
            ),
            }
        }
        WatchDriveSelection::None => {
            progress.emit(Phase::Failure, "Flash refused: no watch drive", None, None);
            FlashResult {
                status: FlashStatus::NoWatch,
                message: "Watch not found (is it in bootloader mode?)".to_string(),
            }
        }
        WatchDriveSelection::One(candidate) => {
            if let Some(selected_drive) = &request.selected_drive {
                if !drive_identity_matches(selected_drive, &candidate) {
                    progress.emit(
                        Phase::Failure,
                        "Flash refused: selected drive identity changed",
                        None,
                        None,
                    );
                    return FlashResult {
                        status: FlashStatus::Failed,
                        message:
                            "Refusing to flash: the selected watch drive changed or was replaced"
                                .to_string(),
                    };
                }
            }
            write_to_drive_with_identity(
                Some(&candidate),
                &candidate.root,
                &data,
                |path| std::fs::read(path),
                progress,
            )
        }
    }
}

fn read_verified_artifact<R, A>(
    request: &FlashRequest,
    progress: &ProgressSink,
    mut read: R,
    after_inspection: A,
) -> Result<Vec<u8>, FlashResult>
where
    R: FnMut(&Path) -> std::io::Result<Vec<u8>>,
    A: FnOnce(),
{
    let data = read(&request.path).map_err(|error| {
        progress.emit(
            Phase::Failure,
            format!("Artifact read failed: {error}"),
            None,
            None,
        );
        FlashResult {
            status: FlashStatus::ArtifactInvalid,
            message: format!("Refusing to copy artifact: {error}"),
        }
    })?;
    progress.emit(
        Phase::Artifact,
        format!("Artifact bytes loaded: {} bytes", data.len()),
        Some(data.len() as u64),
        Some(data.len() as u64),
    );

    let inspected = crate::build::inspect_artifact(&request.path).map_err(|error| {
        progress.emit(
            Phase::Failure,
            format!("Artifact validation failed: {error}"),
            None,
            None,
        );
        FlashResult {
            status: FlashStatus::ArtifactInvalid,
            message: format!("Refusing to copy invalid artifact: {error}"),
        }
    })?;
    after_inspection();
    let revalidated = crate::build::inspect_artifact(&request.path).map_err(|error| {
        progress.emit(
            Phase::Failure,
            format!("Artifact revalidation failed: {error}"),
            None,
            None,
        );
        FlashResult {
            status: FlashStatus::ArtifactChanged,
            message: format!("Refusing to copy: approved artifact changed ({error})"),
        }
    })?;
    progress.emit(
        Phase::Revalidation,
        "UF2, manifest, sidecar, and approval metadata revalidated",
        None,
        None,
    );

    if inspected != revalidated || revalidated != request.approved {
        progress.emit(
            Phase::Failure,
            "Artifact approval binding failed: artifact or sidecar changed",
            None,
            None,
        );
        return Err(FlashResult {
            status: FlashStatus::ArtifactChanged,
            message: "Refusing to copy: approved artifact or sidecar changed since approval"
                .to_string(),
        });
    }

    let parsed = sensor_watch_core::uf2::validate(&data).map_err(|error| {
        progress.emit(
            Phase::Failure,
            format!("Artifact byte validation failed: {error}"),
            None,
            None,
        );
        FlashResult {
            status: FlashStatus::ArtifactChanged,
            message: format!("Refusing to copy: approved artifact bytes changed ({error})"),
        }
    })?;
    let uf2_sha256 = sha256_hex(&data);
    let payload_sha256 = sha256_hex(&parsed.image);
    if !uf2_sha256.eq_ignore_ascii_case(&request.approved.sha256)
        || !payload_sha256.eq_ignore_ascii_case(&request.approved.payload_sha256)
        || !uf2_sha256.eq_ignore_ascii_case(&revalidated.sha256)
        || !payload_sha256.eq_ignore_ascii_case(&revalidated.payload_sha256)
    {
        progress.emit(
            Phase::Failure,
            "Artifact approval binding failed: UF2 or payload bytes changed",
            None,
            None,
        );
        return Err(FlashResult {
            status: FlashStatus::ArtifactChanged,
            message: "Refusing to copy: approved UF2 or payload bytes changed since approval"
                .to_string(),
        });
    }
    Ok(data)
}

fn read_file(path: &Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_to_drive(root: &Path, data: &[u8]) -> FlashResult {
    write_to_drive_with_read(root, data, |path| std::fs::read(path))
}

fn write_to_drive_with_read<F>(root: &Path, data: &[u8], read: F) -> FlashResult
where
    F: FnMut(&Path) -> std::io::Result<Vec<u8>>,
{
    write_to_drive_with_progress(root, data, read, &ProgressSink::disabled())
}

fn write_to_drive_with_progress<F>(
    root: &Path,
    data: &[u8],
    read: F,
    progress: &ProgressSink,
) -> FlashResult
where
    F: FnMut(&Path) -> std::io::Result<Vec<u8>>,
{
    write_to_drive_with_identity(None, root, data, read, progress)
}

fn write_to_drive_with_identity<F>(
    candidate: Option<&WatchDriveCandidate>,
    root: &Path,
    data: &[u8],
    mut read: F,
    progress: &ProgressSink,
) -> FlashResult
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
        progress.emit(
            Phase::Transfer,
            format!("Writing temporary UF2: {} bytes", data.len()),
            Some(0),
            Some(data.len() as u64),
        );
        std::fs::write(&temp_path, data).map_err(classify_io)?;
        let written = read(&temp_path).map_err(classify_io)?;
        progress.emit(
            Phase::Transfer,
            format!("Temporary UF2 readback verified: {} bytes", written.len()),
            Some(written.len() as u64),
            Some(data.len() as u64),
        );
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
            progress.emit(Phase::Transfer, "Staging previous CURRENT.UF2", None, None);
            std::fs::rename(&dest_path, &backup_path).map_err(classify_io)?;
        }
        if let Some(expected) = candidate {
            let Some(info) = read_info_uf2(root.join("INFO_UF2.TXT")) else {
                if had_old {
                    let _ = std::fs::rename(&backup_path, &dest_path);
                }
                return Err(FlashResult {
                    status: FlashStatus::DriveDisappeared,
                    message: "Refusing to publish: watch identity is unavailable".to_string(),
                });
            };
            let actual = WatchDriveCandidate {
                root: root.to_path_buf(),
                info,
            };
            if !drive_identity_matches(expected, &actual) {
                if had_old {
                    let _ = std::fs::rename(&backup_path, &dest_path);
                }
                return Err(FlashResult {
                    status: FlashStatus::Failed,
                    message:
                        "Refusing to publish: the selected watch drive changed or was replaced"
                            .to_string(),
                });
            }
            progress.emit(
                Phase::Identity,
                "INFO_UF2 identity revalidated immediately before publication",
                None,
                None,
            );
        }
        progress.emit(
            Phase::Transfer,
            "Publishing temporary UF2 by rename",
            None,
            None,
        );
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
                progress.emit(
                    Phase::Rollback,
                    "Rollback started after published readback failure",
                    None,
                    None,
                );
                let rollback_result = rollback_published(&dest_path, &backup_path, had_old);
                progress.emit(
                    Phase::Rollback,
                    if rollback_result.is_ok() {
                        "Rollback succeeded"
                    } else {
                        "Rollback failed"
                    },
                    None,
                    None,
                );
                return Err(match rollback_result {
                    Ok(()) => read_result,
                    Err(rollback_error) => FlashResult {
                        status: FlashStatus::Failed,
                        message: format!(
                            "{}. Rollback failed: {}",
                            read_result.message, rollback_error.message
                        ),
                    },
                });
            }
        };
        progress.emit(
            Phase::Revalidation,
            format!("Published UF2 readback: {} bytes", published.len()),
            Some(published.len() as u64),
            Some(data.len() as u64),
        );
        if published != data || sensor_watch_core::uf2::validate(&published).is_err() {
            progress.emit(
                Phase::Rollback,
                "Rollback started after invalid published UF2",
                None,
                None,
            );
            let _ = std::fs::remove_file(&dest_path);
            if had_old {
                let _ = std::fs::rename(&backup_path, &dest_path);
            }
            progress.emit(Phase::Rollback, "Rollback completed", None, None);
            return Err(FlashResult {
                status: FlashStatus::Failed,
                message: format!("Failed to verify published UF2 at {dest}"),
            });
        }
        if had_old {
            progress.emit(
                Phase::Cleanup,
                "Cleaning up staged previous file",
                None,
                None,
            );
            let _ = std::fs::remove_file(&backup_path);
        }
        progress.emit(
            Phase::Complete,
            "Flash copy succeeded. Drive may disconnect next",
            Some(data.len() as u64),
            Some(data.len() as u64),
        );
        Ok(())
    })();

    match result {
        Ok(()) => FlashResult {
            status: FlashStatus::HostCopySucceeded,
            message: format!(
                "Flashed to {dest} (host copy complete. The drive may disconnect next)"
            ),
        },
        Err(result) => {
            let phase = if result.status == FlashStatus::DriveDisappeared {
                "Drive disappeared during flash"
            } else {
                result.message.as_str()
            };
            progress.emit(Phase::Failure, phase, None, None);
            progress.emit(Phase::Cleanup, "Cleaning up temporary file", None, None);
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

fn drive_identity_matches(expected: &WatchDriveCandidate, actual: &WatchDriveCandidate) -> bool {
    expected == actual && crate::probe::is_watch_info(&actual.info)
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
    fn removable_root_filter_rejects_fixed_drive_and_accepts_removable_watch() {
        let fixed = PathBuf::from("C:\\");
        let removable = temp_drive("removable-watch");
        std::fs::create_dir_all(&removable).unwrap();
        std::fs::write(
            removable.join("INFO_UF2.TXT"),
            "UF2 Bootloader; Board-ID: Sensor Watch Green",
        )
        .unwrap();

        let roots =
            removable_drive_roots([fixed.clone(), removable.clone()], |root| root == removable);
        assert_eq!(roots, vec![removable.clone()]);
        assert_eq!(
            select_watch_drive(roots),
            WatchDriveSelection::One(WatchDriveCandidate {
                root: removable.clone(),
                info: "UF2 Bootloader; Board-ID: Sensor Watch Green".to_string(),
            })
        );
        assert!(removable_drive_roots([fixed], |_| false).is_empty());
        let _ = std::fs::remove_dir_all(removable);
    }

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
    fn copy_guard_requires_current_artifact_digest_and_one_drive() {
        let approved = crate::build::ArtifactInspection {
            path: PathBuf::from("firmware.uf2"),
            generation: "g1".into(),
            family_id: "f1".into(),
            uf2_bytes: "10".into(),
            uf2_blocks: "1".into(),
            payload_bytes: "4".into(),
            sha256: "approved-digest".into(),
            payload_sha256: "payload".into(),
            manifest_digest: "manifest".into(),
        };
        let drive = WatchDriveCandidate {
            root: PathBuf::from("E:\\"),
            info: "UF2 Sensor Watch".into(),
        };
        assert!(validate_copy_guard(
            &approved,
            Ok(&approved),
            &WatchDriveSelection::One(drive.clone())
        )
        .is_ok());
        assert!(validate_copy_guard(
            &approved,
            Ok(&crate::build::ArtifactInspection {
                sha256: "changed".into(),
                ..approved.clone()
            }),
            &WatchDriveSelection::One(drive.clone())
        )
        .is_err());
        assert!(validate_copy_guard(&approved, Ok(&approved), &WatchDriveSelection::None).is_err());
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
            flash_with_start(
                FlashRequest {
                    path,
                    approved,
                    selected_drive: None,
                },
                || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                },
            )
        });

        // The worker has entered before it attempts to read the missing path.
        // The test thread remains immediately usable while that worker is held.
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!handle.is_finished());
        release_tx.send(()).unwrap();
        assert_eq!(handle.join().unwrap().status, FlashStatus::ArtifactInvalid);
    }

    #[test]
    fn valid_uf2_replacement_is_rejected_before_drive_writes() {
        let root = temp_drive("artifact-replacement-race");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("recovery.uf2");
        let original = sensor_watch_core::uf2::convert_to_uf2(&[0x11; 1024]);
        let replacement = sensor_watch_core::uf2::convert_to_uf2(&[0x22; 1024]);
        std::fs::write(&path, &original).unwrap();
        let manifest =
            sensor_watch_tools::create_manifest(&path, Some("g-race".into()), Some(&path)).unwrap();
        sensor_watch_tools::write_manifest(&path.with_extension("uf2.json"), &manifest).unwrap();
        let request = FlashRequest {
            path: path.clone(),
            approved: crate::build::inspect_artifact(&path).unwrap(),
            selected_drive: None,
        };

        let result = read_verified_artifact(
            &request,
            &ProgressSink::disabled(),
            |path| std::fs::read(path),
            || std::fs::write(&path, &replacement).unwrap(),
        )
        .unwrap_err();

        assert_eq!(result.status, FlashStatus::ArtifactChanged);
        assert_eq!(std::fs::read(&path).unwrap(), replacement);
        assert!(!root.join("CURRENT.UF2").exists());
        assert!(!root.join(".current.uf2.tmp").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn selected_drive_identity_accepts_unchanged_candidate() {
        let candidate = WatchDriveCandidate {
            root: PathBuf::from("E:\\"),
            info: "UF2 Sensor Watch".to_string(),
        };
        assert!(drive_identity_matches(&candidate, &candidate));
    }

    #[test]
    fn selected_drive_identity_rejects_changed_metadata_and_replacement_root() {
        let expected = WatchDriveCandidate {
            root: PathBuf::from("E:\\"),
            info: "UF2 Sensor Watch green".to_string(),
        };
        let changed_metadata = WatchDriveCandidate {
            root: expected.root.clone(),
            info: "UF2 Sensor Watch blue".to_string(),
        };
        let replacement = WatchDriveCandidate {
            root: PathBuf::from("F:\\"),
            info: expected.info.clone(),
        };
        assert!(!drive_identity_matches(&expected, &changed_metadata));
        assert!(!drive_identity_matches(&expected, &replacement));
    }

    fn temp_drive(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sensor-watch-flash-{name}-{}", std::process::id()))
    }
}
