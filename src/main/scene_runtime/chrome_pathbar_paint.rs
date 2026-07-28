struct LocationBarLayout {
    size: PhysicalSize<u32>,
    rect: ViewRect,
    clip: ViewRect,
}

struct LocationBarContent<'a> {
    label: &'a str,
    active: bool,
    cursor: Option<usize>,
}

impl ShellScene {

    fn push_location_focus_shine(
        &self,
        vertices: &mut Vec<QuadVertex>,
        rect: ViewRect,
        clip: ViewRect,
        current_value: f32,
        size: PhysicalSize<u32>,
    ) {
        let Some(inner) = inset_rect(rect, self.scale_metric(2.0)) else {
            return;
        };
        let Some(clip) = intersect_rect(inner, clip) else {
            return;
        };
        let min_width = (48.0 * self.ui_scale()).min(inner.width.max(1.0));
        let band_width = (114.0 * self.ui_scale()).clamp(min_width, inner.width.max(min_width));
        let shine_x = rect.x + (rect.width + band_width) * current_value - band_width;
        let strips = 24;
        let strip_width = band_width / strips as f32;
        let peak = 0.666_463_6;
        for index in 0..strips {
            let local = (index as f32 + 0.5) / strips as f32;
            let falloff = if local <= peak {
                local / peak
            } else {
                1.0 - (local - peak) / (1.0 - peak)
            }
            .clamp(0.0, 1.0);
            let alpha = 0.20 * falloff;
            if alpha <= 0.0 {
                continue;
            }
            push_clipped_rect(
                vertices,
                ViewRect {
                    x: shine_x + strip_width * index as f32,
                    y: inner.y,
                    width: strip_width + 1.0,
                    height: inner.height,
                },
                clip,
                [0.173, 0.655, 0.973, alpha],
                size,
            );
        }
    }

