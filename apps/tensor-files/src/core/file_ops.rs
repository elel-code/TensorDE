use crate::core::operation_runtime::OperationController;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use compio::buf::{BufResult, IntoInner};
use compio::io::{AsyncReadAt, AsyncWriteAtExt};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Clone, Debug)]
pub struct TransferOutcome {
    pub destination: PathBuf,
    pub overwritten_backup: Option<PathBuf>,
}

struct TransferPlan {
    destination: PathBuf,
    overwritten_backup: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashMetadata {
    pub original_path: PathBuf,
    pub deletion_date: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferTargetRelation {
    Same,
    Descendant,
}

pub fn perform_transfer_with_progress(
    operation: &str,
    source: &Path,
    target_dir: &Path,
    conflict_policy: &str,
    controller: Option<OperationController>,
    progress: impl FnMut(TransferProgress),
) -> Result<PathBuf, String> {
    let outcome = perform_transfer_with_progress_outcome(
        operation,
        source,
        target_dir,
        conflict_policy,
        controller,
        progress,
    )?;
    if let Some(backup) = outcome.overwritten_backup {
        remove_path(&backup).map_err(|err| err.to_string())?;
    }
    Ok(outcome.destination)
}

pub fn perform_transfer_with_progress_outcome(
    operation: &str,
    source: &Path,
    target_dir: &Path,
    conflict_policy: &str,
    controller: Option<OperationController>,
    mut progress: impl FnMut(TransferProgress),
) -> Result<TransferOutcome, String> {
    if !path_exists(source) {
        return Err("source no longer exists".to_string());
    }
    if !target_dir.is_dir() {
        return Err("target is not a folder".to_string());
    }
    if let Some(relation) = transfer_target_relation(source, target_dir) {
        return Err(transfer_target_relation_error(relation).to_string());
    }

    let file_name = source
        .file_name()
        .ok_or_else(|| "source has no file name".to_string())?;
    let destination = transfer_destination(source, target_dir, file_name, conflict_policy)?;

    if !matches!(operation, "move" | "copy" | "link") {
        return Err(format!("unknown operation: {operation}"));
    }

    let overwrite_backup = if conflict_policy == "overwrite" && path_exists(&destination) {
        Some(backup_existing_destination(&destination)?)
    } else {
        None
    };

    let result = match operation {
        "move" => move_path(source, &destination, controller.as_ref(), &mut progress),
        "copy" => copy_path(source, &destination, controller.as_ref(), &mut progress),
        "link" => link_path(source, &destination),
        _ => unreachable!("operation was validated before dispatch"),
    };

    match result {
        Ok(()) => {
            // Keep the backup in the richer outcome so callers that expose Undo can restore it.
            // The simpler transfer API removes it immediately for one-shot operations.
        }
        Err(err) => {
            if let Some(backup) = overwrite_backup {
                restore_overwrite_backup(&destination, &backup)?;
            } else if err.kind() == io::ErrorKind::Interrupted {
                let _ = remove_path(&destination);
            }
            return Err(err.to_string());
        }
    }

    Ok(TransferOutcome {
        destination,
        overwritten_backup: overwrite_backup,
    })
}

pub async fn perform_transfer_with_progress_outcome_async(
    operation: &str,
    source: &Path,
    target_dir: &Path,
    conflict_policy: &str,
    controller: Option<OperationController>,
    mut progress: impl FnMut(TransferProgress),
) -> Result<TransferOutcome, String> {
    let plan = prepare_transfer_async(
        operation.to_string(),
        source.to_path_buf(),
        target_dir.to_path_buf(),
        conflict_policy.to_string(),
    )
    .await?;

    let result = match operation {
        "move" => {
            move_path_async(
                source,
                &plan.destination,
                controller.as_ref(),
                &mut progress,
            )
            .await
        }
        "copy" => {
            copy_path_async(
                source,
                &plan.destination,
                controller.as_ref(),
                &mut progress,
            )
            .await
        }
        "link" => link_path_async(source, &plan.destination).await,
        _ => unreachable!("operation was validated before dispatch"),
    };

    match result {
        Ok(()) => {}
        Err(err) => {
            if let Some(backup) = plan.overwritten_backup.as_ref() {
                restore_overwrite_backup_async(&plan.destination, backup).await?;
            } else if err.kind() == io::ErrorKind::Interrupted {
                let _ = remove_path_async(&plan.destination).await;
            }
            return Err(err.to_string());
        }
    }

    Ok(TransferOutcome {
        destination: plan.destination,
        overwritten_backup: plan.overwritten_backup,
    })
}

async fn prepare_transfer_async(
    operation: String,
    source: PathBuf,
    target_dir: PathBuf,
    conflict_policy: String,
) -> Result<TransferPlan, String> {
    if !path_exists_async(&source).await {
        return Err("source no longer exists".to_string());
    }
    if !metadata_async(&target_dir)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("target is not a folder".to_string());
    }
    if let Some(relation) = transfer_target_relation_async(&source, &target_dir).await {
        return Err(transfer_target_relation_error(relation).to_string());
    }

    let file_name = source
        .file_name()
        .ok_or_else(|| "source has no file name".to_string())?
        .to_os_string();
    let destination =
        transfer_destination_async(&source, &target_dir, &file_name, &conflict_policy).await?;

    if !matches!(operation.as_str(), "move" | "copy" | "link") {
        return Err(format!("unknown operation: {operation}"));
    }

    let overwritten_backup =
        if conflict_policy == "overwrite" && path_exists_async(&destination).await {
            Some(backup_existing_destination_async(&destination).await?)
        } else {
            None
        };

    Ok(TransferPlan {
        destination,
        overwritten_backup,
    })
}

pub fn base_destination(source: &Path, target_dir: &Path) -> Result<PathBuf, String> {
    let file_name = source
        .file_name()
        .ok_or_else(|| "source has no file name".to_string())?;
    Ok(target_dir.join(file_name))
}

pub fn transfer_target_relation(
    source: &Path,
    target_dir: &Path,
) -> Option<TransferTargetRelation> {
    if source == target_dir || canonical_paths_equal(source, target_dir) {
        return Some(TransferTargetRelation::Same);
    }
    if target_is_source_descendant(source, target_dir) {
        return Some(TransferTargetRelation::Descendant);
    }
    None
}

async fn transfer_target_relation_async(
    source: &Path,
    target_dir: &Path,
) -> Option<TransferTargetRelation> {
    if source == target_dir || canonical_paths_equal_async(source, target_dir).await {
        return Some(TransferTargetRelation::Same);
    }
    if target_is_source_descendant_async(source, target_dir).await {
        return Some(TransferTargetRelation::Descendant);
    }
    None
}

pub fn target_is_source_or_descendant(source: &Path, target_dir: &Path) -> bool {
    transfer_target_relation(source, target_dir).is_some()
}

fn transfer_target_relation_error(relation: TransferTargetRelation) -> &'static str {
    match relation {
        TransferTargetRelation::Same => "cannot transfer an item onto itself",
        TransferTargetRelation::Descendant => "cannot transfer a folder into itself",
    }
}

