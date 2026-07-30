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

    pub(crate) fn pane_projections_from_layouts(
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

    pub(crate) fn update_visible_slot_pools_for_projection_layouts(
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

    /// Builds structural quad layers independent of glyph-atlas, sampled-icon,
    /// and decode work for the native Vulkan frame.
    pub(crate) fn build_native_frame_layers(
        &self,
        size: PhysicalSize<u32>,
        projections: &[ShellPaneProjection<'_>],
    ) -> crate::vulkan_rect::NativeFrameLayers {
        let mut layers = crate::vulkan_rect::NativeFrameLayers::with_capacities(192, 0);
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let Some(slot0_projection) = projections
            .iter()
            .find(|projection| projection.geometry.kind == ShellPaneId::SLOT_0)
        else {
            return layers;
        };
        let paint = ShellPaintPalettes::from_shell_theme(self.theme());
        let theme = paint.shell;
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };

        push_native_rect_fill(
            &mut layers.base_rects,
            screen,
            screen,
            theme.view_mode_surface(slot0_projection.view.view_mode),
            size,
        );
        self.push_native_app_toolbar(&mut layers.base_rects, size, theme);
        self.push_native_places_chrome(&mut layers.base_rects, size, theme);
        self.push_native_places_rows_chrome(&mut layers.base_rects, size, paint);
        if let Some(metrics) = self.split_pane_metrics(size) {
            push_native_rect_fill(
                &mut layers.base_rects,
                metrics.divider,
                screen,
                theme.divider(),
                size,
            );
        }

        for projection in projections {
            push_native_rect_fill(
                &mut layers.base_rects,
                projection.geometry.top_bar,
                screen,
                theme.field(),
                size,
            );
            self.push_native_location_bar_chrome(
                &mut layers.base_rects,
                projection.geometry.kind,
                projection.geometry.top_bar,
                size,
                theme,
            );
            self.push_native_pane_body_border(
                &mut layers.base_rects,
                projection,
                theme,
                size,
            );
            if projection.geometry.kind == ShellPaneId::SLOT_0 {
                self.push_native_filter_bar_chrome(&mut layers.base_rects, size, theme);
            }
            if projection.view.view_mode == ShellViewMode::Details {
                self.push_native_details_header_chrome(&mut layers.base_rects, projection, size, theme);
            }
            let item_context = PaneItemPaintContext {
                palette: paint.file_manager_item,
                size,
                theme,
            };
            for item in projection.visible_items.iter().copied() {
                let Some(item) = self.prepare_pane_item(projection, item) else {
                    continue;
                };
                self.push_native_pane_item_chrome(
                    &mut layers.base_rects,
                    projection,
                    item,
                    item_context,
                );
            }
            if projection.geometry.kind == self.active_pane() {
                self.push_native_rubber_band_for_projection(
                    &mut layers.base_rects,
                    projection,
                    theme,
                    size,
                );
            }
            push_native_rect_fill(
                &mut layers.base_rects,
                projection.geometry.status_bar,
                screen,
                theme.field(),
                size,
            );
            self.push_native_pane_status_chrome(
                &mut layers.base_rects,
                projection,
                size,
                theme,
            );
            self.push_native_content_scrollbar_for_projection(
                &mut layers.base_rects,
                projection,
                theme,
                size,
            );
            push_native_rect_outline(
                &mut layers.base_rects,
                projection.geometry.pane,
                screen,
                0.0,
                self.scale_metric(1.0).max(1.0),
                theme.divider(),
                size,
            );
        }
        layers
    }

    /// Populates the R8 atlas stage for native Vulkan directly from retained
    /// pane projections. This bypasses icon resolution and the regular CPU
    /// quad stream while sharing the exact Places, location, filter, Details,
    /// file-item, and status text recipes with the default renderer.
    pub(crate) fn push_native_frame_text(
        &self,
        text: &mut TextFrameBuilder<'_>,
        projections: &[ShellPaneProjection<'_>],
        size: PhysicalSize<u32>,
    ) {
        let theme = ShellPaintPalettes::from_shell_theme(self.theme()).shell;
        self.push_places_sidebar_text(text, size, theme);
        for projection in projections {
            self.push_native_location_bar_text(
                text,
                projection.geometry.kind,
                projection.geometry.top_bar,
                size,
                theme,
            );
            if projection.geometry.kind == ShellPaneId::SLOT_0 {
                self.push_filter_bar_text(text, size, theme);
            }
            if projection.view.view_mode == ShellViewMode::Details {
                self.push_details_header_text(text, projection, theme);
            }
            for item in projection.visible_items.iter().copied() {
                self.push_native_pane_item_text(text, projection, item, theme);
            }
            self.push_native_pane_status_text(text, projection, theme);
        }
    }

    fn push_native_places_chrome(
        &self,
        instances: &mut Vec<crate::vulkan_rect::VulkanRectInstance>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return;
        }
        let panel = self.places_panel_rect(size);
        push_native_rect_outline(
            instances,
            panel,
            sidebar,
            self.scale_metric(12.0),
            self.scale_metric(1.0),
            theme.divider(),
            size,
        );
        push_native_rect_fill(
            instances,
            ViewRect {
                x: sidebar.right(),
                y: sidebar.y,
                width: self.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH),
                height: sidebar.height,
            },
            sidebar,
            theme.divider(),
            size,
        );
    }

    fn push_native_pane_body_border(
        &self,
        instances: &mut Vec<crate::vulkan_rect::VulkanRectInstance>,
        projection: &ShellPaneProjection<'_>,
        theme: ShellTheme,
        size: PhysicalSize<u32>,
    ) {
        let body = ViewRect {
            x: projection.geometry.pane.x,
            y: projection.geometry.top_bar.bottom(),
            width: projection.geometry.pane.width,
            height: (projection.geometry.status_bar.y - projection.geometry.top_bar.bottom())
                .max(1.0),
        };
        push_native_rect_fill(
            instances,
            ViewRect {
                x: body.x,
                y: body.y,
                width: body.width,
                height: 1.0,
            },
            projection.geometry.pane,
            theme.divider(),
            size,
        );
    }
}

fn push_native_rect_fill(
    instances: &mut Vec<crate::vulkan_rect::VulkanRectInstance>,
    rect: ViewRect,
    clip: ViewRect,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    push_native_rounded_rect_fill(instances, rect, clip, 0.0, color, size);
}

fn push_native_rounded_rect_fill(
    instances: &mut Vec<crate::vulkan_rect::VulkanRectInstance>,
    rect: ViewRect,
    clip: ViewRect,
    radius: f32,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    if let Some(instance) =
        crate::vulkan_rect::VulkanRectInstance::fill(rect, clip, radius, color, size)
    {
        instances.push(instance);
    }
}

fn push_native_rect_outline(
    instances: &mut Vec<crate::vulkan_rect::VulkanRectInstance>,
    rect: ViewRect,
    clip: ViewRect,
    radius: f32,
    stroke_width: f32,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    if let Some(instance) = crate::vulkan_rect::VulkanRectInstance::outline(
        rect,
        clip,
        radius,
        stroke_width,
        color,
        size,
    ) {
        instances.push(instance);
    }
}
