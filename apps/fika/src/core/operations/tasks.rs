use super::{
    CreateUndoItem, CreatedItemKind, TransferUndoItem, TrashUndoItem, UndoPayload, UndoRecord,
};
use crate::core::file_ops;
use crate::core::operation_runtime::{OperationController, run_operation_blocking};
use crate::core::pane::PaneId;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileTransferMode {
    Copy,
    Move,
    Link,
}

impl FileTransferMode {
    pub fn operation(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Move => "move",
            Self::Link => "link",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
            Self::Link => "Link",
        }
    }

    pub fn progress_label(self, item_count: usize) -> String {
        let verb = match self {
            Self::Copy => "Copying",
            Self::Move => "Moving",
            Self::Link => "Linking",
        };
        format!("{verb} {item_count} item(s)")
    }
}

#[derive(Clone, Debug)]
pub struct TrashSelectionResult {
    pub pane_id: PaneId,
    pub success_count: usize,
    pub failure_count: usize,
    pub affected_dirs: Vec<PathBuf>,
    pub undo_items: Vec<TrashUndoItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrashViewOperation {
    Restore {
        conflict_policy: file_ops::TrashRestoreConflictPolicy,
    },
    DeletePermanently,
    Empty,
}

impl TrashViewOperation {
    pub fn progress_label(self, count: usize) -> String {
        match self {
            Self::Restore { .. } => format!("Restoring {count} item(s)"),
            Self::DeletePermanently => format!("Deleting {count} item(s) permanently"),
            Self::Empty => "Emptying Trash".to_string(),
        }
    }

