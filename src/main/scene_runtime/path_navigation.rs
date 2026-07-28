impl ShellScene {

    fn apply_loaded_path_to_pane(
        &mut self,
        pane: ShellPaneId,
        path: PathBuf,
        entries: Vec<Entry>,
        size: PhysicalSize<u32>,
    ) {
        let pane = self.normalized_pane_id(pane);
        let view_mode = self
            .pane_state(pane)
            .map(|state| state.view_mode)
            .unwrap_or(ShellViewMode::Icons);
        let zoom_step = self.pane_zoom_step(pane).unwrap_or(0);
        if let Some(old_path) = self.pane_state(pane).map(|state| state.path.clone()) {
            self.folder_preview_roles
                .borrow_mut()
                .clear_path_prefix(&old_path);
        }
        let filter_pattern = self.filter_pattern_for_pane(pane).to_string();
        self.cancel_metadata_role_work_for_pane(pane);
        self.panes.set(
            pane,
            ShellPaneState::from_entries(
                path,
                view_mode,
                entries,
                self.show_hidden,
                &filter_pattern,
            ),
        );
        if let Some(state) = self.pane_state_mut(pane) {
            state.zoom_step = zoom_step;
        }
        self.invalidate_layout_caches(pane);
        self.visible_slots.clear(pane);
        if let Some(state) = self.pane_state_mut(pane) {
            state.scroll_x = 0.0;
            state.scroll_y = 0.0;
        }
        self.active_pane = pane;
        self.clear_transient_after_pane_content_change(pane, true);
        self.path_changes += 1;
        self.clamp_scroll(size);
    }

    fn clear_transient_after_pane_content_change(&mut self, pane: ShellPaneId, clear_open: bool) {
        if self
            .location_draft
            .as_ref()
            .is_some_and(|draft| draft.pane == pane)
        {
            let old_draft = self.location_draft.take();
            self.update_location_focus_shine_after_draft_change(old_draft.as_ref());
        }
        self.rubber_band = None;
        self.scrollbar_drag = None;
        self.last_item_click = None;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        if clear_open {
            self.create_dialog = None;
            self.rename_dialog = None;
        }
        self.open_with_chooser = None;
        self.trash_conflict_dialog = None;
        self.internal_drag = None;
        self.external_drag = None;
        self.place_press = None;
        self.pending_drop_request = None;
        self.clear_dnd_hover_target();
    }

    fn log_loaded_path_for_pane(
        &self,
        pane: ShellPaneId,
        dir_count: usize,
        preview: &str,
        elapsed: Duration,
    ) {
        let Some(state) = self.pane_state(pane) else {
            return;
        };
        fika_log!(
            "[fika-wgpu] pane={} path={} entries={} dirs={} files={} load={}us changes={}",
            pane.as_str(),
            state.path.display(),
            state.entries.len(),
            dir_count,
            state.entries.len().saturating_sub(dir_count),
            elapsed.as_micros(),
            self.path_changes
        );
        if !preview.is_empty() {
            fika_log!("[fika-wgpu] first-entries={preview}");
        }
    }

    fn log_reloaded_path_for_pane(
        &self,
        pane: ShellPaneId,
        dir_count: usize,
        preview: &str,
        elapsed: Duration,
        selection_changed: bool,
    ) {
        let Some(state) = self.pane_state(pane) else {
            return;
        };
        fika_log!(
            "[fika-wgpu] reload pane={} path={} entries={} dirs={} files={} load={}us reloads={} selected={} selection_changed={}",
            pane.as_str(),
            state.path.display(),
            state.entries.len(),
            dir_count,
            state.entries.len().saturating_sub(dir_count),
            elapsed.as_micros(),
            self.directory_reloads,
            state.selection.len(),
            selection_changed as u8
        );
        if !preview.is_empty() {
            fika_log!("[fika-wgpu] first-entries={preview}");
        }
    }

    fn set_view_mode(&mut self, view_mode: ShellViewMode, size: PhysicalSize<u32>) -> bool {
        let pane_id = self.active_pane();
        let Some(pane) = self.pane_state_mut(pane_id) else {
            return false;
        };
        if pane.view_mode == view_mode {
            return false;
        }
        pane.view_mode = view_mode;
        pane.scroll_x = 0.0;
        pane.scroll_y = 0.0;
        self.visible_slots.clear(pane_id);
        self.folder_preview_roles
            .borrow_mut()
            .clear_request_lifecycle();
        self.rubber_band = None;
        self.scrollbar_drag = None;
        self.view_switches += 1;
        self.clamp_scroll(size);
        fika_log!(
            "[fika-wgpu] view-mode pane={} mode={} switches={} scroll_x={:.1} scroll_y={:.1}",
            pane_id.as_str(),
            view_mode.as_str(),
            self.view_switches,
            self.pane_state(pane_id)
                .map(|pane| pane.scroll_x)
                .unwrap_or(0.0),
            self.pane_state(pane_id)
                .map(|pane| pane.scroll_y)
                .unwrap_or(0.0)
        );
        true
    }

    fn zoom(&mut self, action: ZoomAction, size: PhysicalSize<u32>) -> bool {
        self.zoom_pane(self.active_pane(), action, size)
    }

    fn zoom_pane(
        &mut self,
        pane_id: ShellPaneId,
        action: ZoomAction,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some(current_step) = self.pane_zoom_step(pane_id) else {
            return false;
        };
        let next_step = match action {
            ZoomAction::In => current_step + 1,
            ZoomAction::Out => current_step - 1,
            ZoomAction::Reset => 0,
        };
        self.set_zoom_step(pane_id, next_step, size, true)
    }

    fn set_zoom_fraction(
        &mut self,
        pane_id: ShellPaneId,
        fraction: f32,
        size: PhysicalSize<u32>,
        clear_scrollbar_drag: bool,
    ) -> bool {
        let span = (FILE_MANAGER_ZOOM_LEVEL_MAX - FILE_MANAGER_ZOOM_LEVEL_MIN).max(1) as f32;
        let level = FILE_MANAGER_ZOOM_LEVEL_MIN + (fraction.clamp(0.0, 1.0) * span).round() as i32;
        self.set_zoom_step(
            pane_id,
            level - FILE_MANAGER_ZOOM_LEVEL_DEFAULT,
            size,
            clear_scrollbar_drag,
        )
    }

    fn set_zoom_step(
        &mut self,
        pane_id: ShellPaneId,
        next_step: i32,
        size: PhysicalSize<u32>,
        clear_scrollbar_drag: bool,
    ) -> bool {
        let next_step = next_step.clamp(ZOOM_STEP_MIN, ZOOM_STEP_MAX);
        let Some(old_step) = self.pane_zoom_step(pane_id) else {
            return false;
        };
        if next_step == old_step {
            return false;
        }

        if let Some(pane) = self.pane_state_mut(pane_id) {
            pane.zoom_step = next_step;
        }
        self.invalidate_layout_caches_for_pane(pane_id);
        self.folder_preview_roles
            .borrow_mut()
            .clear_request_lifecycle();
        self.rubber_band = None;
        if clear_scrollbar_drag {
            self.scrollbar_drag = None;
        }
        self.zoom_changes += 1;
        let active_pane = self.normalized_pane_id(pane_id);
        if let Some(index) = self
            .pane_selection(active_pane)
            .and_then(ShellSelection::focus_or_first_selected)
        {
            self.ensure_index_visible_in_pane(active_pane, index, size);
        } else {
            self.clamp_scroll(size);
        }
        fika_log!(
            "[fika-wgpu] zoom pane={} step={} percent={} changes={} scroll_x={:.1} scroll_y={:.1}",
            active_pane.as_str(),
            next_step,
            self.zoom_percent_for_pane(active_pane),
            self.zoom_changes,
            self.panes[active_pane].scroll_x,
            self.panes[active_pane].scroll_y
        );
        true
    }

    fn apply_selection_command(&mut self, command: SelectionCommand) -> bool {
        let rubber_band_changed = self.rubber_band.take().is_some();
        let active_pane = self.active_pane();
        let filtered_indexes = self
            .pane_state(active_pane)
            .map(|pane| pane.filtered_indexes.clone())
            .unwrap_or_default();
        let selection_changed = match command {
            SelectionCommand::SelectAll => self
                .pane_selection_mut(active_pane)
                .is_some_and(|selection| selection.select_indexes(&filtered_indexes)),
            SelectionCommand::Clear => self
                .pane_selection_mut(active_pane)
                .is_some_and(ShellSelection::clear),
        };
        if selection_changed {
            self.selection_changes += 1;
        }
        if selection_changed || rubber_band_changed {
            fika_log!(
                "[fika-wgpu] selection command={} selected={} changes={}",
                command.as_str(),
                self.active_selection_len(),
                self.selection_changes
            );
        }
        selection_changed || rubber_band_changed
    }

    fn is_location_editing(&self) -> bool {
        self.location_draft.is_some()
    }

    fn location_draft_pane(&self) -> Option<ShellPaneId> {
        self.location_draft
            .as_ref()
            .map(|draft| self.normalized_pane_id(draft.pane))
    }

    fn location_draft_value(&self) -> Option<&str> {
        self.location_draft
            .as_ref()
            .map(|draft| draft.draft.value.as_str())
    }

    fn location_draft_purpose(&self) -> Option<LocationDraftPurpose> {
        self.location_draft.as_ref().map(|draft| draft.purpose)
    }

    fn location_focus_token_for_draft(
        &self,
        draft: &ShellLocationDraft,
    ) -> (ShellPaneId, LocationDraftPurpose) {
        (self.normalized_pane_id(draft.pane), draft.purpose)
    }

    fn update_location_focus_shine_after_draft_change(
        &mut self,
        old_draft: Option<&ShellLocationDraft>,
    ) {
        let old_focus = old_draft.map(|draft| self.location_focus_token_for_draft(draft));
        let new_focus = self
            .location_draft
            .as_ref()
            .map(|draft| self.location_focus_token_for_draft(draft));
        match (old_focus, new_focus) {
            (old_focus, Some(new_focus)) if Some(new_focus) != old_focus => {
                self.start_location_focus_shine();
            }
            (Some(_), None) => {
                self.stop_location_focus_shine();
            }
            _ => {}
        }
    }

    fn location_label_for_pane(&self, pane: ShellPaneId) -> String {
        if let Some(draft) = self
            .location_draft
            .as_ref()
            .filter(|draft| self.normalized_pane_id(draft.pane) == self.normalized_pane_id(pane))
        {
            return text_with_preedit(
                &draft.draft.value,
                draft.draft.cursor,
                if draft.draft.replace_on_insert {
                    0
                } else {
                    draft.draft.cursor
                },
                draft.draft.preedit.as_ref(),
            )
            .into_owned();
        }
        self.pane_state(pane)
            .map(|pane| pane.path.display().to_string())
            .unwrap_or_default()
    }

    fn location_cursor_for_pane(&self, pane: ShellPaneId) -> Option<usize> {
        self.location_draft
            .as_ref()
            .filter(|draft| self.normalized_pane_id(draft.pane) == self.normalized_pane_id(pane))
            .map(|draft| {
                cursor_with_preedit(
                    &draft.draft.value,
                    draft.draft.cursor,
                    if draft.draft.replace_on_insert {
                        0
                    } else {
                        draft.draft.cursor
                    },
                    draft.draft.preedit.as_ref(),
                )
            })
    }

    fn location_text_rect_for_path_bar_rect(&self, rect: ViewRect) -> ViewRect {
        let icon_size = self
            .scale_metric(18.0)
            .min((rect.height - self.scale_metric(8.0)).max(1.0));
        let icon_right = rect.x + self.scale_metric(8.0) + icon_size;
        let separator_x = icon_right + self.scale_metric(8.0);
        let text_x = separator_x + self.scale_metric(9.0);
        ViewRect {
            x: text_x,
            y: rect.y + (rect.height - self.text_line_height()) / 2.0,
            width: (rect.right() - text_x - self.scale_metric(8.0)).max(1.0),
            height: self.text_line_height(),
        }
    }

    fn location_cursor_for_screen_point(
        &self,
        pane: ShellPaneId,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<usize> {
        let rect = self.pane_path_bar_rect(pane, size)?;
        if !rect.contains(point) {
            return None;
        }
        let text_rect = self.location_text_rect_for_path_bar_rect(rect);
        let label = self.location_label_for_pane(pane);
        Some(self.text_hit_tests.borrow_mut().cursor_for_offset(
            &label,
            text_rect,
            point.x - text_rect.x,
            LabelAlignment::Start,
            LabelWrap::None,
            self.ui_scale(),
        ))
    }

    fn location_bar_active_for_pane(&self, pane: ShellPaneId) -> bool {
        let pane = self.normalized_pane_id(pane);
        self.location_draft_pane() == Some(pane)
            || (self.location_draft.is_none() && self.active_pane() == pane)
    }

    fn resolved_location_draft(&self) -> Option<(ShellPaneId, PathBuf)> {
        let pane = self.location_draft_pane()?;
        let value = self.location_draft_value()?;
        let base = &self.pane_state(pane)?.path;
        resolve_location_input(base, value).map(|path| (pane, path))
    }

    fn close_location_draft(&mut self, size: PhysicalSize<u32>) -> bool {
        let old_draft = self.location_draft.take();
        if old_draft.is_none() {
            return false;
        }
        self.update_location_focus_shine_after_draft_change(old_draft.as_ref());
        self.location_changes += 1;
        self.rubber_band = None;
        self.clamp_scroll(size);
        fika_log!(
            "[fika-wgpu] location active=0 value=\"\" changes={}",
            self.location_changes
        );
        true
    }

    fn close_location_draft_if_outside(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some(pane) = self.location_draft_pane() else {
            return false;
        };
        if self
            .pane_path_bar_rect(pane, size)
            .is_some_and(|rect| rect.contains(point))
        {
            return false;
        }
        self.close_location_draft(size)
    }

    fn activate_path_bar_at_screen_point(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some(pane) = self.path_bar_pane_at_screen_point(point, size) else {
            return false;
        };
        let pane = self.normalized_pane_id(pane);
        let old_pane = self.active_pane();
        let old_draft = self.location_draft.clone();
        let old_filter_active = self.filter_active;
        self.active_pane = self.normalized_pane_id(pane);
        if self.location_draft_pane() != Some(pane) {
            let Some(path) = self.pane_state(pane).map(|pane| pane.path.clone()) else {
                return old_pane != self.active_pane();
            };
            self.location_draft = Some(ShellLocationDraft::new(pane, path.display().to_string()));
            self.filter_active = false;
        }
        if let Some(cursor) = self.location_cursor_for_screen_point(pane, point, size)
            && let Some(draft) = self.location_draft.as_mut()
        {
            draft.draft.set_cursor(cursor);
        }

        let location_changed =
            old_draft != self.location_draft || old_filter_active != self.filter_active;
        if location_changed {
            self.update_location_focus_shine_after_draft_change(old_draft.as_ref());
            self.reset_text_caret_blink();
            self.location_changes += 1;
            self.rubber_band = None;
            self.clamp_scroll(size);
            fika_log!(
                "[fika-wgpu] location active={} value={:?} changes={}",
                self.location_draft.is_some() as u8,
                self.location_draft_value().unwrap_or(""),
                self.location_changes
            );
        }
        location_changed || old_pane != self.active_pane()
    }
}
