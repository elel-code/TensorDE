pub fn restore_trash_paths_with_policy(
    paths: &[PathBuf],
    conflict_policy: TrashRestoreConflictPolicy,
) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    for path in paths {
        match restore_trash_path(path, conflict_policy) {
            Ok(record) => summary.successes.push(record),
            Err(TrashRestoreError::Conflict(conflict)) => {
                summary.restore_conflicts.push(conflict);
            }
            Err(TrashRestoreError::Failure(err)) => {
                summary.failures.push(format!("{}: {err}", path.display()));
            }
        }
    }
    if !summary.successes.is_empty() {
        let _ = sync_trash_status_empty();
    }
    summary
}

pub fn permanently_delete_trash_paths(paths: &[PathBuf]) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    for path in paths {
        match permanently_delete_trash_path(path) {
            Ok(record) => summary.successes.push(record),
            Err(err) => summary.failures.push(format!("{}: {err}", path.display())),
        }
    }
    if !summary.successes.is_empty() {
        let _ = sync_trash_status_empty();
    }
    summary
}

pub fn empty_trash() -> FileActionSummary {
    empty_trash_in_dirs(trash_files_dir(), trash_info_dir(), trashrc_path())
}

fn empty_trash_in_dirs(
    files_dir: PathBuf,
    info_dir: PathBuf,
    trashrc_path: PathBuf,
) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    let Some(trash_dir) = files_dir.parent().map(Path::to_path_buf) else {
        summary.failures.push(format!(
            "{}: Trash files directory has no parent",
            files_dir.display()
        ));
        return summary;
    };

    if !path_exists(&files_dir) {
        if path_exists(&info_dir) {
            match swap_trash_dir_for_emptying(&trash_dir, &info_dir, "info") {
                Ok(old_info_dir) => {
                    if let Err(err) = fs::create_dir_all(&info_dir) {
                        summary
                            .failures
                            .push(format!("{}: {err}", info_dir.display()));
                        let _ = fs::rename(&old_info_dir, &info_dir);
                        return summary;
                    }
                    spawn_trash_emptying_cleanup(vec![old_info_dir]);
                }
                Err(err) => {
                    summary
                        .failures
                        .push(format!("{}: {err}", info_dir.display()));
                    return summary;
                }
            }
        }
        let _ = write_trash_status_empty_at(&trashrc_path, true);
        return summary;
    }

    let entries = match read_dir_entries(&files_dir) {
        Ok(entries) => entries,
        Err(err) => {
            summary
                .failures
                .push(format!("{}: {err}", files_dir.display()));
            return summary;
        }
    };

    let mut successes = Vec::with_capacity(entries.len());
    for (entry_path, _) in &entries {
        successes.push(TrashRecord {
            original_path: entry_path.clone(),
            trash_path: entry_path.clone(),
        });
    }

    let info_exists = path_exists(&info_dir);
    let old_info_dir = if info_exists {
        match swap_trash_dir_for_emptying(&trash_dir, &info_dir, "info") {
            Ok(path) => Some(path),
            Err(err) => {
                summary
                    .failures
                    .push(format!("{}: {err}", info_dir.display()));
                return summary;
            }
        }
    } else {
        None
    };
    let old_files_dir = match swap_trash_dir_for_emptying(&trash_dir, &files_dir, "files") {
        Ok(path) => path,
        Err(err) => {
            if let Some(old_info_dir) = old_info_dir.as_ref() {
                let _ = fs::rename(old_info_dir, &info_dir);
            }
            summary
                .failures
                .push(format!("{}: {err}", files_dir.display()));
            return summary;
        }
    };

    if let Err(err) = fs::create_dir_all(&files_dir) {
        summary
            .failures
            .push(format!("{}: {err}", files_dir.display()));
        let _ = fs::rename(&old_files_dir, &files_dir);
        if let Some(old_info_dir) = old_info_dir.as_ref() {
            let _ = fs::rename(old_info_dir, &info_dir);
        }
        return summary;
    }
    if let Err(err) = fs::create_dir_all(&info_dir) {
        summary
            .failures
            .push(format!("{}: {err}", info_dir.display()));
        let _ = remove_path_if_present(&files_dir);
        let _ = fs::rename(&old_files_dir, &files_dir);
        if let Some(old_info_dir) = old_info_dir.as_ref() {
            let _ = fs::rename(old_info_dir, &info_dir);
        }
        return summary;
    }

    let _ = write_trash_status_empty_at(&trashrc_path, true);
    summary.successes = successes;
    let mut cleanup_paths = vec![old_files_dir];
    if let Some(old_info_dir) = old_info_dir {
        cleanup_paths.push(old_info_dir);
    }
    spawn_trash_emptying_cleanup(cleanup_paths);
    summary
}

