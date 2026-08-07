impl ShellScene {
    /// Builds only sampled icon draws from retained pane projections.
    /// Structural chrome, glyphs, and geometric fallbacks stay in their
    /// dedicated native streams, so this does not allocate discarded CPU
    /// quad vertices.
    pub(crate) fn push_native_frame_icons(
        &self,
        icons: &mut IconFrameBuilder<'_>,
        projections: &[ShellPaneProjection<'_>],
        size: PhysicalSize<u32>,
        text_midline_shift: f32,
    ) {
        self.push_native_places_icons(icons, size);
        for projection in projections {
            for visible in projection.visible_items.iter().copied() {
                let Some(item) = self.prepare_pane_item(projection, visible) else {
                    continue;
                };
                let Some(entry) = projection.view.entries.get(item.entry_index) else {
                    continue;
                };
                let pixmap_layout = ItemPixmapLayout {
                    view_mode: projection.view.view_mode,
                    icon_rect: item.icon_rect,
                    text_rect: item.text_rect,
                    text_midline_shift,
                };
                let folder_preview = self.folder_preview_role_for_pane_entry(
                    projection.view,
                    item.entry_index,
                    pixmap_layout,
                );
                let slot_pool = self.visible_slots.get(projection.geometry.kind);
                if let Some((path, retained_role)) = slot_pool.retained_visual_for_entry(entry) {
                    icons.push_thumbnail_or_icon_with_shared_path(
                        path,
                        entry,
                        retained_role,
                        folder_preview.as_ref(),
                        pixmap_layout,
                        item.content_clip,
                    );
                } else {
                    let path = Arc::from(entry_path_for_thumbnail(projection.view.path, entry));
                    icons.push_thumbnail_or_icon_with_shared_path(
                        &path,
                        entry,
                        None,
                        folder_preview.as_ref(),
                        pixmap_layout,
                        item.content_clip,
                    );
                }
            }
            self.queue_thumbnail_read_ahead_for_projection(projection, icons);
        }
    }

    fn push_native_places_icons(&self, icons: &mut IconFrameBuilder<'_>, size: PhysicalSize<u32>) {
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return;
        }
        let panel = self.places_panel_rect(size);
        let padding_x = self.scale_metric(PLACES_SIDEBAR_PADDING_X);
        let row_height = self.scale_metric(PLACES_ROW_HEIGHT);
        let row_gap = self.scale_metric(PLACES_ROW_GAP);
        let icon_size = self.scale_metric(PLACES_ICON_SIZE);
        let mut y = panel.y
            + self.scale_metric(PLACES_SIDEBAR_TOP_PADDING)
            + self.scale_metric(PLACES_TITLE_HEIGHT)
            - self.places_scroll_y;
        let mut previous_group = None;
        for place in &self.places {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                y += self.scale_metric(PLACES_SECTION_HEIGHT);
            }
            let row = ViewRect {
                x: panel.x + padding_x,
                y,
                width: (panel.width - padding_x * 2.0).max(1.0),
                height: row_height,
            };
            if row.y < panel.bottom() && row.bottom() > panel.y {
                let icon = ViewRect {
                    x: row.x + self.scale_metric(8.0),
                    y: row.y + (row.height - icon_size) / 2.0,
                    width: icon_size,
                    height: icon_size,
                };
                let icon_name = if self.trash_place_has_items(place) {
                    "user-trash-full"
                } else {
                    place.icon_name
                };
                icons.push_named_theme_icon(
                    icon_name,
                    NamedIconFallback::Service,
                    icon,
                    panel,
                    IconDrawLayer::Content,
                );
            }
            y += row_height + row_gap;
            previous_group = Some(place.group);
        }
    }
}

fn icon_path_for_entry(
    slots: &ShellPaneVisibleSlotPools,
    pane: ShellPaneId,
    directory: &Path,
    entry: &Entry,
) -> Arc<Path> {
    slots
        .get(pane)
        .retained_shared_path_for_entry(entry)
        .unwrap_or_else(|| Arc::from(entry_path_for_thumbnail(directory, entry)))
}