    fn push_location_bar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        layout: LocationBarLayout,
        content: LocationBarContent<'_>,
        theme: ShellTheme,
    ) {
        let LocationBarLayout { size, rect, clip } = layout;
        let LocationBarContent {
            label,
            active,
            cursor,
        } = content;
        let radius = self.scale_metric(7.0);
        let editing = active && cursor.is_some();
        let border_color = theme.divider();
        push_clipped_rounded_rect(vertices, rect, clip, radius, border_color, size);
        if let Some(inner) = inset_rect(rect, self.scale_metric(1.0)) {
            push_clipped_rounded_rect(
                vertices,
                inner,
                clip,
                (radius - self.scale_metric(1.0)).max(1.0),
                theme.field(),
                size,
            );
        }
        if editing {
            if let Some(shine_value) = self.location_focus_shine_value() {
                self.push_location_focus_shine(vertices, rect, clip, shine_value, size);
            }
            let mut focus_border = theme.accent();
            focus_border[3] = 0.92;
            push_clipped_rounded_highlight(
                vertices,
                rect,
                clip,
                radius,
                RoundedHighlightStyle {
                    fill: [0.0, 0.0, 0.0, 0.0],
                    border: focus_border,
                    border_width: (0.75 * self.ui_scale()).clamp(1.0, 1.5),
                },
                size,
            );
        }

        let icon_size = self
            .scale_metric(18.0)
            .min((rect.height - self.scale_metric(8.0)).max(1.0));
        let icon_rect = ViewRect {
            x: rect.x + self.scale_metric(8.0),
            y: rect.y + (rect.height - icon_size) / 2.0,
            width: icon_size,
            height: icon_size,
        };
        push_location_bar_icon(
            vertices,
            icon_rect,
            clip,
            false,
            theme,
            self.ui_scale(),
            size,
        );
        let separator_x = icon_rect.right() + self.scale_metric(8.0);
        push_clipped_rect(
            vertices,
            ViewRect {
                x: separator_x,
                y: rect.y + self.scale_metric(7.0),
                width: self.scale_metric(1.0),
                height: (rect.height - self.scale_metric(14.0)).max(1.0),
            },
            clip,
            theme.field_separator(),
            size,
        );
        let text_rect = self.location_text_rect_for_path_bar_rect(rect);
        let cursor_x = cursor.map(|cursor| {
            text.measure_label_cursor_x(
                label,
                text_rect,
                cursor,
                LabelAlignment::Start,
                LabelWrap::None,
            )
        });
        text.push_label_aligned_no_wrap(
            label,
            text_rect,
            clip,
            theme.primary_text(),
            LabelAlignment::Start,
        );
        if editing && self.text_caret_visible() {
            let caret_width = self.scale_metric(1.25);
            let caret_height = self
                .scale_metric(17.0)
                .min((rect.height - self.scale_metric(10.0)).max(1.0));
            let caret_x = (text_rect.x + cursor_x.unwrap_or(0.0)).clamp(
                text_rect.x,
                (text_rect.right() - caret_width).max(text_rect.x),
            );
            push_clipped_rounded_rect(
                vertices,
                ViewRect {
                    x: caret_x,
                    y: rect.y + (rect.height - caret_height) / 2.0,
                    width: caret_width,
                    height: caret_height,
                },
                clip,
                caret_width / 2.0,
                text_color_to_vertex_color(theme.primary_text()),
                size,
            );
        }
    }

    /// Emits the texture-free location-bar chrome for the native Vulkan path.
    /// Labels and the text-measured caret remain with the text renderer, while
    /// every field, focus treatment, and folder glyph is analytic geometry.
    fn push_native_location_bar_chrome(
        &self,
        instances: &mut Vec<crate::vulkan_rect::VulkanRectInstance>,
        pane: ShellPaneId,
        top_bar: ViewRect,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let Some(rect) = self.pane_path_bar_rect(pane, size) else {
            return;
        };
        let radius = self.scale_metric(7.0);
        push_native_rounded_rect_fill(instances, rect, top_bar, radius, theme.divider(), size);
        if let Some(inner) = inset_rect(rect, self.scale_metric(1.0)) {
            push_native_rounded_rect_fill(
                instances,
                inner,
                top_bar,
                (radius - self.scale_metric(1.0)).max(1.0),
                theme.field(),
                size,
            );
        }

        let editing = self.location_bar_active_for_pane(pane)
            && self.location_cursor_for_pane(pane).is_some();
        if editing {
            if let Some(shine_value) = self.location_focus_shine_value() {
                self.push_native_location_focus_shine(
                    instances,
                    rect,
                    top_bar,
                    shine_value,
                    size,
                );
            }
            let mut focus_border = theme.accent();
            focus_border[3] = 0.92;
            push_native_rect_outline(
                instances,
                rect,
                top_bar,
                radius,
                (0.75 * self.ui_scale()).clamp(1.0, 1.5),
                focus_border,
                size,
            );
        }

        let icon_size = self
            .scale_metric(18.0)
            .min((rect.height - self.scale_metric(8.0)).max(1.0));
        let icon_rect = ViewRect {
            x: rect.x + self.scale_metric(8.0),
            y: rect.y + (rect.height - icon_size) / 2.0,
            width: icon_size,
            height: icon_size,
        };
        let icon_colors = theme.toolbar_button(false);
        push_native_rounded_rect_fill(
            instances,
            icon_rect,
            top_bar,
            self.scale_metric(5.0),
            icon_colors.fill,
            size,
        );
        let icon_metric = |value: f32| {
            (value * icon_rect.width.min(icon_rect.height) / 18.0)
                .round()
                .max(1.0)
        };
        push_native_rounded_rect_fill(
            instances,
            ViewRect {
                x: icon_rect.x + icon_metric(5.0),
                y: icon_rect.y + icon_metric(6.0),
                width: icon_metric(7.0),
                height: icon_metric(3.0),
            },
            top_bar,
            icon_metric(1.0),
            icon_colors.icon,
            size,
        );
        push_native_rounded_rect_fill(
            instances,
            ViewRect {
                x: icon_rect.x + icon_metric(4.0),
                y: icon_rect.y + icon_metric(8.0),
                width: icon_rect.width - icon_metric(8.0),
                height: icon_rect.height - icon_metric(11.0),
            },
            top_bar,
            icon_metric(2.0),
            icon_colors.icon,
            size,
        );
        let separator_x = icon_rect.right() + self.scale_metric(8.0);
        push_native_rect_fill(
            instances,
            ViewRect {
                x: separator_x,
                y: rect.y + self.scale_metric(7.0),
                width: self.scale_metric(1.0),
                height: (rect.height - self.scale_metric(14.0)).max(1.0),
            },
            top_bar,
            theme.field_separator(),
            size,
        );
    }

    fn push_native_location_focus_shine(
        &self,
        instances: &mut Vec<crate::vulkan_rect::VulkanRectInstance>,
        rect: ViewRect,
        clip: ViewRect,
        current_value: f32,
        size: PhysicalSize<u32>,
    ) {
        let Some(inner) = inset_rect(rect, self.scale_metric(2.0)) else {
            return;
        };
        let Some(clip) = intersect_rect(inner, clip) else {
            return;
        };
        let min_width = (48.0 * self.ui_scale()).min(inner.width.max(1.0));
        let band_width = (114.0 * self.ui_scale()).clamp(min_width, inner.width.max(min_width));
        let shine_x = rect.x + (rect.width + band_width) * current_value - band_width;
        let strips = 24;
        let strip_width = band_width / strips as f32;
        let peak = 0.666_463_6;
        for index in 0..strips {
            let local = (index as f32 + 0.5) / strips as f32;
            let falloff = if local <= peak {
                local / peak
            } else {
                1.0 - (local - peak) / (1.0 - peak)
            }
            .clamp(0.0, 1.0);
            let alpha = 0.20 * falloff;
            if alpha <= 0.0 {
                continue;
            }
            push_native_rect_fill(
                instances,
                ViewRect {
                    x: shine_x + strip_width * index as f32,
                    y: inner.y,
                    width: strip_width + 1.0,
                    height: inner.height,
                },
                clip,
                [0.173, 0.655, 0.973, alpha],
                size,
            );
        }
    }

    fn prewarm_file_metadata_roles(
        &self,
        projections: &[ShellPaneProjection<'_>],
    ) -> MetadataRolePrewarmStats {
        self.metadata_roles
            .prewarm(projections, Generation(self.path_changes))
    }

    fn drain_metadata_role_results(&mut self) -> MetadataRolePrewarmStats {
        let (mut stats, results) = self.metadata_roles.drain_ready_results();
        for (result, priority) in results {
            if self.apply_metadata_role_result(result) {
                stats.applied += 1;
                stats.visible_applied +=
                    usize::from(priority == fika_core::MetadataRolePriority::Visible);
            }
        }
        stats
    }

    fn resolve_visible_metadata_roles_synchronously(
        &self,
        projections: &[ShellPaneProjection<'_>],
    ) -> (MetadataRoleSyncStats, Vec<MetadataRoleResult>) {
        self.metadata_roles.resolve_visible_synchronously(
            projections,
            Generation(self.path_changes),
            FILE_MANAGER_MAX_BLOCK_TIMEOUT,
        )
    }

    fn apply_synchronous_metadata_role_results(
        &mut self,
        results: Vec<MetadataRoleResult>,
    ) -> usize {
        let mut applied = 0;
        for result in results {
            applied += usize::from(self.apply_metadata_role_result(result));
        }
        applied
    }

    fn drain_folder_preview_role_results(&self) -> FolderPreviewRoleDrainStats {
        self.folder_preview_roles.borrow_mut().drain_results()
    }

    fn update_folder_preview_roles_for_projections(
        &self,
        projections: &[ShellPaneProjection<'_>],
    ) -> FolderPreviewRoleUpdateStats {
        let mut requests = Vec::new();
        for projection in projections {
            let read_ahead_size_px = self.folder_preview_role_size_px_for_view_mode(
                projection.view.view_mode,
                projection.view.zoom_step,
            );
            for item in &projection.visible_items {
                let Some(entry_index) = projection
                    .view
                    .filtered_indexes
                    .get(item.layout.model_index)
                    .copied()
                else {
                    continue;
                };
                let pixmap_layout =
                    ItemPixmapLayout::from_item_layout(projection.view.view_mode, item.layout);
                let size_px = self.folder_preview_role_size_px_for_item(pixmap_layout);
                if let Some(request) = self.folder_preview_role_request_for_pane_entry(
                    projection.view,
                    entry_index,
                    size_px,
                    ThumbnailRequestPriority::Visible,
                ) {
                    requests.push(request);
                }
            }
            let Some(visible_range) = visible_layout_range_for_projection(projection) else {
                continue;
            };
            let item_count = projection.view.filtered_entry_count();
            for layout_index in shell_file_manager_read_ahead_indexes(
                visible_range,
                item_count,
                projection.visible_items.len(),
            )
            .into_iter()
            .take(THUMBNAIL_READ_AHEAD_QUEUE_BUDGET_PER_FRAME)
            {
                let Some(entry_index) = projection.view.filtered_indexes.get(layout_index).copied()
                else {
                    continue;
                };
                if let Some(request) = self.folder_preview_role_request_for_pane_entry(
                    projection.view,
                    entry_index,
                    read_ahead_size_px,
                    ThumbnailRequestPriority::Deferred,
                ) {
                    requests.push(request);
                }
            }
        }
        self.folder_preview_roles
            .borrow_mut()
            .queue_candidates(requests)
    }

    fn folder_preview_role_for_pane_entry(
        &self,
        view: ShellPaneView<'_>,
        entry_index: usize,
        pixmap_layout: ItemPixmapLayout,
    ) -> Option<FolderPreviewReady> {
        let entry = view.entries.get(entry_index)?;
        if !entry.is_dir || !entry.metadata_complete {
            return None;
        }
        let modified_secs = entry.modified_secs?;
        let path = self.entry_path_for_pane_view(view, entry_index)?;
        if is_network_path(&path) {
            return None;
        }
        let size_px = self.folder_preview_role_size_px_for_item(pixmap_layout);
        self.folder_preview_roles
            .borrow_mut()
            .preview_or_closest_touch(&path, modified_secs, size_px)
            .cloned()
    }

    fn folder_preview_role_request_for_pane_entry(
        &self,
        view: ShellPaneView<'_>,
        entry_index: usize,
        size_px: u16,
        priority: ThumbnailRequestPriority,
    ) -> Option<FolderPreviewRoleRequest> {
        let entry = view.entries.get(entry_index)?;
        if !entry.is_dir || !entry.metadata_complete {
            return None;
        }
        let modified_secs = entry.modified_secs?;
        let path = self.entry_path_for_pane_view(view, entry_index)?;
        if is_network_path(&path) {
            return None;
        }
        Some(FolderPreviewRoleRequest {
            key: FolderPreviewRoleKey::new(path, modified_secs, size_px),
            priority,
        })
    }

    fn folder_preview_role_size_px_for_item(&self, pixmap_layout: ItemPixmapLayout) -> u16 {
        folder_preview_role_cache_size(
            pixmap_layout
                .icon_rect
                .width
                .max(pixmap_layout.icon_rect.height)
                .clamp(16.0, 256.0),
        )
    }

    fn folder_preview_role_size_px_for_view_mode(
        &self,
        view_mode: ShellViewMode,
        zoom_step: i32,
    ) -> u16 {
        let item = match view_mode {
            ShellViewMode::Icons => {
                let options = self.icons_options_for_viewport(1.0, 1.0, zoom_step);
                ItemLayout {
                    model_index: 0,
                    column: 0,
                    row: 0,
                    item_rect: ViewRect {
                        x: 0.0,
                        y: 0.0,
                        width: options.item_width,
                        height: options.item_height,
                    },
                    visual_rect: ViewRect::default(),
                    icon_rect: ViewRect {
                        x: (options.item_width - options.icon_size).max(0.0) / 2.0,
                        y: options.padding,
                        width: options.icon_size,
                        height: options.icon_size,
                    },
                    text_rect: ViewRect {
                        x: options.padding,
                        y: options.icon_size + options.padding * 2.0,
                        width: (options.item_width - options.padding * 2.0).max(1.0),
                        height: options.text_height,
                    },
                }
            }
            ShellViewMode::Compact => {
                let options = self.compact_options_for_viewport(1.0, 1.0, zoom_step);
                ItemLayout {
                    model_index: 0,
                    column: 0,
                    row: 0,
                    item_rect: ViewRect {
                        x: 0.0,
                        y: 0.0,
                        width: options.item_width,
                        height: options.item_height,
                    },
                    visual_rect: ViewRect::default(),
                    icon_rect: ViewRect {
                        x: options.padding,
                        y: (options.item_height - options.icon_size) / 2.0,
                        width: options.icon_size,
                        height: options.icon_size,
                    },
                    text_rect: ViewRect {
                        x: options.padding + options.icon_size + options.text_gap,
                        y: (options.item_height - options.text_height) / 2.0,
                        width: (options.item_width
                            - options.padding * 2.0
                            - options.icon_size
                            - options.text_gap)
                            .max(1.0),
                        height: options.text_height,
                    },
                }
            }
            ShellViewMode::Details => {
                let icon_size = self.details_icon_size_for_step(zoom_step);
                let row_height = self.details_row_height_for_step(zoom_step);
                let icon_padding = self.scale_metric(8.0);
                ItemLayout {
                    model_index: 0,
                    column: 0,
                    row: 0,
                    item_rect: ViewRect {
                        x: 0.0,
                        y: 0.0,
                        width: self.details_name_width(),
                        height: row_height,
                    },
                    visual_rect: ViewRect::default(),
                    icon_rect: ViewRect {
                        x: icon_padding,
                        y: (row_height - icon_size) / 2.0,
                        width: icon_size,
                        height: icon_size,
                    },
                    text_rect: ViewRect {
                        x: icon_padding + icon_size + self.scale_metric(8.0),
                        y: (row_height - self.text_line_height()).max(0.0) / 2.0,
                        width: self.details_name_width().max(1.0),
                        height: self.text_line_height(),
                    },
                }
            }
        };
        self.folder_preview_role_size_px_for_item(ItemPixmapLayout::from_item_layout(
            view_mode, item,
        ))
    }

    fn apply_metadata_role_result(&mut self, result: MetadataRoleResult) -> bool {
        let Some(pane_id) = shell_pane_id_for_core_pane(result.pane_id) else {
            return false;
        };
        let Some(index) = shell_metadata_entry_index(result.item_id) else {
            return false;
        };
        let Some(role) = result.role else {
            return false;
        };
        let Some(pane) = self.pane_state_mut(pane_id) else {
            return false;
        };
        let Some(entry) = pane.entries.get(index) else {
            return false;
        };
        if entry.is_dir
            || shell_entry_path(&pane.path, entry) != result.path
            || entry.modified_secs != role.modified_secs
            || entry.size_bytes != role.size_bytes
            || (entry.mime_magic_checked && entry.mime_type == role.mime_type)
        {
            return false;
        }
        pane.entries[index] = entry_with_metadata_role(entry, role);
        true
    }

    fn metadata_role_visible_work_pending(&self) -> bool {
        self.metadata_roles.has_visible_pending()
    }

    fn cancel_metadata_role_work_for_pane(&self, pane: ShellPaneId) {
        self.metadata_roles.cancel_pane(pane);
    }

    fn prewarm_visible_file_icon_roles(
        &self,
        projections: &[ShellPaneProjection<'_>],
        resolver: &mut FileIconResolver,
        reason: &str,
    ) -> IconRolePrewarmStats {
        let mut stats = IconRolePrewarmStats::default();
        let deadline = Instant::now() + icon_role_prewarm_budget_for_frame(reason);
        for projection in projections {
            for item in &projection.visible_items {
                if Instant::now() >= deadline {
                    stats.over_budget = true;
                    return stats;
                }
                let Some(entry_index) = projection
                    .view
                    .filtered_indexes
                    .get(item.layout.model_index)
                    .copied()
                else {
                    continue;
                };
                let Some(entry) = projection.view.entries.get(entry_index) else {
                    continue;
                };
                let icon_size = item
                    .layout
                    .icon_rect
                    .width
                    .max(item.layout.icon_rect.height)
                    .clamp(16.0, 256.0);
                let resolve_start = Instant::now();
                if visible_exact_icon_roles_enabled_for_frame(reason) {
                    let snapshot =
                        resolver.resolve_entry_visible_fast(projection.view.path, entry, icon_size);
                    let _ = snapshot;
                } else {
                    let (snapshot, deferred) =
                        resolver.resolve_entry_visible(projection.view.path, entry, icon_size);
                    if deferred {
                        stats.deferred += 1;
                    }
                    let _ = snapshot;
                }
                stats.resolve_us += resolve_start.elapsed().as_micros();
                stats.entries += 1;
                if Instant::now() >= deadline {
                    stats.over_budget = true;
                    return stats;
                }
            }
        }
        let small_directory_read_ahead =
            self.enqueue_file_manager_small_directory_icon_roles(projections);
        for projection in projections {
            let Some(visible_range) = visible_layout_range_for_projection(projection) else {
                continue;
            };
            let Some(icon_size) = projection.visible_items.first().map(|item| {
                item.layout
                    .icon_rect
                    .width
                    .max(item.layout.icon_rect.height)
                    .clamp(16.0, 256.0)
            }) else {
                continue;
            };
            let item_count = projection.view.filtered_entry_count();
            for layout_index in shell_file_manager_read_ahead_indexes(
                visible_range,
                item_count,
                projection.visible_items.len(),
            ) {
                if stats.read_ahead >= ICON_ROLE_READ_AHEAD_LIMIT {
                    return stats;
                }
                if Instant::now() >= deadline {
                    stats.over_budget = true;
                    return stats;
                }
                let Some(entry_index) = projection.view.filtered_indexes.get(layout_index).copied()
                else {
                    continue;
                };
                let Some(entry) = projection.view.entries.get(entry_index) else {
                    continue;
                };
                self.enqueue_icon_role_read_ahead(projection.view.path, entry, icon_size);
                if Instant::now() >= deadline {
                    stats.over_budget = true;
                    return stats;
                }
            }
        }
        let read_ahead_budget =
            icon_role_read_ahead_queue_budget_for_frame(reason, small_directory_read_ahead);
        if read_ahead_budget > 0 {
            self.resolve_next_icon_role_read_ahead(
                resolver,
                &mut stats,
                deadline,
                read_ahead_budget,
            );
        }
        stats
    }
}
