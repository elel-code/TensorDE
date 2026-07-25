impl ShellScene {
    fn details_header_y(&self) -> f32 {
        self.pane_top_y() + self.top_bar_height() + self.filter_bar_height()
    }

    fn filter_bar_height(&self) -> f32 {
        if self.filter_active || !self.filter_pattern.is_empty() {
            self.scale_metric(FILTER_BAR_HEIGHT)
        } else {
            0.0
        }
    }

    fn filter_bar_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        let height = self.filter_bar_height();
        (height > 0.0).then(|| ViewRect {
            x: self.content_origin_x(size),
            y: self.pane_top_y() + self.top_bar_height(),
            width: self.content_width(size),
            height,
        })
    }

    fn content_width(&self, size: PhysicalSize<u32>) -> f32 {
        let reserved = if self.content_scrollbar_axis() == ContentScrollbarAxis::Vertical {
            self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT)
        } else {
            0.0
        };
        (self.pane_width(size) - reserved).max(1.0)
    }

    fn pane_width(&self, size: PhysicalSize<u32>) -> f32 {
        self.split_pane_metrics(size)
            .map(|metrics| metrics.left_width)
            .unwrap_or_else(|| (size.width as f32 - self.content_origin_x(size)).max(1.0))
    }

    fn pane_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let y = self.pane_top_y();
        let bottom_margin = self.pane_margin();
        ViewRect {
            x: self.content_origin_x(size),
            y,
            width: self.pane_width(size),
            height: (size.height.max(1) as f32 - y - bottom_margin).max(1.0),
        }
    }

    fn viewport_height(&self, size: PhysicalSize<u32>) -> f32 {
        let reserved = if self.content_scrollbar_axis() == ContentScrollbarAxis::Horizontal {
            self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT)
        } else {
            0.0
        };
        (self.status_bar_rect(size).y - self.content_origin_y() - reserved).max(1.0)
    }

    fn places_sidebar_width(&self, size: PhysicalSize<u32>) -> f32 {
        if !self.places_visible {
            return 0.0;
        }
        let (min_width, max_width) = self.places_sidebar_width_bounds(size);
        if max_width <= f32::EPSILON {
            return 0.0;
        }
        self.scale_metric(self.places_width)
            .clamp(min_width, max_width)
    }

    fn places_sidebar_width_bounds(&self, size: PhysicalSize<u32>) -> (f32, f32) {
        let width = size.width.max(1) as f32;
        let reserve = self.scale_metric(PLACES_SIDEBAR_RIGHT_RESERVE);
        let max_for_window = (width - reserve).max(0.0);
        let min_width = self
            .scale_metric(PLACES_SIDEBAR_MIN_WIDTH)
            .min(max_for_window);
        let responsive_width = (width * PLACES_SIDEBAR_MAX_WIDTH_RATIO).max(min_width);
        let max_width = responsive_width.min(max_for_window).max(0.0);
        (min_width.min(max_width), max_width)
    }

    fn set_places_sidebar_width_px(&mut self, desired_width: f32, size: PhysicalSize<u32>) -> bool {
        if !self.places_visible {
            return false;
        }
        let (min_width, max_width) = self.places_sidebar_width_bounds(size);
        if max_width <= f32::EPSILON {
            return false;
        }
        let next_width = desired_width.clamp(min_width, max_width);
        let old_width = self.places_sidebar_width(size);
        if (old_width - next_width).abs() <= 0.5 {
            return false;
        }
        self.places_width = next_width / self.ui_scale().max(f32::EPSILON);
        self.places_resize_changes += 1;
        fika_log!(
            "[fika-wgpu] places-resize width={:.1} min={:.1} max={:.1} changes={}",
            next_width,
            min_width,
            max_width,
            self.places_resize_changes
        );
        true
    }

    fn split_pane_metrics(&self, size: PhysicalSize<u32>) -> Option<ShellPaneSplitMetrics> {
        self.panes.get(ShellPaneId::SLOT_1)?;
        let origin_x = self.content_origin_x(size);
        let total_width = (size.width.max(1) as f32 - origin_x).max(1.0);
        let divider_width = self.scale_metric(SPLIT_PANE_DIVIDER_WIDTH);
        let (available_width, min_width, max_left_width) =
            self.split_pane_width_bounds_for_total(total_width, divider_width);
        let left_width = (available_width * self.split_pane_left_fraction)
            .clamp(min_width, max_left_width)
            .round()
            .max(1.0);
        let divider = ViewRect {
            x: origin_x + left_width,
            y: self.pane_top_y(),
            width: divider_width,
            height: (size.height.max(1) as f32 - self.pane_top_y() - self.pane_margin()).max(1.0),
        };
        let right_x = divider.right();
        let right_pane = ViewRect {
            x: right_x,
            y: divider.y,
            width: (size.width.max(1) as f32 - right_x).max(1.0),
            height: divider.height,
        };
        Some(ShellPaneSplitMetrics {
            divider,
            right_pane,
            left_width,
        })
    }

    fn split_pane_width_bounds(&self, size: PhysicalSize<u32>) -> Option<(f32, f32, f32)> {
        self.panes.get(ShellPaneId::SLOT_1)?;
        let origin_x = self.content_origin_x(size);
        let total_width = (size.width.max(1) as f32 - origin_x).max(1.0);
        let divider_width = self.scale_metric(SPLIT_PANE_DIVIDER_WIDTH);
        Some(self.split_pane_width_bounds_for_total(total_width, divider_width))
    }

    fn split_pane_width_bounds_for_total(
        &self,
        total_width: f32,
        divider_width: f32,
    ) -> (f32, f32, f32) {
        let available_width = (total_width - divider_width).max(1.0);
        let min_width = self
            .scale_metric(SPLIT_PANE_MIN_WIDTH)
            .min((available_width / 2.0).max(1.0));
        let max_left_width = (available_width - min_width).max(min_width);
        (available_width, min_width, max_left_width)
    }

    fn set_split_pane_left_width_px(
        &mut self,
        desired_left_width: f32,
        size: PhysicalSize<u32>,
    ) -> bool {
        if !self.panes.is_open(ShellPaneId::SLOT_1) {
            return false;
        }
        let Some((available_width, min_width, max_left_width)) = self.split_pane_width_bounds(size)
        else {
            return false;
        };
        let next_width = desired_left_width.clamp(min_width, max_left_width);
        let Some(old_width) = self
            .split_pane_metrics(size)
            .map(|metrics| metrics.left_width)
        else {
            return false;
        };
        if (old_width - next_width).abs() <= 0.5 {
            return false;
        }
        self.split_pane_left_fraction = (next_width / available_width).clamp(0.0, 1.0);
        self.split_pane_changes += 1;
        fika_log!(
            "[fika-wgpu] split-pane-resize left_width={:.1} fraction={:.3} changes={}",
            next_width,
            self.split_pane_left_fraction,
            self.split_pane_changes
        );
        true
    }

    fn content_scrollbar_axis(&self) -> ContentScrollbarAxis {
        scrollbar_axis_for_view_mode(self.panes[ShellPaneId::SLOT_0].view_mode)
    }

    fn pane_content_scrollbar_axis(&self, kind: ShellPaneId) -> Option<ContentScrollbarAxis> {
        self.pane_view(kind)
            .map(|pane| scrollbar_axis_for_view_mode(pane.view_mode))
    }

    fn status_bar_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let height = size.height.max(1) as f32;
        let bar_height = self.status_bar_height().min(height);
        let x = self.content_origin_x(size);
        let pane = self.pane_rect(size);
        ViewRect {
            x,
            y: pane.bottom() - bar_height,
            width: self.pane_width(size),
            height: bar_height,
        }
    }

    fn status_zoom_indicator_rects_for_pane(
        &self,
        pane: ShellPaneId,
        size: PhysicalSize<u32>,
    ) -> Option<StatusZoomIndicatorRects> {
        let pane = self.normalized_pane_id(pane);
        let geometry = self.pane_geometry(pane, size)?;
        pane_status_zoom_indicator_rects(
            geometry.status_bar,
            self.ui_scale(),
            self.text_line_height(),
            self.zoom_fraction_for_pane(pane),
        )
    }

    fn status_zoom_indicator_rects_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<(ShellPaneId, StatusZoomIndicatorRects)> {
        ShellPaneId::ALL.into_iter().find_map(|pane| {
            let rects = self.status_zoom_indicator_rects_for_pane(pane, size)?;
            rects.outer.contains(point).then_some((pane, rects))
        })
    }

    fn status_zoom_contains_screen_point(&self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.status_zoom_indicator_rects_at_screen_point(point, size)
            .is_some()
    }

    fn status_zoom_control_rects_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<(ShellPaneId, StatusZoomIndicatorRects)> {
        self.status_zoom_indicator_rects_at_screen_point(point, size)
            .filter(|(_, rects)| status_zoom_control_contains_point(*rects, point))
    }

    fn clamp_scroll(&mut self, size: PhysicalSize<u32>) {
        self.clamp_pane_scroll(ShellPaneId::SLOT_0, size);
        self.clamp_pane_scroll(ShellPaneId::SLOT_1, size);
        self.clamp_places_scroll(size);
        self.refresh_hover(size);
    }

    fn scroll_by(&mut self, delta_y: f32, size: PhysicalSize<u32>) -> bool {
        self.scroll_by_delta(0.0, delta_y, size)
    }

    /// Scroll under the pointer using both wheel axes.
    ///
    /// Places only consume vertical motion. Content panes project the dominant
    /// wheel component onto their single scrollbar axis so vertical mice still
    /// drive horizontal compact/details layouts, and touchpad horizontal
    /// swipes drive horizontal scroll when that is the active axis.
    fn scroll_by_delta(&mut self, delta_x: f32, delta_y: f32, size: PhysicalSize<u32>) -> bool {
        if self
            .pointer
            .is_some_and(|point| self.places_panel_rect(size).contains(point))
        {
            return self.scroll_places_by(delta_y, size);
        }

        let pane = self
            .pointer
            .and_then(|point| self.pane_id_at_screen_point(point, size))
            .unwrap_or(ShellPaneId::SLOT_0);
        let old_active = self.active_pane();
        self.active_pane = self.normalized_pane_id(pane);
        let scrolled = self.scroll_pane_by_delta(pane, delta_x, delta_y, size);
        let hover_changed = self.refresh_hover(size);
        scrolled || hover_changed || old_active != self.active_pane()
    }

    fn pane_id_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPaneId> {
        self.pane_geometries(size)
            .into_iter()
            .find(|geometry| geometry.content.contains(point))
            .map(|geometry| geometry.kind)
    }

    fn scroll_pane_by_delta(
        &mut self,
        kind: ShellPaneId,
        delta_x: f32,
        delta_y: f32,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some(axis) = self.pane_content_scrollbar_axis(kind) else {
            return false;
        };
        let Some(metrics) = self.pane_scroll_metrics(kind, size) else {
            return false;
        };
        let (old_x, old_y) = self.pane_scroll_offset(kind).unwrap_or((0.0, 0.0));
        // Map 2D wheel onto the pane's single scroll axis: prefer the matching
        // component, fall back to the other so mouse wheels still work on
        // horizontal layouts and trackpads still work on vertical ones.
        let delta = match axis {
            ContentScrollbarAxis::Horizontal => {
                if delta_x.abs() > f32::EPSILON {
                    delta_x
                } else {
                    delta_y
                }
            }
            ContentScrollbarAxis::Vertical => {
                if delta_y.abs() > f32::EPSILON {
                    delta_y
                } else {
                    delta_x
                }
            }
        };
        match axis {
            ContentScrollbarAxis::Horizontal => {
                self.set_pane_scroll_offset(
                    kind,
                    (old_x + delta).clamp(0.0, metrics.max_scroll_x),
                    0.0,
                );
            }
            ContentScrollbarAxis::Vertical => {
                self.set_pane_scroll_offset(
                    kind,
                    0.0,
                    (old_y + delta).clamp(0.0, metrics.max_scroll_y),
                );
            }
        }
        let (new_x, new_y) = self.pane_scroll_offset(kind).unwrap_or((0.0, 0.0));
        let scrolled = (new_x - old_x).abs() > f32::EPSILON || (new_y - old_y).abs() > f32::EPSILON;
        if scrolled {
            self.content_scroll_changes += 1;
        }
        scrolled
    }

    fn pane_scroll_metrics(
        &self,
        kind: ShellPaneId,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPaneScrollMetrics> {
        let geometry = self.pane_geometry(kind, size)?;
        let view = self.pane_view(kind)?;
        let layout =
            self.pane_layout_for_pane(kind, view, geometry.content.width, geometry.content.height);
        Some(ShellPaneScrollMetrics::new(
            layout.content_size(),
            geometry.content,
        ))
    }

    fn pane_scroll_offset(&self, kind: ShellPaneId) -> Option<(f32, f32)> {
        self.pane_state(kind)
            .map(|pane| (pane.scroll_x, pane.scroll_y))
    }

    fn set_pane_scroll_offset(&mut self, kind: ShellPaneId, scroll_x: f32, scroll_y: f32) {
        if let Some(pane) = self.pane_state_mut(kind) {
            pane.scroll_x = scroll_x;
            pane.scroll_y = scroll_y;
        }
    }

    fn clamp_pane_scroll(&mut self, kind: ShellPaneId, size: PhysicalSize<u32>) {
        let Some(metrics) = self.pane_scroll_metrics(kind, size) else {
            return;
        };
        let Some(axis) = self.pane_content_scrollbar_axis(kind) else {
            return;
        };
        let (scroll_x, scroll_y) = self.pane_scroll_offset(kind).unwrap_or((0.0, 0.0));
        match axis {
            ContentScrollbarAxis::Horizontal => {
                self.set_pane_scroll_offset(kind, scroll_x.clamp(0.0, metrics.max_scroll_x), 0.0);
            }
            ContentScrollbarAxis::Vertical => {
                self.set_pane_scroll_offset(kind, 0.0, scroll_y.clamp(0.0, metrics.max_scroll_y));
            }
        }
    }

    fn clamp_places_scroll(&mut self, size: PhysicalSize<u32>) {
        self.places_scroll_y = self
            .places_scroll_y
            .clamp(0.0, self.max_places_scroll_y(size));
    }

    fn scroll_places_by(&mut self, delta_y: f32, size: PhysicalSize<u32>) -> bool {
        let old_y = self.places_scroll_y;
        self.places_scroll_y =
            (self.places_scroll_y + delta_y).clamp(0.0, self.max_places_scroll_y(size));
        let scrolled = (self.places_scroll_y - old_y).abs() > f32::EPSILON;
        if scrolled {
            self.places_scroll_changes += 1;
        }
        let hover_changed = self.refresh_hover(size);
        scrolled || hover_changed
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn max_scroll_x(&self, size: PhysicalSize<u32>) -> f32 {
        self.pane_scroll_metrics(ShellPaneId::SLOT_0, size)
            .map(|metrics| metrics.max_scroll_x)
            .unwrap_or(0.0)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn max_scroll_y(&self, size: PhysicalSize<u32>) -> f32 {
        self.pane_scroll_metrics(ShellPaneId::SLOT_0, size)
            .map(|metrics| metrics.max_scroll_y)
            .unwrap_or(0.0)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn content_scrollbar_rects(&self, size: PhysicalSize<u32>) -> Option<(ViewRect, ViewRect)> {
        self.pane_content_scrollbar_rects(ShellPaneId::SLOT_0, size)
    }

    fn pane_content_scrollbar_rects(
        &self,
        kind: ShellPaneId,
        size: PhysicalSize<u32>,
    ) -> Option<(ViewRect, ViewRect)> {
        let projection = self.pane_projection(kind, size)?;
        self.content_scrollbar_rects_for_projection(&projection)
    }

    fn content_scrollbar_rects_for_projection(
        &self,
        projection: &ShellPaneProjection<'_>,
    ) -> Option<(ViewRect, ViewRect)> {
        let metrics = projection.scroll_metrics;
        match scrollbar_axis_for_view_mode(projection.view.view_mode) {
            ContentScrollbarAxis::Vertical => {
                let max_scroll = metrics.max_scroll_y;
                if max_scroll <= f32::EPSILON {
                    return None;
                }
                let viewport_extent = metrics.viewport_height;
                let slot = ViewRect {
                    x: projection.geometry.content.right(),
                    y: projection.geometry.content.y,
                    width: self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT).min(
                        (projection.geometry.pane.right() - projection.geometry.content.x).max(1.0),
                    ),
                    height: viewport_extent,
                };
                let track = inset_content_scrollbar_slot(slot, self.ui_scale())?;
                let content_extent = metrics.content_size.height;
                let min_thumb_size = self.scale_metric(CONTENT_SCROLLBAR_MIN_THUMB_SIZE);
                let thumb_extent = (track.height * (viewport_extent / content_extent))
                    .clamp(min_thumb_size.min(track.height), track.height);
                if thumb_extent >= track.height {
                    return None;
                }
                let travel = (track.height - thumb_extent).max(0.0);
                let thumb_y =
                    track.y + (projection.view.scroll_y / max_scroll).clamp(0.0, 1.0) * travel;
                Some((
                    track,
                    ViewRect {
                        x: track.x,
                        y: thumb_y,
                        width: track.width,
                        height: thumb_extent,
                    },
                ))
            }
            ContentScrollbarAxis::Horizontal => {
                let max_scroll = metrics.max_scroll_x;
                if max_scroll <= f32::EPSILON {
                    return None;
                }
                let viewport_extent = metrics.viewport_width;
                let slot = ViewRect {
                    x: projection.geometry.content.x,
                    y: projection.geometry.content.bottom(),
                    width: viewport_extent,
                    height: self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT).min(
                        (projection.geometry.status_bar.y - projection.geometry.content.y).max(1.0),
                    ),
                };
                let track = inset_content_scrollbar_slot(slot, self.ui_scale())?;
                let content_extent = metrics.content_size.width;
                let min_thumb_size = self.scale_metric(CONTENT_SCROLLBAR_MIN_THUMB_SIZE);
                let thumb_extent = (track.width * (viewport_extent / content_extent))
                    .clamp(min_thumb_size.min(track.width), track.width);
                if thumb_extent >= track.width {
                    return None;
                }
                let travel = (track.width - thumb_extent).max(0.0);
                let thumb_x =
                    track.x + (projection.view.scroll_x / max_scroll).clamp(0.0, 1.0) * travel;
                Some((
                    track,
                    ViewRect {
                        x: thumb_x,
                        y: track.y,
                        width: thumb_extent,
                        height: track.height,
                    },
                ))
            }
        }
    }
}