    pub fn completed_label(self) -> &'static str {
        match self {
            Self::Restore { .. } => "Restored from trash",
            Self::DeletePermanently => "Deleted permanently",
            Self::Empty => "Emptied Trash",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TrashViewOperationResult {
    pub pane_id: PaneId,
    pub operation: TrashViewOperation,
    pub success_count: usize,
    pub failure_count: usize,
    pub affected_dirs: Vec<PathBuf>,
    pub restore_conflicts: Vec<file_ops::TrashRestoreConflict>,
}

#[derive(Clone, Debug)]
pub struct TransferTaskResult {
    pub pane_id: PaneId,
    pub mode: FileTransferMode,
    pub label: &'static str,
    pub clear_clipboard: bool,
    pub success_count: usize,
    pub failure_count: usize,
    pub affected_dirs: Vec<PathBuf>,
    pub refresh_dirs: Vec<PathBuf>,
    pub undo_items: Vec<TransferUndoItem>,
    pub created_items: Vec<CreateUndoItem>,
}

#[derive(Clone, Debug)]
pub struct RenameItemResult {
    pub pane_id: PaneId,
    pub original_path: PathBuf,
    pub affected_dirs: Vec<PathBuf>,
    pub result: Result<PathBuf, String>,
}

#[derive(Clone, Debug)]
pub struct CreateItemResult {
    pub pane_id: PaneId,
    pub kind: CreatedItemKind,
    pub affected_dirs: Vec<PathBuf>,
    pub result: Result<PathBuf, String>,
}

#[derive(Clone, Debug)]
pub struct UndoTaskResult {
    pub record: UndoRecord,
    pub result: Result<String, String>,
}

pub fn rename_item_result(
    pane_id: PaneId,
    original_path: PathBuf,
    new_name: String,
) -> RenameItemResult {
    let mut affected_dirs = parent_dirs([original_path.clone()]);
    let result = file_ops::rename_path(&original_path, &new_name);
    if let Ok(renamed_path) = &result
        && let Some(parent) = renamed_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
    {
        push_unique_path(&mut affected_dirs, parent.to_path_buf());
    }

    RenameItemResult {
        pane_id,
        original_path,
        affected_dirs,
        result,
    }
}

pub async fn rename_item_result_async(
    pane_id: PaneId,
    original_path: PathBuf,
    new_name: String,
) -> RenameItemResult {
    let mut affected_dirs = parent_dirs([original_path.clone()]);
    let result = file_ops::rename_path_async(&original_path, &new_name).await;
    if let Ok(renamed_path) = &result
        && let Some(parent) = renamed_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
    {
        push_unique_path(&mut affected_dirs, parent.to_path_buf());
    }

    RenameItemResult {
        pane_id,
        original_path,
        affected_dirs,
        result,
    }
}

pub fn create_item_result(
    pane_id: PaneId,
    parent_dir: PathBuf,
    kind: CreatedItemKind,
) -> CreateItemResult {
    let result = match kind {
        CreatedItemKind::File => {
            file_ops::create_file(&parent_dir, default_created_item_name(kind))
        }
        CreatedItemKind::Folder => {
            file_ops::create_folder(&parent_dir, default_created_item_name(kind))
        }
    };
    CreateItemResult {
        pane_id,
        kind,
        affected_dirs: vec![parent_dir],
        result,
    }
}

pub async fn create_item_result_async(
    pane_id: PaneId,
    parent_dir: PathBuf,
    kind: CreatedItemKind,
) -> CreateItemResult {
    let result = match kind {
        CreatedItemKind::File => {
            file_ops::create_file_async(&parent_dir, default_created_item_name(kind)).await
        }
        CreatedItemKind::Folder => {
            file_ops::create_folder_async(&parent_dir, default_created_item_name(kind)).await
        }
    };
    CreateItemResult {
        pane_id,
        kind,
        affected_dirs: vec![parent_dir],
        result,
    }
}

pub fn default_created_item_name(kind: CreatedItemKind) -> &'static str {
    match kind {
        CreatedItemKind::File => "New File.txt",
        CreatedItemKind::Folder => "New Folder",
    }
}

pub fn created_item_label(kind: CreatedItemKind) -> &'static str {
    match kind {
        CreatedItemKind::File => "File",
        CreatedItemKind::Folder => "Folder",
    }
}

pub fn paste_text_result(pane_id: PaneId, target_dir: PathBuf, text: &str) -> TransferTaskResult {
    let mut affected_dirs = Vec::new();
    let mut refresh_dirs = Vec::new();
    let mut created_items = Vec::new();
    let result = file_ops::write_unique_file(&target_dir, "Pasted Text", "txt", text.as_bytes());
    let (success_count, failure_count) = match result {
        Ok(path) => {
            push_unique_path(&mut affected_dirs, target_dir.clone());
            push_unique_path(&mut refresh_dirs, target_dir);
            created_items.push(CreateUndoItem {
                path,
                kind: CreatedItemKind::File,
            });
            (1, 0)
        }
        Err(_) => {
            push_unique_path(&mut refresh_dirs, target_dir);
            (0, 1)
        }
    };

    TransferTaskResult {
        pane_id,
        mode: FileTransferMode::Copy,
        label: "Paste",
        clear_clipboard: false,
        success_count,
        failure_count,
        affected_dirs,
        refresh_dirs,
        undo_items: Vec::new(),
        created_items,
    }
}

pub async fn paste_text_result_async(
    pane_id: PaneId,
    target_dir: PathBuf,
    text: String,
) -> TransferTaskResult {
    let fallback_target = target_dir.clone();
    run_operation_blocking(move || paste_text_result(pane_id, target_dir, &text))
        .await
        .unwrap_or_else(|_| TransferTaskResult {
            pane_id,
            mode: FileTransferMode::Copy,
            label: "Paste",
            clear_clipboard: false,
            success_count: 0,
            failure_count: 1,
            affected_dirs: Vec::new(),
            refresh_dirs: vec![fallback_target],
            undo_items: Vec::new(),
            created_items: Vec::new(),
        })
}

pub fn transfer_paths_result(
    pane_id: PaneId,
    target_dir: PathBuf,
    mode: FileTransferMode,
    paths: Vec<PathBuf>,
    label: &'static str,
    clear_clipboard: bool,
    controller: Option<OperationController>,
) -> TransferTaskResult {
    let operation = mode.operation();
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut affected_dirs = Vec::new();
    let mut refresh_dirs = Vec::new();
    let mut undo_items = Vec::new();

    for source in paths {
        if controller
            .as_ref()
            .is_some_and(OperationController::is_cancelled)
        {
            failure_count += 1;
            continue;
        }
        let progress_controller = controller.clone();
        match file_ops::perform_transfer_with_progress_outcome(
            operation,
            &source,
            &target_dir,
            "keep-both",
            controller.clone(),
            move |transfer_progress| {
                if let Some(controller) = &progress_controller {
                    controller.set_progress(transfer_progress);
                }
            },
        ) {
            Ok(outcome) => {
                success_count += 1;
                push_unique_path(&mut affected_dirs, target_dir.clone());
                push_unique_path(&mut refresh_dirs, target_dir.clone());
                if mode == FileTransferMode::Move
                    && let Some(parent) = source
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                {
                    push_unique_path(&mut affected_dirs, parent.to_path_buf());
                    push_unique_path(&mut refresh_dirs, parent.to_path_buf());
                }
                undo_items.push(TransferUndoItem {
                    operation: operation.to_string(),
                    original_source: source,
                    destination: outcome.destination,
                    overwritten_backup: outcome.overwritten_backup,
                });
            }
            Err(_) => {
                failure_count += 1;
                push_unique_path(&mut refresh_dirs, target_dir.clone());
            }
        }
    }

    TransferTaskResult {
        pane_id,
        mode,
        label,
        clear_clipboard,
        success_count,
        failure_count,
        affected_dirs,
        refresh_dirs,
        undo_items,
        created_items: Vec::new(),
    }
}

pub async fn transfer_paths_result_async(
    pane_id: PaneId,
    target_dir: PathBuf,
    mode: FileTransferMode,
    paths: Vec<PathBuf>,
    label: &'static str,
    clear_clipboard: bool,
    controller: Option<OperationController>,
) -> TransferTaskResult {
    let operation = mode.operation();
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut affected_dirs = Vec::new();
    let mut refresh_dirs = Vec::new();
    let mut undo_items = Vec::new();

    for source in paths {
        if controller
            .as_ref()
            .is_some_and(OperationController::is_cancelled)
        {
            failure_count += 1;
            continue;
        }
        let progress_controller = controller.clone();
        match file_ops::perform_transfer_with_progress_outcome_async(
            operation,
            &source,
            &target_dir,
            "keep-both",
            controller.clone(),
            move |transfer_progress| {
                if let Some(controller) = &progress_controller {
                    controller.set_progress(transfer_progress);
                }
            },
        )
        .await
        {
            Ok(outcome) => {
                success_count += 1;
                push_unique_path(&mut affected_dirs, target_dir.clone());
                push_unique_path(&mut refresh_dirs, target_dir.clone());
                if mode == FileTransferMode::Move
                    && let Some(parent) = source
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                {
                    push_unique_path(&mut affected_dirs, parent.to_path_buf());
                    push_unique_path(&mut refresh_dirs, parent.to_path_buf());
                }
                undo_items.push(TransferUndoItem {
                    operation: operation.to_string(),
                    original_source: source,
                    destination: outcome.destination,
                    overwritten_backup: outcome.overwritten_backup,
                });
            }
            Err(_) => {
                failure_count += 1;
                push_unique_path(&mut refresh_dirs, target_dir.clone());
            }
        }
    }

    TransferTaskResult {
        pane_id,
        mode,
        label,
        clear_clipboard,
        success_count,
        failure_count,
        affected_dirs,
        refresh_dirs,
        undo_items,
        created_items: Vec::new(),
    }
}

pub fn trash_selection_result(
    pane_id: PaneId,
    selected_paths: Vec<PathBuf>,
) -> TrashSelectionResult {
    let summary = file_ops::trash_paths(&selected_paths);
    let success_count = summary.successes.len();
    let failure_count = summary.failures.len();
    let undo_items = summary
        .successes
        .iter()
        .map(|record| TrashUndoItem {
            original_path: record.original_path.clone(),
            trash_path: record.trash_path.clone(),
        })
        .collect::<Vec<_>>();
    let mut affected_dirs = parent_dirs(
        summary
            .successes
            .iter()
            .map(|record| record.original_path.clone()),
    );
    if success_count > 0 {
        push_unique_path(&mut affected_dirs, file_ops::trash_files_dir());
    }

    TrashSelectionResult {
        pane_id,
        success_count,
        failure_count,
        affected_dirs,
        undo_items,
    }
}

pub async fn trash_selection_result_async(
    pane_id: PaneId,
    selected_paths: Vec<PathBuf>,
) -> TrashSelectionResult {
    run_operation_blocking(move || trash_selection_result(pane_id, selected_paths))
        .await
        .unwrap_or_else(|_| TrashSelectionResult {
            pane_id,
            success_count: 0,
            failure_count: 1,
            affected_dirs: Vec::new(),
            undo_items: Vec::new(),
        })
}

pub fn trash_view_operation_result(
    pane_id: PaneId,
    operation: TrashViewOperation,
    paths: Vec<PathBuf>,
) -> TrashViewOperationResult {
    let summary = match operation {
        TrashViewOperation::Restore { conflict_policy } => {
            file_ops::restore_trash_paths_with_policy(&paths, conflict_policy)
        }
        TrashViewOperation::DeletePermanently => file_ops::permanently_delete_trash_paths(&paths),
        TrashViewOperation::Empty => file_ops::empty_trash(),
    };
    trash_view_operation_result_from_summary(pane_id, operation, summary)
}

fn trash_view_operation_result_from_summary(
    pane_id: PaneId,
    operation: TrashViewOperation,
    summary: file_ops::FileActionSummary,
) -> TrashViewOperationResult {
    let success_count = summary.successes.len();
    let failure_count = summary.failures.len();
    let restore_conflicts = summary.restore_conflicts;
    let mut affected_dirs = Vec::new();
    if success_count > 0 {
        push_unique_path(&mut affected_dirs, file_ops::trash_files_dir());
    }
    if matches!(operation, TrashViewOperation::Restore { .. }) {
        for record in &summary.successes {
            if let Some(parent) = record
                .original_path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                push_unique_path(&mut affected_dirs, parent.to_path_buf());
            }
        }
    }

