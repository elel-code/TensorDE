impl ShellScene {
    #[cfg(test)]
    fn load(path: PathBuf, view_mode: ShellViewMode) -> Result<Self, String> {
        Self::load_with_hidden_visibility(path, view_mode, false)
    }

    fn load_with_hidden_visibility(
        path: PathBuf,
        view_mode: ShellViewMode,
        show_hidden: bool,
    ) -> Result<Self, String> {
        let load_start = Instant::now();
        let entries = read_shell_entries_sync(&path)?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let preview = entries
            .iter()
            .take(8)
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");

        tensor_files_log!(
            "[tensor-files] path={} entries={} dirs={} files={} load={}us",
            path.display(),
            entries.len(),
            dir_count,
            entries.len().saturating_sub(dir_count),
            elapsed.as_micros()
        );
        if !preview.is_empty() {
            tensor_files_log!("[tensor-files] first-entries={preview}");
        }

        let slot0_pane = ShellPaneState::from_entries(path, view_mode, entries, show_hidden, "");
        let places = build_shell_places();
        let trash_has_items = file_ops::trash_has_items();
        tensor_files_log!("[tensor-files] places entries={}", places.len());

        Ok(Self::from_primary_pane(
            slot0_pane,
            places,
            trash_has_items,
            show_hidden,
        ))
    }

    /// Build a scene around a primary pane with default chrome and counters.
    ///
    /// Production load and test fixtures share this so new `ShellScene` fields
    /// only need one default assignment.
    fn from_primary_pane(
        primary: ShellPaneState,
        places: Vec<ShellPlace>,
        trash_has_items: bool,
        show_hidden: bool,
    ) -> Self {
        Self {
            panes: ShellPaneStates::new(primary),
            compact_layout_cache: CompactLayoutCache::new(),
            icons_layout_height_cache: IconsLayoutHeightCache::new(),
            active_pane: ShellPaneId::SLOT_0,
            places,
            trash_has_items,
            location_draft: None,
            filter_active: false,
            filter_pattern: String::new(),
            show_hidden,
            dark_mode: false,
            background_blur: false,
            background_opacity: 1.0,
            places_visible: true,
            places_width: PLACES_SIDEBAR_WIDTH,
            places_scroll_y: 0.0,
            scrollbar_drag: None,
            pointer: None,
            hovered_item: None,
            hovered_place: None,
            last_item_click: None,
            histories: ShellPaneHistories::default(),
            context_target: None,
            context_menu: None,
            context_menu_safe_triangle: ShellContextMenuSafeTriangleRuntime::default(),
            drop_menu: None,
            properties_overlay: None,
            create_dialog: None,
            rename_dialog: None,
            open_with_chooser: None,
            trash_conflict_dialog: None,
            task_detail_dialog: None,
            split_pane_left_fraction: 0.5,
            visible_slots: ShellPaneVisibleSlotPools::default(),
            visible_slot_stats: ShellVisibleItemSlotStats::default(),
            metadata_roles: ShellMetadataRoleRuntime::new(),
            folder_preview_roles: RefCell::new(ShellFolderPreviewRoleRuntime::new()),
            folder_preview_request_staging: RefCell::new(Vec::new()),
            icon_role_read_ahead: RefCell::new(ShellIconRoleReadAheadQueue::new()),
            internal_drag: None,
            external_drag: None,
            place_press: None,
            dnd_hover_target: None,
            pending_drop_request: None,
            task_statuses: ShellTaskStatusStore::new(),
            rubber_band: None,
            item_reflow: ui::item_reflow::ShellItemReflowRuntime::default(),
            animations: ShellAnimationRuntime::default(),
            text_hit_tests: RefCell::new(TextHitTestRuntime::new()),
            scale_factor: 1.0,
            hit_tests: 0,
            selection_changes: 0,
            context_target_changes: 0,
            context_menu_actions: 0,
            properties_changes: 0,
            create_changes: 0,
            rename_changes: 0,
            open_with_changes: 0,
            open_changes: 0,
            copy_location_changes: 0,
            file_clipboard_changes: 0,
            paste_changes: 0,
            trash_changes: 0,
            places_changes: 0,
            places_resize_changes: 0,
            places_scroll_changes: 0,
            content_scroll_changes: 0,
            keyboard_navigation: 0,
            rubber_band_updates: 0,
            view_switches: 0,
            path_changes: 0,
            directory_reloads: 0,
            location_changes: 0,
            filter_changes: 0,
            hidden_changes: 0,
            appearance_changes: 0,
            zoom_changes: 0,
            split_pane_changes: 0,
            dnd_hover_changes: 0,
            dnd_drop_requests: 0,
        }
    }

    fn invalidate_layout_caches(&self, pane: ShellPaneId) {
        self.compact_layout_cache.invalidate_pane(pane.index());
        self.icons_layout_height_cache.invalidate_pane(pane.index());
    }

    fn apply_startup_zoom_levels(&mut self, zoom_levels: ShellPaneZoomLevels) {
        self.panes[ShellPaneId::SLOT_0].zoom_levels = zoom_levels;
        self.invalidate_layout_caches_for_pane(ShellPaneId::SLOT_0);
    }

    fn invalidate_layout_caches_for_pane(&mut self, pane: ShellPaneId) {
        self.invalidate_layout_caches(pane);
        self.visible_slots.clear(pane);
    }

    fn invalidate_all_layout_caches(&self) {
        self.compact_layout_cache.clear();
        self.icons_layout_height_cache.clear();
    }

    fn record_trash_content_change(&mut self) {
        self.trash_changes += 1;
        self.trash_has_items = file_ops::trash_has_items();
    }

    fn trash_place_has_items(&self, place: &ShellPlace) -> bool {
        place.trash && self.trash_has_items
    }

    #[cfg(test)]
    fn load_path(&mut self, path: PathBuf, size: PhysicalSize<u32>) -> Result<bool, String> {
        self.load_path_in_pane(ShellPaneId::SLOT_0, path, size, true)
    }

    fn load_path_in_pane(
        &mut self,
        pane: ShellPaneId,
        path: PathBuf,
        size: PhysicalSize<u32>,
        push_history: bool,
    ) -> Result<bool, String> {
        let pane = self.normalized_pane_id(pane);
        let Some(current_path) = self.pane_state(pane).map(|state| state.path.clone()) else {
            return Err(format!("pane {} is not open", pane.as_str()));
        };
        if path == current_path {
            return Ok(false);
        }
        let load_start = Instant::now();
        let entries = read_shell_entries_sync(&path)?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let preview = entries
            .iter()
            .take(8)
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");

        if push_history {
            let history = self.pane_history_mut(pane);
            history.push_back(current_path);
            history.clear_forward();
        }
        self.apply_loaded_path_to_pane(pane, path, entries, size);

        self.log_loaded_path_for_pane(pane, dir_count, &preview, elapsed);
        Ok(true)
    }

    fn reload_current_path(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        self.reload_pane_path(self.active_pane(), size)
    }

    fn reload_pane_path(
        &mut self,
        pane: ShellPaneId,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        let pane = self.normalized_pane_id(pane);
        let Some(current) = self.pane_state(pane) else {
            return Err(format!("pane {} is not open", pane.as_str()));
        };
        let view_mode = current.view_mode;
        let path = current.path.clone();
        let previous_visible_rects = self.visible_item_reflow_rects_for_pane(pane, size);
        let load_start = Instant::now();
        let entries = read_shell_entries_sync(&path)?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let preview = entries
            .iter()
            .take(8)
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");

        let remapped_selection = self.selection_for_reloaded_pane_entries(pane, &entries);
        let previous_selection = self.pane_selection(pane).cloned().unwrap_or_default();
        let filter_pattern = self.filter_pattern_for_pane(pane).to_string();
        let show_hidden = self.show_hidden;

        self.cancel_metadata_role_work_for_pane(pane);
        self.folder_preview_roles
            .borrow_mut()
            .clear_path_prefix(&path);
        let Some(state) = self.pane_state_mut(pane) else {
            return Err(format!("pane {} is not open", pane.as_str()));
        };
        let (selection_changed, pruned_selection) = {
            state.view_mode = view_mode;
            state.entries = entries;
            state.dir_count = dir_count;
            state.selection = remapped_selection;
            let pruned_selection =
                state.rebuild_filtered_indexes_with_pattern(show_hidden, &filter_pattern);
            let selection_changed = previous_selection != state.selection;
            (selection_changed, pruned_selection)
        };
        if selection_changed || pruned_selection {
            self.selection_changes += 1;
        }
        self.invalidate_layout_caches(pane);
        self.directory_reloads += 1;
        self.clear_transient_after_pane_content_change(pane, false);
        self.clamp_scroll(size);
        self.start_item_reflow_transitions(pane, previous_visible_rects, size);
        self.log_reloaded_path_for_pane(pane, dir_count, &preview, elapsed, selection_changed);
        Ok(true)
    }

    fn visible_item_reflow_rects_for_pane(
        &self,
        pane: ShellPaneId,
        size: PhysicalSize<u32>,
    ) -> ui::animation::ShellItemReflowRects {
        ui::item_reflow::visible_item_reflow_rects_for_pane(self, pane, size)
    }

    fn visible_item_reflow_rects_for_open_panes(
        &self,
        size: PhysicalSize<u32>,
    ) -> Vec<(ShellPaneId, ui::animation::ShellItemReflowRects)> {
        ui::item_reflow::visible_item_reflow_rects_for_open_panes(self, size)
    }

    pub(crate) fn reflow_pane_items_after_window_resize(
        &mut self,
        previous_size: PhysicalSize<u32>,
        next_size: PhysicalSize<u32>,
    ) -> bool {
        ui::item_reflow::reflow_pane_items_after_window_resize(self, previous_size, next_size)
    }

    fn start_item_reflow_transitions(
        &mut self,
        pane: ShellPaneId,
        previous_rects: ui::animation::ShellItemReflowRects,
        size: PhysicalSize<u32>,
    ) -> bool {
        ui::item_reflow::start_item_reflow_transitions(self, pane, previous_rects, size)
    }

    fn start_item_reflow_transitions_for_panes(
        &mut self,
        previous_rects_by_pane: Vec<(ShellPaneId, ui::animation::ShellItemReflowRects)>,
        size: PhysicalSize<u32>,
    ) -> bool {
        ui::item_reflow::start_item_reflow_transitions_for_panes(self, previous_rects_by_pane, size)
    }

    fn clear_item_reflow_for_pane(&mut self, pane: ShellPaneId) {
        ui::item_reflow::clear_item_reflow_for_pane(self, pane);
    }

    fn item_reflow_active_for_pane_at(&self, pane: ShellPaneId, now: Instant) -> bool {
        ui::item_reflow::item_reflow_active_for_pane_at(self, pane, now)
    }

    fn has_item_reflow_for_pane(&self, pane: ShellPaneId) -> bool {
        ui::item_reflow::has_item_reflow_for_pane(self, pane)
    }

    fn item_reflow_entity_path_for_entry<'a>(
        &'a self,
        pane: ShellPaneId,
        view: ShellPaneView<'a>,
        entry_index: usize,
    ) -> Option<(Option<bevy_ecs::entity::Entity>, std::borrow::Cow<'a, Path>)> {
        let entry = view.entries.get(entry_index)?;
        if let Some((entity, path)) = self
            .visible_slots
            .get(pane)
            .retained_entity_path_for_entry(entry)
        {
            return Some((Some(entity), std::borrow::Cow::Borrowed(path)));
        }
        if let Some(path) = entry.target_path.as_deref() {
            return Some((None, std::borrow::Cow::Borrowed(path)));
        }
        Some((
            None,
            std::borrow::Cow::Owned(view.path.join(entry.name.as_ref())),
        ))
    }

    #[cfg(test)]
    fn item_reflow_path_for_entry<'a>(
        &'a self,
        pane: ShellPaneId,
        view: ShellPaneView<'a>,
        entry_index: usize,
    ) -> Option<std::borrow::Cow<'a, Path>> {
        self.item_reflow_entity_path_for_entry(pane, view, entry_index)
            .map(|(_, path)| path)
    }

    fn item_reflow_offset_for_entry_at(
        &self,
        pane: ShellPaneId,
        view: ShellPaneView<'_>,
        entry_index: usize,
        now: Instant,
    ) -> Option<(f32, f32)> {
        let (entity, path) = self.item_reflow_entity_path_for_entry(pane, view, entry_index)?;
        ui::item_reflow::item_reflow_offset_for_entity_or_path_at(
            self,
            pane,
            entity,
            path.as_ref(),
            now,
        )
    }

    fn animation_active(&self) -> bool {
        self.animations.active()
    }

    fn next_animation_frame_deadline(&self) -> Option<Instant> {
        [
            self.animations.next_frame_deadline(),
            ui::item_reflow::next_item_reflow_deadline(self),
            self.context_menu_safe_triangle.next_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn prune_finished_animations(&mut self) -> bool {
        let item_reflow_started =
            ui::item_reflow::start_due_item_reflow_transitions(self, Instant::now());
        let context_menu_hover_due = self.apply_due_context_menu_hover(Instant::now());
        self.animations.prune_finished() || item_reflow_started || context_menu_hover_due
    }

    fn start_hover_animation(&mut self) {
        self.animations.start_hover_transition();
    }

    fn hover_animation_factor(&self) -> f32 {
        self.animations.hover_factor()
    }

    fn start_location_focus_shine(&mut self) {
        self.animations.start_location_focus_shine();
    }

    fn location_focus_shine_value(&self) -> Option<f32> {
        self.animations.location_focus_shine_value()
    }

    fn stop_location_focus_shine(&mut self) -> bool {
        self.animations.stop_location_focus_shine()
    }

    fn reset_text_caret_blink(&mut self) {
        self.animations.reset_text_caret_blink();
    }

    fn text_caret_visible(&self) -> bool {
        self.animations.text_caret_visible()
    }

    fn location_text_caret_active(&self) -> bool {
        self.location_draft.is_some()
    }

    fn open_with_text_caret_active(&self) -> bool {
        self.open_with_chooser.is_some()
    }

    fn text_caret_blink_active(&self) -> bool {
        self.location_text_caret_active() || self.open_with_text_caret_active()
    }

    fn next_text_caret_blink_deadline(&self) -> Option<Instant> {
        self.animations
            .next_text_caret_blink_deadline(self.text_caret_blink_active())
    }

    fn location_text_caret_dirty_value(&self) -> u64 {
        self.animations
            .text_caret_dirty_value(self.location_text_caret_active())
    }

    fn open_with_text_caret_dirty_value(&self) -> u64 {
        self.animations
            .text_caret_dirty_value(self.open_with_text_caret_active())
    }

    fn reload_panes_showing_path(
        &mut self,
        path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        let pane_ids = ShellPaneId::ALL
            .into_iter()
            .filter(|pane| {
                self.pane_state(*pane)
                    .is_some_and(|state| same_directory(&state.path, path))
            })
            .collect::<Vec<_>>();
        let mut changed = false;
        for pane in pane_ids {
            changed |= self.reload_pane_path(pane, size)?;
        }
        Ok(changed)
    }

    #[cfg(test)]
    fn go_history_back(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let pane = self.active_pane();
        let Some(path) = self.pane_history(pane).back.last().cloned() else {
            return Ok(false);
        };
        let current_path = self
            .pane_state(pane)
            .map(|state| state.path.clone())
            .ok_or_else(|| format!("pane {} is not open", pane.as_str()))?;
        {
            let history = self.pane_history_mut(pane);
            history.back.pop();
            history.push_forward(current_path);
        }
        self.load_path_in_pane(pane, path, size, false)
    }

    #[cfg(test)]
    fn go_history_forward(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let pane = self.active_pane();
        let Some(path) = self.pane_history(pane).forward.last().cloned() else {
            return Ok(false);
        };
        let current_path = self
            .pane_state(pane)
            .map(|state| state.path.clone())
            .ok_or_else(|| format!("pane {} is not open", pane.as_str()))?;
        {
            let history = self.pane_history_mut(pane);
            history.forward.pop();
            history.push_back(current_path);
        }
        self.load_path_in_pane(pane, path, size, false)
    }
}
