impl ShellScene {

    fn drop_target_at_screen_point_for_drag(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        drag: &ShellInternalDrag,
    ) -> Option<ShellDropTarget> {
        if let Some(index) =
            self.place_gap_at_screen_point_for_drag_source(point, size, &drag.source)
        {
            let target = ShellDropTarget::PlacesGap { index };
            if self.local_drag_can_drop_on_target(&drag.source, &drag.paths, &target) {
                return Some(target);
            }
        }
        let target = self.drop_target_at_screen_point(point, size)?;
        self.local_drag_can_drop_on_target(&drag.source, &drag.paths, &target)
            .then_some(target)
    }

    fn local_drag_can_drop_on_target(
        &self,
        source: &ShellInternalDragSource,
        paths: &[PathBuf],
        target: &ShellDropTarget,
    ) -> bool {
        match target {
            ShellDropTarget::PlacesGap { index } => match source {
                ShellInternalDragSource::Place {
                    index: source_index,
                } => {
                    self.place_dnd_gap_index_is_valid(*index)
                        && self.place_participates_in_dnd(*source_index)
                        && *index != *source_index
                        && *index != source_index.saturating_add(1)
                }
                ShellInternalDragSource::PaneItem {
                    is_dir: true,
                    source_path,
                    ..
                } => !is_network_path(source_path),
                ShellInternalDragSource::PaneItem { is_dir: false, .. } => false,
            },
            ShellDropTarget::Place { index, .. } => {
                self.place_participates_in_dnd(*index)
                    && matches!(source, ShellInternalDragSource::PaneItem { .. })
            }
            ShellDropTarget::PaneItem { is_dir, path, .. } if *is_dir => {
                !paths.iter().any(|source| source == path)
            }
            ShellDropTarget::PaneBlank { path, .. } => {
                !paths.iter().any(|source| source == path)
            }
            ShellDropTarget::PaneItem { .. } | ShellDropTarget::PlacesBlank => false,
        }
    }

    fn drop_target_at_screen_point_for_external_drag(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        drag: &ShellExternalDrag,
    ) -> Option<ShellDropTarget> {
        if let Some(source) = drag.local_source.as_ref() {
            if let Some(index) =
                self.place_gap_at_screen_point_for_drag_source(point, size, source)
            {
                let target = ShellDropTarget::PlacesGap { index };
                if self.local_drag_can_drop_on_target(source, &drag.sources, &target) {
                    return Some(target);
                }
            }
            let target = self.drop_target_at_screen_point(point, size)?;
            return self
                .local_drag_can_drop_on_target(source, &drag.sources, &target)
                .then_some(target);
        }
        let target = self.drop_target_at_screen_point(point, size)?;
        self.external_drag_can_drop_on_target(&drag.sources, &target)
            .then_some(target)
    }

