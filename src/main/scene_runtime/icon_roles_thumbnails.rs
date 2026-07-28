use crate::shell::file_item_view::item_paint::FileManagerItemPaint;
use crate::vulkan_rect::VulkanRectInstance;

#[derive(Clone, Copy)]
struct PaneItemPaintContext {
    palette: FileManagerItemPalette,
    size: PhysicalSize<u32>,
    theme: ShellTheme,
}

/// Screen-space geometry shared by texture-free item chrome and the later
/// icon/text stages. Keeping this in one preparation step prevents the native
/// Vulkan path from drifting from Fika's regular frame projection.
#[derive(Clone, Copy)]
struct PreparedPaneItem {
    entry_index: usize,
    item_rect: ViewRect,
    visual_rect: ViewRect,
    icon_rect: ViewRect,
    text_rect: ViewRect,
    content_clip: ViewRect,
    reflow_dx: f32,
}

#[derive(Clone, Copy)]
struct PaneItemChrome {
    paint: FileManagerItemPaint,
    dnd_hovered: bool,
}

impl ShellScene {

    fn prepare_pane_item(
        &self,
        projection: &ShellPaneProjection<'_>,
        item: ShellPaneVisibleItem,
    ) -> Option<PreparedPaneItem> {
        let layout = item.layout;
        let entry_index = projection
            .view
            .filtered_indexes
            .get(layout.model_index)
            .copied()?;
        projection.view.entries.get(entry_index)?;
        let entry_path = self.entry_path_for_pane_view(projection.view, entry_index);
        let (reflow_dx, reflow_dy) = entry_path
            .as_deref()
            .and_then(|path| self.item_reflow_offset_for_path(projection.geometry.kind, path))
            .unwrap_or((0.0, 0.0));
        Some(PreparedPaneItem {
            entry_index,
            item_rect: translated_rect(
                pane_content_rect_to_screen(layout.item_rect, projection),
                reflow_dx,
                reflow_dy,
            ),
            visual_rect: translated_rect(
                pane_content_rect_to_screen(layout.visual_rect, projection),
                reflow_dx,
                reflow_dy,
            ),
            icon_rect: translated_rect(
                pane_content_rect_to_screen(layout.icon_rect, projection),
                reflow_dx,
                reflow_dy,
            ),
            text_rect: translated_rect(
                pane_content_rect_to_screen(layout.text_rect, projection),
                reflow_dx,
                reflow_dy,
            ),
            content_clip: projection.geometry.content,
            reflow_dx,
        })
    }

    fn pane_item_chrome(
        &self,
        projection: &ShellPaneProjection<'_>,
        item: PreparedPaneItem,
        context: PaneItemPaintContext,
    ) -> PaneItemChrome {
        let selected = projection.view.selection.contains(item.entry_index);
        let hovered = self.hovered_item
            == Some(ShellPaneItemTarget {
                pane: projection.geometry.kind,
                index: item.entry_index,
            });
        let dnd_hovered = matches!(
            self.dnd_hover_target,
            Some(ShellDropTarget::PaneItem {
                pane,
                index,
                is_dir: true,
                ..
            }) if pane == projection.geometry.kind && index == item.entry_index
        );
        let current = projection.geometry.kind == self.active_pane()
            && projection.view.selection.focus == Some(item.entry_index);
        let hover_progress = if hovered {
            self.hover_animation_factor()
        } else {
            1.0
        };
        PaneItemChrome {
            paint: file_manager_item_paint_with_palette_and_hover_progress(
            projection.view.view_mode,
            FileManagerItemGeometry {
                item: item.item_rect,
                visual: item.visual_rect,
                content: item.visual_rect,
            },
            FileManagerItemInteraction {
                selected,
                hovered,
                current,
                alternate: item.entry_index % 2 == 1,
            },
            self.ui_scale(),
            context.palette,
            hover_progress,
            ),
            dnd_hovered,
        }
    }

