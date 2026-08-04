impl ShellScene {

    fn ui_scale(&self) -> f32 {
        normalized_scale_factor(self.scale_factor).max(1.0)
    }

    fn scale_metric(&self, value: f32) -> f32 {
        (value * self.ui_scale()).round().max(1.0)
    }

    fn pane_zoom_level(&self, pane: ShellPaneId) -> Option<i32> {
        self.pane_state(self.normalized_pane_id(pane))
            .map(ShellPaneState::zoom_level)
    }

    fn preview_default_zoom_level_for_view_mode(view_mode: ShellViewMode) -> i32 {
        match view_mode {
            ShellViewMode::Icons => FILE_MANAGER_ICONS_PREVIEW_ZOOM_LEVEL_DEFAULT,
            ShellViewMode::Compact => FILE_MANAGER_COMPACT_PREVIEW_ZOOM_LEVEL_DEFAULT,
            ShellViewMode::Details => FILE_MANAGER_DETAILS_PREVIEW_ZOOM_LEVEL_DEFAULT,
        }
    }

    fn reset_default_zoom_level_for_view_mode(view_mode: ShellViewMode) -> i32 {
        match view_mode {
            ShellViewMode::Icons => FILE_MANAGER_ICONS_RESET_ZOOM_LEVEL_DEFAULT,
            ShellViewMode::Compact => FILE_MANAGER_COMPACT_RESET_ZOOM_LEVEL_DEFAULT,
            ShellViewMode::Details => FILE_MANAGER_DETAILS_RESET_ZOOM_LEVEL_DEFAULT,
        }
    }

    fn zoom_icon_metric_for_level(&self, zoom_level: i32, min: f32, max: f32) -> f32 {
        let scale = self.ui_scale();
        (file_manager_icon_size_for_zoom_level(zoom_level) * scale)
            .round()
            .clamp(min * scale, max * scale)
    }

    fn text_line_height(&self) -> f32 {
        self.scale_metric(TEXT_LINE_HEIGHT)
    }

    fn small_text_line_height(&self) -> f32 {
        self.scale_metric(14.0)
    }

    fn app_toolbar_height(&self) -> f32 {
        self.scale_metric(APP_TOOLBAR_HEIGHT)
    }

    fn app_toolbar_y(&self) -> f32 {
        0.0
    }

    fn pane_margin(&self) -> f32 {
        self.scale_metric(PANE_MARGIN)
    }

    fn pane_top_y(&self) -> f32 {
        self.app_toolbar_height() + self.pane_margin()
    }

    fn top_bar_height(&self) -> f32 {
        self.scale_metric(TOP_BAR_HEIGHT)
    }

    fn status_bar_height(&self) -> f32 {
        self.scale_metric(STATUS_BAR_HEIGHT)
    }

    fn details_header_height(&self) -> f32 {
        self.scale_metric(DETAILS_HEADER_HEIGHT)
    }

    fn details_name_width(&self) -> f32 {
        self.scale_metric(DETAILS_NAME_WIDTH)
    }

    fn details_size_width(&self) -> f32 {
        self.scale_metric(DETAILS_SIZE_WIDTH)
    }

    fn details_modified_width(&self) -> f32 {
        self.scale_metric(DETAILS_MODIFIED_WIDTH)
    }

    fn selection_for_reloaded_pane_entries(
        &self,
        pane_id: ShellPaneId,
        entries: &[Entry],
    ) -> ShellSelection {
        let Some(pane) = self.pane_state(pane_id) else {
            return ShellSelection::default();
        };
        if pane.selection.selected.is_empty() {
            return ShellSelection::default();
        }

        let selected_names = pane
            .selection
            .selected
            .iter()
            .filter_map(|index| pane.entries.get(*index))
            .map(|entry| entry.name.to_string())
            .collect::<BTreeSet<_>>();
        let anchor_name = pane
            .selection
            .anchor
            .and_then(|index| pane.entries.get(index))
            .map(|entry| entry.name.to_string());
        let focus_name = pane
            .selection
            .focus
            .and_then(|index| pane.entries.get(index))
            .map(|entry| entry.name.to_string());

        let selected = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                selected_names
                    .contains(entry.name.as_ref())
                    .then_some(index)
            })
            .collect::<BTreeSet<_>>();
        if selected.is_empty() {
            return ShellSelection::default();
        }

        let anchor = anchor_name
            .and_then(|name| entry_index_by_name(entries, &name))
            .filter(|index| selected.contains(index))
            .or_else(|| selected.iter().next().copied());
        let focus = focus_name
            .and_then(|name| entry_index_by_name(entries, &name))
            .filter(|index| selected.contains(index))
            .or_else(|| selected.iter().next_back().copied());

        ShellSelection {
            selected,
            anchor,
            focus,
        }
    }

    fn view_mode_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellViewMode> {
        self.toolbar_view_mode_segment_at_screen_point(point, size)
            .map(|segment| segment.mode)
    }

    fn pane_path_bar_rect(&self, kind: ShellPaneId, size: PhysicalSize<u32>) -> Option<ViewRect> {
        let pane = self.pane_state(kind)?;
        let geometry = self.pane_geometry(kind, size)?;
        let margin = self.scale_metric(8.0);
        let path_x = geometry.pane.x + margin;
        let available_width = (geometry.pane.right() - path_x - margin).max(0.0);
        let rect = ViewRect {
            x: path_x,
            y: geometry.pane.y + self.scale_metric(4.0),
            width: available_width,
            height: self.scale_metric(28.0),
        };
        let _ = pane;
        (rect.width > self.scale_metric(24.0)).then_some(rect)
    }

    #[cfg(test)]
    fn path_bar_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        self.pane_path_bar_rect(ShellPaneId::SLOT_0, size)
    }

    fn path_bar_contains_screen_point(&self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.path_bar_pane_at_screen_point(point, size).is_some()
    }

    fn path_bar_pane_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPaneId> {
        ShellPaneId::ALL.into_iter().find(|kind| {
            self.pane_path_bar_rect(*kind, size)
                .is_some_and(|rect| rect.contains(point))
        })
    }

    fn path_navigation_action_at_screen_point(
        &self,
        point: ViewPoint,
        _size: PhysicalSize<u32>,
    ) -> Option<PathNavigationAction> {
        let _ = point;
        None
    }

    fn app_toolbar_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        ViewRect {
            x: 0.0,
            y: self.app_toolbar_y(),
            width: size.width.max(1) as f32,
            height: self.app_toolbar_height(),
        }
    }

    fn app_toolbar_layout(&self, size: PhysicalSize<u32>) -> ShellToolbarLayout {
        build_app_toolbar_layout(self.app_toolbar_rect(size), self.ui_scale())
    }

    fn places_toggle_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        self.app_toolbar_layout(size).places_toggle
    }

    fn split_view_button_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        self.app_toolbar_layout(size).split_view
    }

    fn overflow_button_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        self.app_toolbar_layout(size).overflow
    }

    fn toolbar_view_mode_badge_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        self.app_toolbar_layout(size)
            .view_mode
            .map(|control| control.outer)
    }

    fn toolbar_view_mode_segment_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellToolbarViewModeSegment> {
        self.app_toolbar_layout(size)
            .view_mode_segment_at_point(point)
    }

    fn split_view_button_at_screen_point(&self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.split_view_button_rect(size).contains(point)
    }

    fn overflow_button_contains_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        self.overflow_button_rect(size).contains(point)
    }

    fn places_toggle_contains_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        self.places_toggle_rect(size).contains(point)
    }

    #[cfg(test)]
    fn toggle_places_at_screen_point(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<bool> {
        self.places_toggle_rect(size)
            .contains(point)
            .then(|| self.toggle_places_visibility(size))
    }

    fn toggle_places_visibility(&mut self, size: PhysicalSize<u32>) -> bool {
        self.places_visible = !self.places_visible;
        self.places_changes += 1;
        self.scrollbar_drag = None;
        self.rubber_band = None;
        self.hovered_place = None;
        self.last_item_click = None;
        self.clamp_scroll(size);
        tensor_files_log!(
            "[tensor-files] places visible={} width={:.1} changes={}",
            self.places_visible as u8,
            self.places_sidebar_width(size),
            self.places_changes
        );
        true
    }

    fn places_resize_handle_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        if !self.places_visible {
            return None;
        }
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return None;
        }
        let handle_width = self
            .scale_metric(PLACES_RESIZE_HANDLE_WIDTH)
            .max(self.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH));
        let splitter_cover = self.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH + 2.0);
        Some(ViewRect {
            x: (sidebar.right() - handle_width).max(sidebar.x),
            y: sidebar.y,
            width: (handle_width + splitter_cover).min(sidebar.width + splitter_cover),
            height: sidebar.height,
        })
    }

    fn split_pane_resize_handle_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        let divider = self.split_pane_metrics(size)?.divider;
        let handle_width = self
            .scale_metric(SPLIT_PANE_RESIZE_HANDLE_WIDTH)
            .max(divider.width);
        Some(ViewRect {
            x: divider.x + (divider.width - handle_width) / 2.0,
            y: divider.y,
            width: handle_width,
            height: divider.height,
        })
    }

    fn cursor_icon(&self, size: PhysicalSize<u32>) -> CursorIcon {
        if self.scrollbar_drag.is_some_and(|drag| {
            matches!(
                drag.target,
                ScrollbarDragTarget::PlacesResize | ScrollbarDragTarget::SplitPaneResize
            )
        }) {
            return CursorIcon::ColResize;
        }
        if self
            .scrollbar_drag
            .is_some_and(|drag| matches!(drag.target, ScrollbarDragTarget::StatusZoom { .. }))
        {
            return CursorIcon::Pointer;
        }
        if self.scrollbar_drag.is_some() {
            return CursorIcon::Default;
        }
        let Some(point) = self.pointer else {
            return CursorIcon::Default;
        };
        if self
            .places_scrollbar_rects(size)
            .is_some_and(|(track, _)| track.contains(point))
        {
            return CursorIcon::Default;
        }
        if self
            .places_task_area_rect(size)
            .is_some_and(|rect| rect.contains(point))
        {
            return CursorIcon::Pointer;
        }
        if self.places_toggle_contains_screen_point(point, size)
            || self.split_view_button_at_screen_point(point, size)
            || self.overflow_button_contains_screen_point(point, size)
            || self.status_zoom_contains_screen_point(point, size)
            || self
                .toolbar_view_mode_badge_rect(size)
                .is_some_and(|rect| rect.contains(point))
        {
            return CursorIcon::Pointer;
        }
        if self
            .places_resize_handle_rect(size)
            .is_some_and(|rect| rect.contains(point))
            || self
                .split_pane_resize_handle_rect(size)
                .is_some_and(|rect| rect.contains(point))
        {
            CursorIcon::ColResize
        } else if self.path_bar_contains_screen_point(point, size) {
            CursorIcon::Text
        } else {
            CursorIcon::Default
        }
    }

    fn open_with_chooser_cursor_icon(&self, size: PhysicalSize<u32>) -> CursorIcon {
        let Some(point) = self.pointer else {
            return CursorIcon::Default;
        };
        let Some(chooser) = self.open_with_chooser.as_ref() else {
            return CursorIcon::Default;
        };
        match open_with_chooser_pointer_role_at_point(chooser, point, size, self.ui_scale()) {
            OpenWithChooserPointerRole::Text => CursorIcon::Text,
            OpenWithChooserPointerRole::Action => CursorIcon::Pointer,
            OpenWithChooserPointerRole::Default => CursorIcon::Default,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn place_activation_for_press(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPlaceActivation> {
        let index = self.place_index_at_screen_point(point, size)?;
        self.activate_place_index(index, point)
    }

    fn activate_place_index(
        &mut self,
        index: usize,
        point: ViewPoint,
    ) -> Option<ShellPlaceActivation> {
        let target_pane = self.active_pane();
        self.pointer = Some(point);
        let hover_changed = self.set_hovered_place(Some(index));
        let item_hover_changed = self.set_hovered_item(None);
        self.rubber_band = None;
        self.internal_drag = None;
        self.external_drag = None;
        self.place_press = None;
        self.last_item_click = None;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        let place = self.places.get(index)?;
        if place.device.as_ref().is_some_and(|device| !device.mounted) {
            let device = place.device.as_ref()?;
            self.places_changes += 1;
            tensor_files_log!(
                "[tensor-files] place-mount index={} label={:?} target_pane={} path={} changes={}",
                index,
                place.label,
                target_pane.as_str(),
                place.path.display(),
                self.places_changes
            );
            return Some(ShellPlaceActivation::DeviceAction(DeviceActionRequest {
                id: device.id.clone(),
                label: place.label.clone(),
                action: ShellContextMenuAction::MountDevice,
                operation: DevicePlaceOperation::Mount,
                pane: target_pane,
                path: place.path.clone(),
            }));
        }
        self.places_changes += 1;
        tensor_files_log!(
            "[tensor-files] place-open index={} label={:?} target_pane={} path={} hover_changed={} item_hover_changed={} changes={}",
            index,
            place.label,
            target_pane.as_str(),
            place.path.display(),
            hover_changed as u8,
            item_hover_changed as u8,
            self.places_changes
        );
        Some(ShellPlaceActivation::Open {
            pane: target_pane,
            path: place.path.clone(),
        })
    }

    fn begin_place_pointer(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> Option<bool> {
        let index = self.place_index_at_screen_point(point, size)?;
        self.pointer = Some(point);
        let hover_changed = self.set_hovered_place(Some(index));
        let item_hover_changed = self.set_hovered_item(None);
        self.rubber_band = None;
        self.external_drag = None;
        self.last_item_click = None;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        let drag_started = if self.place_participates_in_dnd(index) {
            self.place_press = None;
            self.begin_internal_drag_for_place(index, point)
        } else {
            self.place_press = Some(ShellPlacePress { index, point });
            self.internal_drag = None;
            false
        };
        Some(hover_changed || item_hover_changed || drag_started)
    }
}