pub async fn empty_trash_async() -> FileActionSummary {
    empty_trash_in_dirs_async(trash_files_dir(), trash_info_dir(), trashrc_path()).await
}

async fn empty_trash_in_dirs_async(
    files_dir: PathBuf,
    info_dir: PathBuf,
    trashrc_path: PathBuf,
) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    let Some(trash_dir) = files_dir.parent().map(Path::to_path_buf) else {
        summary.failures.push(format!(
            "{}: Trash files directory has no parent",
            files_dir.display()
        ));
        return summary;
    };

    let files_exists = path_exists_async(&files_dir).await;
    let info_exists = path_exists_async(&info_dir).await;
    if !files_exists {
        if info_exists {
            match swap_trash_dir_for_emptying_async(&trash_dir, &info_dir, "info").await {
                Ok(old_info_dir) => {
                    if let Err(err) = create_dir_all_async(&info_dir).await {
                        summary
                            .failures
                            .push(format!("{}: {err}", info_dir.display()));
                        let _ = compio::fs::rename(&old_info_dir, &info_dir).await;
                        return summary;
                    }
                    spawn_trash_emptying_cleanup(vec![old_info_dir]);
                }
                Err(err) => {
                    summary
                        .failures
                        .push(format!("{}: {err}", info_dir.display()));
                    return summary;
                }
            }
        }
        let _ = write_trash_status_empty_at_async(&trashrc_path, true).await;
        return summary;
    }

    let entries = match read_dir_entries_async(&files_dir).await {
        Ok(entries) => entries,
        Err(err) => {
            summary
                .failures
                .push(format!("{}: {err}", files_dir.display()));
            return summary;
        }
    };

    let mut successes = Vec::with_capacity(entries.len());
    for (entry_path, _) in &entries {
        successes.push(TrashRecord {
            original_path: entry_path.clone(),
            trash_path: entry_path.clone(),
        });
    }

    let old_info_dir = if info_exists {
        match swap_trash_dir_for_emptying_async(&trash_dir, &info_dir, "info").await {
            Ok(path) => Some(path),
            Err(err) => {
                summary
                    .failures
                    .push(format!("{}: {err}", info_dir.display()));
                return summary;
            }
        }
    } else {
        None
    };
    let old_files_dir =
        match swap_trash_dir_for_emptying_async(&trash_dir, &files_dir, "files").await {
            Ok(path) => path,
            Err(err) => {
                if let Some(old_info_dir) = old_info_dir.as_ref() {
                    let _ = compio::fs::rename(old_info_dir, &info_dir).await;
                }
                summary
                    .failures
                    .push(format!("{}: {err}", files_dir.display()));
                return summary;
            }
        };

    if let Err(err) = create_dir_all_async(&files_dir).await {
        summary
            .failures
            .push(format!("{}: {err}", files_dir.display()));
        let _ = compio::fs::rename(&old_files_dir, &files_dir).await;
        if let Some(old_info_dir) = old_info_dir.as_ref() {
            let _ = compio::fs::rename(old_info_dir, &info_dir).await;
        }
        return summary;
    }
    if let Err(err) = create_dir_all_async(&info_dir).await {
        summary
            .failures
            .push(format!("{}: {err}", info_dir.display()));
        let _ = remove_path_if_present_async(&files_dir).await;
        let _ = compio::fs::rename(&old_files_dir, &files_dir).await;
        if let Some(old_info_dir) = old_info_dir.as_ref() {
            let _ = compio::fs::rename(old_info_dir, &info_dir).await;
        }
        return summary;
    }

    let _ = write_trash_status_empty_at_async(&trashrc_path, true).await;
    summary.successes = successes;
    let mut cleanup_paths = vec![old_files_dir];
    if let Some(old_info_dir) = old_info_dir {
        cleanup_paths.push(old_info_dir);
    }
    spawn_trash_emptying_cleanup(cleanup_paths);
    summary
}

async fn swap_trash_dir_for_emptying_async(
    trash_dir: &Path,
    path: &Path,
    prefix: &str,
) -> io::Result<PathBuf> {
    for attempt in 0..32_u8 {
        let destination = trash_emptying_temp_path(trash_dir, prefix, attempt);
        if path_exists_async(&destination).await {
            continue;
        }
        match compio::fs::rename(path, &destination).await {
            Ok(()) => return Ok(destination),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve Trash emptying directory",
    ))
}