    fn push_pane_item_chrome(
        &self,
        vertices: &mut Vec<QuadVertex>,
        projection: &ShellPaneProjection<'_>,
        item: PreparedPaneItem,
        context: PaneItemPaintContext,
    ) {
        let PaneItemChrome { paint, dnd_hovered } =
            self.pane_item_chrome(projection, item, context);

        if let Some(background) = paint.alternate_background {
            push_clipped_rect(
                vertices,
                background.rect,
                item.content_clip,
                background.color,
                context.size,
            );
        }
        if let Some(background) = paint.background {
            if background.radius <= 0.0 {
                push_clipped_rect(
                    vertices,
                    background.rect,
                    item.content_clip,
                    background.color,
                    context.size,
                );
            } else {
                push_clipped_rounded_rect(
                    vertices,
                    background.rect,
                    item.content_clip,
                    background.radius,
                    background.color,
                    context.size,
                );
            }
        }
        if let Some(focus) = paint.focus {
            push_clipped_rounded_highlight(
                vertices,
                focus.rect,
                item.content_clip,
                focus.radius,
                RoundedHighlightStyle {
                    fill: [0.0, 0.0, 0.0, 0.0],
                    border: focus.color,
                    border_width: focus.stroke_width,
                },
                context.size,
            );
        }
        if dnd_hovered {
            let radius = self.scale_metric(7.0);
            let drop_target = context.theme.drop_target();
            push_clipped_rounded_highlight(
                vertices,
                item.visual_rect,
                item.content_clip,
                radius,
                RoundedHighlightStyle {
                    fill: drop_target.fill,
                    border: drop_target.border,
                    border_width: self.scale_metric(1.0),
                },
                context.size,
            );
        }
    }

    /// Emits the same item chrome state as [`Self::push_pane_item_chrome`],
    /// but keeps rounded corners and outlines as GPU-evaluated instances.
    fn push_native_pane_item_chrome(
        &self,
        instances: &mut Vec<VulkanRectInstance>,
        projection: &ShellPaneProjection<'_>,
        item: PreparedPaneItem,
        context: PaneItemPaintContext,
    ) {
        let Some(entry) = projection.view.entries.get(item.entry_index) else {
            return;
        };
        let PaneItemChrome { paint, dnd_hovered } =
            self.pane_item_chrome(projection, item, context);
        if let Some(background) = paint.alternate_background
            && let Some(instance) = VulkanRectInstance::fill(
                background.rect,
                item.content_clip,
                0.0,
                background.color,
                context.size,
            )
        {
            instances.push(instance);
        }
        if let Some(background) = paint.background
            && let Some(instance) = VulkanRectInstance::fill(
                background.rect,
                item.content_clip,
                background.radius,
                background.color,
                context.size,
            )
        {
            instances.push(instance);
        }
        if let Some(focus) = paint.focus
            && let Some(instance) = VulkanRectInstance::outline(
                focus.rect,
                item.content_clip,
                focus.radius,
                focus.stroke_width,
                focus.color,
                context.size,
            )
        {
            instances.push(instance);
        }
        if dnd_hovered {
            let radius = self.scale_metric(7.0);
            let drop_target = context.theme.drop_target();
            if let Some(instance) = VulkanRectInstance::fill(
                item.visual_rect,
                item.content_clip,
                radius,
                drop_target.fill,
                context.size,
            ) {
                instances.push(instance);
            }
            if let Some(instance) = VulkanRectInstance::outline(
                item.visual_rect,
                item.content_clip,
                radius,
                self.scale_metric(1.0),
                drop_target.border,
                context.size,
            ) {
                instances.push(instance);
            }
        }
        crate::shell::ui_chrome::push_native_fallback_file_icon(
            instances,
            entry,
            item.icon_rect,
            item.content_clip,
            context.theme,
            context.size,
        );
    }

