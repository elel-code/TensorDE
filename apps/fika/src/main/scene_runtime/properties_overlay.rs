impl ShellScene {

    fn is_properties_overlay_open(&self) -> bool {
        self.properties_overlay.is_some()
    }

    fn open_properties_overlay_from_context(&mut self) -> bool {
        let Some(overlay) = self.properties_overlay_for_context_target() else {
            fika_log!("[fika] properties-error target=none");
            return false;
        };
        let changed = self.properties_overlay.as_ref() != Some(&overlay);
        self.properties_overlay = Some(overlay);
        if changed {
            self.properties_changes += 1;
            if let Some(overlay) = self.properties_overlay.as_ref() {
                fika_log!(
                    "[fika] properties open=1 title={:?} rows={} changes={}",
                    overlay.title,
                    overlay.rows.len(),
                    self.properties_changes
                );
            }
        }
        changed
    }

    fn close_properties_overlay(&mut self) -> bool {
        if self.properties_overlay.take().is_none() {
            return false;
        }
        self.properties_changes += 1;
        fika_log!(
            "[fika] properties open=0 changes={}",
            self.properties_changes
        );
        true
    }

    #[cfg(test)]
    fn close_properties_overlay_if_outside(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some(overlay) = self.properties_overlay.as_ref() else {
            return false;
        };
        if properties_overlay_rect_scaled(overlay, size, self.ui_scale()).contains(point) {
            return false;
        }
        self.close_properties_overlay()
    }

    fn is_create_dialog_open(&self) -> bool {
        self.create_dialog.is_some()
    }

    fn open_create_dialog_from_context(&mut self) -> bool {
        self.open_create_dialog_from_context_with_kind(CreateEntryKind::Folder, false)
    }

    fn open_create_dialog_for_autosmoke(&mut self) -> bool {
        let pane = self.active_pane();
        let Some(path) = self.pane_state(pane).map(|state| state.path.clone()) else {
            fika_log!("[fika] dialog-smoke failed reason=no-active-pane");
            return false;
        };
        self.open_create_dialog_for_parent(pane, path, CreateEntryKind::Folder, false)
    }

    fn open_open_with_chooser_for_autosmoke(&mut self, cache: &MimeApplicationCache) -> bool {
        let pane = self.active_pane();
        let Some(target) = self.first_item_context_target_for_autosmoke(pane) else {
            fika_log!("[fika] dialog-smoke failed reason=no-open-with-target");
            return false;
        };
        self.context_target = Some(target);
        self.open_open_with_chooser_from_context(cache)
    }

    fn open_rename_dialog_for_autosmoke(&mut self) -> bool {
        let pane = self.active_pane();
        let Some(target) = self.first_item_context_target_for_autosmoke(pane) else {
            fika_log!("[fika] dialog-smoke failed reason=no-rename-target");
            return false;
        };
        let index = match target {
            ShellContextTarget::Item { index, .. } => index,
            ShellContextTarget::Blank { .. } | ShellContextTarget::Place { .. } => return false,
        };
        if self
            .pane_selection_mut(pane)
            .is_some_and(|selection| selection.apply_navigation(index, false))
        {
            self.selection_changes += 1;
        }
        self.open_rename_dialog_from_active_selection(false)
    }

    fn first_item_context_target_for_autosmoke(
        &self,
        pane: ShellPaneId,
    ) -> Option<ShellContextTarget> {
        let view = self.pane_view(pane)?;
        view.filtered_indexes.iter().copied().find_map(|index| {
            let entry = view.entries.get(index)?;
            Some(ShellContextTarget::Item {
                pane,
                index,
                path: self.entry_path_for_pane_view(view, index)?,
                is_dir: entry.is_dir,
                selection_count: 1,
            })
        })
    }

    fn open_create_dialog_from_context_with_kind(
        &mut self,
        kind: CreateEntryKind,
        privileged: bool,
    ) -> bool {
        let Some(ShellContextTarget::Blank { pane, path, .. }) = self.context_target.as_ref()
        else {
            fika_log!(
                "[fika] create-new-error target={}",
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none")
            );
            return false;
        };
        self.open_create_dialog_for_parent(*pane, path.clone(), kind, privileged)
    }

    fn open_create_dialog_for_parent(
        &mut self,
        pane: ShellPaneId,
        path: PathBuf,
        kind: CreateEntryKind,
        privileged: bool,
    ) -> bool {
        let dialog = ShellCreateDialog::new(pane, path.clone(), kind, privileged);
        let changed = self.create_dialog.as_ref() != Some(&dialog);
        self.create_dialog = Some(dialog);
        self.properties_overlay = None;
        self.rename_dialog = None;
        self.rubber_band = None;
        if changed {
            self.create_changes += 1;
            if let Some(dialog) = self.create_dialog.as_ref() {
                fika_log!(
                    "[fika] create-new open=1 kind={} parent={} name={:?} privileged={} changes={}",
                    dialog.kind.as_str(),
                    dialog.parent.display(),
                    dialog.name,
                    dialog.privileged as u8,
                    self.create_changes
                );
            }
        }
        changed
    }

    fn apply_create_command(&mut self, command: CreateCommand, _size: PhysicalSize<u32>) -> bool {
        let old_dialog = self.create_dialog.clone();
        match command {
            CreateCommand::Insert(value) => {
                let Some(dialog) = self.create_dialog.as_mut() else {
                    return false;
                };
                if dialog.replace_on_insert {
                    dialog.name.clear();
                    dialog.replace_on_insert = false;
                }
                dialog.preedit = None;
                dialog.name.push_str(&value);
                dialog.error = None;
            }
            CreateCommand::Backspace => {
                let Some(dialog) = self.create_dialog.as_mut() else {
                    return false;
                };
                if dialog.replace_on_insert {
                    dialog.name.clear();
                    dialog.replace_on_insert = false;
                } else {
                    dialog.name.pop();
                }
                dialog.preedit = None;
                dialog.error = None;
            }
            CreateCommand::Cancel => {
                return self.close_create_dialog();
            }
            CreateCommand::SetKind(kind) => {
                let Some(dialog) = self.create_dialog.as_mut() else {
                    return false;
                };
                if dialog.kind == kind {
                    return false;
                }
                dialog.kind = kind;
                dialog.name = unique_child_name(&dialog.parent, kind.default_name());
                dialog.error = None;
                dialog.replace_on_insert = true;
                dialog.preedit = None;
            }
            CreateCommand::Commit | CreateCommand::Ignore => return false,
        }

        let changed = old_dialog != self.create_dialog;
        if changed {
            self.create_changes += 1;
            self.log_create_dialog_state();
        }
        changed
    }

    fn create_entry_request(&self) -> Result<CreateEntryRequest, String> {
        let dialog = self
            .create_dialog
            .as_ref()
            .ok_or_else(|| "create dialog is not open".to_string())?;
        if dialog.busy {
            return Err("create is already in progress".to_string());
        }
        let name = dialog.name.trim();
        validate_create_name(name)?;
        let path = dialog.parent.join(name);
        if path.exists() {
            return Err(format!("{} already exists", path.display()));
        }
        Ok(CreateEntryRequest {
            pane: dialog.pane,
            parent: dialog.parent.clone(),
            path,
            kind: dialog.kind,
            name: name.to_string(),
            privileged: dialog.privileged,
        })
    }

    fn set_create_dialog_error(&mut self, error: String) -> bool {
        let Some(dialog) = self.create_dialog.as_mut() else {
            fika_log!("[fika] create-new-error {error}");
            return false;
        };
        let already = dialog.error.as_ref() == Some(&error) && !dialog.busy;
        if already {
            return false;
        }
        dialog.error = Some(error);
        dialog.busy = false;
        dialog.replace_on_insert = false;
        self.create_changes += 1;
        self.log_create_dialog_state();
        true
    }

    fn set_create_dialog_busy(&mut self, busy: bool) -> bool {
        let Some(dialog) = self.create_dialog.as_mut() else {
            return false;
        };
        if dialog.busy == busy {
            return false;
        }
        dialog.busy = busy;
        if busy {
            dialog.error = None;
        }
        self.create_changes += 1;
        true
    }

    fn close_create_dialog(&mut self) -> bool {
        if self.create_dialog.take().is_none() {
            return false;
        }
        self.create_changes += 1;
        fika_log!(
            "[fika] create-new open=0 changes={}",
            self.create_changes
        );
        true
    }

    fn close_create_dialog_after_success(&mut self, request: &CreateEntryRequest) -> bool {
        if self.create_dialog.take().is_none() {
            return false;
        }
        self.create_changes += 1;
        fika_log!(
            "[fika] create-new created kind={} path={} changes={}",
            request.kind.as_str(),
            request.path.display(),
            self.create_changes
        );
        true
    }

    fn create_dialog_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> CreateDialogClick {
        let Some(dialog) = self.create_dialog.as_ref() else {
            return CreateDialogClick::Outside;
        };
        let scale = self.ui_scale();
        let rect = create_dialog_rect_scaled(dialog, size, scale);
        if !rect.contains(point) {
            return CreateDialogClick::Outside;
        }
        for kind in [CreateEntryKind::Folder, CreateEntryKind::File] {
            if create_kind_button_rect_scaled(rect, kind, scale).contains(point) {
                return CreateDialogClick::Kind(kind);
            }
        }
        if create_dialog_cancel_button_rect_scaled(rect, scale).contains(point) {
            return CreateDialogClick::Cancel;
        }
        if create_dialog_commit_button_rect_scaled(rect, scale).contains(point) {
            return CreateDialogClick::Commit;
        }
        CreateDialogClick::Inside
    }

    #[cfg(test)]
    fn select_entry_by_name(&mut self, name: &str, size: PhysicalSize<u32>) -> bool {
        self.select_entry_by_name_in_pane(self.active_pane(), name, size)
    }

    fn select_entry_by_name_in_pane(
        &mut self,
        pane_id: ShellPaneId,
        name: &str,
        size: PhysicalSize<u32>,
    ) -> bool {
        let pane_id = self.normalized_pane_id(pane_id);
        let Some(pane) = self.pane_state(pane_id) else {
            return false;
        };
        let Some(index) = entry_index_by_name(&pane.entries, name) else {
            return false;
        };
        if pane.filtered_indexes.binary_search(&index).is_err() {
            return false;
        }
        let changed = self
            .pane_selection_mut(pane_id)
            .is_some_and(|selection| selection.apply_navigation(index, false));
        if changed {
            self.selection_changes += 1;
        }
        self.ensure_index_visible_in_pane(pane_id, index, size);
        changed
    }

    fn log_create_dialog_state(&self) {
        match self.create_dialog.as_ref() {
            Some(dialog) => fika_log!(
                "[fika] create-new open=1 kind={} parent={} name={:?} privileged={} error={:?} changes={}",
                dialog.kind.as_str(),
                dialog.parent.display(),
                dialog.name,
                dialog.privileged as u8,
                dialog.error,
                self.create_changes
            ),
            None => fika_log!(
                "[fika] create-new open=0 changes={}",
                self.create_changes
            ),
        }
    }

    fn is_rename_dialog_open(&self) -> bool {
        self.rename_dialog.is_some()
    }

    fn open_rename_dialog_from_context(&mut self, privileged: bool) -> bool {
        let Some(ShellContextTarget::Item {
            pane, path, is_dir, ..
        }) = self.context_target.as_ref()
        else {
            fika_log!(
                "[fika] rename-error target={}",
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none")
            );
            return false;
        };
        let Some(dialog) = ShellRenameDialog::new(*pane, path.clone(), *is_dir, privileged) else {
            fika_log!(
                "[fika] rename-error path={} error=no-file-name",
                path.display()
            );
            return false;
        };
        let changed = self.rename_dialog.as_ref() != Some(&dialog);
        self.rename_dialog = Some(dialog);
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rubber_band = None;
        if changed {
            self.rename_changes += 1;
            if let Some(dialog) = self.rename_dialog.as_ref() {
                fika_log!(
                    "[fika] rename open=1 source={} name={:?} dir={} privileged={} changes={}",
                    dialog.source.display(),
                    dialog.name,
                    dialog.is_dir as u8,
                    dialog.privileged as u8,
                    self.rename_changes
                );
            }
        }
        changed
    }

    fn apply_rename_command(&mut self, command: RenameCommand) -> bool {
        let old_dialog = self.rename_dialog.clone();
        match command {
            RenameCommand::Insert(value) => {
                let Some(dialog) = self.rename_dialog.as_mut() else {
                    return false;
                };
                if dialog.replace_on_insert {
                    dialog.name.clear();
                    dialog.replace_on_insert = false;
                }
                dialog.preedit = None;
                dialog.name.push_str(&value);
                dialog.error = None;
            }
            RenameCommand::Backspace => {
                let Some(dialog) = self.rename_dialog.as_mut() else {
                    return false;
                };
                if dialog.replace_on_insert {
                    dialog.name.clear();
                    dialog.replace_on_insert = false;
                } else {
                    dialog.name.pop();
                }
                dialog.preedit = None;
                dialog.error = None;
            }
            RenameCommand::Cancel => {
                return self.close_rename_dialog();
            }
            RenameCommand::Commit | RenameCommand::Ignore => return false,
        }

        let changed = old_dialog != self.rename_dialog;
        if changed {
            self.rename_changes += 1;
            self.log_rename_dialog_state();
        }
        changed
    }

    fn rename_entry_request(&self) -> Result<RenameEntryRequest, String> {
        let dialog = self
            .rename_dialog
            .as_ref()
            .ok_or_else(|| "rename dialog is not open".to_string())?;
        if dialog.busy {
            return Err("rename is already in progress".to_string());
        }
        let name = dialog.name.trim();
        validate_create_name(name)?;
        if name == dialog.original_name {
            return Err("name is unchanged".to_string());
        }
        let target = dialog.parent.join(name);
        if target.exists() {
            return Err(format!("{} already exists", target.display()));
        }
        Ok(RenameEntryRequest {
            pane: dialog.pane,
            source: dialog.source.clone(),
            target,
            original_name: dialog.original_name.clone(),
            name: name.to_string(),
            is_dir: dialog.is_dir,
            privileged: dialog.privileged,
        })
    }
}
