impl ShellScene {

    fn active_selection_item_paths(&self) -> Result<Option<Vec<PathBuf>>, String> {
        let pane = self.active_pane();
        let Some(pane_view) = self.pane_view(pane) else {
            return Err("active pane no longer exists".to_string());
        };
        let Some(selection) = self.pane_selection(pane) else {
            return Ok(None);
        };
        if selection.selected.is_empty() {
            return Ok(None);
        }
        let paths = selection
            .selected
            .iter()
            .copied()
            .filter_map(|index| self.entry_path_for_pane_view(pane_view, index))
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return Err("active selection no longer exists".to_string());
        }
        Ok(Some(paths))
    }

    fn open_rename_dialog_from_active_selection(&mut self, privileged: bool) -> bool {
        let pane = self.active_pane();
        let Some(selection) = self.pane_selection(pane) else {
            fika_log!("[fika-wgpu] rename-error target=none");
            return false;
        };
        let Some(index) = selection.focus_or_first_selected() else {
            fika_log!("[fika-wgpu] rename-error target=none");
            return false;
        };
        let Some(view) = self.pane_view(pane) else {
            fika_log!("[fika-wgpu] rename-error target=none");
            return false;
        };
        let Some(entry) = view.entries.get(index) else {
            fika_log!("[fika-wgpu] rename-error target=none");
            return false;
        };
        let Some(path) = self.entry_path_for_pane_view(view, index) else {
            fika_log!("[fika-wgpu] rename-error target=none");
            return false;
        };
        let Some(dialog) = ShellRenameDialog::new(pane, path.clone(), entry.is_dir, privileged)
        else {
            fika_log!(
                "[fika-wgpu] rename-error path={} error=no-file-name",
                path.display()
            );
            return false;
        };
        let changed = self.rename_dialog.as_ref() != Some(&dialog);
        self.rename_dialog = Some(dialog);
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rubber_band = None;
        if changed {
            self.rename_changes += 1;
            self.log_rename_dialog_state();
        }
        changed
    }

    // Production delete path is async via FikaWgpuApp::delete_active_selection.
    #[cfg(test)]
    fn delete_active_selection(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let pane = self.active_pane();
        let affected_dir = self.pane_state(pane).map(|state| state.path.clone());
        let Some(paths) = self.active_selection_item_paths()? else {
            return Ok(false);
        };
        if paths
            .iter()
            .all(|path| file_ops::is_in_trash_files_dir(path))
        {
            let result = trash_view_operation_result(
                WGPU_SHELL_PANE_ID,
                TrashViewOperation::DeletePermanently,
                paths,
            );
            self.apply_trash_view_result("delete-active-selection", pane, &result, size)?;
            return Ok(result.success_count > 0);
        }
        if paths.iter().any(|path| is_network_path(path)) {
            return Err("remote trash is not available yet".to_string());
        }
        let result = match trash_paths_with_privilege(&paths, false) {
            Ok(result) => result,
            Err(error) => {
                self.record_task_status(ShellTaskStatus::failed(
                    "Move to Trash failed",
                    error.clone(),
                    false,
                ));
                return Err(error);
            }
        };
        let changed = result.changed();
        self.record_trash_content_change();
        fika_log!(
            "[fika-wgpu] trash paths={} success={} failure={} privileged={} changes={}",
            paths.len(),
            result.success_count,
            result.failure_count,
            result.privileged as u8,
            self.trash_changes
        );
        self.record_task_status(if result.failure_count > 0 {
            ShellTaskStatus::failed(
                "Move to Trash failed",
                transfer_task_detail(
                    result.success_count,
                    result.failure_count,
                    Path::new("Trash"),
                    result.first_error.as_deref(),
                    result.administrator_available,
                ),
                result.privileged,
            )
        } else {
            ShellTaskStatus::completed(
                "Moved to Trash",
                paths_task_summary(&paths),
                result.privileged,
            )
        });
        if changed {
            self.context_target = None;
            self.context_menu = None;
            self.drop_menu = None;
            self.properties_overlay = None;
            self.create_dialog = None;
            self.rename_dialog = None;
            self.rubber_band = None;
            if let Some(selection) = self.pane_selection_mut(pane) {
                selection.clear();
            }
            if let Some(affected_dir) = affected_dir {
                self.reload_panes_showing_path(&affected_dir, size)?;
            }
        }
        Ok(changed)
    }

    fn context_target_trash_paths(&self) -> Result<Vec<PathBuf>, String> {
        self.context_target_item_paths()?
            .ok_or_else(|| "no item context target to move to trash".to_string())
    }

    fn context_target_trash_view_operation(
        &self,
        action: ShellContextMenuAction,
    ) -> Result<(TrashViewOperation, Vec<PathBuf>), String> {
        match action {
            ShellContextMenuAction::RestoreFromTrash => Ok((
                TrashViewOperation::Restore {
                    conflict_policy: file_ops::TrashRestoreConflictPolicy::Skip,
                },
                self.context_target_trash_view_item_paths()?,
            )),
            ShellContextMenuAction::DeletePermanently => Ok((
                TrashViewOperation::DeletePermanently,
                self.context_target_trash_view_item_paths()?,
            )),
            ShellContextMenuAction::EmptyTrash => {
                if self.context_target_can_empty_trash() {
                    Ok((TrashViewOperation::Empty, Vec::new()))
                } else {
                    Err("Empty Trash is only available from Trash".to_string())
                }
            }
            _ => Err(format!(
                "action {} is not a Trash view action",
                action.as_str()
            )),
        }
    }

    fn context_target_trash_view_item_paths(&self) -> Result<Vec<PathBuf>, String> {
        let paths = self
            .context_target_item_paths()?
            .ok_or_else(|| "no Trash item context target".to_string())?;
        if paths.is_empty() {
            return Err("no Trash item context target".to_string());
        }
        if paths.iter().any(|path| {
            file_ops::is_trash_files_dir(path) || !file_ops::is_in_trash_files_dir(path)
        }) {
            return Err("Trash item action is only available for items inside Trash".to_string());
        }
        Ok(paths)
    }

    fn context_target_can_empty_trash(&self) -> bool {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Blank { path, .. }) => file_ops::is_trash_files_dir(path),
            Some(ShellContextTarget::Place { trash, .. }) => *trash,
            _ => false,
        }
    }

    // Production trash-view path is async via FikaWgpuApp::perform_trash_view_context_action.
    #[cfg(test)]
    fn perform_trash_view_context_action(
        &mut self,
        action: ShellContextMenuAction,
        size: PhysicalSize<u32>,
    ) -> Result<TrashViewOperationResult, String> {
        let pane_to_reload = self
            .context_target_pane()
            .unwrap_or_else(|| self.active_pane());
        let (operation, paths) = self.context_target_trash_view_operation(action)?;
        let result = trash_view_operation_result(WGPU_SHELL_PANE_ID, operation, paths);
        self.apply_trash_view_result(action.as_str(), pane_to_reload, &result, size)?;
        Ok(result)
    }

    // Production conflict replace path is async via FikaWgpuApp::replace_trash_restore_conflicts.
    #[cfg(test)]
    fn replace_trash_restore_conflicts(
        &mut self,
        size: PhysicalSize<u32>,
    ) -> Result<TrashViewOperationResult, String> {
        let Some(dialog) = self.trash_conflict_dialog.take() else {
            return Err("no Trash restore conflicts to replace".to_string());
        };
        let paths = dialog
            .conflicts
            .into_iter()
            .map(|conflict| conflict.trash_path)
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return Err("no Trash restore conflicts to replace".to_string());
        }
        let result = trash_view_operation_result(
            WGPU_SHELL_PANE_ID,
            TrashViewOperation::Restore {
                conflict_policy: file_ops::TrashRestoreConflictPolicy::Replace,
            },
            paths,
        );
        let pane_to_reload = self.active_pane();
        self.apply_trash_view_result("replace-trash-conflicts", pane_to_reload, &result, size)?;
        Ok(result)
    }

    #[cfg(test)]
    fn apply_trash_view_result(
        &mut self,
        action: &str,
        pane_to_reload: ShellPaneId,
        result: &TrashViewOperationResult,
        size: PhysicalSize<u32>,
    ) -> Result<(), String> {
        self.apply_trash_view_result_with_task(None, action, pane_to_reload, result, size)
    }

    fn apply_trash_view_result_with_task(
        &mut self,
        task_id: Option<ShellTaskId>,
        action: &str,
        pane_to_reload: ShellPaneId,
        result: &TrashViewOperationResult,
        size: PhysicalSize<u32>,
    ) -> Result<(), String> {
        let affected_dir = self
            .pane_state(pane_to_reload)
            .map(|state| state.path.clone());
        self.record_trash_content_change();
        fika_log!(
            "[fika-wgpu] trash-view action={} success={} failure={} conflicts={} changes={}",
            action,
            result.success_count,
            result.failure_count,
            result.restore_conflicts.len(),
            self.trash_changes
        );
        for conflict in &result.restore_conflicts {
            fika_log!(
                "[fika-wgpu] trash-restore-conflict original={} trash={}",
                conflict.original_path.display(),
                conflict.trash_path.display()
            );
        }
        let label = match &result.operation {
            TrashViewOperation::Restore { .. } => "Restore from Trash",
            TrashViewOperation::DeletePermanently => "Delete Permanently",
            TrashViewOperation::Empty => "Empty Trash",
        };
        let detail = if result.failure_count > 0 || !result.restore_conflicts.is_empty() {
            format!(
                "{} completed, {} failed, {} conflict(s)",
                result.success_count,
                result.failure_count,
                result.restore_conflicts.len()
            )
        } else {
            count_label(result.success_count, "item", "items")
        };
        let status = if result.failure_count > 0 || !result.restore_conflicts.is_empty() {
            ShellTaskStatus::failed(format!("{label} needs attention"), detail, false)
        } else {
            ShellTaskStatus::completed(label, detail, false)
        };
        if let Some(task_id) = task_id {
            self.finish_task_status(task_id, status);
        } else {
            self.record_task_status(status);
        }

        if let Some(dialog) = ShellTrashConflictDialog::new(result.restore_conflicts.clone()) {
            self.trash_conflict_dialog = Some(dialog);
            self.context_target = None;
            self.context_menu = None;
            self.drop_menu = None;
            self.properties_overlay = None;
            self.create_dialog = None;
            self.rename_dialog = None;
            self.rubber_band = None;
            fika_log!(
                "[fika-wgpu] trash-conflict open=1 conflicts={} changes={}",
                result.restore_conflicts.len(),
                self.trash_changes
            );
        }

        if result.success_count > 0 {
            let pane_to_clear = pane_to_reload;
            self.context_target = None;
            self.context_menu = None;
            self.drop_menu = None;
            self.properties_overlay = None;
            self.create_dialog = None;
            self.rename_dialog = None;
            self.rubber_band = None;
            if let Some(selection) = self.pane_selection_mut(pane_to_clear) {
                selection.clear();
            }
            if let Some(affected_dir) = affected_dir {
                self.reload_panes_showing_path(&affected_dir, size)?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn move_context_target_to_trash(
        &mut self,
        size: PhysicalSize<u32>,
        privileged: bool,
    ) -> Result<ShellTrashResult, String> {
        let pane_to_reload = self
            .context_target_pane()
            .unwrap_or_else(|| self.active_pane());
        let affected_dir = self
            .pane_state(pane_to_reload)
            .map(|state| state.path.clone());
        let paths = self.context_target_trash_paths()?;
        if paths.iter().any(|path| is_network_path(path)) {
            return Err("remote trash is not available yet".to_string());
        }

        let result = match trash_paths_with_privilege(&paths, privileged) {
            Ok(result) => result,
            Err(error) => {
                self.record_task_status(ShellTaskStatus::failed(
                    "Move to Trash failed",
                    task_error_detail(
                        &error,
                        !privileged && should_attempt_privileged_operation(&error),
                    ),
                    privileged,
                ));
                return Err(error);
            }
        };
        self.record_trash_content_change();
        fika_log!(
            "[fika-wgpu] trash paths={} success={} failure={} privileged={} changes={}",
            paths.len(),
            result.success_count,
            result.failure_count,
            result.privileged as u8,
            self.trash_changes
        );
        self.record_task_status(if result.failure_count > 0 {
            ShellTaskStatus::failed(
                if result.privileged {
                    "Administrator move to Trash failed"
                } else {
                    "Move to Trash failed"
                },
                transfer_task_detail(
                    result.success_count,
                    result.failure_count,
                    Path::new("Trash"),
                    result.first_error.as_deref(),
                    result.administrator_available,
                ),
                result.privileged,
            )
        } else {
            ShellTaskStatus::completed(
                if result.privileged {
                    "Administrator move to Trash"
                } else {
                    "Moved to Trash"
                },
                paths_task_summary(&paths),
                result.privileged,
            )
        });

        if !result.changed() {
            return Ok(result);
        }

        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.rubber_band = None;
        if let Some(affected_dir) = affected_dir {
            self.reload_panes_showing_path(&affected_dir, size)?;
        }
        Ok(result)
    }

    fn is_trash_conflict_dialog_open(&self) -> bool {
        self.trash_conflict_dialog.is_some()
    }

    fn close_trash_conflict_dialog(&mut self) -> bool {
        if self.trash_conflict_dialog.take().is_none() {
            return false;
        }
        self.trash_changes += 1;
        fika_log!(
            "[fika-wgpu] trash-conflict open=0 changes={}",
            self.trash_changes
        );
        true
    }

    #[cfg(test)]
    fn trash_conflict_dialog_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> TrashConflictDialogClick {
        let Some(dialog) = self.trash_conflict_dialog.as_ref() else {
            return TrashConflictDialogClick::Outside;
        };
        let scale = self.ui_scale();
        let rect = trash_conflict_dialog_rect_scaled(dialog, size, scale);
        if !rect.contains(point) {
            return TrashConflictDialogClick::Outside;
        }
        if trash_conflict_dialog_cancel_button_rect_scaled(rect, scale).contains(point) {
            return TrashConflictDialogClick::Cancel;
        }
        if trash_conflict_dialog_replace_button_rect_scaled(rect, scale).contains(point) {
            return TrashConflictDialogClick::Replace;
        }
        TrashConflictDialogClick::Inside
    }

    fn trash_conflict_dialog_window_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> TrashConflictDialogClick {
        if self.trash_conflict_dialog.is_none() {
            return TrashConflictDialogClick::Outside;
        }
        let rect = trash_conflict_dialog_window_rect(size);
        if trash_conflict_dialog_cancel_button_rect_scaled(rect, self.ui_scale()).contains(point) {
            return TrashConflictDialogClick::Cancel;
        }
        if trash_conflict_dialog_replace_button_rect_scaled(rect, self.ui_scale()).contains(point) {
            return TrashConflictDialogClick::Replace;
        }
        TrashConflictDialogClick::Inside
    }
}
