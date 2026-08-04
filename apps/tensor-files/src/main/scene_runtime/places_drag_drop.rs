impl ShellScene {

    fn add_pane_folder_to_places_gap(
        &mut self,
        path: PathBuf,
        gap_index: usize,
        user_places_path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        if is_network_path(&path) {
            return Ok(false);
        }
        if let Some(existing_index) = self.places.iter().position(|place| place.path == path) {
            if !self.place_participates_in_dnd(existing_index) {
                return Ok(false);
            }
            return self.move_place_to_gap(existing_index, gap_index, user_places_path, size);
        }
        let label = default_shell_place_label(&path);
        if !add_user_place_at_path(user_places_path, &path, label.clone())? {
            return Ok(false);
        }
        self.places = rebuild_shell_places_for_user_path(user_places_path);
        let Some(source_index) = self.places.iter().position(|place| place.path == path) else {
            save_shell_place_order(user_places_path, &self.places)?;
            return Ok(false);
        };
        let target_gap = gap_index.min(self.places.len());
        if self.move_place_to_gap(source_index, target_gap, user_places_path, size)? {
            return Ok(true);
        }
        save_shell_place_order(user_places_path, &self.places)?;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.rubber_band = None;
        self.clamp_places_scroll(size);
        self.places_changes += 1;
        self.refresh_hover(size);
        tensor_files_log!(
            "[tensor-files] add-place label={:?} path={} gap={} places={} changes={}",
            label,
            path.display(),
            gap_index,
            self.places.len(),
            self.places_changes
        );
        Ok(true)
    }

    fn target_dir_for_drop_target(&self, target: &ShellDropTarget) -> Option<PathBuf> {
        match target {
            ShellDropTarget::PaneItem { path, is_dir, .. } if *is_dir => Some(path.clone()),
            ShellDropTarget::PaneBlank { path, .. } | ShellDropTarget::Place { path, .. } => {
                Some(path.clone())
            }
            ShellDropTarget::PaneItem { .. }
            | ShellDropTarget::PlacesGap { .. }
            | ShellDropTarget::PlacesBlank => None,
        }
    }

    fn open_context_target(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        let target = self.context_target_for_screen_point(point, size);
        let old_target = self.context_target.clone();
        let old_rubber_band_active = self.rubber_band.as_ref().is_some_and(|band| band.active);
        let rubber_band_cleared = self.rubber_band.take().is_some();
        let hover = target.as_ref().and_then(|target| match target {
            ShellContextTarget::Item { pane, index, .. } => Some(ShellPaneItemTarget {
                pane: *pane,
                index: *index,
            }),
            ShellContextTarget::Blank { .. } | ShellContextTarget::Place { .. } => None,
        });
        let hover_changed = self.set_hovered_item(hover);
        let place_hover = target.as_ref().and_then(|target| match target {
            ShellContextTarget::Place { index, .. } => Some(*index),
            ShellContextTarget::Item { .. } | ShellContextTarget::Blank { .. } => None,
        });
        let place_hover_changed = self.set_hovered_place(place_hover);

        let mut selection_changed = false;
        if let Some(ShellContextTarget::Item { pane, index, .. }) = target.as_ref() {
            self.active_pane = self.normalized_pane_id(*pane);
            selection_changed = if self
                .pane_selection(*pane)
                .is_some_and(|selection| selection.contains(*index))
            {
                self.pane_selection_mut(*pane)
                    .is_some_and(|selection| selection.focus_selected(*index))
            } else {
                self.pane_selection_mut(*pane)
                    .is_some_and(|selection| selection.apply_click(Some(*index), false, false))
            };
            if selection_changed {
                self.selection_changes += 1;
            }
        } else if let Some(ShellContextTarget::Blank { pane, .. }) = target.as_ref() {
            self.active_pane = self.normalized_pane_id(*pane);
            selection_changed = self
                .pane_selection_mut(*pane)
                .is_some_and(ShellSelection::clear);
            if selection_changed {
                self.selection_changes += 1;
            }
        }

        let target_changed = old_target != target;
        self.context_target = target;
        if target_changed {
            self.context_target_changes += 1;
            self.log_context_target();
        }

        target_changed
            || hover_changed
            || place_hover_changed
            || selection_changed
            || rubber_band_cleared
            || old_rubber_band_active
    }

    fn is_context_menu_open(&self) -> bool {
        self.context_menu.is_some()
    }

    fn is_drop_menu_open(&self) -> bool {
        self.drop_menu.is_some()
    }

    fn close_drop_menu(&mut self) -> bool {
        if self.drop_menu.take().is_none() {
            return false;
        }
        tensor_files_log!("[tensor-files] dnd-menu open=0");
        true
    }

    fn update_drop_menu_hover(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let row = self.drop_menu_row_at_screen_point(point, size);
        let Some(menu) = self.drop_menu.as_mut() else {
            return false;
        };
        let changed = menu.hovered_row != row;
        menu.hovered_row = row;
        changed
    }

    fn drop_menu_command_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellDropMenuCommand> {
        let row = self.drop_menu_row_at_screen_point(point, size)?;
        drop_menu_items().get(row).map(|item| item.command)
    }

    fn drop_menu_row_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<usize> {
        let menu = self.drop_menu.as_ref()?;
        drop_menu_row_at_screen_point(menu, point, size, self.ui_scale())
    }

    fn activate_or_close_drop_menu_request(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellDropOperationRequest> {
        let command = self.drop_menu_command_at_screen_point(point, size);
        let menu = self.drop_menu.take()?;
        match command {
            Some(ShellDropMenuCommand::Mode { mode, privileged }) => {
                let request = ShellDropOperationRequest {
                    sources: menu.sources,
                    target_dir: menu.target_dir,
                    target: menu.target,
                    mode,
                    privileged,
                };
                self.pending_drop_request = Some(request.clone());
                self.dnd_drop_requests += 1;
                tensor_files_log!(
                    "[tensor-files] dnd-drop-request sources={} target={} mode={} privileged={} requests={}",
                    request.sources.len(),
                    request.target_dir.display(),
                    request.mode.operation(),
                    request.privileged as u8,
                    self.dnd_drop_requests
                );
                Some(request)
            }
            Some(ShellDropMenuCommand::Cancel) | None => {
                tensor_files_log!("[tensor-files] dnd-menu open=0");
                None
            }
        }
    }

    // Production drop path is async via TensorFilesApp::perform_drop_operation_request.
    // Sync helper reuses apply_transfer_result so bookkeeping matches production.
    #[cfg(test)]
    #[allow(dead_code)]
    fn perform_drop_operation_request(
        &mut self,
        request: &ShellDropOperationRequest,
        size: PhysicalSize<u32>,
    ) -> Result<ShellPasteResult, String> {
        self.validate_drop_operation_request(request)?;
        let transfer = transfer_paths_with_privilege(
            request.target_dir.clone(),
            request.mode,
            request.sources.clone(),
            request.mode.label(),
            false,
            request.privileged,
        );
        self.apply_transfer_result(
            &transfer,
            ShellAsyncTransferSource::Drop,
            &request.target_dir,
            None,
            size,
        )
    }

    fn validate_drop_operation_request(
        &self,
        request: &ShellDropOperationRequest,
    ) -> Result<(), String> {
        if request.sources.is_empty() {
            return Err("no drop sources".to_string());
        }
        if is_network_path(&request.target_dir) {
            return Err("remote drop target is not available yet".to_string());
        }
        if request.sources.iter().any(|path| is_network_path(path)) {
            return Err("remote drop source is not available yet".to_string());
        }
        Ok(())
    }

    #[cfg(test)]
    fn open_context_menu(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.open_context_menu_with_cache(point, size, &MimeApplicationCache::empty())
    }

    fn open_context_menu_with_cache(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        cache: &MimeApplicationCache,
    ) -> bool {
        let changed = self.open_context_target(point, size);
        let old_menu = self.context_menu.clone();
        self.drop_menu = None;
        self.context_menu_safe_triangle.reset();
        self.context_menu = self.context_target.clone().map(|target| {
            let (open_with_apps, service_actions) = self.context_menu_dynamic_data(&target, cache);
            ShellContextMenu::with_dynamic(target, point, open_with_apps, service_actions)
        });
        let menu_changed = old_menu != self.context_menu;
        if menu_changed {
            tensor_files_log!(
                "[tensor-files] context-menu open={} target={} actions={}",
                self.context_menu.is_some() as u8,
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none"),
                self.context_menu
                    .as_ref()
                    .map(|menu| context_menu_items(menu).len())
                    .unwrap_or(0)
            );
        }
        changed || menu_changed
    }

    fn context_menu_dynamic_data(
        &self,
        target: &ShellContextTarget,
        cache: &MimeApplicationCache,
    ) -> (Vec<MimeApplication>, Vec<ServiceMenuAction>) {
        match target {
            ShellContextTarget::Item {
                pane,
                index,
                path,
                is_dir,
                ..
            } => {
                let mime_type = self
                    .pane_state(*pane)
                    .and_then(|pane| pane.entries.get(*index))
                    .and_then(|entry| entry.mime_type.as_deref());
                let open_with_apps = if file_ops::is_in_trash_files_dir(path)
                    || (*is_dir && is_network_path(path))
                {
                    Vec::new()
                } else if *is_dir {
                    open_with_applications_for_mime(cache, Some("inode/directory"))
                } else {
                    open_with_applications_for_mime(cache, mime_type)
                };
                let mut service_actions =
                    if file_ops::is_in_trash_files_dir(path) || is_network_path(path) {
                        Vec::new()
                    } else {
                        cache.service_actions_for_targets(
                            &self.service_menu_targets_for_context_item(
                                *pane, *index, *is_dir, mime_type,
                            ),
                        )
                    };
                self.append_builtin_ark_service_actions(target, &mut service_actions);
                (open_with_apps, service_actions)
            }
            ShellContextTarget::Blank { path, .. } => {
                let open_with_apps = if file_ops::is_trash_files_dir(path) || is_network_path(path)
                {
                    Vec::new()
                } else {
                    open_with_applications_for_mime(cache, Some("inode/directory"))
                };
                let service_actions =
                    if file_ops::is_trash_files_dir(path) || is_network_path(path) {
                        Vec::new()
                    } else {
                        cache.service_actions_for_targets(&[ServiceMenuTarget::new(
                            Some("inode/directory"),
                            true,
                        )])
                    };
                (open_with_apps, service_actions)
            }
            ShellContextTarget::Place { .. } => (Vec::new(), Vec::new()),
        }
    }

    fn append_builtin_ark_service_actions(
        &self,
        target: &ShellContextTarget,
        service_actions: &mut Vec<ServiceMenuAction>,
    ) {
        let Ok(items) = self.context_target_ark_items(target) else {
            return;
        };
        ui::ark::append_builtin_service_actions(&items, service_actions);
    }

    fn context_target_ark_items(
        &self,
        target: &ShellContextTarget,
    ) -> Result<Vec<ArkContextItem>, String> {
        let ShellContextTarget::Item {
            pane,
            index,
            path,
            is_dir,
            selection_count,
        } = target
        else {
            return Ok(Vec::new());
        };
        if file_ops::is_in_trash_files_dir(path) || is_network_path(path) {
            return Ok(Vec::new());
        }

        let pane_view = self
            .pane_view(*pane)
            .ok_or_else(|| "context target pane no longer exists".to_string())?;
        if *selection_count > 1 && pane_view.selection.contains(*index) {
            let items = self
                .pane_selection(*pane)
                .into_iter()
                .flat_map(|selection| selection.selected.iter())
                .copied()
                .filter_map(|selected| {
                    let entry = pane_view.entries.get(selected)?;
                    Some(ArkContextItem {
                        path: self.entry_path_for_pane_view(pane_view, selected)?,
                        is_dir: entry.is_dir,
                        mime_type: entry.mime_type.as_deref().map(str::to_string),
                    })
                })
                .collect::<Vec<_>>();
            return Ok(items);
        }

        let mime_type = pane_view
            .entries
            .get(*index)
            .and_then(|entry| entry.mime_type.as_deref())
            .map(str::to_string);
        Ok(vec![ArkContextItem {
            path: path.clone(),
            is_dir: *is_dir,
            mime_type,
        }])
    }
}