fn canonical_paths_equal(source: &Path, target_dir: &Path) -> bool {
    let Ok(source) = source.canonicalize() else {
        return false;
    };
    let Ok(target_dir) = target_dir.canonicalize() else {
        return false;
    };
    source == target_dir
}

async fn canonical_paths_equal_async(source: &Path, target_dir: &Path) -> bool {
    let Ok(source) = canonicalize_async(source).await else {
        return false;
    };
    let Ok(target_dir) = canonicalize_async(target_dir).await else {
        return false;
    };
    source == target_dir
}

fn target_is_source_descendant(source: &Path, target_dir: &Path) -> bool {
    if target_dir.starts_with(source) {
        return true;
    }

    let Ok(source) = source.canonicalize() else {
        return false;
    };
    let Ok(target_dir) = target_dir.canonicalize() else {
        return false;
    };
    target_dir.starts_with(source)
}

async fn target_is_source_descendant_async(source: &Path, target_dir: &Path) -> bool {
    if target_dir.starts_with(source) {
        return true;
    }

    let Ok(source) = canonicalize_async(source).await else {
        return false;
    };
    let Ok(target_dir) = canonicalize_async(target_dir).await else {
        return false;
    };
    target_dir.starts_with(source)
}

pub fn renamed_destination(target_dir: &Path, name: &str) -> Result<PathBuf, String> {
    if !target_dir.is_dir() {
        return Err("target is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = target_dir.join(name);
    if path_exists(&destination) {
        return Err("an item with that name already exists".to_string());
    }
    Ok(destination)
}

pub fn undo_transfer(
    operation: &str,
    original_source: &Path,
    destination: &Path,
) -> Result<String, String> {
    undo_transfer_with_backup(operation, original_source, destination, None)
}

pub fn undo_transfer_with_backup(
    operation: &str,
    original_source: &Path,
    destination: &Path,
    overwritten_backup: Option<&Path>,
) -> Result<String, String> {
    if let Some(backup) = overwritten_backup {
        return undo_overwrite_transfer(operation, original_source, destination, backup);
    }

    match operation {
        "copy" | "link" => {
            if !path_exists(destination) {
                return Err("undo target no longer exists".to_string());
            }
            remove_path(destination).map_err(|err| err.to_string())?;
            Ok(format!("removed {}", destination.display()))
        }
        "move" => {
            if !path_exists(destination) {
                return Err("moved item no longer exists".to_string());
            }
            if path_exists(original_source) {
                return Err("original location is already occupied".to_string());
            }
            if let Some(parent) = original_source.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            match fs::rename(destination, original_source) {
                Ok(()) => Ok(format!("restored {}", original_source.display())),
                Err(err) if err.raw_os_error() == Some(18) => {
                    copy_path(destination, original_source, None, &mut |_| {})
                        .map_err(|err| err.to_string())?;
                    remove_path(destination).map_err(|err| err.to_string())?;
                    Ok(format!("restored {}", original_source.display()))
                }
                Err(err) => Err(err.to_string()),
            }
        }
        _ => Err(format!("cannot undo operation: {operation}")),
    }
}

fn undo_overwrite_transfer(
    operation: &str,
    original_source: &Path,
    destination: &Path,
    backup: &Path,
) -> Result<String, String> {
    if !path_exists(backup) {
        return Err("overwritten item backup no longer exists".to_string());
    }

    match operation {
        "copy" | "link" => {
            if path_exists(destination) {
                remove_path(destination).map_err(|err| err.to_string())?;
            }
            fs::rename(backup, destination).map_err(|err| err.to_string())?;
            Ok(format!("restored overwritten {}", destination.display()))
        }
        "move" => {
            if !path_exists(destination) {
                return Err("moved item no longer exists".to_string());
            }
            if path_exists(original_source) {
                return Err("original location is already occupied".to_string());
            }
            if let Some(parent) = original_source.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            match fs::rename(destination, original_source) {
                Ok(()) => {}
                Err(err) if err.raw_os_error() == Some(18) => {
                    copy_path(destination, original_source, None, &mut |_| {})
                        .map_err(|err| err.to_string())?;
                    remove_path(destination).map_err(|err| err.to_string())?;
                }
                Err(err) => return Err(err.to_string()),
            }
            fs::rename(backup, destination).map_err(|err| err.to_string())?;
            Ok(format!(
                "restored {} and overwritten {}",
                original_source.display(),
                destination.display()
            ))
        }
        _ => Err(format!("cannot undo operation: {operation}")),
    }
}

pub fn create_folder(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !parent.is_dir() {
        return Err("current location is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = unique_destination(parent, name.as_ref());
    fs::create_dir(&destination).map_err(|err| err.to_string())?;
    Ok(destination)
}

pub async fn create_folder_async(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !metadata_async(parent)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("current location is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = unique_destination_async(parent, name.as_ref()).await;
    create_folder_at_async(&destination).await?;
    Ok(destination)
}

pub fn create_file(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !parent.is_dir() {
        return Err("current location is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = unique_destination(parent, name.as_ref());
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|err| err.to_string())?;
    Ok(destination)
}

pub async fn create_file_async(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !metadata_async(parent)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("current location is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = unique_destination_async(parent, name.as_ref()).await;
    create_file_at_async(&destination).await?;
    Ok(destination)
}

pub async fn create_folder_at_async(path: &Path) -> Result<(), String> {
    compio::fs::create_dir(path)
        .await
        .map_err(|err| err.to_string())
}

pub async fn create_file_at_async(path: &Path) -> Result<(), String> {
    let mut options = compio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let file = options.open(path).await.map_err(|err| err.to_string())?;
    file.close().await.map_err(|err| err.to_string())
}

pub fn write_unique_file(
    parent: &Path,
    base_name: &str,
    extension: &str,
    data: &[u8],
) -> Result<PathBuf, String> {
    if !parent.is_dir() {
        return Err("current location is not a folder".to_string());
    }
    let extension = extension.trim_start_matches('.');
    let file_name = if extension.is_empty() {
        sanitize_child_name(base_name)?
    } else {
        sanitize_child_name(&format!("{base_name}.{extension}"))?
    };
    let destination = unique_destination(parent, file_name.as_ref());
    fs::write(&destination, data).map_err(|err| err.to_string())?;
    Ok(destination)
}

pub fn rename_path(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    if !path_exists(path) {
        return Err("item no longer exists".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "item has no parent folder".to_string())?;
    let new_name = sanitize_child_name(new_name)?;
    let destination = parent.join(new_name);
    if destination == path {
        return Ok(destination);
    }
    if path_exists(&destination) {
        return Err("an item with that name already exists".to_string());
    }
    fs::rename(path, &destination).map_err(|err| err.to_string())?;
    Ok(destination)
}

pub async fn rename_path_async(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    if !path_exists_async(path).await {
        return Err("item no longer exists".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "item has no parent folder".to_string())?;
    let new_name = sanitize_child_name(new_name)?;
    let destination = parent.join(new_name);
    if destination == path {
        return Ok(destination);
    }
    if path_exists_async(&destination).await {
        return Err("an item with that name already exists".to_string());
    }
    rename_path_to_async(path, &destination).await?;
    Ok(destination)
}

pub async fn rename_path_to_async(source: &Path, destination: &Path) -> Result<(), String> {
    compio::fs::rename(source, destination)
        .await
        .map_err(|err| err.to_string())
}

pub fn undo_create_folder(path: &Path) -> Result<String, String> {
    if !path_exists(path) {
        return Err("created folder no longer exists".to_string());
    }
    if !path.is_dir() {
        return Err("created item is no longer a folder".to_string());
    }
    fs::remove_dir(path).map_err(|err| err.to_string())?;
    Ok(format!("removed {}", path.display()))
}

pub fn undo_create_file(path: &Path) -> Result<String, String> {
    if !path_exists(path) {
        return Err("created file no longer exists".to_string());
    }
    if !path.is_file() {
        return Err("created item is no longer a file".to_string());
    }
    fs::remove_file(path).map_err(|err| err.to_string())?;
    Ok(format!("removed {}", path.display()))
}

pub fn undo_rename(original_path: &Path, renamed_path: &Path) -> Result<String, String> {
    if !path_exists(renamed_path) {
        return Err("renamed item no longer exists".to_string());
    }
    if path_exists(original_path) {
        return Err("original name is already occupied".to_string());
    }
    fs::rename(renamed_path, original_path).map_err(|err| err.to_string())?;
    Ok(format!("restored {}", original_path.display()))
}

pub fn trash_paths(paths: &[PathBuf]) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    for path in paths {
        match trash_path(path) {
            Ok(destination) => summary.successes.push(TrashRecord {
                original_path: path.clone(),
                trash_path: destination,
            }),
            Err(err) => summary.failures.push(format!("{}: {err}", path.display())),
        }
    }
    summary
}

pub async fn trash_paths_async(paths: Vec<PathBuf>) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    for path in paths {
        match trash_path_async(&path).await {
            Ok(destination) => summary.successes.push(TrashRecord {
                original_path: path,
                trash_path: destination,
            }),
            Err(err) => summary.failures.push(format!("{}: {err}", path.display())),
        }
    }
    summary
}

pub fn trash_path(path: &Path) -> Result<PathBuf, String> {
    if !path_exists(path) {
        return Err("item no longer exists".to_string());
    }
    if is_in_trash_files_dir(path) {
        return Err("item is already in Trash".to_string());
    }

    ensure_trash_dirs()?;
    let files_dir = trash_files_dir();
    let info_dir = trash_info_dir();

    let file_name = path
        .file_name()
        .ok_or_else(|| "item has no file name".to_string())?;
    let destination = unique_destination(&files_dir, file_name);
    move_path(path, &destination, None, &mut |_| {}).map_err(|err| err.to_string())?;

    let info_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "trash destination has invalid name".to_string())?;
    fs::write(
        info_dir.join(format!("{info_name}.trashinfo")),
        trashinfo(path),
    )
    .map_err(|err| err.to_string())?;
    let _ = set_trash_status_empty(false);
    Ok(destination)
}

pub async fn trash_path_async(path: &Path) -> Result<PathBuf, String> {
    if !path_exists_async(path).await {
        return Err("item no longer exists".to_string());
    }
    if is_in_trash_files_dir(path) {
        return Err("item is already in Trash".to_string());
    }

    ensure_trash_dirs_async().await?;
    let files_dir = trash_files_dir();
    let info_dir = trash_info_dir();

    let file_name = path
        .file_name()
        .ok_or_else(|| "item has no file name".to_string())?;
    let destination = unique_destination_async(&files_dir, file_name).await;
    move_path_async(path, &destination, None, &mut |_| {})
        .await
        .map_err(|err| err.to_string())?;

    let info_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "trash destination has invalid name".to_string())?;
    write_file_async(
        &info_dir.join(format!("{info_name}.trashinfo")),
        trashinfo(path),
    )
    .await
    .map_err(|err| err.to_string())?;
    let _ = set_trash_status_empty_async(false).await;
    Ok(destination)
}

pub fn ensure_trash_dirs() -> Result<(), String> {
    fs::create_dir_all(trash_files_dir()).map_err(|err| err.to_string())?;
    fs::create_dir_all(trash_info_dir()).map_err(|err| err.to_string())
}

async fn ensure_trash_dirs_async() -> Result<(), String> {
    create_dir_all_async(&trash_files_dir())
        .await
        .map_err(|err| err.to_string())?;
    create_dir_all_async(&trash_info_dir())
        .await
        .map_err(|err| err.to_string())
}

pub fn trash_files_dir() -> PathBuf {
    trash_home().join("files")
}

pub fn trash_info_dir() -> PathBuf {
    trash_home().join("info")
}

pub fn trashrc_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("trashrc")
}

pub fn trash_has_items() -> bool {
    fs::read_dir(trash_files_dir())
        .ok()
        .is_some_and(|mut entries| entries.any(|entry| entry.is_ok()))
}

pub fn trash_status_empty() -> bool {
    trash_status_empty_at(&trashrc_path())
}

pub fn set_trash_status_empty(empty: bool) -> Result<(), String> {
    write_trash_status_empty_at(&trashrc_path(), empty)
}

async fn set_trash_status_empty_async(empty: bool) -> Result<(), String> {
    write_trash_status_empty_at_async(&trashrc_path(), empty).await
}

pub fn sync_trash_status_empty() -> Result<bool, String> {
    let empty = !trash_has_items();
    set_trash_status_empty(empty)?;
    Ok(empty)
}

pub fn is_trash_files_dir(path: &Path) -> bool {
    path == trash_files_dir()
}

pub fn is_in_trash_files_dir(path: &Path) -> bool {
    path.starts_with(trash_files_dir())
}

pub fn undo_trash(items: &[(PathBuf, PathBuf)]) -> Result<String, String> {
    if items.is_empty() {
        return Err("no trash entries to restore".to_string());
    }

    for (original_path, trash_path) in items {
        if !path_exists(trash_path) {
            return Err(format!(
                "trash item no longer exists: {}",
                trash_path.display()
            ));
        }
        if path_exists(original_path) {
            return Err(format!(
                "original location is already occupied: {}",
                original_path.display()
            ));
        }
    }

    for (original_path, trash_path) in items {
        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        move_path(trash_path, original_path, None, &mut |_| {}).map_err(|err| err.to_string())?;
        let _ = remove_trashinfo(trash_path);
    }
    let _ = sync_trash_status_empty();

    Ok(format!("restored {} item(s)", items.len()))
}

include!("file_ops/trash_restore.rs");
include!("file_ops/transfer_paths.rs");
include!("file_ops/path_helpers.rs");

#[cfg(test)]
#[path = "file_ops/tests.rs"]
mod tests;
