impl ShellScene {

    fn update_rubber_band(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let pane_id = self.active_pane();
        let Some(projection) = self.pane_projection(pane_id, size) else {
            return self.refresh_hover(size);
        };
        let current = clamped_screen_to_content_point(
            point,
            ViewPoint {
                x: projection.view.scroll_x,
                y: projection.view.scroll_y,
            },
            projection.geometry.content,
        );
        let Some((active_rect, mode, base_selection, rect_changed)) = ({
            let Some(band) = self.rubber_band.as_mut() else {
                return self.refresh_hover(size);
            };
            let old_active_rect = band.active_rect();
            band.update(current);
            let active_rect = band
                .active_rect()
                .filter(|rect| rect.width > 0.0 && rect.height > 0.0);
            Some((
                active_rect,
                band.mode,
                band.base_selection.clone(),
                old_active_rect != active_rect,
            ))
        }) else {
            return self.refresh_hover(size);
        };

        let hover_changed = self.refresh_hover(size);
        let Some(rect) = active_rect else {
            return hover_changed || rect_changed;
        };

        let indexes = self.rubber_band_indexes_for_pane(pane_id, rect, size);
        let selection_changed = self
            .pane_selection_mut(pane_id)
            .is_some_and(|selection| selection.apply_rubber_band(&base_selection, &indexes, mode));
        if selection_changed {
            self.selection_changes += 1;
        }
        self.rubber_band_updates += 1;
        hover_changed || rect_changed || selection_changed
    }

    fn rubber_band_indexes_for_pane(
        &self,
        pane_id: ShellPaneId,
        rect: ViewRect,
        size: PhysicalSize<u32>,
    ) -> Vec<usize> {
        let Some(projection) = self.pane_projection(pane_id, size) else {
            return Vec::new();
        };
        let layout = self.pane_layout_for_pane(
            pane_id,
            projection.view,
            projection.geometry.content.width,
            projection.geometry.content.height,
        );
        layout
            .indexes_intersecting(rect)
            .iter()
            .filter_map(|layout_index| {
                layout
                    .item(*layout_index)
                    .is_some_and(|item| item.visual_rect.intersects(rect))
                    .then(|| projection.view.filtered_indexes.get(*layout_index).copied())
                    .flatten()
            })
            .collect()
    }

    fn navigate(
        &mut self,
        action: NavigationAction,
        extend: bool,
        size: PhysicalSize<u32>,
    ) -> bool {
        let pane_id = self.active_pane();
        let Some(projection) = self.pane_projection(pane_id, size) else {
            return false;
        };
        if projection.view.filtered_entry_count() == 0 {
            return false;
        }

        let old_scroll = self.pane_scroll_offset(pane_id).unwrap_or((0.0, 0.0));
        let old_hovered = self.hovered_item;
        let old_hovered_place = self.hovered_place;
        let current = projection
            .view
            .selection
            .focus_or_first_selected()
            .and_then(|index| projection.view.filtered_indexes.binary_search(&index).ok())
            .unwrap_or(0);
        let layout = self.pane_layout_for_pane(
            pane_id,
            projection.view,
            projection.geometry.content.width,
            projection.geometry.content.height,
        );
        let Some(target_layout_index) = navigation_target(
            action,
            current,
            projection.view.filtered_entry_count(),
            &layout,
        ) else {
            return false;
        };
        let Some(target) = projection
            .view
            .filtered_indexes
            .get(target_layout_index)
            .copied()
        else {
            return false;
        };

        let selection_changed = self
            .pane_selection_mut(pane_id)
            .is_some_and(|selection| selection.apply_navigation(target, extend));
        if selection_changed {
            self.selection_changes += 1;
        }
        self.keyboard_navigation += 1;
        self.ensure_index_visible_in_pane(pane_id, target, size);
        let next_hovered_place = self
            .pointer
            .and_then(|point| self.place_index_at_screen_point(point, size));
        let next_hovered_item = self
            .pointer
            .filter(|_| next_hovered_place.is_none())
            .and_then(|point| self.pane_item_at_screen_point(point, size));
        let hover_changed =
            self.hovered_item != next_hovered_item || self.hovered_place != next_hovered_place;
        self.hovered_place = next_hovered_place;
        self.hovered_item = next_hovered_item;
        if hover_changed {
            self.start_hover_animation();
        }
        let new_scroll = self.pane_scroll_offset(pane_id).unwrap_or((0.0, 0.0));

        selection_changed
            || (new_scroll.0 - old_scroll.0).abs() > f32::EPSILON
            || (new_scroll.1 - old_scroll.1).abs() > f32::EPSILON
            || self.hovered_item != old_hovered
            || self.hovered_place != old_hovered_place
    }

    fn refresh_hover(&mut self, size: PhysicalSize<u32>) -> bool {
        let place_hit = self
            .pointer
            .and_then(|point| self.place_index_at_screen_point(point, size));
        let item_hit = if place_hit.is_none() {
            self.pointer
                .and_then(|point| self.pane_item_at_screen_point(point, size))
        } else {
            None
        };
        self.hit_tests += 1;
        let changed = self.hovered_place != place_hit || self.hovered_item != item_hit;
        self.hovered_place = place_hit;
        self.hovered_item = item_hit;
        if changed {
            self.start_hover_animation();
        }
        changed
    }

    fn set_hovered_item(&mut self, hovered_item: Option<ShellPaneItemTarget>) -> bool {
        self.hit_tests += 1;
        let changed = self.hovered_item != hovered_item;
        self.hovered_item = hovered_item;
        if changed {
            self.start_hover_animation();
        }
        changed
    }