    fn external_drag_can_drop_on_target(
        &self,
        sources: &[PathBuf],
        target: &ShellDropTarget,
    ) -> bool {
        if sources.is_empty()
            || sources.iter().any(|path| is_network_path(path))
            || self
                .target_dir_for_drop_target(target)
                .is_some_and(|target_dir| is_network_path(&target_dir))
        {
            return false;
        }
        match target {
            ShellDropTarget::Place { index, path } => {
                self.place_participates_in_dnd(*index)
                    && !sources.iter().any(|source| source == path)
            }
            ShellDropTarget::PaneItem { is_dir, path, .. } if *is_dir => {
                !sources.iter().any(|source| source == path)
            }
            ShellDropTarget::PaneBlank { path, .. } => !sources.iter().any(|source| source == path),
            ShellDropTarget::PaneItem { .. }
            | ShellDropTarget::PlacesGap { .. }
            | ShellDropTarget::PlacesBlank => false,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn update_dnd_hover_target(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let next = self
            .internal_drag
            .as_ref()
            .and_then(|drag| self.drop_target_at_screen_point_for_drag(point, size, drag));
        let changed = self.dnd_hover_target != next;
        if changed {
            self.dnd_hover_target = next;
            self.dnd_hover_changes += 1;
        }
        changed
    }

    fn begin_data_transfer_drag(
        &mut self,
        sources: Vec<PathBuf>,
        local_source: Option<ShellInternalDragSource>,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        self.pointer = Some(point);
        self.internal_drag = None;
        self.place_press = None;
        self.rubber_band = None;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        let old_drag = self.external_drag.clone();
        self.external_drag = ShellExternalDrag::new(sources, local_source);
        let hover_changed = self.update_external_dnd_hover_target(point, size);
        old_drag != self.external_drag || hover_changed
    }

    fn update_external_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        self.update_external_dnd_hover_target(point, size)
    }

    fn external_drag_sources(&self) -> Option<Vec<PathBuf>> {
        self.external_drag
            .as_ref()
            .map(|drag| drag.sources.clone())
            .filter(|sources| !sources.is_empty())
    }

    fn active_internal_drag_paths(&self) -> Option<Vec<PathBuf>> {
        self.internal_drag
            .as_ref()
            .filter(|drag| drag.active)
            .map(|drag| drag.paths.clone())
            .filter(|paths| !paths.is_empty())
    }

    fn active_internal_drag_source(&self) -> Option<ShellInternalDragSource> {
        self.internal_drag
            .as_ref()
            .filter(|drag| drag.active)
            .map(|drag| drag.source.clone())
    }

    fn active_internal_drag_preview_source(
        &self,
        size: PhysicalSize<u32>,
    ) -> Option<ShellInternalDragPreviewSource> {
        let drag = self.internal_drag.as_ref().filter(|drag| drag.active)?;
        match &drag.source {
            ShellInternalDragSource::PaneItem { pane, index, .. } => {
                let pane_id = *pane;
                let pane = self.pane_view(pane_id)?;
                let entry = pane.entries.get(*index)?.clone();
                let preview_item_limit =
                    crate::ui::drag_preview_layout::multi_drag_preview_layout(
                        drag.paths.len(),
                        self.ui_scale(),
                    )
                    .item_count;
                let items = if pane.selection.contains(*index) {
                    pane.selection
                        .selected
                        .iter()
                        .copied()
                        .filter_map(|entry_index| {
                            Some(ShellInternalDragPreviewItem {
                                path: self.entry_path_for_pane_view(pane, entry_index)?,
                                entry: pane.entries.get(entry_index)?.clone(),
                            })
                        })
                        .take(preview_item_limit)
                        .collect::<Vec<_>>()
                } else {
                    self.entry_path_for_pane_view(pane, *index)
                        .map(|path| {
                            vec![ShellInternalDragPreviewItem {
                                path,
                                entry: entry.clone(),
                            }]
                        })
                        .unwrap_or_default()
                };
                let layout = crate::ui::drag_preview::drag_preview_layout_for_source(
                    self,
                    &drag.source,
                    &drag.label,
                    drag.start,
                    size,
                )?;
                let pixmap_layout = ItemPixmapLayout {
                    view_mode: pane.view_mode,
                    icon_rect: layout.icon,
                    text_rect: layout.label.map(|label| label.rect).unwrap_or(layout.icon),
                    text_midline_shift: 0.0,
                };
                let folder_preview = self.folder_preview_role_for_pane_entry(
                    pane,
                    *index,
                    pixmap_layout,
                );
                Some(ShellInternalDragPreviewSource::PaneItem {
                    directory: pane.path.to_path_buf(),
                    entry,
                    items,
                    label: drag.label.clone(),
                    view_mode: pane.view_mode,
                    layout,
                    folder_preview,
                })
            }
            ShellInternalDragSource::Place { index } => {
                let place = self.places.get(*index)?;
                let icon_name = if self.trash_place_has_items(place) {
                    "user-trash-full"
                } else {
                    place.icon_name
                };
                Some(ShellInternalDragPreviewSource::Place {
                    label: drag.label.clone(),
                    icon_name: icon_name.to_string(),
                    layout: crate::ui::drag_preview::drag_preview_layout_for_source(
                        self,
                        &drag.source,
                        &drag.label,
                        drag.start,
                        size,
                    )?,
                })
            }
        }
    }

    fn drag_preview_icon_size_for_pane_item(
        &self,
        pane: ShellPaneView<'_>,
        item_layout: Option<ItemLayout>,
    ) -> f32 {
        item_layout
            .map(|layout| layout.icon_rect.width.max(layout.icon_rect.height))
            .unwrap_or_else(|| match pane.view_mode {
                ShellViewMode::Icons => {
                    self.zoom_icon_metric_for_step(pane.zoom_step, ICONS_ICON_SIZE, 16.0, 256.0)
                }
                ShellViewMode::Compact => {
                    self.zoom_icon_metric_for_step(pane.zoom_step, COMPACT_ICON_SIZE, 16.0, 144.0)
                }
                ShellViewMode::Details => self.details_icon_size_for_step(pane.zoom_step),
            })
    }

    fn internal_drag_active(&self) -> bool {
        self.internal_drag.as_ref().is_some_and(|drag| drag.active)
    }

    fn clear_internal_drag(&mut self) -> bool {
        let changed = self.internal_drag.take().is_some() || self.clear_dnd_hover_target();
        if changed {
            fika_log!("[fika] internal-dnd clear=1");
        }
        changed
    }

    fn clear_external_drag(&mut self) -> bool {
        let changed = self.external_drag.take().is_some() || self.clear_dnd_hover_target();
        if changed {
            fika_log!("[fika] external-dnd clear=1");
        }
        changed
    }

    fn finish_external_drag(
        &mut self,
        sources: Vec<PathBuf>,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        self.pointer = Some(point);
        let sources = normalized_external_drop_sources(sources);
        let local_source = self
            .external_drag
            .as_ref()
            .and_then(|drag| drag.local_source.clone());
        let drag_cleared = self.external_drag.take().is_some();
        let Some(drag) = ShellExternalDrag::new(sources, local_source) else {
            let hover_cleared = self.clear_dnd_hover_target();
            return Ok(drag_cleared || hover_cleared);
        };
        let Some(target) = self.drop_target_at_screen_point_for_external_drag(point, size, &drag)
        else {
            let hover_cleared = self.clear_dnd_hover_target();
            return Ok(drag_cleared || hover_cleared);
        };
        if let (
            Some(source),
            ShellDropTarget::PlacesGap { index },
        ) = (drag.local_source, &target)
        {
            let changed = self.drop_local_drag_to_places_gap(
                source,
                *index,
                &default_user_places_path(),
                size,
            )?;
            let hover_cleared = self.clear_dnd_hover_target();
            return Ok(drag_cleared || changed || hover_cleared);
        }
        let Some(target_dir) = self.target_dir_for_drop_target(&target) else {
            let hover_cleared = self.clear_dnd_hover_target();
            return Ok(drag_cleared || hover_cleared);
        };
        let old_menu = self.drop_menu.clone();
        self.drop_menu = Some(ShellDropMenu::new(
            drag.sources,
            target_dir,
            target,
            point,
        ));
        self.context_menu = None;
        self.context_target = None;
        self.rubber_band = None;
        self.internal_drag = None;
        self.place_press = None;
        let _ = self.clear_dnd_hover_target();
        let changed = drag_cleared || old_menu != self.drop_menu;
        if changed {
            fika_log!(
                "[fika] external-dnd-menu open=1 sources={} target={}",
                self.drop_menu
                    .as_ref()
                    .map(|menu| menu.sources.len())
                    .unwrap_or(0),
                self.drop_menu
                    .as_ref()
                    .map(|menu| menu.target.kind())
                    .unwrap_or("none")
            );
        }
        Ok(changed)
    }

    fn update_external_dnd_hover_target(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        let next = self.external_drag.as_ref().and_then(|drag| {
            self.drop_target_at_screen_point_for_external_drag(point, size, drag)
        });
        let changed = self.dnd_hover_target != next;
        if changed {
            self.dnd_hover_target = next;
            self.dnd_hover_changes += 1;
        }
        changed
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn clear_dnd_hover_target(&mut self) -> bool {
        let changed = self.dnd_hover_target.take().is_some();
        if changed {
            self.dnd_hover_changes += 1;
        }
        changed
    }

    fn pane_drag_paths_for_index(&self, pane_id: ShellPaneId, index: usize) -> Vec<PathBuf> {
        let Some(pane) = self.pane_view(pane_id) else {
            return Vec::new();
        };
        if pane.selection.contains(index) {
            let paths = self
                .pane_selection(pane_id)
                .into_iter()
                .flat_map(|selection| selection.selected.iter())
                .copied()
                .filter_map(|index| self.entry_path_for_pane_view(pane, index))
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                return paths;
            }
        }
        self.entry_path_for_pane_view(pane, index)
            .into_iter()
            .collect()
    }

    fn pane_drag_source_for_index(
        &self,
        pane_id: ShellPaneId,
        index: usize,
    ) -> Option<(PathBuf, bool, String)> {
        let pane = self.pane_view(pane_id)?;
        let entry = pane.entries.get(index)?;
        let path = self.entry_path_for_pane_view(pane, index)?;
        Some((path, entry.is_dir, entry.name.as_ref().to_string()))
    }

    fn drag_label_for_paths(paths: &[PathBuf], fallback: String) -> String {
        if paths.len() > 1 {
            return format!("{} items", paths.len());
        }
        paths
            .first()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or(fallback)
    }

    fn begin_internal_drag_for_pane_item(
        &mut self,
        pane: ShellPaneId,
        index: usize,
        point: ViewPoint,
    ) -> bool {
        let Some((source_path, is_dir, fallback_label)) =
            self.pane_drag_source_for_index(pane, index)
        else {
            self.internal_drag = None;
            self.external_drag = None;
            return false;
        };
        let paths = self.pane_drag_paths_for_index(pane, index);
        if paths.is_empty() {
            self.internal_drag = None;
            self.external_drag = None;
            return false;
        }
        let label = Self::drag_label_for_paths(&paths, fallback_label);
        self.external_drag = None;
        self.internal_drag = Some(ShellInternalDrag::new(
            ShellInternalDragSource::PaneItem {
                pane,
                index,
                source_path,
                is_dir,
            },
            paths,
            label,
            point,
        ));
        true
    }

    fn begin_internal_drag_for_place(&mut self, index: usize, point: ViewPoint) -> bool {
        if !self.place_participates_in_dnd(index) {
            self.internal_drag = None;
            self.external_drag = None;
            return false;
        }
        let Some(place) = self.places.get(index) else {
            self.internal_drag = None;
            self.external_drag = None;
            return false;
        };
        self.external_drag = None;
        self.internal_drag = Some(ShellInternalDrag::new(
            ShellInternalDragSource::Place { index },
            vec![place.path.clone()],
            place.label.clone(),
            point,
        ));
        true
    }

    fn update_internal_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let activation_changed = {
            let Some(drag) = self.internal_drag.as_mut() else {
                return false;
            };
            // Track pointer for the activation threshold and drop hit tests.
            // The compositor paints the Wayland DnD icon, so pointer motion
            // after activation must not dirty the window by itself.
            let was_active = drag.active;
            let _ = drag.update(point);
            drag.active != was_active
        };
        if !self.internal_drag.as_ref().is_some_and(|drag| drag.active) {
            return activation_changed;
        }
        let hover_cleared = if self
            .internal_drag
            .as_ref()
            .and_then(ShellInternalDrag::source_place_index)
            .is_some()
        {
            let place_hover_cleared = if self.hovered_place.is_some() {
                self.set_hovered_place(None)
            } else {
                false
            };
            let item_hover_cleared = if self.hovered_item.is_some() {
                self.set_hovered_item(None)
            } else {
                false
            };
            place_hover_cleared || item_hover_cleared
        } else {
            false
        };
        let hover_changed = self.update_dnd_hover_target(point, size);
        activation_changed || hover_cleared || hover_changed
    }

