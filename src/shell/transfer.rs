use std::path::{Path, PathBuf};

use fika_core::{
    DevicePlaceOperationResult, Entry, FileTransferMode, OperationController, PrivilegedCommand,
    TransferTaskResult, TransferUndoItem, TrashViewOperationResult, file_ops, push_unique_path,
};

use crate::CopyLocationRequest;
use crate::DeviceActionRequest;
use crate::shell::clipboard::FileClipboardExportRequest;
use crate::shell::context_menu::ShellContextMenuAction;
use crate::shell::create_rename::{CreateEntryRequest, RenameEntryRequest};
use crate::shell::metrics::WGPU_SHELL_PANE_ID;
use crate::shell::pane::ShellPaneId;
use crate::shell::privilege::{
    ShellPrivilegeOutcome, run_privileged_command_sync, should_attempt_privileged_operation,
};
use crate::shell::tasks::ShellTaskId;

#[derive(Clone, Debug)]
pub(crate) struct ShellPasteResult {
    pub(crate) mode: FileTransferMode,
    pub(crate) success_count: usize,
    pub(crate) failure_count: usize,
    pub(crate) clear_clipboard: bool,
    pub(crate) privileged: bool,
    pub(crate) administrator_available: bool,
    pub(crate) first_error: Option<String>,
}

impl ShellPasteResult {
    pub(crate) fn from_transfer(execution: &ShellTransferExecution) -> Self {
        Self {
            mode: execution.result.mode,
            success_count: execution.result.success_count,
            failure_count: execution.result.failure_count,
            clear_clipboard: execution.result.clear_clipboard,
            privileged: execution.privileged,
            administrator_available: execution.administrator_available,
            first_error: execution.first_error.clone(),
        }
    }

