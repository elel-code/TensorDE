impl ShellScene {

    fn open_add_network_folder_location_draft(&mut self, size: PhysicalSize<u32>) -> bool {
        let old_draft = self.location_draft.clone();
        let pane = self.active_pane();
        self.location_draft = Some(ShellLocationDraft::add_network_folder(pane));
        self.filter_active = false;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.open_with_chooser = None;
        self.rubber_band = None;
        let changed = old_draft != self.location_draft;
        if changed {
            self.update_location_focus_shine_after_draft_change(old_draft.as_ref());
            self.reset_text_caret_blink();
            self.location_changes += 1;
            self.clamp_scroll(size);
            tensor_files_log!(
                "[tensor-files] add-network-folder input=1 value={:?} changes={}",
                self.location_draft_value().unwrap_or(""),
                self.location_changes
            );
        }
        changed
    }

    fn add_network_folder_request_from_draft(&self) -> Result<AddNetworkFolderRequest, String> {
        let Some(draft) = self.location_draft.as_ref() else {
            return Err("network folder input is not open".to_string());
        };
        if draft.purpose != LocationDraftPurpose::AddNetworkFolder {
            return Err("location input is not adding a network folder".to_string());
        }
        let pane = self.normalized_pane_id(draft.pane);
        let input = draft.draft.value.trim();
        let path = network_path_from_uri(input).map_err(|error| error.to_string())?;
        if path == network_root_path() {
            return Err("enter a network server or share URL".to_string());
        }
        let label = network_path_display_name(&path)
            .filter(|label| !label.trim().is_empty())
            .unwrap_or_else(|| path.display().to_string());
        Ok(AddNetworkFolderRequest { pane, path, label })
    }

    fn apply_location_command(
        &mut self,
        command: LocationCommand,
        size: PhysicalSize<u32>,
    ) -> bool {
        let old_draft = self.location_draft.clone();
        let old_filter_active = self.filter_active;
        let reset_caret_on_noop = matches!(
            &command,
            LocationCommand::Activate
                | LocationCommand::Insert(_)
                | LocationCommand::Backspace
                | LocationCommand::Delete
                | LocationCommand::MoveLeft
                | LocationCommand::MoveRight
                | LocationCommand::MoveHome
                | LocationCommand::MoveEnd
                | LocationCommand::Complete
        );

        match command {
            LocationCommand::Activate => {
                let pane = self.active_pane();
                let Some(path) = self.pane_state(pane).map(|pane| pane.path.clone()) else {
                    return false;
                };
                self.location_draft =
                    Some(ShellLocationDraft::new(pane, path.display().to_string()));
                self.filter_active = false;
            }
            LocationCommand::Insert(value) => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.draft.insert(&value);
            }
            LocationCommand::Backspace => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.draft.backspace();
            }
            LocationCommand::Delete => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.draft.delete();
            }
            LocationCommand::MoveLeft => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.draft.move_left();
            }
            LocationCommand::MoveRight => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.draft.move_right();
            }
            LocationCommand::MoveHome => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.draft.move_home();
            }
            LocationCommand::MoveEnd => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.draft.move_end();
            }
            LocationCommand::Cancel => {
                self.location_draft = None;
            }
            LocationCommand::Complete => {
                let Some(location) = self.location_draft.as_ref() else {
                    return false;
                };
                let pane = self.normalized_pane_id(location.pane);
                let Some(base) = self.pane_state(pane).map(|pane| pane.path.clone()) else {
                    return false;
                };
                let Some(completed) = complete_location_input(&base, &location.draft.value) else {
                    return false;
                };
                if let Some(draft) = self.location_draft.as_mut() {
                    draft.draft.set_completed(completed);
                }
            }
            LocationCommand::Commit | LocationCommand::Ignore => return false,
        }

        let changed = old_draft != self.location_draft || old_filter_active != self.filter_active;
        if !changed {
            if reset_caret_on_noop && self.location_text_caret_active() {
                self.reset_text_caret_blink();
                return true;
            }
            return false;
        }

        self.reset_text_caret_blink();
        self.update_location_focus_shine_after_draft_change(old_draft.as_ref());
        self.location_changes += 1;
        self.rubber_band = None;
        self.clamp_scroll(size);
        tensor_files_log!(
            "[tensor-files] location active={} value={:?} changes={}",
            self.location_draft.is_some() as u8,
            self.location_draft_value().unwrap_or(""),
            self.location_changes
        );
        true
    }

    fn apply_filter_command(&mut self, command: FilterCommand, size: PhysicalSize<u32>) -> bool {
        let old_active = self.filter_active;
        let old_pattern = self.filter_pattern.clone();

        match command {
            FilterCommand::Activate => {
                self.filter_active = true;
            }
            FilterCommand::Insert(value) => {
                self.filter_active = true;
                self.filter_pattern.push_str(&value);
            }
            FilterCommand::Backspace => {
                self.filter_active = true;
                self.filter_pattern.pop();
            }
            FilterCommand::ClearAndDeactivate => {
                self.filter_active = false;
                self.filter_pattern.clear();
            }
            FilterCommand::Deactivate => {
                self.filter_active = false;
            }
        }

        let filter_changed = old_active != self.filter_active || old_pattern != self.filter_pattern;
        if !filter_changed {
            return false;
        }

        self.filter_changes += 1;
        self.rubber_band = None;
        let selection_changed = self.rebuild_filtered_indexes();
        if selection_changed {
            self.selection_changes += 1;
        }
        self.clamp_scroll(size);
        tensor_files_log!(
            "[tensor-files] filter active={} pattern={:?} matches={} changes={} selection_changed={}",
            self.filter_active as u8,
            self.filter_pattern,
            self.panes[ShellPaneId::SLOT_0].filtered_indexes.len(),
            self.filter_changes,
            selection_changed as u8
        );
        true
    }

    fn toggle_hidden_visibility(&mut self, size: PhysicalSize<u32>) -> bool {
        self.show_hidden = !self.show_hidden;
        self.hidden_changes += 1;
        self.rubber_band = None;
        let selection_changed = self.rebuild_filtered_indexes();
        if selection_changed {
            self.selection_changes += 1;
        }
        self.clamp_scroll(size);
        tensor_files_log!(
            "[tensor-files] hidden show={} visible={} changes={} selection_changed={}",
            self.show_hidden as u8,
            self.filtered_entry_count(),
            self.hidden_changes,
            selection_changed as u8
        );
        true
    }

    fn theme(&self) -> ShellTheme {
        ShellTheme::for_transparent_background(self.dark_mode, self.background_opacity)
    }

    fn toggle_dark_mode(&mut self) {
        self.dark_mode = !self.dark_mode;
        self.view_switches += 1;
        self.rubber_band = None;
        self.animations.clear();
        tensor_files_log!(
            "[tensor-files] dark-mode enabled={} view_switches={}",
            self.dark_mode as u8,
            self.view_switches
        );
    }

    fn open_split_pane_from_context(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let (path, source_pane) = self.context_target_split_pane_request().unwrap_or_else(|| {
            let pane = self.active_pane();
            let path = self
                .pane_state(pane)
                .map(|pane| pane.path.clone())
                .unwrap_or_else(|| self.panes[ShellPaneId::SLOT_0].path.clone());
            (path, pane)
        });
        let view_mode = self
            .pane_state(source_pane)
            .map(|pane| pane.view_mode)
            .unwrap_or_else(|| self.active_view_mode());
        let zoom_levels = self
            .pane_state(source_pane)
            .map(|pane| pane.zoom_levels)
            .unwrap_or_default();
        self.open_split_pane_with_view_mode(path, view_mode, zoom_levels, size)
    }

    #[cfg(test)]
    fn open_split_pane(&mut self, path: PathBuf, size: PhysicalSize<u32>) -> Result<bool, String> {
        let view_mode = self.active_view_mode();
        let zoom_levels = self
            .pane_state(self.active_pane())
            .map(|pane| pane.zoom_levels)
            .unwrap_or_default();
        self.open_split_pane_with_view_mode(path, view_mode, zoom_levels, size)
    }

    fn open_split_pane_with_view_mode(
        &mut self,
        path: PathBuf,
        view_mode: ShellViewMode,
        zoom_levels: ShellPaneZoomLevels,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        let mut split_pane = ShellPaneState::load(path, view_mode, self.show_hidden)?;
        split_pane.zoom_levels = zoom_levels;
        split_pane.scroll_x = 0.0;
        split_pane.scroll_y = 0.0;
        self.panes.set(ShellPaneId::SLOT_1, split_pane);
        self.invalidate_layout_caches(ShellPaneId::SLOT_1);
        self.visible_slots.clear(ShellPaneId::SLOT_1);
        self.active_pane = ShellPaneId::SLOT_1;
        self.split_pane_left_fraction = 0.5;
        self.split_pane_changes += 1;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.open_with_chooser = None;
        self.trash_conflict_dialog = None;
        self.internal_drag = None;
        self.external_drag = None;
        self.place_press = None;
        self.pending_drop_request = None;
        self.clear_dnd_hover_target();
        self.rubber_band = None;
        self.scrollbar_drag = None;
        self.clamp_scroll(size);
        tensor_files_log!(
            "[tensor-files] split-pane open=1 changes={} left={} right={}",
            self.split_pane_changes,
            self.panes[ShellPaneId::SLOT_0].path.display(),
            self.panes
                .get(ShellPaneId::SLOT_1)
                .map(|pane| pane.path.display().to_string())
                .unwrap_or_default()
        );
        Ok(true)
    }

    fn open_split_pane_from_active(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let pane = self.active_pane();
        let Some(state) = self.pane_state(pane) else {
            return Err(format!("pane {} is not open", pane.as_str()));
        };
        let current_path = state.path.clone();
        let view_mode = state.view_mode;
        let zoom_levels = state.zoom_levels;
        let path = self
            .single_selected_directory_path_for_pane(pane)
            .unwrap_or(current_path);
        self.open_split_pane_with_view_mode(path, view_mode, zoom_levels, size)
    }

    fn toggle_split_view_from_toolbar(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        if self.panes.is_open(ShellPaneId::SLOT_1) {
            Ok(self.close_active_split_pane(size))
        } else {
            self.open_split_pane_from_active(size)
        }
    }

    fn close_active_split_pane(&mut self, size: PhysicalSize<u32>) -> bool {
        if !self.panes.is_open(ShellPaneId::SLOT_1) {
            return false;
        }
        let active = self.active_pane();
        match active {
            ShellPaneId::SLOT_0 => {
                let Some(remaining) = self.panes.take(ShellPaneId::SLOT_1) else {
                    return false;
                };
                self.panes.set(ShellPaneId::SLOT_0, remaining);
                let remaining_history = self.histories.take(ShellPaneId::SLOT_1);
                self.histories.set(ShellPaneId::SLOT_0, remaining_history);
                self.histories.clear(ShellPaneId::SLOT_1);
            }
            ShellPaneId::SLOT_1 => {
                if self.panes.take(ShellPaneId::SLOT_1).is_none() {
                    return false;
                }
                self.histories.clear(ShellPaneId::SLOT_1);
            }
        }
        self.active_pane = ShellPaneId::SLOT_0;
        self.visible_slots.clear(ShellPaneId::SLOT_0);
        self.visible_slots.clear(ShellPaneId::SLOT_1);
        self.invalidate_all_layout_caches();
        self.split_pane_left_fraction = 0.5;
        self.split_pane_changes += 1;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.open_with_chooser = None;
        self.trash_conflict_dialog = None;
        self.internal_drag = None;
        self.external_drag = None;
        self.place_press = None;
        self.pending_drop_request = None;
        self.clear_dnd_hover_target();
        self.rubber_band = None;
        self.scrollbar_drag = None;
        self.clamp_scroll(size);
        tensor_files_log!(
            "[tensor-files] split-pane open=0 closed={} changes={} remaining={}",
            active.as_str(),
            self.split_pane_changes,
            self.panes[ShellPaneId::SLOT_0].path.display()
        );
        true
    }

    fn context_target_split_pane_request(&self) -> Option<(PathBuf, ShellPaneId)> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Item {
                pane, path, is_dir, ..
            } if *is_dir => Some((path.clone(), self.normalized_pane_id(*pane))),
            ShellContextTarget::Item { pane, path, .. } => path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .or_else(|| self.pane_state(*pane).map(|state| state.path.clone()))
                .map(|path| (path, self.normalized_pane_id(*pane))),
            ShellContextTarget::Blank { pane, path, .. } => {
                Some((path.clone(), self.normalized_pane_id(*pane)))
            }
            ShellContextTarget::Place { path, .. } => Some((path.clone(), self.active_pane())),
        }
    }

    fn single_selected_directory_path_for_pane(&self, pane: ShellPaneId) -> Option<PathBuf> {
        let selection = self.pane_selection(pane)?;
        if selection.len() != 1 {
            return None;
        }
        selection
            .focus_or_first_selected()
            .and_then(|index| self.directory_path_for_pane_index(pane, index))
    }

    fn rebuild_filtered_indexes(&mut self) -> bool {
        let mut selection_changed = false;
        let show_hidden = self.show_hidden;
        for pane in ShellPaneId::ALL {
            let filter_pattern = self.filter_pattern_for_pane(pane).to_string();
            if let Some(state) = self.pane_state_mut(pane) {
                selection_changed |=
                    state.rebuild_filtered_indexes_with_pattern(show_hidden, &filter_pattern);
                self.invalidate_layout_caches(pane);
            }
        }
        selection_changed
    }

    fn filtered_entry_count(&self) -> usize {
        self.panes[ShellPaneId::SLOT_0].filtered_indexes.len()
    }

    pub(crate) fn set_scale_factor(&mut self, scale_factor: f32, size: PhysicalSize<u32>) -> bool {
        let next = normalized_scale_factor(scale_factor);
        if (self.scale_factor - next).abs() <= 0.01 {
            self.scale_factor = next;
            self.clamp_scroll(size);
            return false;
        }

        let old_ui_scale = self.ui_scale();
        self.scale_factor = next;
        self.invalidate_all_layout_caches();
        self.folder_preview_roles
            .borrow_mut()
            .clear_request_lifecycle();
        let next_ui_scale = self.ui_scale();
        if old_ui_scale > f32::EPSILON {
            let ratio = next_ui_scale / old_ui_scale;
            for pane in ShellPaneId::ALL {
                if let Some(state) = self.pane_state_mut(pane) {
                    state.scroll_x *= ratio;
                    state.scroll_y *= ratio;
                }
            }
            self.places_scroll_y *= ratio;
        }
        self.clamp_scroll(size);
        tensor_files_log!(
            "[tensor-files] scale-factor={:.2} ui_scale={:.2} scroll_x={:.1} scroll_y={:.1}",
            self.scale_factor,
            self.ui_scale(),
            self.panes[ShellPaneId::SLOT_0].scroll_x,
            self.panes[ShellPaneId::SLOT_0].scroll_y
        );
        true
    }
}
