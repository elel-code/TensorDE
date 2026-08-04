use crate::windowing::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::TensorFilesApp;
use crate::ui::context_menu::ShellContextMenuAction;
use crate::ui::operation_request::ShellOperationRequest;
use crate::ui::tasks::ShellTaskStatus;
use tensor_files_core::{TrashViewOperation, file_ops, is_network_path};

impl TensorFilesApp {
    pub(crate) fn perform_trash_view_context_action(
        &mut self,
        _event_loop: &ActiveEventLoop,
        action: ShellContextMenuAction,
    ) {
        let pane_to_reload = self
            .scene
            .context_target_pane()
            .unwrap_or_else(|| self.scene.active_pane());
        let (operation, paths) = match self.scene.context_target_trash_view_operation(action) {
            Ok(value) => value,
            Err(error) => {
                tensor_files_log!(
                    "[tensor-files] trash-view-error action={} {error}",
                    action.as_str()
                );
                self.scene.record_task_status(ShellTaskStatus::failed(
                    format!("{} failed", action.label()),
                    error,
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };
        self.submit_operation_request(ShellOperationRequest::trash_view(
            action,
            operation,
            paths,
            pane_to_reload,
        ));
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn move_context_target_to_trash(
        &mut self,
        _event_loop: &ActiveEventLoop,
        privileged: bool,
    ) {
        let pane_to_reload = self
            .scene
            .context_target_pane()
            .unwrap_or_else(|| self.scene.active_pane());
        let paths = match self.scene.context_target_trash_paths() {
            Ok(paths) => paths,
            Err(error) => {
                tensor_files_log!("[tensor-files] trash-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    if privileged {
                        "Administrator move to Trash failed"
                    } else {
                        "Move to Trash failed"
                    },
                    error,
                    privileged,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };
        if paths.iter().any(|path| is_network_path(path)) {
            self.scene.record_task_status(ShellTaskStatus::failed(
                if privileged {
                    "Administrator move to Trash failed"
                } else {
                    "Move to Trash failed"
                },
                "remote trash is not available yet",
                privileged,
            ));
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        }

        self.submit_operation_request(ShellOperationRequest::move_to_trash(
            paths,
            pane_to_reload,
            privileged,
            None,
        ));
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn delete_active_selection(&mut self, _event_loop: &ActiveEventLoop) {
        let pane = self.scene.active_pane();
        let paths = match self.scene.active_selection_item_paths() {
            Ok(Some(paths)) => paths,
            Ok(None) => return,
            Err(error) => {
                tensor_files_log!("[tensor-files] delete-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Move to Trash failed",
                    error,
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };

        if paths
            .iter()
            .all(|path| file_ops::is_in_trash_files_dir(path))
        {
            self.submit_operation_request(ShellOperationRequest::trash_view(
                ShellContextMenuAction::DeletePermanently,
                TrashViewOperation::DeletePermanently,
                paths,
                pane,
            ));
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        }
        if paths.iter().any(|path| is_network_path(path)) {
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Move to Trash failed",
                "remote trash is not available yet",
                false,
            ));
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        }

        self.submit_operation_request(ShellOperationRequest::move_to_trash(
            paths,
            pane,
            false,
            Some(pane),
        ));
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn replace_trash_restore_conflicts(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(dialog) = self.scene.trash_conflict_dialog.take() else {
            tensor_files_log!(
                "[tensor-files] trash-conflict-error no Trash restore conflicts to replace"
            );
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        };
        let paths = dialog
            .conflicts
            .into_iter()
            .map(|conflict| conflict.trash_path)
            .collect::<Vec<_>>();
        if paths.is_empty() {
            tensor_files_log!(
                "[tensor-files] trash-conflict-error no Trash restore conflicts to replace"
            );
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        }
        let pane_to_reload = self.scene.active_pane();
        self.scene.trash_changes = self.scene.trash_changes.saturating_add(1);
        self.submit_operation_request(ShellOperationRequest::trash_view(
            ShellContextMenuAction::RestoreFromTrash,
            TrashViewOperation::Restore {
                conflict_policy: file_ops::TrashRestoreConflictPolicy::Replace,
            },
            paths,
            pane_to_reload,
        ));
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }
}
