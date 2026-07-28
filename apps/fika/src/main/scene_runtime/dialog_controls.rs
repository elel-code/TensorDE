impl ShellScene {

    fn open_with_chooser_scrollbar_rects(
        &self,
        size: PhysicalSize<u32>,
    ) -> Option<(ViewRect, ViewRect)> {
        let chooser = self.open_with_chooser.as_ref()?;
        let scale = self.ui_scale();
        let rect = open_with_chooser_rect_scaled(chooser, size, scale);
        let list = open_with_chooser_list_rect_scaled(rect, chooser, scale);
        open_with_chooser_scrollbar_rects_scaled(list, chooser, scale)
    }

    fn begin_open_with_scrollbar_drag(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<bool> {
        let (track, thumb) = self.open_with_chooser_scrollbar_rects(size)?;
        if !track.contains(point) {
            return None;
        }
        let grab_offset = if thumb.contains(point) {
            point.y - thumb.y
        } else {
            thumb.height / 2.0
        };
        self.scrollbar_drag = Some(ScrollbarDrag {
            target: ScrollbarDragTarget::OpenWith,
            grab_offset,
        });
        self.pointer = Some(point);
        Some(self.update_scrollbar_drag(point, size))
    }

    fn begin_scrollbar_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> Option<bool> {
        if let Some((track, thumb)) = self.places_scrollbar_rects(size)
            && track.contains(point)
        {
            let grab_offset = if thumb.contains(point) {
                point.y - thumb.y
            } else {
                thumb.height / 2.0
            };
            self.scrollbar_drag = Some(ScrollbarDrag {
                target: ScrollbarDragTarget::Places,
                grab_offset,
            });
            self.pointer = Some(point);
            return Some(self.update_scrollbar_drag(point, size));
        }

        if let Some(handle) = self.places_resize_handle_rect(size)
            && handle.contains(point)
        {
            let sidebar = self.places_sidebar_rect(size);
            self.scrollbar_drag = Some(ScrollbarDrag {
                target: ScrollbarDragTarget::PlacesResize,
                grab_offset: point.x - sidebar.right(),
            });
            self.pointer = Some(point);
            return Some(self.update_scrollbar_drag(point, size));
        }

        if let Some(handle) = self.split_pane_resize_handle_rect(size)
            && handle.contains(point)
            && let Some(metrics) = self.split_pane_metrics(size)
        {
            self.scrollbar_drag = Some(ScrollbarDrag {
                target: ScrollbarDragTarget::SplitPaneResize,
                grab_offset: point.x - metrics.divider.x,
            });
            self.pointer = Some(point);
            return Some(self.update_scrollbar_drag(point, size));
        }

        if let Some((pane, rects)) = self.status_zoom_control_rects_at_screen_point(point, size) {
            if rects.label.contains(point) {
                self.pointer = Some(point);
                return Some(self.set_zoom_step(pane, 0, size, true));
            }
            let thumb_center_offset = if rects.thumb_outer.contains(point) {
                point.x - (rects.thumb_outer.x + rects.thumb_outer.width / 2.0)
            } else {
                0.0
            };
            self.scrollbar_drag = Some(ScrollbarDrag {
                target: ScrollbarDragTarget::StatusZoom { pane },
                grab_offset: thumb_center_offset,
            });
            self.pointer = Some(point);
            return Some(self.update_scrollbar_drag(point, size));
        }

        if let Some((pane, axis, _track, thumb)) = self.content_scrollbar_hit_at_point(point, size)
        {
            let grab_offset = match axis {
                ContentScrollbarAxis::Vertical => {
                    if thumb.contains(point) {
                        point.y - thumb.y
                    } else {
                        thumb.height / 2.0
                    }
                }
                ContentScrollbarAxis::Horizontal => {
                    if thumb.contains(point) {
                        point.x - thumb.x
                    } else {
                        thumb.width / 2.0
                    }
                }
            };
            self.scrollbar_drag = Some(ScrollbarDrag {
                target: ScrollbarDragTarget::Content { pane, axis },
                grab_offset,
            });
            self.pointer = Some(point);
            return Some(self.update_scrollbar_drag(point, size));
        }

        None
    }

    fn scrollbar_drag_hit_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        if let Some((track, _thumb)) = self.places_scrollbar_rects(size)
            && track.contains(point)
        {
            return true;
        }
        if let Some(handle) = self.places_resize_handle_rect(size)
            && handle.contains(point)
        {
            return true;
        }
        if let Some(handle) = self.split_pane_resize_handle_rect(size)
            && handle.contains(point)
            && self.split_pane_metrics(size).is_some()
        {
            return true;
        }
        if self
            .status_zoom_control_rects_at_screen_point(point, size)
            .is_some()
        {
            return true;
        }
        self.content_scrollbar_hit_at_point(point, size).is_some()
    }

    fn content_scrollbar_hit_at_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<(ShellPaneId, ContentScrollbarAxis, ViewRect, ViewRect)> {
        for kind in ShellPaneId::ALL {
            let Some(axis) = self.pane_content_scrollbar_axis(kind) else {
                continue;
            };
            let Some((track, thumb)) = self.pane_content_scrollbar_rects(kind, size) else {
                continue;
            };
            if track.contains(point) {
                return Some((kind, axis, track, thumb));
            }
        }
        None
    }

    fn update_scrollbar_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let Some(drag) = self.scrollbar_drag else {
            return false;
        };
        let old_x = self.panes[ShellPaneId::SLOT_0].scroll_x;
        let old_y = self.panes[ShellPaneId::SLOT_0].scroll_y;
        let old_split_scroll = self
            .panes
            .get(ShellPaneId::SLOT_1)
            .map(|pane| (pane.scroll_x, pane.scroll_y));
        let old_places_y = self.places_scroll_y;
        let old_places_width = self.places_sidebar_width(size);
        let old_split_left_width = self
            .split_pane_metrics(size)
            .map(|metrics| metrics.left_width);
        let old_open_with_scroll = self
            .open_with_chooser
            .as_ref()
            .map(|chooser| chooser.scroll_row);
        let old_zoom_steps = ShellPaneId::ALL.map(|pane| self.pane_zoom_step(pane));

        match drag.target {
            ScrollbarDragTarget::OpenWith => {
                if let Some((track, thumb)) = self.open_with_chooser_scrollbar_rects(size) {
                    let max_scroll = self
                        .open_with_chooser
                        .as_ref()
                        .map(|chooser| {
                            chooser
                                .tree_row_count()
                                .saturating_sub(open_with_chooser_visible_row_count(chooser))
                        })
                        .unwrap_or(0);
                    let next_row = scrollbar_scroll_from_pointer(
                        point.y,
                        drag.grab_offset,
                        track.y,
                        track.height,
                        thumb.height,
                        max_scroll as f32,
                    )
                    .round() as usize;
                    if let Some(chooser) = self.open_with_chooser.as_mut() {
                        chooser.scroll_row = next_row.min(max_scroll);
                    }
                }
            }
            ScrollbarDragTarget::PlacesResize => {
                let desired_width = point.x - drag.grab_offset;
                self.set_places_sidebar_width_px(desired_width, size);
            }
            ScrollbarDragTarget::SplitPaneResize => {
                let desired_left_width = point.x - self.content_origin_x(size) - drag.grab_offset;
                self.set_split_pane_left_width_px(desired_left_width, size);
            }
            ScrollbarDragTarget::StatusZoom { pane } => {
                if let Some(rects) = self.status_zoom_indicator_rects_for_pane(pane, size) {
                    let thumb_center_x = point.x - drag.grab_offset;
                    let fraction =
                        ((thumb_center_x - rects.track.x) / rects.track.width).clamp(0.0, 1.0);
                    self.set_zoom_fraction(pane, fraction, size, false);
                }
            }
            ScrollbarDragTarget::Places => {
                if let Some((track, thumb)) = self.places_scrollbar_rects(size) {
                    self.places_scroll_y = scrollbar_scroll_from_pointer(
                        point.y,
                        drag.grab_offset,
                        track.y,
                        track.height,
                        thumb.height,
                        self.max_places_scroll_y(size),
                    );
                }
            }
            ScrollbarDragTarget::Content {
                pane,
                axis: ContentScrollbarAxis::Vertical,
            } => {
                if let Some((track, thumb)) = self.pane_content_scrollbar_rects(pane, size) {
                    let next_y = scrollbar_scroll_from_pointer(
                        point.y,
                        drag.grab_offset,
                        track.y,
                        track.height,
                        thumb.height,
                        self.pane_scroll_metrics(pane, size)
                            .map(|metrics| metrics.max_scroll_y)
                            .unwrap_or(0.0),
                    );
                    self.set_pane_scroll_offset(pane, 0.0, next_y);
                }
            }
            ScrollbarDragTarget::Content {
                pane,
                axis: ContentScrollbarAxis::Horizontal,
            } => {
                if let Some((track, thumb)) = self.pane_content_scrollbar_rects(pane, size) {
                    let next_x = scrollbar_scroll_from_pointer(
                        point.x,
                        drag.grab_offset,
                        track.x,
                        track.width,
                        thumb.width,
                        self.pane_scroll_metrics(pane, size)
                            .map(|metrics| metrics.max_scroll_x)
                            .unwrap_or(0.0),
                    );
                    self.set_pane_scroll_offset(pane, next_x, 0.0);
                }
            }
        }

        self.clamp_scroll(size);
        let content_changed = (self.panes[ShellPaneId::SLOT_0].scroll_x - old_x).abs()
            > f32::EPSILON
            || (self.panes[ShellPaneId::SLOT_0].scroll_y - old_y).abs() > f32::EPSILON;
        let split_content_changed = old_split_scroll
            .zip(
                self.panes
                    .get(ShellPaneId::SLOT_1)
                    .map(|pane| (pane.scroll_x, pane.scroll_y)),
            )
            .is_some_and(|((old_x, old_y), (new_x, new_y))| {
                (old_x - new_x).abs() > f32::EPSILON || (old_y - new_y).abs() > f32::EPSILON
            });
        let places_changed = (self.places_scroll_y - old_places_y).abs() > f32::EPSILON;
        let places_resized =
            (self.places_sidebar_width(size) - old_places_width).abs() > f32::EPSILON;
        let split_resized = old_split_left_width
            .zip(
                self.split_pane_metrics(size)
                    .map(|metrics| metrics.left_width),
            )
            .is_some_and(|(old_width, new_width)| (old_width - new_width).abs() > f32::EPSILON);
        let open_with_changed = old_open_with_scroll
            .zip(
                self.open_with_chooser
                    .as_ref()
                    .map(|chooser| chooser.scroll_row),
            )
            .is_some_and(|(old_scroll, new_scroll)| old_scroll != new_scroll);
        let zoom_changed = ShellPaneId::ALL
            .into_iter()
            .zip(old_zoom_steps)
            .any(|(pane, old_step)| self.pane_zoom_step(pane) != old_step);
        if places_changed {
            self.places_scroll_changes += 1;
        }
        if content_changed || split_content_changed {
            self.content_scroll_changes += 1;
        }
        if open_with_changed {
            self.open_with_changes += 1;
        }
        let hover_changed = self.refresh_hover(size);
        content_changed
            || split_content_changed
            || places_changed
            || open_with_changed
            || zoom_changed
            || places_resized
            || split_resized
            || hover_changed
    }

    fn end_scrollbar_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        if self.scrollbar_drag.is_none() {
            return false;
        }
        self.pointer = Some(point);
        let changed = self.update_scrollbar_drag(point, size);
        self.scrollbar_drag = None;
        changed
    }

    fn is_scrollbar_dragging(&self) -> bool {
        self.scrollbar_drag.is_some()
    }

    fn set_pointer(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        if self.scrollbar_drag.is_some() {
            return self.update_scrollbar_drag(point, size);
        }
        if self.external_drag.is_some() {
            return self.update_external_drag(point, size);
        }
        if self.drop_menu.is_some() {
            return self.update_drop_menu_hover(point, size);
        }
        if self.context_menu.is_some() {
            return self.update_context_menu_hover(point, size);
        }
        if self.internal_drag.is_some() {
            return self.update_internal_drag(point, size);
        }
        if self.rubber_band.is_some() {
            return self.update_rubber_band(point, size);
        }
        self.refresh_hover(size)
    }

    fn clear_pointer(&mut self) -> bool {
        self.pointer = None;
        self.context_menu_safe_triangle.reset();
        let changed = self.hovered_item.take().is_some()
            || self.hovered_place.take().is_some()
            || self.internal_drag.take().is_some()
            || self.external_drag.take().is_some()
            || self.place_press.take().is_some()
            || self.clear_dnd_hover_target();
        if changed {
            self.hit_tests += 1;
        }
        changed
    }

    fn begin_pane_pointer(&mut self, click: SelectionClick, size: PhysicalSize<u32>) -> bool {
        self.rubber_band = None;
        self.external_drag = None;
        self.place_press = None;
        self.pointer = Some(click.point);
        let active_changed = self.focus_pane_at_screen_point(click.point, size);
        let hit = self.pane_item_at_screen_point(click.point, size);
        let hover_changed = self.set_hovered_item(hit);
        if let Some(target) = hit {
            let drag_started = !click.extend
                && !click.toggle
                && self.begin_internal_drag_for_pane_item(target.pane, target.index, click.point);
            let selection_changed = self
                .pane_selection_mut(target.pane)
                .is_some_and(|selection| {
                    selection.apply_click(Some(target.index), click.extend, click.toggle)
                });
            if selection_changed {
                self.selection_changes += 1;
            }
            return active_changed || hover_changed || selection_changed || drag_started;
        }
        self.internal_drag = None;
        self.place_press = None;
        let pane_id = self.active_pane();
        let Some(projection) = self.pane_projection(pane_id, size) else {
            return active_changed || hover_changed;
        };
        if !projection.geometry.content.contains(click.point) {
            return active_changed || hover_changed;
        }

        let Some(start) = screen_to_content_point(
            click.point,
            ViewPoint {
                x: projection.view.scroll_x,
                y: projection.view.scroll_y,
            },
            projection.geometry.content,
        ) else {
            return active_changed || hover_changed;
        };
        let base_selection = projection.view.selection.clone();
        self.rubber_band = Some(RubberBand::new(
            start,
            RubberBandMode::from_modifiers(click.extend, click.toggle),
            base_selection,
        ));
        let selection_changed = self
            .pane_selection_mut(pane_id)
            .is_some_and(|selection| selection.apply_click(None, click.extend, click.toggle));
        if selection_changed {
            self.selection_changes += 1;
        }
        active_changed || hover_changed || selection_changed
    }

    fn end_pane_pointer(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        if let Some(drag) = self.internal_drag.as_ref() {
            let was_active = drag.active;
            let drop_changed = self.finish_internal_drag(point, size);
            let hover_changed = self.refresh_hover(size);
            return was_active || drop_changed || hover_changed;
        }
        let band_was_active = self.rubber_band.as_ref().is_some_and(|band| band.active);
        let changed = if self.rubber_band.is_some() {
            self.update_rubber_band(point, size)
        } else {
            self.refresh_hover(size)
        };
        if self.rubber_band.take().is_some() {
            return changed || band_was_active;
        }
        changed
    }
}
