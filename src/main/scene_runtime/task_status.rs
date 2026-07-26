impl ShellScene {

    fn dismiss_task_status(&mut self, index: usize) -> (bool, Option<ShellTaskId>) {
        let dismissal = self.task_statuses.dismiss(index);
        if !dismissal.changed {
            return (false, None);
        }
        if self.task_statuses.is_empty() {
            self.task_detail_dialog = None;
        }
        (true, dismissal.cancel_task_id)
    }

    #[cfg(test)]
    fn task_detail_dialog_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> TaskDetailDialogClick {
        if self.task_detail_dialog.is_none() {
            return TaskDetailDialogClick::Outside;
        }
        let scale = self.ui_scale();
        let rect = task_detail_dialog_rect_scaled(self.task_statuses.len(), size, scale);
        if !rect.contains(point) {
            return TaskDetailDialogClick::Outside;
        }
        if task_detail_cancel_button_rect_scaled(rect, scale).contains(point) {
            return TaskDetailDialogClick::Cancel;
        }
        if task_detail_clear_button_rect_scaled(rect, scale).contains(point) {
            return TaskDetailDialogClick::Clear;
        }
        for index in 0..self.task_statuses.len().min(4) {
            if task_detail_dismiss_button_rect_scaled(rect, index, scale).contains(point) {
                return TaskDetailDialogClick::Dismiss(index);
            }
        }
        TaskDetailDialogClick::Inside
    }

    fn task_detail_dialog_window_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> TaskDetailDialogClick {
        if self.task_detail_dialog.is_none() {
            return TaskDetailDialogClick::Outside;
        }
        let rect = task_detail_dialog_window_rect(size);
        if task_detail_cancel_button_rect_scaled(rect, self.ui_scale()).contains(point) {
            return TaskDetailDialogClick::Cancel;
        }
        if task_detail_clear_button_rect_scaled(rect, self.ui_scale()).contains(point) {
            return TaskDetailDialogClick::Clear;
        }
        for index in 0..self.task_statuses.len().min(4) {
            if task_detail_dismiss_button_rect_scaled(rect, index, self.ui_scale()).contains(point) {
                return TaskDetailDialogClick::Dismiss(index);
            }
        }
        TaskDetailDialogClick::Inside
    }

    fn context_target_add_place_candidate(&self) -> Result<(String, PathBuf), String> {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Item {
                path, is_dir: true, ..
            })
            | Some(ShellContextTarget::Blank { path, .. }) => {
                Ok((default_shell_place_label(path), path.clone()))
            }
            Some(ShellContextTarget::Item { is_dir: false, .. }) => {
                Err("only directories can be added to Places".to_string())
            }
            Some(ShellContextTarget::Place { .. }) => {
                Err("place context targets cannot be added to Places".to_string())
            }
            None => Err("no context target to add to Places".to_string()),
        }
    }

    fn add_context_target_to_places(
        &mut self,
        user_places_path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        let (label, path) = self.context_target_add_place_candidate()?;
        if self.places.iter().any(|place| place.path == path) {
            fika_log!(
                "[fika-wgpu] add-place label={:?} path={} added=0 duplicate=1 changes={}",
                label,
                path.display(),
                self.places_changes
            );
            self.record_task_status(ShellTaskStatus::failed(
                "Add to Places skipped",
                format!("{label} is already in Places"),
                false,
            ));
            return Ok(false);
        }
        if !add_user_place_at_path(user_places_path, &path, label.clone())? {
            fika_log!(
                "[fika-wgpu] add-place label={:?} path={} added=0 duplicate=1 changes={}",
                label,
                path.display(),
                self.places_changes
            );
            self.record_task_status(ShellTaskStatus::failed(
                "Add to Places skipped",
                format!("{label} was not added"),
                false,
            ));
            return Ok(false);
        }

        self.places = rebuild_shell_places_for_user_path(user_places_path);
        save_shell_place_order(user_places_path, &self.places)?;
        self.clamp_places_scroll(size);
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.rubber_band = None;
        self.places_changes += 1;
        self.refresh_hover(size);
        fika_log!(
            "[fika-wgpu] add-place label={:?} path={} added=1 places={} changes={}",
            label,
            path.display(),
            self.places.len(),
            self.places_changes
        );
        self.record_task_status(ShellTaskStatus::completed(
            "Added to Places",
            format!("{label} -> {}", path.display()),
            false,
        ));
        Ok(true)
    }

    fn add_network_folder_place(
        &mut self,
        user_places_path: &Path,
        path: &Path,
        label: String,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        if !is_network_path(path) || path == network_root_path() {
            return Err(format!("{} is not a network folder URL", path.display()));
        }
        if self.places.iter().any(|place| place.path == path) {
            fika_log!(
                "[fika-wgpu] add-network-folder label={:?} path={} added=0 duplicate=1 changes={}",
                label,
                path.display(),
                self.places_changes
            );
            self.record_task_status(ShellTaskStatus::completed(
                "Network Folder already added",
                format!("{label} -> {}", path.display()),
                false,
            ));
            return Ok(false);
        }
        if !add_user_place_at_path(user_places_path, path, label.clone())? {
            self.record_task_status(ShellTaskStatus::completed(
                "Network Folder already added",
                format!("{label} -> {}", path.display()),
                false,
            ));
            return Ok(false);
        }

        self.places = rebuild_shell_places_for_user_path(user_places_path);
        save_shell_place_order(user_places_path, &self.places)?;
        self.clamp_places_scroll(size);
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.rubber_band = None;
        self.places_changes += 1;
        self.refresh_hover(size);
        fika_log!(
            "[fika-wgpu] add-network-folder label={:?} path={} added=1 places={} changes={}",
            label,
            path.display(),
            self.places.len(),
            self.places_changes
        );
        self.record_task_status(ShellTaskStatus::completed(
            "Added Network Folder",
            format!("{label} -> {}", path.display()),
            false,
        ));
        Ok(true)
    }

    fn remove_context_place(
        &mut self,
        user_places_path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        let Some(ShellContextTarget::Place {
            label,
            path,
            editable,
            ..
        }) = self.context_target.as_ref()
        else {
            return Err("no place context target to remove".to_string());
        };
        if !editable {
            return Err(format!("place {label:?} is not removable"));
        }
        let label = label.clone();
        let path = path.clone();
        if !remove_user_place_at_path(user_places_path, &path)? {
            fika_log!(
                "[fika-wgpu] remove-place label={:?} path={} removed=0 changes={}",
                label,
                path.display(),
                self.places_changes
            );
            self.record_task_status(ShellTaskStatus::failed(
                "Remove Place skipped",
                format!("{label} was not removed"),
                false,
            ));
            return Ok(false);
        }

        self.places = rebuild_shell_places_for_user_path(user_places_path);
        self.clamp_places_scroll(size);
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.rubber_band = None;
        self.places_changes += 1;
        self.refresh_hover(size);
        fika_log!(
            "[fika-wgpu] remove-place label={:?} path={} removed=1 places={} changes={}",
            label,
            path.display(),
            self.places.len(),
            self.places_changes
        );
        self.record_task_status(ShellTaskStatus::completed(
            "Removed Place",
            format!("{label} -> {}", path.display()),
            false,
        ));
        Ok(true)
    }

    fn context_target_file_clipboard_request(
        &self,
        action: ShellContextMenuAction,
    ) -> Result<Option<FileClipboardExportRequest>, String> {
        let role = match action {
            ShellContextMenuAction::Copy => FileClipboardRole::Copy,
            ShellContextMenuAction::Cut => FileClipboardRole::Cut,
            _ => return Ok(None),
        };
        let Some(paths) = self.context_target_item_paths()? else {
            return Ok(None);
        };
        if role == FileClipboardRole::Cut && paths.iter().any(|path| is_network_path(path)) {
            return Err("remote cut is not available yet".to_string());
        }
        let text = encode_file_clipboard_text(role, &paths);
        Ok(Some(FileClipboardExportRequest { role, paths, text }))
    }

    fn active_file_clipboard_request(
        &self,
        role: FileClipboardRole,
    ) -> Result<Option<FileClipboardExportRequest>, String> {
        let Some(paths) = self.active_selection_item_paths()? else {
            return Ok(None);
        };
        if role == FileClipboardRole::Cut && paths.iter().any(|path| is_network_path(path)) {
            return Err("remote cut is not available yet".to_string());
        }
        let text = encode_file_clipboard_text(role, &paths);
        Ok(Some(FileClipboardExportRequest { role, paths, text }))
    }

    fn record_file_clipboard_export(&mut self, request: &FileClipboardExportRequest) {
        self.file_clipboard_changes += 1;
        fika_log!(
            "[fika-wgpu] clipboard-export role={} paths={} bytes={} changes={}",
            file_clipboard_role_as_str(request.role),
            request.paths.len(),
            request.text.len(),
            self.file_clipboard_changes
        );
        let label = match request.role {
            FileClipboardRole::Copy => "Copied to Clipboard",
            FileClipboardRole::Cut => "Cut to Clipboard",
        };
        self.record_task_status(ShellTaskStatus::completed(
            label,
            paths_task_summary(&request.paths),
            false,
        ));
    }

    fn context_target_paste_directory(&self) -> Option<(ShellPaneId, PathBuf)> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Blank { pane, path, .. } => Some((*pane, path.clone())),
            _ => None,
        }
    }

    fn active_pane_paste_directory(&self) -> Option<(ShellPaneId, PathBuf)> {
        let pane = self.active_pane();
        self.pane_state(pane)
            .map(|state| (pane, state.path.clone()))
    }

    // Production paste path is async via FikaWgpuApp::finish_paste_from_clipboard_text.
    #[cfg(test)]
    fn paste_clipboard_text_from_context(
        &mut self,
        clipboard_text: &str,
        size: PhysicalSize<u32>,
        privileged: bool,
    ) -> Result<ShellPasteResult, String> {
        let (target_pane, target_dir) = self
            .context_target_paste_directory()
            .or_else(|| self.active_pane_paste_directory())
            .ok_or_else(|| "no paste target pane".to_string())?;
        self.paste_clipboard_text_to_pane(target_pane, target_dir, clipboard_text, size, privileged)
    }

    #[cfg(test)]
    fn paste_clipboard_text_into_active_pane(
        &mut self,
        clipboard_text: &str,
        size: PhysicalSize<u32>,
        privileged: bool,
    ) -> Result<ShellPasteResult, String> {
        let (target_pane, target_dir) = self
            .active_pane_paste_directory()
            .ok_or_else(|| "no paste target pane".to_string())?;
        self.paste_clipboard_text_to_pane(target_pane, target_dir, clipboard_text, size, privileged)
    }

    #[cfg(test)]
    fn paste_clipboard_text_to_pane(
        &mut self,
        _target_pane: ShellPaneId,
        target_dir: PathBuf,
        clipboard_text: &str,
        size: PhysicalSize<u32>,
        privileged: bool,
    ) -> Result<ShellPasteResult, String> {
        if is_network_path(&target_dir) {
            return Err("remote paste target is not available yet".to_string());
        }
        if clipboard_text.trim().is_empty() {
            return Err("clipboard is empty".to_string());
        }

        let transfer = if let Some(payload) = decode_file_clipboard_text(clipboard_text) {
            if payload.paths.iter().any(|path| is_network_path(path)) {
                return Err("remote paste source is not available yet".to_string());
            }
            let mode = match payload.role {
                FileClipboardRole::Copy => FileTransferMode::Copy,
                FileClipboardRole::Cut => FileTransferMode::Move,
            };
            transfer_paths_with_privilege(
                target_dir.clone(),
                mode,
                payload.paths,
                "Paste",
                payload.role == FileClipboardRole::Cut,
                privileged,
            )
        } else if privileged {
            return Err(
                "administrator paste is only available for file clipboard items".to_string(),
            );
        } else {
            ShellTransferExecution {
                result: paste_text_result(WGPU_SHELL_PANE_ID, target_dir.clone(), clipboard_text),
                privileged: false,
                administrator_available: false,
                first_error: None,
                cancelled: false,
            }
        };

        self.apply_transfer_result(
            &transfer,
            ShellAsyncTransferSource::Paste,
            &target_dir,
            None,
            size,
        )
    }

    fn context_target_item_paths(&self) -> Result<Option<Vec<PathBuf>>, String> {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Item {
                pane,
                index,
                path,
                selection_count,
                ..
            }) => {
                let Some(pane_view) = self.pane_view(*pane) else {
                    return Err("context target pane no longer exists".to_string());
                };
                if *selection_count > 1 && pane_view.selection.contains(*index) {
                    let paths = self
                        .pane_selection(*pane)
                        .into_iter()
                        .flat_map(|selection| selection.selected.iter())
                        .copied()
                        .filter_map(|index| self.entry_path_for_pane_view(pane_view, index))
                        .collect::<Vec<_>>();
                    if paths.is_empty() {
                        return Err("selected context target no longer exists".to_string());
                    }
                    Ok(Some(paths))
                } else {
                    Ok(Some(vec![path.clone()]))
                }
            }
            Some(ShellContextTarget::Blank { .. })
            | Some(ShellContextTarget::Place { .. })
            | None => Ok(None),
        }
    }
}