    pub(crate) fn changed(&self) -> bool {
        self.success_count > 0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShellTransferExecution {
    pub(crate) result: TransferTaskResult,
    pub(crate) privileged: bool,
    pub(crate) administrator_available: bool,
    pub(crate) first_error: Option<String>,
    pub(crate) cancelled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellAsyncTransferSource {
    Paste,
    Drop,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellAsyncTransferCompletion {
    pub(crate) task_id: ShellTaskId,
    pub(crate) source: ShellAsyncTransferSource,
    pub(crate) target_dir: PathBuf,
    pub(crate) transfer: ShellTransferExecution,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellAsyncTrashViewCompletion {
    pub(crate) task_id: ShellTaskId,
    pub(crate) action: ShellContextMenuAction,
    pub(crate) pane_to_reload: ShellPaneId,
    pub(crate) result: TrashViewOperationResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellNavigationHistoryUpdate {
    Push,
    Back,
    Forward,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellAsyncNavigationCompletion {
    pub(crate) generation: u64,
    pub(crate) pane: ShellPaneId,
    pub(crate) source_path: PathBuf,
    pub(crate) target_path: PathBuf,
    pub(crate) history: ShellNavigationHistoryUpdate,
    pub(crate) reason: &'static str,
    pub(crate) result: Result<Vec<Entry>, String>,
}

#[derive(Clone, Debug)]
pub(crate) enum ShellAsyncClipboardCompletion {
    StoreFile {
        request: FileClipboardExportRequest,
        result: Result<(), String>,
    },
    CopyLocation {
        request: CopyLocationRequest,
        result: Result<(), String>,
    },
    LoadPaste {
        use_context: bool,
        privileged: bool,
        result: Result<String, String>,
    },
    Clear {
        reason: &'static str,
        result: Result<(), String>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ShellAsyncCreateCompletion {
    pub(crate) request: CreateEntryRequest,
    pub(crate) outcome: Result<ShellPrivilegeOutcome, String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellAsyncRenameCompletion {
    pub(crate) request: RenameEntryRequest,
    pub(crate) outcome: Result<ShellPrivilegeOutcome, String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellAsyncDeviceCompletion {
    pub(crate) request: DeviceActionRequest,
    pub(crate) result: DevicePlaceOperationResult,
}

#[derive(Clone, Debug)]
pub(crate) enum ShellAsyncTaskResult {
    Navigation(ShellAsyncNavigationCompletion),
    Transfer(ShellAsyncTransferCompletion),
    TrashView(ShellAsyncTrashViewCompletion),
    Clipboard(ShellAsyncClipboardCompletion),
    Create(ShellAsyncCreateCompletion),
    Rename(ShellAsyncRenameCompletion),
    Device(ShellAsyncDeviceCompletion),
}

pub(crate) fn transfer_paths_with_privilege(
    target_dir: PathBuf,
    mode: FileTransferMode,
    paths: Vec<PathBuf>,
    label: &'static str,
    clear_clipboard: bool,
    privileged: bool,
) -> ShellTransferExecution {
    let operation = mode.operation();
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut affected_dirs = Vec::new();
    let mut refresh_dirs = Vec::new();
    let mut undo_items = Vec::new();
    let mut administrator_available = false;
    let mut first_error = None;

    for source in paths {
        if privileged {
            match run_privileged_command_sync(PrivilegedCommand::Transfer {
                operation: operation.to_string(),
                source: source.clone(),
                target_dir: target_dir.clone(),
            }) {
                Ok(_) => {
                    success_count += 1;
                    push_transfer_refresh_dirs(
                        mode,
                        &source,
                        &target_dir,
                        &mut affected_dirs,
                        &mut refresh_dirs,
                    );
                }
                Err(error) => {
                    failure_count += 1;
                    if first_error.is_none() {
                        first_error = Some(error.clone());
                    }
                    fika_log!(
                        "[fika-wgpu] privileged-transfer-error mode={} source={} target={} error={error}",
                        mode.label(),
                        source.display(),
                        target_dir.display()
                    );
                    push_unique_path(&mut refresh_dirs, target_dir.clone());
                }
            }
            continue;
        }

        match file_ops::perform_transfer_with_progress_outcome(
            operation,
            &source,
            &target_dir,
            "keep-both",
            None,
            |_| {},
        ) {
            Ok(outcome) => {
                success_count += 1;
                push_transfer_refresh_dirs(
                    mode,
                    &source,
                    &target_dir,
                    &mut affected_dirs,
                    &mut refresh_dirs,
                );
                undo_items.push(TransferUndoItem {
                    operation: operation.to_string(),
                    original_source: source,
                    destination: outcome.destination,
                    overwritten_backup: outcome.overwritten_backup,
                });
            }
            Err(error) => {
                failure_count += 1;
                administrator_available |= should_attempt_privileged_operation(&error);
                if first_error.is_none() {
                    first_error = Some(error.clone());
                }
                fika_log!(
                    "[fika-wgpu] transfer-error mode={} source={} target={} error={error}",
                    mode.label(),
                    source.display(),
                    target_dir.display()
                );
                push_unique_path(&mut refresh_dirs, target_dir.clone());
            }
        }
    }

    ShellTransferExecution {
        result: TransferTaskResult {
            pane_id: WGPU_SHELL_PANE_ID,
            mode,
            label,
            clear_clipboard,
            success_count,
            failure_count,
            affected_dirs,
            refresh_dirs,
            undo_items,
            created_items: Vec::new(),
        },
        privileged,
        administrator_available,
        first_error,
        cancelled: false,
    }
}

pub(crate) async fn transfer_paths_async_with_controller(
    target_dir: PathBuf,
    mode: FileTransferMode,
    paths: Vec<PathBuf>,
    label: &'static str,
    clear_clipboard: bool,
    controller: OperationController,
) -> ShellTransferExecution {
    let operation = mode.operation();
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut affected_dirs = Vec::new();
    let mut refresh_dirs = Vec::new();
    let mut undo_items = Vec::new();
    let mut administrator_available = false;
    let mut first_error = None;
    let mut cancelled = false;

    for source in paths {
        if controller.is_cancelled() {
            cancelled = true;
            failure_count += 1;
            if first_error.is_none() {
                first_error = Some("operation cancelled".to_string());
            }
            continue;
        }
        let progress_controller = controller.clone();
        match file_ops::perform_transfer_with_progress_outcome_async(
            operation,
            &source,
            &target_dir,
            "keep-both",
            Some(controller.clone()),
            move |transfer_progress| {
                progress_controller.set_progress(transfer_progress);
            },
        )
        .await
        {
            Ok(outcome) => {
                success_count += 1;
                push_transfer_refresh_dirs(
                    mode,
                    &source,
                    &target_dir,
                    &mut affected_dirs,
                    &mut refresh_dirs,
                );
                undo_items.push(TransferUndoItem {
                    operation: operation.to_string(),
                    original_source: source,
                    destination: outcome.destination,
                    overwritten_backup: outcome.overwritten_backup,
                });
            }
            Err(error) => {
                cancelled |= controller.is_cancelled() || error.contains("operation cancelled");
                administrator_available |= should_attempt_privileged_operation(&error);
                failure_count += 1;
                if first_error.is_none() {
                    first_error = Some(error.clone());
                }
                fika_log!(
                    "[fika-wgpu] async-transfer-error mode={} source={} target={} error={error}",
                    mode.label(),
                    source.display(),
                    target_dir.display()
                );
                push_unique_path(&mut refresh_dirs, target_dir.clone());
            }
        }
    }

    ShellTransferExecution {
        result: TransferTaskResult {
            pane_id: WGPU_SHELL_PANE_ID,
            mode,
            label,
            clear_clipboard,
            success_count,
            failure_count,
            affected_dirs,
            refresh_dirs,
            undo_items,
            created_items: Vec::new(),
        },
        privileged: false,
        administrator_available,
        first_error,
        cancelled,
    }
}

pub(crate) fn async_transfer_task_label(
    source: ShellAsyncTransferSource,
    mode: FileTransferMode,
    item_count: usize,
) -> String {
    match source {
        ShellAsyncTransferSource::Paste => "Pasting".to_string(),
        ShellAsyncTransferSource::Drop => mode.progress_label(item_count),
    }
}

pub(crate) fn async_transfer_task_detail(
    target_dir: &Path,
    item_count: usize,
    clear_clipboard: bool,
) -> String {
    if clear_clipboard {
        format!(
            "{} to {} | clipboard will clear on success",
            count_label(item_count, "item", "items"),
            target_dir.display()
        )
    } else {
        format!(
            "{} to {}",
            count_label(item_count, "item", "items"),
            target_dir.display()
        )
    }
}

pub(crate) fn transfer_runtime_failure(
    target_dir: PathBuf,
    mode: FileTransferMode,
    label: &'static str,
    clear_clipboard: bool,
    error: impl std::fmt::Display,
) -> ShellTransferExecution {
    ShellTransferExecution {
        result: TransferTaskResult {
            pane_id: WGPU_SHELL_PANE_ID,
            mode,
            label,
            clear_clipboard,
            success_count: 0,
            failure_count: 1,
            affected_dirs: Vec::new(),
            refresh_dirs: vec![target_dir],
            undo_items: Vec::new(),
            created_items: Vec::new(),
        },
        privileged: false,
        administrator_available: false,
        first_error: Some(format!("operation runtime failed: {error}")),
        cancelled: false,
    }
}

fn push_transfer_refresh_dirs(
    mode: FileTransferMode,
    source: &Path,
    target_dir: &Path,
    affected_dirs: &mut Vec<PathBuf>,
    refresh_dirs: &mut Vec<PathBuf>,
) {
    push_unique_path(affected_dirs, target_dir.to_path_buf());
    push_unique_path(refresh_dirs, target_dir.to_path_buf());
    if mode == FileTransferMode::Move
        && let Some(parent) = source
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
    {
        push_unique_path(affected_dirs, parent.to_path_buf());
        push_unique_path(refresh_dirs, parent.to_path_buf());
    }
}

fn count_label(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}
