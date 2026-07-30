impl ShellScene {

    fn set_open_with_chooser_error(&mut self, error: String) -> bool {
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            fika_log!("[fika] open-with-error {error}");
            return false;
        };
        if chooser.error.as_ref() == Some(&error) {
            return false;
        }
        chooser.error = Some(error);
        self.open_with_changes += 1;
        self.log_open_with_chooser_state();
        true
    }

    fn close_open_with_chooser(&mut self) -> bool {
        if self.open_with_chooser.take().is_none() {
            return false;
        }
        self.open_with_changes += 1;
        fika_log!(
            "[fika] open-with open=0 changes={}",
            self.open_with_changes
        );
        true
    }

    fn close_open_with_chooser_after_success(&mut self, request: &OpenWithLaunchRequest) -> bool {
        if self.open_with_chooser.take().is_none() {
            return false;
        }
        self.open_with_changes += 1;
        fika_log!(
            "[fika] open-with path={} app={:?} changes={}",
            request.path.display(),
            request.app_name,
            self.open_with_changes
        );
        true
    }

    fn open_with_chooser_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> OpenWithChooserClick {
        let Some(chooser) = self.open_with_chooser.as_ref() else {
            return OpenWithChooserClick::Outside;
        };
        open_with_chooser_click_at_point(chooser, point, size, self.ui_scale())
    }

    fn log_open_with_chooser_state(&self) {
        match self.open_with_chooser.as_ref() {
            Some(chooser) => fika_log!(
                "[fika] open-with open=1 path={} mime={} apps={} filtered={} category={} selected={} scroll={} set_default={} query={:?} cursor={} error={:?} changes={}",
                chooser.path.display(),
                chooser.mime_type.as_deref().unwrap_or("unknown"),
                chooser.applications.len(),
                chooser.filtered_count(),
                chooser
                    .selected_category_row()
                    .map(|category| category.label)
                    .unwrap_or("unknown"),
                chooser.selected_index,
                chooser.scroll_row,
                chooser.set_as_default as u8,
                chooser.query,
                chooser.query_cursor,
                chooser.error,
                self.open_with_changes
            ),
            None => fika_log!(
                "[fika] open-with open=0 changes={}",
                self.open_with_changes
            ),
        }
    }

    fn context_target_copy_location_request(&self) -> Option<CopyLocationRequest> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Item { path, .. } | ShellContextTarget::Place { path, .. } => {
                Some(CopyLocationRequest {
                    path: path.clone(),
                    text: copy_location_text_for_path(path),
                })
            }
            ShellContextTarget::Blank { .. } => None,
        }
    }

    fn context_target_pane(&self) -> Option<ShellPaneId> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Item { pane, .. } | ShellContextTarget::Blank { pane, .. } => {
                Some(self.normalized_pane_id(*pane))
            }
            ShellContextTarget::Place { .. } => Some(self.active_pane()),
        }
    }

    fn context_target_device_action(
        &self,
        action: ShellContextMenuAction,
    ) -> Option<DeviceActionRequest> {
        let operation = device_place_operation_for_context_action(action)?;
        let ShellContextTarget::Place {
            label,
            path,
            device: Some(device),
            ..
        } = self.context_target.as_ref()?
        else {
            return None;
        };
        Some(DeviceActionRequest {
            id: device.id.clone(),
            label: label.clone(),
            action,
            operation,
            pane: self
                .context_target_pane()
                .unwrap_or_else(|| self.active_pane()),
            path: path.clone(),
        })
    }

    fn apply_device_place_operation_result(
        &mut self,
        request: &DeviceActionRequest,
        result: &DevicePlaceOperationResult,
        size: PhysicalSize<u32>,
    ) -> Result<(), String> {
        self.places = build_shell_places();
        self.clamp_places_scroll(size);
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.rubber_band = None;
        self.internal_drag = None;
        self.external_drag = None;
        self.place_press = None;
        self.places_changes += 1;
        self.refresh_hover(size);
        match &result.result {
            Ok(mount_point) => {
                let mount_detail = mount_point
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "-".to_string());
                fika_log!(
                    "[fika] device-action-finished action={} id={:?} label={:?} mount={} changes={}",
                    request.action.as_str(),
                    result.device_id,
                    result.label,
                    mount_detail,
                    self.places_changes
                );
                self.record_task_status(ShellTaskStatus::completed(
                    request.action.label(),
                    format!("{} | mount {}", request.label, mount_detail),
                    false,
                ));
            }
            Err(error) => {
                fika_log!(
                    "[fika] device-action-finished action={} id={:?} label={:?} error={error} changes={}",
                    request.action.as_str(),
                    result.device_id,
                    result.label,
                    self.places_changes
                );
                self.record_task_status(ShellTaskStatus::failed(
                    format!("{} failed", request.action.label()),
                    format!("{} | {error}", request.label),
                    false,
                ));
            }
        }

        if result.result.is_ok() && !matches!(request.operation, DevicePlaceOperation::Mount) {
            self.leave_device_path_after_unmount(request, size)?;
        }
        Ok(())
    }

    fn leave_device_path_after_unmount(
        &mut self,
        request: &DeviceActionRequest,
        size: PhysicalSize<u32>,
    ) -> Result<(), String> {
        if is_network_path(&request.path) || !request.path.is_absolute() {
            return Ok(());
        }
        let home = home_dir();
        let pane_ids = ShellPaneId::ALL
            .into_iter()
            .filter(|pane| {
                self.pane_state(*pane).is_some_and(|state| {
                    state.path == request.path || state.path.starts_with(&request.path)
                })
            })
            .collect::<Vec<_>>();
        for pane in pane_ids {
            self.load_path_in_pane(pane, home.clone(), size, true)?;
        }
        Ok(())
    }

    fn record_copy_location(&mut self, request: &CopyLocationRequest) {
        self.copy_location_changes += 1;
        fika_log!(
            "[fika] copy-location path={} text={:?} changes={}",
            request.path.display(),
            request.text,
            self.copy_location_changes
        );
        self.record_task_status(ShellTaskStatus::completed(
            "Copied Location",
            request.path.display().to_string(),
            false,
        ));
    }

    fn record_task_status(&mut self, status: ShellTaskStatus) {
        self.task_statuses.record(status);
        if let Some(status) = self.task_statuses.front() {
            fika_log!(
                "[fika] task-status kind={:?} label={:?} privileged={} detail={:?} changes={}",
                status.kind,
                status.label,
                status.privileged as u8,
                status.detail,
                self.task_statuses.change_generation()
            );
        }
    }

    fn finish_task_status(&mut self, task_id: ShellTaskId, status: ShellTaskStatus) {
        self.task_statuses.finish(task_id, status);
    }

    fn record_async_transfer_started(
        &mut self,
        task_id: ShellTaskId,
        source: ShellAsyncTransferSource,
        mode: FileTransferMode,
        item_count: usize,
        detail: String,
        privileged: bool,
    ) {
        let label = async_transfer_task_label(source, mode, item_count, privileged);
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.rubber_band = None;
        self.internal_drag = None;
        self.external_drag = None;
        self.place_press = None;
        self.dnd_hover_target = None;
        self.pending_drop_request = None;
        self.record_task_status(ShellTaskStatus::running(
            task_id,
            label,
            detail,
            privileged,
        ));
    }

    fn record_async_move_to_trash_started(
        &mut self,
        task_id: ShellTaskId,
        path_count: usize,
        privileged: bool,
    ) {
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.rubber_band = None;
        self.internal_drag = None;
        self.external_drag = None;
        self.place_press = None;
        self.dnd_hover_target = None;
        self.pending_drop_request = None;
        self.record_task_status(ShellTaskStatus::running(
            task_id,
            if privileged {
                "Administrator move to Trash"
            } else {
                "Moving to Trash"
            },
            count_label(path_count, "item", "items"),
            privileged,
        ));
    }

    fn record_async_trash_view_started(
        &mut self,
        task_id: ShellTaskId,
        operation: TrashViewOperation,
        item_count: usize,
    ) {
        let detail = match operation {
            TrashViewOperation::Empty => "Trash contents".to_string(),
            _ => count_label(item_count, "item", "items"),
        };
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.rubber_band = None;
        self.internal_drag = None;
        self.external_drag = None;
        self.place_press = None;
        self.dnd_hover_target = None;
        self.pending_drop_request = None;
        self.record_task_status(ShellTaskStatus::running_uncancellable(
            task_id,
            operation.progress_label(item_count),
            detail,
            false,
        ));
    }

    fn update_running_task_detail(&mut self, task_id: ShellTaskId, detail: String) -> bool {
        self.task_statuses.update_running_detail(task_id, detail)
    }

    fn apply_async_transfer_completion(
        &mut self,
        completion: &ShellAsyncTransferCompletion,
        size: PhysicalSize<u32>,
    ) -> Result<ShellPasteResult, String> {
        self.apply_transfer_result(
            &completion.transfer,
            completion.source,
            &completion.target_dir,
            Some(completion.task_id),
            size,
        )
    }

    /// Shared paste/drop completion for production async and scene test sync helpers.
    fn apply_transfer_result(
        &mut self,
        transfer: &ShellTransferExecution,
        source: ShellAsyncTransferSource,
        target_dir: &Path,
        task_id: Option<ShellTaskId>,
        size: PhysicalSize<u32>,
    ) -> Result<ShellPasteResult, String> {
        let result = ShellPasteResult::from_transfer(transfer);
        self.paste_changes += 1;
        fika_log!(
            "[fika] transfer source={:?} mode={} target={} success={} failure={} cancelled={} privileged={} changes={}",
            source,
            result.mode.label(),
            target_dir.display(),
            result.success_count,
            result.failure_count,
            transfer.cancelled as u8,
            result.privileged as u8,
            self.paste_changes
        );
        let status = if transfer.cancelled {
            ShellTaskStatus::cancelled(
                match source {
                    ShellAsyncTransferSource::Paste => {
                        if result.privileged {
                            "Administrator paste cancelled".to_string()
                        } else {
                            "Paste cancelled".to_string()
                        }
                    }
                    ShellAsyncTransferSource::Drop => {
                        if result.privileged {
                            format!("Administrator {} cancelled", result.mode.label())
                        } else {
                            format!("{} cancelled", result.mode.label())
                        }
                    }
                },
                transfer_task_detail(
                    result.success_count,
                    result.failure_count,
                    target_dir,
                    result.first_error.as_deref(),
                    false,
                ),
                result.privileged,
            )
        } else if result.failure_count > 0 {
            ShellTaskStatus::failed(
                match source {
                    ShellAsyncTransferSource::Paste => {
                        if result.privileged {
                            "Administrator paste failed".to_string()
                        } else {
                            "Paste failed".to_string()
                        }
                    }
                    ShellAsyncTransferSource::Drop => {
                        if result.privileged {
                            format!("Administrator {} failed", result.mode.label())
                        } else {
                            format!("{} failed", result.mode.label())
                        }
                    }
                },
                transfer_task_detail(
                    result.success_count,
                    result.failure_count,
                    target_dir,
                    result.first_error.as_deref(),
                    result.administrator_available,
                ),
                result.privileged,
            )
        } else {
            ShellTaskStatus::completed(
                match source {
                    ShellAsyncTransferSource::Paste => {
                        if result.privileged {
                            "Administrator paste".to_string()
                        } else {
                            "Pasted".to_string()
                        }
                    }
                    ShellAsyncTransferSource::Drop => {
                        if result.privileged {
                            format!("Administrator {}", result.mode.label())
                        } else {
                            result.mode.label().to_string()
                        }
                    }
                },
                transfer_task_detail(
                    result.success_count,
                    result.failure_count,
                    target_dir,
                    None,
                    false,
                ),
                result.privileged,
            )
        };
        if let Some(task_id) = task_id {
            self.finish_task_status(task_id, status);
        } else {
            self.record_task_status(status);
        }

        if result.changed() {
            self.context_target = None;
            self.context_menu = None;
            self.drop_menu = None;
            self.properties_overlay = None;
            self.create_dialog = None;
            self.rename_dialog = None;
            self.rubber_band = None;
            for affected_dir in &transfer.result.refresh_dirs {
                self.reload_panes_showing_path(affected_dir, size)?;
            }
        }
        Ok(result)
    }

    fn apply_async_move_to_trash_completion(
        &mut self,
        completion: &ShellAsyncMoveToTrashCompletion,
        size: PhysicalSize<u32>,
    ) -> Result<ShellTrashResult, String> {
        self.apply_move_to_trash_result(
            completion.result.clone(),
            &completion.paths,
            completion.pane_to_reload,
            completion.clear_selection_pane,
            Some(completion.task_id),
            size,
        )
    }

    /// Shared completion for production async trash and scene test sync helpers.
    fn apply_move_to_trash_result(
        &mut self,
        result: ShellTrashResult,
        paths: &[PathBuf],
        pane_to_reload: ShellPaneId,
        clear_selection_pane: Option<ShellPaneId>,
        task_id: Option<ShellTaskId>,
        size: PhysicalSize<u32>,
    ) -> Result<ShellTrashResult, String> {
        self.record_trash_content_change();
        fika_log!(
            "[fika] trash paths={} success={} failure={} privileged={} changes={}",
            paths.len(),
            result.success_count,
            result.failure_count,
            result.privileged as u8,
            self.trash_changes
        );
        let status = if result.failure_count > 0 {
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
                paths_task_summary(paths),
                result.privileged,
            )
        };
        if let Some(task_id) = task_id {
            self.finish_task_status(task_id, status);
        } else {
            self.record_task_status(status);
        }

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
        if let Some(pane) = clear_selection_pane
            && let Some(selection) = self.pane_selection_mut(pane)
        {
            selection.clear();
        }
        if let Some(affected_dir) = self
            .pane_state(pane_to_reload)
            .map(|state| state.path.clone())
        {
            self.reload_panes_showing_path(&affected_dir, size)?;
        }
        Ok(result)
    }
    fn apply_async_trash_view_completion(
        &mut self,
        completion: &ShellAsyncTrashViewCompletion,
        size: PhysicalSize<u32>,
    ) -> Result<(), String> {
        self.apply_trash_view_result_with_task(
            Some(completion.task_id),
            completion.action.as_str(),
            completion.pane_to_reload,
            &completion.result,
            size,
        )
    }

    fn is_task_detail_dialog_open(&self) -> bool {
        self.task_detail_dialog.is_some()
    }

    fn open_task_detail_dialog_at_screen_point(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<bool> {
        let rect = self.places_task_area_rect(size)?;
        if !rect.contains(point) {
            return None;
        }
        if self.task_statuses.is_empty() {
            return Some(false);
        }
        let changed = self.task_detail_dialog.is_none();
        self.task_detail_dialog = Some(ShellTaskDetailDialog);
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.open_with_chooser = None;
        self.trash_conflict_dialog = None;
        self.rubber_band = None;
        self.internal_drag = None;
        self.external_drag = None;
        self.place_press = None;
        Some(changed)
    }

    fn task_detail_area_contains_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        self.places_task_area_rect(size)
            .is_some_and(|rect| rect.contains(point))
    }

    fn close_task_detail_dialog(&mut self) -> bool {
        if self.task_detail_dialog.take().is_none() {
            return false;
        }
        self.task_statuses.mark_changed();
        true
    }

    fn clear_task_statuses(&mut self) -> bool {
        if self.task_statuses.is_empty() && self.task_detail_dialog.is_none() {
            return false;
        }
        let statuses_changed = self.task_statuses.clear_finished();
        let mut dialog_changed = false;
        if self.task_statuses.is_empty() {
            dialog_changed = self.task_detail_dialog.take().is_some();
        }
        if dialog_changed && !statuses_changed {
            self.task_statuses.mark_changed();
        }
        statuses_changed || dialog_changed
    }
}