    fn finish_internal_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let user_places_path = default_user_places_path();
        match self.finish_internal_drag_with_user_places_path(point, size, &user_places_path) {
            Ok(changed) => changed,
            Err(error) => {
                fika_log!("[fika] dnd-error {error}");
                false
            }
        }
    }

    fn finish_internal_drag_with_user_places_path(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        user_places_path: &Path,
    ) -> Result<bool, String> {
        let Some(drag) = self.internal_drag.take() else {
            return Ok(false);
        };
        if !drag.active {
            let _ = self.clear_dnd_hover_target();
            return Ok(false);
        }
        let Some(target) = self.drop_target_at_screen_point_for_drag(point, size, &drag) else {
            let _ = self.clear_dnd_hover_target();
            return Ok(false);
        };
        if let ShellDropTarget::PlacesGap { index } = target {
            let changed =
                self.drop_local_drag_to_places_gap(drag.source, index, user_places_path, size)?;
            let _ = self.clear_dnd_hover_target();
            return Ok(changed);
        }
        let Some(target_dir) = self.target_dir_for_drop_target(&target) else {
            let _ = self.clear_dnd_hover_target();
            return Ok(false);
        };
        if drag.paths.iter().any(|source| source == &target_dir) {
            let _ = self.clear_dnd_hover_target();
            return Ok(false);
        }
        let old_menu = self.drop_menu.clone();
        self.drop_menu = Some(ShellDropMenu::new(drag.paths, target_dir, target, point));
        self.context_menu = None;
        self.context_target = None;
        self.rubber_band = None;
        let _ = self.clear_dnd_hover_target();
        let changed = old_menu != self.drop_menu;
        if changed {
            fika_log!(
                "[fika] dnd-menu open=1 sources={} target={}",
                self.drop_menu
                    .as_ref()
                    .map(|menu| menu.sources.len())
                    .unwrap_or(0),
                self.drop_menu
                    .as_ref()
                    .map(|menu| menu.target.kind())
                    .unwrap_or("none")
            );
        }
        Ok(changed)
    }

    fn drop_local_drag_to_places_gap(
        &mut self,
        source: ShellInternalDragSource,
        index: usize,
        user_places_path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        match source {
            ShellInternalDragSource::Place {
                index: source_index,
            } => self.move_place_to_gap(source_index, index, user_places_path, size),
            ShellInternalDragSource::PaneItem {
                source_path,
                is_dir: true,
                ..
            } => self.add_pane_folder_to_places_gap(source_path, index, user_places_path, size),
            ShellInternalDragSource::PaneItem { .. } => Ok(false),
        }
    }

    fn move_place_to_gap(
        &mut self,
        source_index: usize,
        gap_index: usize,
        user_places_path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        if source_index >= self.places.len()
            || gap_index > self.places.len()
            || !self.place_participates_in_dnd(source_index)
            || !self.place_dnd_gap_index_is_valid(gap_index)
            || gap_index == source_index
            || gap_index == source_index.saturating_add(1)
        {
            return Ok(false);
        }
        let place = self.places.remove(source_index);
        let insert_index = if gap_index > source_index {
            gap_index.saturating_sub(1)
        } else {
            gap_index
        }
        .min(self.places.len());
        let path = place.path.clone();
        let label = place.label.clone();
        self.places.insert(insert_index, place);
        save_shell_place_order(user_places_path, &self.places)?;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.rubber_band = None;
        self.clamp_places_scroll(size);
        self.places_changes += 1;
        self.refresh_hover(size);
        fika_log!(
            "[fika] places-reorder label={:?} path={} from={} gap={} to={} changes={}",
            label,
            path.display(),
            source_index,
            gap_index,
            insert_index,
            self.places_changes
        );
        Ok(true)
    }
}
