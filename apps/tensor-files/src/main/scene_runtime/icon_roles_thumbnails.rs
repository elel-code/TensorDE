use crate::ui::file_item_view::item_paint::FileManagerItemPaint;
use crate::vulkan_rect::VulkanRectInstance;

#[derive(Clone, Copy)]
struct PaneItemPaintContext {
    palette: FileManagerItemPalette,
    size: PhysicalSize<u32>,
    theme: ShellTheme,
}

/// Screen-space geometry shared by texture-free item chrome and the later
/// icon/text stages. Keeping this in one preparation step prevents the native
/// Vulkan path from drifting from Tensor Files's regular frame projection.
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

    fn push_native_pane_item_chrome(
        &self,
        instances: &mut Vec<VulkanRectInstance>,
        projection: &ShellPaneProjection<'_>,
        item: PreparedPaneItem,
        context: PaneItemPaintContext,
    ) {
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

    fn push_pane_item_text(
        &self,
        text: &mut TextFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        item: PreparedPaneItem,
        untransformed_item_rect: ViewRect,
        untransformed_text_rect: ViewRect,
        theme: ShellTheme,
    ) {
        let entry_index = item.entry_index;
        let Some(entry) = projection.view.entries.get(entry_index) else {
            return;
        };
        let selected = projection.view.selection.contains(entry_index);
        let text_color = pane_item_text_color(projection.view.view_mode, entry, selected, theme);
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

    /// Adds visible file-item labels to the native atlas without running icon
    /// resolution or CPU quad generation. Geometry comes from the same
    /// prepared item and text recipe as the regular frame.
    fn push_native_pane_item_text(
        &self,
        text: &mut TextFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        item: ShellPaneVisibleItem,
        theme: ShellTheme,
    ) {
        let Some(item) = self.prepare_pane_item(projection, item) else {
            return;
        };
        self.push_pane_item_text(
            text,
            projection,
            item,
            item.item_rect,
            item.text_rect,
            theme,
        );
    }
}