    fn set_hovered_place(&mut self, hovered_place: Option<usize>) -> bool {
        self.hit_tests += 1;
        let changed = self.hovered_place != hovered_place;
        self.hovered_place = hovered_place;
        if changed {
            self.start_hover_animation();
        }
        changed
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn hit_test_screen_point(&self, point: ViewPoint, size: PhysicalSize<u32>) -> Option<usize> {
        let pane = self.pane_view(ShellPaneId::SLOT_0)?;
        let geometry = self.pane_geometry(ShellPaneId::SLOT_0, size)?;
        self.pane_hit_test_screen_point(pane, geometry, point)
    }

    fn pane_item_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPaneItemTarget> {
        for geometry in self.pane_geometries(size) {
            if !geometry.content.contains(point) {
                continue;
            }
            let pane = self.pane_view(geometry.kind)?;
            let index = self.pane_hit_test_screen_point(pane, geometry, point)?;
            return Some(ShellPaneItemTarget {
                pane: geometry.kind,
                index,
            });
        }
        None
    }

    fn pane_hit_test_screen_point(
        &self,
        pane: ShellPaneView<'_>,
        geometry: ShellPaneGeometry,
        point: ViewPoint,
    ) -> Option<usize> {
        if !geometry.content.contains(point) {
            return None;
        }
        let content_point = screen_to_content_point(
            point,
            ViewPoint {
                x: pane.scroll_x,
                y: pane.scroll_y,
            },
            geometry.content,
        )?;
        let layout = self.pane_layout_for_pane(
            geometry.kind,
            pane,
            geometry.content.width,
            geometry.content.height,
        );
        let layout_index = layout.hit_test_content_point(content_point)?;
        let item = layout.item(layout_index)?;
        item.visual_rect
            .contains(content_point)
            .then(|| pane.filtered_indexes.get(layout_index).copied())
            .flatten()
    }

    fn pane_context_hit_test_screen_point(
        &self,
        pane: ShellPaneView<'_>,
        geometry: ShellPaneGeometry,
        point: ViewPoint,
    ) -> Option<usize> {
        if pane.selection.len() > 1 {
            self.pane_selection_core_hit_test_screen_point(pane, geometry, point)
        } else {
            self.pane_hit_test_screen_point(pane, geometry, point)
        }
    }

    fn pane_selection_core_hit_test_screen_point(
        &self,
        pane: ShellPaneView<'_>,
        geometry: ShellPaneGeometry,
        point: ViewPoint,
    ) -> Option<usize> {
        if !geometry.content.contains(point) {
            return None;
        }
        let content_point = screen_to_content_point(
            point,
            ViewPoint {
                x: pane.scroll_x,
                y: pane.scroll_y,
            },
            geometry.content,
        )?;
        let layout = self.pane_layout_for_pane(
            geometry.kind,
            pane,
            geometry.content.width,
            geometry.content.height,
        );
        let layout_index = layout.hit_test_content_point(content_point)?;
        let item = layout.item(layout_index)?;
        let entry_index = pane.filtered_indexes.get(layout_index).copied()?;
        let selected = pane.selection.contains(entry_index);
        file_manager_selection_core_rect(
            pane.view_mode,
            item.item_rect,
            item.visual_rect,
            item.icon_rect,
            item.text_rect,
            selected,
        )
        .contains(content_point)
        .then_some(entry_index)
    }

    fn pane_drop_hit_test_screen_point(
        &self,
        pane: ShellPaneView<'_>,
        geometry: ShellPaneGeometry,
        point: ViewPoint,
    ) -> Option<usize> {
        self.pane_selection_core_hit_test_screen_point(pane, geometry, point)
    }

    fn ensure_index_visible_in_pane(
        &mut self,
        pane_id: ShellPaneId,
        index: usize,
        size: PhysicalSize<u32>,
    ) {
        let Some(projection) = self.pane_projection(pane_id, size) else {
            return;
        };
        let Some(layout_index) = projection.view.filtered_indexes.binary_search(&index).ok() else {
            return;
        };
        let layout = self.pane_layout_for_pane(
            pane_id,
            projection.view,
            projection.geometry.content.width,
            projection.geometry.content.height,
        );
        let Some(item) = layout.item(layout_index) else {
            return;
        };
        let padding = 8.0;
        let mut next_scroll = self.pane_scroll_offset(pane_id).unwrap_or((0.0, 0.0));
        match projection.view.view_mode {
            ShellViewMode::Compact => {
                if item.visual_rect.x < projection.view.scroll_x + padding {
                    next_scroll.0 = (item.visual_rect.x - padding).max(0.0);
                } else if item.visual_rect.right()
                    > projection.view.scroll_x + projection.geometry.content.width - padding
                {
                    next_scroll.0 =
                        item.visual_rect.right() - projection.geometry.content.width + padding;
                }
                next_scroll.1 = 0.0;
            }
            ShellViewMode::Icons | ShellViewMode::Details => {
                if item.visual_rect.y < projection.view.scroll_y + padding {
                    next_scroll.1 = (item.visual_rect.y - padding).max(0.0);
                } else if item.visual_rect.bottom()
                    > projection.view.scroll_y + projection.geometry.content.height - padding
                {
                    next_scroll.1 =
                        item.visual_rect.bottom() - projection.geometry.content.height + padding;
                }
                next_scroll.0 = 0.0;
            }
        }
        self.set_pane_scroll_offset(pane_id, next_scroll.0, next_scroll.1);
        self.clamp_scroll(size);
    }
}