    TrashViewOperationResult {
        pane_id,
        operation,
        success_count,
        failure_count,
        affected_dirs,
        restore_conflicts,
    }
}

pub async fn trash_view_operation_result_async(
    pane_id: PaneId,
    operation: TrashViewOperation,
    paths: Vec<PathBuf>,
) -> TrashViewOperationResult {
    if operation == TrashViewOperation::Empty {
        let summary = file_ops::empty_trash_async().await;
        return trash_view_operation_result_from_summary(pane_id, operation, summary);
    }

    run_operation_blocking(move || trash_view_operation_result(pane_id, operation, paths))
        .await
        .unwrap_or_else(|_| TrashViewOperationResult {
            pane_id,
            operation,
            success_count: 0,
            failure_count: 1,
            affected_dirs: Vec::new(),
            restore_conflicts: Vec::new(),
        })
}

pub fn undo_record_result(record: UndoRecord) -> UndoTaskResult {
    let result = match &record.payload {
        UndoPayload::Create { items } => {
            for (removed_count, item) in items.iter().enumerate() {
                let result = match item.kind {
                    CreatedItemKind::File => file_ops::undo_create_file(&item.path),
                    CreatedItemKind::Folder => file_ops::undo_create_folder(&item.path),
                };
                if let Err(err) = result {
                    return UndoTaskResult {
                        record,
                        result: Err(format!(
                            "removed {removed_count} item(s), then failed: {err}"
                        )),
                    };
                }
            }
            Ok(format!("removed {} item(s)", items.len()))
        }
        UndoPayload::Trash { items } => {
            let restore_pairs = items
                .iter()
                .map(|item| (item.original_path.clone(), item.trash_path.clone()))
                .collect::<Vec<_>>();
            file_ops::undo_trash(&restore_pairs)
        }
        UndoPayload::Rename { items } => {
            for (restored_count, item) in items.iter().enumerate() {
                if let Err(err) = file_ops::undo_rename(&item.original_path, &item.renamed_path) {
                    return UndoTaskResult {
                        record,
                        result: Err(format!(
                            "restored {restored_count} item(s), then failed: {err}"
                        )),
                    };
                }
            }
            Ok(format!("restored {} item(s)", items.len()))
        }
        UndoPayload::Transfer { items } => {
            for (restored_count, item) in items.iter().enumerate() {
                if let Err(err) = file_ops::undo_transfer_with_backup(
                    &item.operation,
                    &item.original_source,
                    &item.destination,
                    item.overwritten_backup.as_deref(),
                ) {
                    return UndoTaskResult {
                        record,
                        result: Err(format!(
                            "restored {restored_count} item(s), then failed: {err}"
                        )),
                    };
                }
            }
            Ok(format!("restored {} item(s)", items.len()))
        }
        UndoPayload::None => Err(format!("no undo action for {}", record.label)),
    };
    UndoTaskResult { record, result }
}

pub async fn undo_record_result_async(record: UndoRecord) -> UndoTaskResult {
    let fallback_record = record.clone();
    run_operation_blocking(move || undo_record_result(record))
        .await
        .unwrap_or_else(|err| UndoTaskResult {
            record: fallback_record,
            result: Err(err.to_string()),
        })
}

pub fn parent_dirs(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for path in paths {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            push_unique_path(&mut dirs, parent.to_path_buf());
        }
    }
    dirs
}

pub fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

pub fn action_status(label: &str, success_count: usize, failure_count: usize) -> String {
    match (success_count, failure_count) {
        (0, 0) => format!("{label}: no changes"),
        (_, 0) => format!("{label}: {success_count} item(s)"),
        (0, _) => format!("{label} failed for {failure_count} item(s)"),
        (_, _) => format!("{label}: {success_count} item(s), {failure_count} failed"),
    }
}

#[cfg(test)]
#[path = "tasks/tests.rs"]
mod tests;
