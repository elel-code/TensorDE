impl ShellScene {

    fn pane_projection_from_prepared(
        &self,
        prepared: ShellPreparedPaneProjection,
    ) -> Option<ShellPaneProjection<'_>> {
        let view = self.pane_view(prepared.geometry.kind)?;
        let slots = self.visible_slots.get(prepared.geometry.kind);
        let visible_items = prepared
            .visible_items
            .into_iter()
            .map(|item| {
                let slot_id = if item.slot_id != 0 {
                    item.slot_id
                } else {
                    item.path
                        .as_deref()
                        .and_then(|path| slots.slot_for_path(path))
                        .unwrap_or_default()
                };
                ShellPaneVisibleItem {
                    layout: item.layout,
                    slot_id,
                }
            })
            .collect();
        Some(ShellPaneProjection {
            view,
            geometry: prepared.geometry,
            visible_items,
            scroll_metrics: prepared.scroll_metrics,
        })
    }

    fn pane_projections_from_layouts(
        &self,
        layouts: ShellPreparedFrameProjectionLayouts,
    ) -> SceneFrameProjections<'_> {
        let projections = layouts
            .layouts
            .into_iter()
            .filter_map(|prepared| self.pane_projection_from_prepared(prepared))
            .collect();
        SceneFrameProjections::new(projections, layouts.layout_us)
    }

    fn update_visible_slot_pools_for_projection_layouts(
        &mut self,
        layouts: &mut ShellPreparedFrameProjectionLayouts,
    ) -> ShellVisibleItemSlotStats {
        let mut stats = ShellVisibleItemSlotStats::default();
        let mut prepared_panes = [false; 2];
        for prepared in &mut layouts.layouts {
            let kind = prepared.geometry.kind;
            prepared_panes[kind.index()] = true;
            let pool = self.visible_slots.get_mut(kind);
            let pane_stats = pool.update_visible_item_slots(&mut prepared.visible_items);
            stats = stats.merged(pane_stats);
        }
        for kind in ShellPaneId::ALL {
            if !prepared_panes[kind.index()] {
                self.visible_slots.clear(kind);
            }
        }
        self.visible_slot_stats = stats;
        stats
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn update_visible_slot_pools(&mut self, size: PhysicalSize<u32>) -> ShellVisibleItemSlotStats {
        let mut layouts = self.prepare_frame_projection_layouts(size);
        self.update_visible_slot_pools_for_projection_layouts(&mut layouts)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn layout(&self, size: PhysicalSize<u32>) -> ShellLayout {
        self.pane_layout(
            self.pane_view(ShellPaneId::SLOT_0)
                .expect("pane slot 0 is open"),
            self.content_width(size),
            self.viewport_height(size),
        )
    }

    fn pane_layout(
        &self,
        pane: ShellPaneView<'_>,
        content_width: f32,
        viewport_height: f32,
    ) -> ShellLayout {
        self.pane_layout_for_pane(ShellPaneId::SLOT_0, pane, content_width, viewport_height)
    }

    fn pane_layout_for_pane(
        &self,
        pane_id: ShellPaneId,
        pane: ShellPaneView<'_>,
        content_width: f32,
        viewport_height: f32,
    ) -> ShellLayout {
        let item_count = pane.filtered_entry_count();
        match pane.view_mode {
            ShellViewMode::Icons => {
                let mut options =
                    self.icons_options_for_viewport(content_width, viewport_height, pane.zoom_step);
                options.scroll_x = pane.scroll_x;
                options.scroll_y = pane.scroll_y;
                ShellLayout::Icons(self.pane_icons_layout(pane_id, pane, options))
            }
            ShellViewMode::Compact => {
                let mut options = self.compact_options_for_viewport(
                    content_width,
                    viewport_height,
                    pane.zoom_step,
                );
                options.scroll_x = pane.scroll_x;
                ShellLayout::Compact(self.pane_compact_layout(pane_id, pane, options))
            }
            ShellViewMode::Details => ShellLayout::Details(DetailsLayout::new(
                item_count,
                content_width,
                viewport_height,
                pane.scroll_y,
                self.details_row_height_for_step(pane.zoom_step),
                self.details_icon_size_for_step(pane.zoom_step),
                self.ui_scale(),
                self.details_name_width(),
                self.details_size_width(),
                self.details_modified_width(),
                self.text_line_height(),
            )),
        }
    }

    fn pane_compact_layout(
        &self,
        pane_id: ShellPaneId,
        pane: ShellPaneView<'_>,
        options: CompactLayoutOptions,
    ) -> ShellCompactLayout {
        let item_count = pane.filtered_entry_count();
        let rows_per_column = CompactLayout::rows_per_column_for_options(options);
        let cache_key = CompactLayoutCacheKey {
            pane: pane_id.index(),
            item_count,
            rows_per_column,
            item_width: options.item_width.to_bits(),
            item_height: options.item_height.to_bits(),
            padding: options.padding.to_bits(),
            icon_size: options.icon_size.to_bits(),
            text_gap: options.text_gap.to_bits(),
            text_scale: self.ui_scale().to_bits(),
        };
        if let Some(cached) = self.compact_layout_cache.get(&cache_key) {
            let layout =
                CompactLayout::new_with_column_widths(item_count, options, cached.column_widths);
            return ShellCompactLayout::new(layout, cached.text_widths);
        }

        let column_count = item_count.div_ceil(rows_per_column);
        let mut text_widths = Vec::with_capacity(item_count);
        let mut column_widths = vec![options.item_width; column_count];
        let font_size = (TEXT_FONT_SIZE * self.text_line_height() / TEXT_LINE_HEIGHT).max(1.0);
        let line_height = self.text_line_height();
        let mut text_runtime = self.text_hit_tests.borrow_mut();
        for layout_index in 0..item_count {
            let Some(entry_index) = pane.filtered_indexes.get(layout_index).copied() else {
                text_widths.push(0.0);
                continue;
            };
            let Some(entry) = pane.entries.get(entry_index) else {
                text_widths.push(0.0);
                continue;
            };
            let text_width =
                text_runtime.no_wrap_width(entry.name.as_ref(), font_size, line_height);
            text_widths.push(text_width);
            let column = layout_index / rows_per_column;
            if let Some(width) = column_widths.get_mut(column) {
                *width = width.max(required_compact_item_width(options, text_width));
            }
        }
        let text_widths = Arc::<[f32]>::from(text_widths);
        let column_widths = Arc::<[f32]>::from(column_widths);
        self.compact_layout_cache.insert(
            cache_key,
            CompactLayoutCacheValue {
                text_widths: Arc::clone(&text_widths),
                column_widths: Arc::clone(&column_widths),
            },
        );
        let layout = CompactLayout::new_with_column_widths(item_count, options, column_widths);
        ShellCompactLayout::new(layout, text_widths)
    }

    fn pane_icons_layout(
        &self,
        pane_id: ShellPaneId,
        pane: ShellPaneView<'_>,
        options: IconsLayoutOptions,
    ) -> IconsLayout {
        let item_count = pane.filtered_entry_count();
        if item_count == 0 {
            return IconsLayout::new(0, options);
        }

        let cache_key = IconsLayoutHeightCacheKey {
            pane: pane_id.index(),
            item_count,
            item_width: options.item_width.to_bits(),
            item_height: options.item_height.to_bits(),
            padding: options.padding.to_bits(),
            icon_size: options.icon_size.to_bits(),
            text_height: options.text_height.to_bits(),
            text_scale: self.ui_scale().to_bits(),
        };
        if let Some(cached) = self.icons_layout_height_cache.get(&cache_key) {
            return IconsLayout::new_with_item_heights(item_count, options, cached.item_heights);
        }

        let available_text_width = (options.item_width - options.padding * 2.0).max(1.0);
        let font_size = (TEXT_FONT_SIZE * self.text_line_height() / TEXT_LINE_HEIGHT).max(1.0);
        let line_height = self.text_line_height();
        let mut text_runtime = self.text_hit_tests.borrow_mut();
        let item_heights = pane
            .filtered_indexes
            .iter()
            .take(item_count)
            .map(|entry_index| {
                pane.entries
                    .get(*entry_index)
                    .map(|entry| {
                        let lines = text_runtime.icons_filename_line_count(
                            entry.name.as_ref(),
                            available_text_width,
                            FILE_MANAGER_ICONS_MAX_TEXT_LINES,
                            font_size,
                            line_height,
                        );
                        (options.padding * 3.0
                            + options.icon_size
                            + options.text_height * lines as f32)
                            .round()
                    })
                    .unwrap_or(options.item_height)
            })
            .collect::<Vec<_>>();
        let item_heights = Arc::<[f32]>::from(item_heights);
        self.icons_layout_height_cache.insert(
            cache_key,
            IconsLayoutHeightCacheValue {
                item_heights: Arc::clone(&item_heights),
            },
        );
        IconsLayout::new_with_item_heights(item_count, options, item_heights)
    }

    #[cfg(test)]
    fn icons_options(&self, size: PhysicalSize<u32>) -> IconsLayoutOptions {
        let mut options = self.icons_options_for_viewport(
            self.content_width(size),
            self.viewport_height(size),
            self.panes[ShellPaneId::SLOT_0].zoom_step,
        );
        options.scroll_x = self.panes[ShellPaneId::SLOT_0].scroll_x;
        options.scroll_y = self.panes[ShellPaneId::SLOT_0].scroll_y;
        options
    }

    fn icons_options_for_viewport(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        zoom_step: i32,
    ) -> IconsLayoutOptions {
        let scale = self.ui_scale();
        let padding = self.scale_metric(2.0);
        let gap = self.scale_metric(12.0);
        let icon_size = self.zoom_icon_metric_for_step(zoom_step, ICONS_ICON_SIZE, 16.0, 256.0);
        let average_char_width = 9.0 * scale;
        let item_width = file_manager_icons_item_width(
            icon_size,
            padding,
            FILE_MANAGER_ICONS_TEXT_WIDTH_INDEX,
            average_char_width,
            scale,
            self.file_manager_zoom_level_for_step(zoom_step),
        );
        let item_height = (padding * 3.0 + icon_size + self.text_line_height()).round();
        IconsLayoutOptions {
            viewport_width,
            viewport_height,
            reserved_bottom: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            padding,
            gap,
            item_width,
            item_height,
            icon_size,
            text_height: self.text_line_height(),
        }
    }

    #[cfg(test)]
    fn compact_options(&self, size: PhysicalSize<u32>) -> CompactLayoutOptions {
        let mut options = self.compact_options_for_viewport(
            self.content_width(size),
            self.viewport_height(size),
            self.panes[ShellPaneId::SLOT_0].zoom_step,
        );
        options.scroll_x = self.panes[ShellPaneId::SLOT_0].scroll_x;
        options
    }

    fn compact_options_for_viewport(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        zoom_step: i32,
    ) -> CompactLayoutOptions {
        let padding = self.scale_metric(2.0);
        let side_padding = self.scale_metric(8.0);
        let gap = self.scale_metric(8.0);
        let text_gap = padding * 2.0;
        let icon_size = self.zoom_icon_metric_for_step(zoom_step, COMPACT_ICON_SIZE, 16.0, 144.0);
        let min_text_width =
            (self.text_line_height() * 5.0).max(self.scale_metric(COMPACT_MIN_TEXT_WIDTH));
        let item_height = (padding * 2.0 + icon_size.max(self.text_line_height())).round();
        CompactLayoutOptions {
            viewport_width,
            viewport_height,
            reserved_bottom: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            padding,
            side_padding,
            gap,
            text_gap,
            item_width: (padding * 4.0 + icon_size + min_text_width).round(),
            item_height,
            icon_size,
            text_height: self.text_line_height(),
        }
    }

    fn zoom_percent(&self) -> i32 {
        self.zoom_percent_for_pane(self.active_pane())
    }

    fn zoom_percent_for_pane(&self, pane: ShellPaneId) -> i32 {
        self.pane_zoom_step(pane)
            .map(|zoom_step| self.zoom_percent_for_step(zoom_step))
            .unwrap_or(100)
    }

    fn zoom_percent_for_step(&self, zoom_step: i32) -> i32 {
        (self.zoom_icon_factor_for_step(zoom_step) * 100.0).round() as i32
    }

    fn zoom_fraction_for_pane(&self, pane: ShellPaneId) -> f32 {
        self.pane_zoom_step(pane)
            .map(|zoom_step| self.zoom_fraction_for_step(zoom_step))
            .unwrap_or_else(|| self.zoom_fraction_for_step(0))
    }

    fn zoom_fraction_for_step(&self, zoom_step: i32) -> f32 {
        let level = self.file_manager_zoom_level_for_step(zoom_step);
        let span = (FILE_MANAGER_ZOOM_LEVEL_MAX - FILE_MANAGER_ZOOM_LEVEL_MIN).max(1) as f32;
        ((level - FILE_MANAGER_ZOOM_LEVEL_MIN) as f32 / span).clamp(0.0, 1.0)
    }

    fn details_row_height_for_step(&self, zoom_step: i32) -> f32 {
        let padding = self.scale_metric(4.0);
        (padding * 2.0
            + self
                .details_icon_size_for_step(zoom_step)
                .max(self.text_line_height()))
        .round()
    }

    fn details_icon_size_for_step(&self, zoom_step: i32) -> f32 {
        self.zoom_icon_metric_for_step(zoom_step, DETAILS_ICON_SIZE, 16.0, 144.0)
    }

    fn build_frame(
        &self,
        size: PhysicalSize<u32>,
        projections: &[ShellPaneProjection<'_>],
        projection_layout_us: u128,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        overlay_text: Option<&mut TextFrameBuilder<'_>>,
    ) -> SceneFrame {
        let layout_start = Instant::now();
        let mut vertices = Vec::with_capacity(64);
        let mut overlay_vertices = Vec::with_capacity(32);
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let slot0_projection = projections
            .iter()
            .find(|projection| projection.geometry.kind == ShellPaneId::SLOT_0)
            .expect("pane slot 0 is open");
        let content_size = slot0_projection.scroll_metrics.content_size;
        let first_item_rect = slot0_projection
            .visible_items
            .first()
            .map(|item| item.layout.item_rect);
        let visible_items = slot0_projection.visible_items.len();
        let thumbnail_candidates = projections
            .iter()
            .map(|projection| self.thumbnail_candidate_count_for_projection(projection))
            .sum();
        let folder_preview_candidates = projections
            .iter()
            .map(|projection| self.folder_preview_role_candidate_count_for_projection(projection))
            .sum();
        let paint = ShellPaintPalettes::from_shell_theme(self.theme());
        let theme = paint.shell;

        push_rect(
            &mut vertices,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            theme.view_mode_surface(slot0_projection.view.view_mode),
            size,
        );
        self.push_app_toolbar(&mut vertices, size, theme);
        self.push_places_sidebar(&mut vertices, text, icons, size, paint);
        if let Some(metrics) = self.split_pane_metrics(size) {
            push_rect(&mut vertices, metrics.divider, theme.divider(), size);
        }

        let mut content_scrollbar_visible = false;
        for projection in projections {
            let scrollbar_visible =
                self.push_pane_projection(&mut vertices, text, icons, projection, size, paint);
            if projection.geometry.kind == ShellPaneId::SLOT_0 {
                content_scrollbar_visible = scrollbar_visible;
            }
            self.queue_thumbnail_read_ahead_for_projection(projection, icons);
        }
        if let Some(overlay_text) = overlay_text {
            // Drag preview is a Wayland DnD icon (compositor surface), not an
            // in-window overlay. Only drop/context menus paint into this layer.
            self.push_drop_menu_overlay(&mut overlay_vertices, overlay_text, theme, size);
            self.push_context_menu_overlay(&mut overlay_vertices, overlay_text, icons, theme, size);
        }

        SceneFrame {
            layout_us: projection_layout_us + layout_start.elapsed().as_micros(),
            visible_items,
            thumbnail_candidates,
            folder_preview_candidates,
            quad_count: (vertices.len() + overlay_vertices.len()) / 6,
            content_size,
            content_scrollbar_visible,
            first_item_rect,
            vertices,
            overlay_vertices,
            quad_upload_us: 0,
            text_stats: TextFrameStats::default(),
            icon_stats: IconFrameStats::default(),
            vertex_upload_stats: VertexBufferUploadStats::default(),
        }
    }
}
