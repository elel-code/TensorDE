impl ShellScene {

    fn place_pointer_target_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        self.place_index_at_screen_point(point, size).is_some()
    }

    fn place_pointer_active(&self) -> bool {
        self.place_press.is_some()
            || self
                .internal_drag
                .as_ref()
                .and_then(ShellInternalDrag::source_place_index)
                .is_some()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn active_place_drag_source_index(&self) -> Option<usize> {
        self.internal_drag
            .as_ref()
            .filter(|drag| drag.active)
            .and_then(ShellInternalDrag::source_place_index)
    }

    fn end_place_pointer(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> (bool, Option<ShellPlaceActivation>) {
        self.pointer = Some(point);
        if let Some(source_index) = self
            .internal_drag
            .as_ref()
            .and_then(ShellInternalDrag::source_place_index)
        {
            let was_active = self.internal_drag.as_ref().is_some_and(|drag| drag.active);
            self.place_press = None;
            let drop_changed = self.finish_internal_drag(point, size);
            let hover_changed = self.refresh_hover(size);
            let activation = if !was_active
                && self.place_index_at_screen_point(point, size) == Some(source_index)
            {
                self.activate_place_index(source_index, point)
            } else {
                None
            };
            return (
                was_active || drop_changed || hover_changed || activation.is_some(),
                activation,
            );
        }

        let press = self.place_press.take();
        let Some(source_index) = press.as_ref().map(|press| press.index) else {
            return (self.refresh_hover(size), None);
        };
        let _ = self.clear_dnd_hover_target();
        let within_click_distance = press
            .as_ref()
            .is_none_or(|press| point_distance(press.point, point) < RUBBER_BAND_START_THRESHOLD);
        let activation = if within_click_distance
            && self.place_index_at_screen_point(point, size) == Some(source_index)
        {
            self.activate_place_index(source_index, point)
        } else {
            None
        };
        let hover_changed = self.refresh_hover(size);
        (hover_changed || activation.is_some(), activation)
    }

    fn place_index_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<usize> {
        if !self.places_sidebar_rect(size).contains(point) {
            return None;
        }
        self.place_row_rects(size)
            .into_iter()
            .find_map(|(index, rect)| rect.contains(point).then_some(index))
    }

    fn place_participates_in_dnd(&self, index: usize) -> bool {
        self.places
            .get(index)
            .is_some_and(|place| place.group.is_empty() && !place.network && place.device.is_none())
    }

    fn place_dnd_gap_index_is_valid(&self, index: usize) -> bool {
        let Some(first_index) = self
            .places
            .iter()
            .enumerate()
            .find_map(|(place_index, _place)| {
                self.place_participates_in_dnd(place_index)
                    .then_some(place_index)
            })
        else {
            return false;
        };
        let Some(last_index) =
            self.places
                .iter()
                .enumerate()
                .rev()
                .find_map(|(place_index, _place)| {
                    self.place_participates_in_dnd(place_index)
                        .then_some(place_index)
                })
        else {
            return false;
        };
        index >= first_index && index <= last_index.saturating_add(1)
    }

    fn place_row_rects(&self, size: PhysicalSize<u32>) -> Vec<(usize, ViewRect)> {
        let panel = self.places_panel_rect(size);
        if panel.width <= 0.0 || panel.height <= 0.0 {
            return Vec::new();
        }
        let mut rows = Vec::with_capacity(self.places.len());
        let top_padding = self.scale_metric(PLACES_SIDEBAR_TOP_PADDING);
        let title_height = self.scale_metric(PLACES_TITLE_HEIGHT);
        let padding_x = self.scale_metric(PLACES_SIDEBAR_PADDING_X);
        let section_height = self.scale_metric(PLACES_SECTION_HEIGHT);
        let row_height = self.scale_metric(PLACES_ROW_HEIGHT);
        let row_gap = self.scale_metric(PLACES_ROW_GAP);
        let mut y = panel.y + top_padding + title_height - self.places_scroll_y;
        let mut previous_group = None;
        for (index, place) in self.places.iter().enumerate() {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                y += section_height;
            }
            let rect = ViewRect {
                x: panel.x + padding_x,
                y,
                width: (panel.width - padding_x * 2.0).max(1.0),
                height: row_height,
            };
            if rect.y < panel.bottom() && rect.bottom() > panel.y {
                rows.push((index, rect));
            }
            y += row_height + row_gap;
            previous_group = Some(place.group);
        }
        rows
    }

    fn place_gap_rects(&self, size: PhysicalSize<u32>) -> Vec<(usize, ViewRect)> {
        let panel = self.places_panel_rect(size);
        if panel.width <= 0.0 || panel.height <= 0.0 {
            return Vec::new();
        }
        let rows = self.place_row_rects(size);
        let dnd_rows = rows
            .into_iter()
            .filter(|(index, _rect)| self.place_participates_in_dnd(*index))
            .collect::<Vec<_>>();
        let Some((first_index, first_rect)) = dnd_rows.first().copied() else {
            return Vec::new();
        };
        let gap_height = self.scale_metric(8.0).max(4.0);
        let mut gaps = Vec::with_capacity(dnd_rows.len() + 1);
        let mut push_gap = |index: usize, center_y: f32, row: ViewRect| {
            let rect = ViewRect {
                x: row.x,
                y: center_y - gap_height / 2.0,
                width: row.width,
                height: gap_height,
            };
            if rect.y < panel.bottom() && rect.bottom() > panel.y {
                gaps.push((index, rect));
            }
        };
        push_gap(first_index, first_rect.y, first_rect);
        for pair in dnd_rows.windows(2) {
            let (index, rect) = pair[1];
            let (_, previous) = pair[0];
            push_gap(index, (previous.bottom() + rect.y) / 2.0, rect);
        }
        if let Some((last_index, last_rect)) = dnd_rows.last().copied() {
            push_gap(last_index + 1, last_rect.bottom(), last_rect);
        }
        gaps
    }

    fn place_gap_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<usize> {
        if !self.places_sidebar_rect(size).contains(point) {
            return None;
        }
        self.place_gap_rects(size)
            .into_iter()
            .find_map(|(index, rect)| rect.contains(point).then_some(index))
    }

    fn place_gap_at_screen_point_for_drag_source(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        source: &ShellInternalDragSource,
    ) -> Option<usize> {
        if let Some(index) = self.place_gap_at_screen_point(point, size) {
            return Some(index);
        }
        if !matches!(source, ShellInternalDragSource::Place { .. }) {
            return None;
        }
        self.place_row_gap_at_screen_point(point, size)
    }

    fn place_row_gap_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<usize> {
        if !self.places_sidebar_rect(size).contains(point) {
            return None;
        }
        self.place_row_rects(size)
            .into_iter()
            .filter(|(index, _rect)| self.place_participates_in_dnd(*index))
            .find_map(|(index, rect)| {
                if !rect.contains(point) {
                    return None;
                }
                let before = point.y < rect.y + rect.height / 2.0;
                Some(if before {
                    index
                } else {
                    index.saturating_add(1)
                })
            })
    }

    fn place_gap_rect_for_index(&self, index: usize, size: PhysicalSize<u32>) -> Option<ViewRect> {
        self.place_gap_rects(size)
            .into_iter()
            .find_map(|(gap_index, rect)| (gap_index == index).then_some(rect))
    }

    fn places_sidebar_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let width = self.places_sidebar_width(size);
        let y = self.pane_top_y();
        let height = (size.height as f32 - y - self.pane_margin()).max(1.0);
        ViewRect {
            x: 0.0,
            y,
            width,
            height,
        }
    }

    fn places_panel_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return ViewRect {
                x: sidebar.x,
                y: sidebar.y,
                width: 0.0,
                height: 0.0,
            };
        }
        let margin_x = self
            .scale_metric(PLACES_SIDEBAR_PANEL_MARGIN_X)
            .min(sidebar.width / 3.0);
        let margin_bottom = self
            .scale_metric(PLACES_SIDEBAR_PANEL_MARGIN_BOTTOM)
            .min(sidebar.height / 3.0);
        let y = sidebar.y;
        let task_top = self
            .places_task_area_rect(size)
            .map(|rect| rect.y - self.scale_metric(PLACES_TASK_AREA_GAP))
            .unwrap_or_else(|| sidebar.bottom() - margin_bottom);
        ViewRect {
            x: sidebar.x + margin_x,
            y,
            width: (sidebar.width - margin_x * 2.0).max(1.0),
            height: (task_top - y).max(1.0),
        }
    }

    fn places_task_area_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        if self.task_statuses.is_empty() {
            return None;
        }
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return None;
        }
        let margin_x = self
            .scale_metric(PLACES_SIDEBAR_PANEL_MARGIN_X)
            .min(sidebar.width / 3.0);
        let margin_bottom = self
            .scale_metric(PLACES_SIDEBAR_PANEL_MARGIN_BOTTOM)
            .min(sidebar.height / 3.0);
        let min_height = self.scale_metric(52.0);
        let desired_height = self.scale_metric(PLACES_TASK_AREA_HEIGHT);
        let max_height = (sidebar.height - self.scale_metric(72.0)).max(0.0);
        let height = desired_height.min(max_height);
        if height < min_height {
            return None;
        }
        Some(ViewRect {
            x: sidebar.x + margin_x,
            y: sidebar.bottom() - margin_bottom - height,
            width: (sidebar.width - margin_x * 2.0).max(1.0),
            height,
        })
    }

    fn places_content_height(&self) -> f32 {
        let top_padding = self.scale_metric(PLACES_SIDEBAR_TOP_PADDING);
        let title_height = self.scale_metric(PLACES_TITLE_HEIGHT);
        let section_height = self.scale_metric(PLACES_SECTION_HEIGHT);
        let row_height = self.scale_metric(PLACES_ROW_HEIGHT);
        let row_gap = self.scale_metric(PLACES_ROW_GAP);
        if self.places.is_empty() {
            return top_padding * 2.0 + title_height;
        }

        let mut height = top_padding + title_height;
        let mut previous_group = None;
        for place in &self.places {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                height += section_height;
            }
            height += row_height + row_gap;
            previous_group = Some(place.group);
        }
        height - row_gap + top_padding
    }

    fn max_places_scroll_y(&self, size: PhysicalSize<u32>) -> f32 {
        let panel = self.places_panel_rect(size);
        (self.places_content_height() - panel.height).max(0.0)
    }

    fn places_scrollbar_rects(&self, size: PhysicalSize<u32>) -> Option<(ViewRect, ViewRect)> {
        let panel = self.places_panel_rect(size);
        let max_scroll = self.max_places_scroll_y(size);
        if panel.width <= 0.0 || panel.height <= 0.0 || max_scroll <= f32::EPSILON {
            return None;
        }

        let scrollbar_margin = self.scale_metric(PLACES_SCROLLBAR_MARGIN);
        let scrollbar_width = self.scale_metric(PLACES_SCROLLBAR_WIDTH);
        let min_thumb_height = self.scale_metric(PLACES_SCROLLBAR_MIN_THUMB_HEIGHT);
        let track_height = (panel.height - scrollbar_margin * 2.0).max(1.0);
        let content_height = self.places_content_height().max(panel.height);
        let thumb_height = (panel.height / content_height * track_height)
            .clamp(min_thumb_height.min(track_height), track_height);
        let travel = (track_height - thumb_height).max(0.0);
        let scroll_ratio = if max_scroll <= f32::EPSILON {
            0.0
        } else {
            (self.places_scroll_y / max_scroll).clamp(0.0, 1.0)
        };
        let track = ViewRect {
            x: panel.right() - scrollbar_margin - scrollbar_width,
            y: panel.y + scrollbar_margin,
            width: scrollbar_width,
            height: track_height,
        };
        let thumb = ViewRect {
            x: track.x,
            y: panel.y + scrollbar_margin + travel * scroll_ratio,
            width: scrollbar_width,
            height: thumb_height,
        };
        Some((track, thumb))
    }

    fn context_target_for_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextTarget> {
        if let Some(index) = self.place_index_at_screen_point(point, size) {
            let place = self.places.get(index)?;
            return Some(ShellContextTarget::Place {
                index,
                label: place.label.clone(),
                path: place.path.clone(),
                group: place.group,
                device: place.device.clone(),
                network: place.network,
                trash: place.trash,
                root: place.root,
                editable: place.editable,
            });
        }
        for geometry in self.pane_geometries(size) {
            if !geometry.content.contains(point) {
                continue;
            }
            let pane = self.pane_view(geometry.kind)?;
            if let Some(index) = self.pane_context_hit_test_screen_point(pane, geometry, point) {
                let entry = pane.entries.get(index)?;
                let selection_count = if pane.selection.contains(index) {
                    pane.selection.len().max(1)
                } else {
                    1
                };
                return Some(ShellContextTarget::Item {
                    pane: geometry.kind,
                    index,
                    path: self.entry_path_for_pane_view(pane, index)?,
                    is_dir: entry.is_dir,
                    selection_count,
                });
            }
            return Some(ShellContextTarget::Blank {
                pane: geometry.kind,
                path: pane.path.to_path_buf(),
            });
        }
        None
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn drop_target_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellDropTarget> {
        if let Some(index) = self.place_index_at_screen_point(point, size) {
            let place = self.places.get(index)?;
            return Some(ShellDropTarget::Place {
                index,
                path: place.path.clone(),
            });
        }
        if self.places_visible && self.places_panel_rect(size).contains(point) {
            return Some(ShellDropTarget::PlacesBlank);
        }

        for geometry in self.pane_geometries(size) {
            if !geometry.content.contains(point) {
                continue;
            }
            let pane = self.pane_view(geometry.kind)?;
            if let Some(index) = self.pane_drop_hit_test_screen_point(pane, geometry, point) {
                let entry = pane.entries.get(index)?;
                return Some(ShellDropTarget::PaneItem {
                    pane: geometry.kind,
                    index,
                    path: self.entry_path_for_pane_view(pane, index)?,
                    is_dir: entry.is_dir,
                });
            }
            return Some(ShellDropTarget::PaneBlank {
                pane: geometry.kind,
                path: pane.path.to_path_buf(),
            });
        }

        None
    }
}