    fn enqueue_file_manager_small_directory_icon_roles(
        &self,
        projections: &[ShellPaneProjection<'_>],
    ) -> bool {
        let mut queued = false;
        for projection in projections {
            if projection.view.filtered_entry_count() > FILE_MANAGER_RESOLVE_ALL_ITEMS_LIMIT {
                continue;
            }
            let Some(icon_size) = projection.visible_items.first().map(|item| {
                item.layout
                    .icon_rect
                    .width
                    .max(item.layout.icon_rect.height)
                    .clamp(16.0, 256.0)
            }) else {
                continue;
            };
            for entry_index in projection.view.filtered_indexes.iter().copied() {
                let Some(entry) = projection.view.entries.get(entry_index) else {
                    continue;
                };
                self.enqueue_icon_role_read_ahead(projection.view.path, entry, icon_size);
                queued = true;
            }
        }
        queued
    }

    fn enqueue_icon_role_read_ahead(&self, directory: &Path, entry: &Entry, icon_size: f32) {
        let path = directory.join(entry.name.as_ref());
        let key = file_icon_path_cache_key_with_stamp(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            entry.modified_secs,
            icon_size,
        );
        self.icon_role_read_ahead.borrow_mut().push_key(key);
    }

    fn resolve_next_icon_role_read_ahead(
        &self,
        resolver: &mut FileIconResolver,
        stats: &mut IconRolePrewarmStats,
        deadline: Instant,
        limit: usize,
    ) {
        for _ in 0..limit {
            if Instant::now() >= deadline {
                stats.over_budget = true;
                return;
            }
            let Some(request) = self.icon_role_read_ahead.borrow_mut().pop_front() else {
                return;
            };
            let resolve_start = Instant::now();
            let snapshot = resolver.resolve_path_cache_key(request.key);
            stats.resolve_us += resolve_start.elapsed().as_micros();
            stats.read_ahead += 1;
            if snapshot.is_none() {
                stats.deferred += 1;
            }
            let _ = snapshot;
        }
    }

    fn prewarm_file_item_text_labels(
        &self,
        projections: &[ShellPaneProjection<'_>],
        text: &mut TextFrameBuilder<'_>,
        mode: TextLabelPrewarmMode,
    ) -> TextLabelPrewarmStats {
        let mut stats = TextLabelPrewarmStats::default();
        let raster_us_start = text.raster_us;
        let deadline = Instant::now() + text_label_prewarm_budget_for_mode(mode);
        let theme = self.theme();

        for projection in projections {
            for item in &projection.visible_items {
                if Instant::now() >= deadline {
                    stats.over_budget = true;
                    stats.raster_us = text.raster_us.saturating_sub(raster_us_start);
                    return stats;
                }
                let outcome =
                    self.prewarm_projection_text_label(projection, item.layout, text, theme);
                if outcome != LabelCacheOutcome::Skipped {
                    stats.entries += 1;
                }
                stats.record(outcome);
            }
        }

        stats.raster_us = text.raster_us.saturating_sub(raster_us_start);
        stats
    }

    fn prewarm_projection_text_label(
        &self,
        projection: &ShellPaneProjection<'_>,
        layout: ItemLayout,
        text: &mut TextFrameBuilder<'_>,
        theme: ShellTheme,
    ) -> LabelCacheOutcome {
        let Some(entry_index) = projection
            .view
            .filtered_indexes
            .get(layout.model_index)
            .copied()
        else {
            return LabelCacheOutcome::Skipped;
        };
        let Some(entry) = projection.view.entries.get(entry_index) else {
            return LabelCacheOutcome::Skipped;
        };
        let selected = projection.view.selection.contains(entry_index);
        let text_color = pane_item_text_color(projection.view.view_mode, entry, selected, theme);
        match projection.view.view_mode {
            ShellViewMode::Compact => text.prewarm_label_aligned_wrapped(
                entry.name.as_ref(),
                layout.text_rect,
                text_color,
                LabelAlignment::Start,
                LabelWrap::None,
            ),
            ShellViewMode::Details => text.prewarm_filename_label_aligned_no_wrap(
                entry.name.as_ref(),
                layout.text_rect,
                text_color,
                LabelAlignment::Start,
            ),
            ShellViewMode::Icons => text.prewarm_filename_label_wrapped(
                entry.name.as_ref(),
                layout.text_rect,
                text_color,
            ),
        }
    }

    fn push_pane_projection(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        size: PhysicalSize<u32>,
        paint: ShellPaintPalettes,
    ) -> bool {
        let theme = paint.shell;
        let pane_id = projection.geometry.kind;
        let pane = projection.geometry.pane;
        let top_bar = projection.geometry.top_bar;
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };

        if let Some(path_rect) = self.pane_path_bar_rect(pane_id, size) {
            let location_active = self.location_bar_active_for_pane(pane_id);
            let path_label = self.location_label_for_pane(pane_id);
            let path_cursor = self.location_cursor_for_pane(pane_id);
            self.push_location_bar(
                vertices,
                text,
                LocationBarLayout {
                    size,
                    rect: path_rect,
                    clip: top_bar,
                },
                LocationBarContent {
                    label: &path_label,
                    active: location_active,
                    cursor: path_cursor,
                },
                theme,
            );
        }

        self.push_pane_body_border(vertices, projection, theme, size);
        if pane_id == ShellPaneId::SLOT_0 {
            self.push_filter_bar(vertices, text, size, theme);
        }
        if projection.view.view_mode == ShellViewMode::Details {
            self.push_details_header_for_projection(vertices, text, projection, size, theme);
        }

        let item_palette = paint.file_manager_item;
        for item in projection.visible_items.iter().copied() {
            self.push_pane_item(
                vertices,
                text,
                icons,
                projection,
                item,
                PaneItemPaintContext {
                    palette: item_palette,
                    size,
                    theme,
                },
            );
        }
        if self.rubber_band.is_some() && pane_id == self.active_pane() {
            self.push_rubber_band_for_projection(vertices, projection, theme, size);
        }
        let content_scrollbar_visible =
            self.push_content_scrollbar_for_projection(vertices, projection, theme, size);
        self.push_pane_status_bar(vertices, text, projection, size, theme);
        push_clipped_rect_outline(
            vertices,
            pane,
            screen,
            self.scale_metric(1.0).max(1.0),
            theme.divider(),
            size,
        );
        content_scrollbar_visible
    }

    fn push_pane_item(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        item: ShellPaneVisibleItem,
        context: PaneItemPaintContext,
    ) {
        let PaneItemPaintContext {
            size,
            theme,
            ..
        } = context;
        let Some(item) = self.prepare_pane_item(projection, item) else {
            return;
        };
        let entry_index = item.entry_index;
        let Some(entry) = projection.view.entries.get(entry_index) else {
            return;
        };
        self.push_pane_item_chrome(vertices, projection, item, context);
        let untransformed_item_rect = item.item_rect;
        let untransformed_text_rect = item.text_rect;
        let pixmap_layout = ItemPixmapLayout {
            view_mode: projection.view.view_mode,
            icon_rect: item.icon_rect,
            text_rect: item.text_rect,
            text_midline_shift: text.file_manager_midline_shift(),
        };
        let selected = projection.view.selection.contains(entry_index);

        let folder_preview =
            self.folder_preview_role_for_pane_entry(projection.view, entry_index, pixmap_layout);
        if !icons.push_thumbnail_or_icon(
            projection.view.path,
            entry,
            folder_preview.as_ref(),
            pixmap_layout,
            item.content_clip,
        ) {
            push_fallback_file_icon(
                vertices,
                entry,
                item.icon_rect,
                item.content_clip,
                theme,
                size,
            );
        }

        let base_text_color =
            pane_item_text_color(projection.view.view_mode, entry, selected, theme);
        let text_color = base_text_color;
        let muted_text = theme.muted_text();
        match projection.view.view_mode {
            ShellViewMode::Compact => {
                text.push_label_aligned_wrapped_with_layout(
                    entry.name.as_ref(),
                    TextLabelLayout {
                        draw: item.text_rect,
                        layout: untransformed_text_rect,
                        clip: item.content_clip,
                    },
                    TextLabelStyle {
                        color: text_color,
                        alignment: LabelAlignment::Start,
                        wrap: LabelWrap::None,
                    },
                );
            }
            ShellViewMode::Details => {
                text.push_filename_label_aligned_no_wrap_with_layout(
                    entry.name.as_ref(),
                    item.text_rect,
                    untransformed_text_rect,
                    item.content_clip,
                    text_color,
                    LabelAlignment::Start,
                );
            }
            ShellViewMode::Icons => {
                text.push_filename_label_wrapped_with_layout(
                    entry.name.as_ref(),
                    item.text_rect,
                    untransformed_text_rect,
                    item.content_clip,
                    text_color,
                );
            }
        }

        if projection.view.view_mode == ShellViewMode::Details {
            let text_height = self.text_line_height();
            let metadata_y = untransformed_item_rect.y
                + (untransformed_item_rect.height - text_height).max(0.0) / 2.0;
            let size_rect = ViewRect {
                x: item.content_clip.x + self.details_name_width() + self.scale_metric(8.0)
                    - projection.view.scroll_x
                    + item.reflow_dx,
                y: metadata_y,
                width: self.details_size_width() - self.scale_metric(16.0),
                height: text_height,
            };
            text.push_label_aligned_no_wrap(
                &details_size_label(entry),
                size_rect,
                item.content_clip,
                muted_text,
                LabelAlignment::Start,
            );
            let modified_rect = ViewRect {
                x: item.content_clip.x
                    + self.details_name_width()
                    + self.details_size_width()
                    + self.scale_metric(8.0)
                    - projection.view.scroll_x
                    + item.reflow_dx,
                y: metadata_y,
                width: self.details_modified_width() - self.scale_metric(16.0),
                height: text_height,
            };
            text.push_label_aligned_no_wrap(
                &format_modified_secs(entry.modified_secs),
                modified_rect,
                item.content_clip,
                muted_text,
                LabelAlignment::Start,
            );
        }
    }
}