fn swap_trash_dir_for_emptying(trash_dir: &Path, path: &Path, prefix: &str) -> io::Result<PathBuf> {
    for attempt in 0..32_u8 {
        let destination = trash_emptying_temp_path(trash_dir, prefix, attempt);
        if path_exists(&destination) {
            continue;
        }
        match fs::rename(path, &destination) {
            Ok(()) => return Ok(destination),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve Trash emptying directory",
    ))
}

fn trash_emptying_temp_path(trash_dir: &Path, prefix: &str, attempt: u8) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    trash_dir.join(format!(
        ".fika-emptying-{prefix}-{}-{nanos}-{attempt}",
        std::process::id()
    ))
}

fn spawn_trash_emptying_cleanup(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let _ = crate::core::operation_runtime::spawn_operation_task(move || async move {
        for path in paths {
            let _ = remove_path_if_present_async(&path).await;
        }
    });
}

fn restore_trash_path(
    trash_path: &Path,
    conflict_policy: TrashRestoreConflictPolicy,
) -> Result<TrashRecord, TrashRestoreError> {
    if is_trash_files_dir(trash_path) || !is_in_trash_files_dir(trash_path) {
        return Err(TrashRestoreError::Failure(
            "item is not inside Trash".to_string(),
        ));
    }
    if !path_exists(trash_path) {
        return Err(TrashRestoreError::Failure(
            "trash item no longer exists".to_string(),
        ));
    }

    let original_path = trash_original_path(trash_path).map_err(TrashRestoreError::Failure)?;
    if original_path == trash_path {
        return Err(TrashRestoreError::Failure(
            "original location matches trash item".to_string(),
        ));
    }

    let overwrite_backup = if path_exists(&original_path) {
        match conflict_policy {
            TrashRestoreConflictPolicy::Skip => {
                return Err(TrashRestoreError::Conflict(TrashRestoreConflict {
                    original_path,
                    trash_path: trash_path.to_path_buf(),
                }));
            }
            TrashRestoreConflictPolicy::Replace => Some(
                backup_existing_destination(&original_path).map_err(TrashRestoreError::Failure)?,
            ),
        }
    } else {
        None
    };

    let restore_backup = |err: String| {
        if let Some(backup) = overwrite_backup.as_ref() {
            let _ = restore_overwrite_backup(&original_path, backup);
        }
        TrashRestoreError::Failure(err)
    };

    if let Some(parent) = original_path.parent() {
        fs::create_dir_all(parent).map_err(|err| restore_backup(err.to_string()))?;
    }
    move_path(trash_path, &original_path, None, &mut |_| {})
        .map_err(|err| restore_backup(err.to_string()))?;
    let _ = remove_trashinfo(trash_path);
    if let Some(backup) = overwrite_backup {
        let _ = cleanup_overwrite_backup(&backup);
    }

    Ok(TrashRecord {
        original_path,
        trash_path: trash_path.to_path_buf(),
    })
}

#[derive(Debug, Eq, PartialEq)]
enum TrashRestoreError {
    Conflict(TrashRestoreConflict),
    Failure(String),
}

fn permanently_delete_trash_path(trash_path: &Path) -> Result<TrashRecord, String> {
    if is_trash_files_dir(trash_path) || !is_in_trash_files_dir(trash_path) {
        return Err("item is not inside Trash".to_string());
    }
    if !path_exists(trash_path) {
        return Err("trash item no longer exists".to_string());
    }

    let original_path =
        trash_original_path(trash_path).unwrap_or_else(|_| trash_path.to_path_buf());
    remove_path(trash_path).map_err(|err| err.to_string())?;
    let _ = remove_trashinfo(trash_path);

    Ok(TrashRecord {
        original_path,
        trash_path: trash_path.to_path_buf(),
    })
}

#[derive(Debug, Default)]
pub struct FileActionSummary {
    pub successes: Vec<TrashRecord>,
    pub failures: Vec<String>,
    pub restore_conflicts: Vec<TrashRestoreConflict>,
}

#[derive(Clone, Debug)]
pub struct TrashRecord {
    pub original_path: PathBuf,
    pub trash_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashRestoreConflict {
    pub original_path: PathBuf,
    pub trash_path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrashRestoreConflictPolicy {
    Skip,
    Replace,
}

pub fn trash_restore_conflict(trash_path: &Path) -> Result<Option<TrashRestoreConflict>, String> {
    if is_trash_files_dir(trash_path) || !is_in_trash_files_dir(trash_path) {
        return Err("item is not inside Trash".to_string());
    }
    if !path_exists(trash_path) {
        return Err("trash item no longer exists".to_string());
    }
    let original_path = trash_original_path(trash_path)?;
    Ok(path_exists(&original_path).then(|| TrashRestoreConflict {
        original_path,
        trash_path: trash_path.to_path_buf(),
    }))
}

