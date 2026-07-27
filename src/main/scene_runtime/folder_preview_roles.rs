impl ShellScene {

    fn thumbnail_candidate_count_for_projection(
        &self,
        projection: &ShellPaneProjection<'_>,
    ) -> usize {
        projection
            .visible_items
            .iter()
            .filter(|item| {
                projection
                    .view
                    .filtered_indexes
                    .get(item.layout.model_index)
                    .copied()
                    .and_then(|entry_index| {
                        self.thumbnail_candidate_for_pane_entry(projection.view, entry_index)
                    })
                    .is_some()
            })
            .count()
    }

    fn folder_preview_role_candidate_count_for_projection(
        &self,
        projection: &ShellPaneProjection<'_>,
    ) -> usize {
        projection
            .visible_items
            .iter()
            .filter(|item| {
                projection
                    .view
                    .filtered_indexes
                    .get(item.layout.model_index)
                    .copied()
                    .and_then(|entry_index| {
                        self.folder_preview_role_requestable_for_pane_entry(
                            projection.view,
                            entry_index,
                        )
                    })
                    .is_some()
            })
            .count()
    }

    fn queue_thumbnail_read_ahead_for_projection(
        &self,
        projection: &ShellPaneProjection<'_>,
        icons: &mut IconFrameBuilder<'_>,
    ) {
        let Some(visible_range) = visible_layout_range_for_projection(projection) else {
            return;
        };
        let size_px =
            self.thumbnail_read_ahead_size_px(projection.view.view_mode, projection.view.zoom_step);
        if size_px < 32 {
            return;
        }
        let item_count = projection.view.filtered_entry_count();
        for layout_index in shell_dolphin_read_ahead_indexes(
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
            if let Some(candidate) =
                self.thumbnail_candidate_for_pane_entry(projection.view, entry_index)
            {
                icons.queue_thumbnail_read_ahead(candidate, size_px);
            }
        }
    }

    fn thumbnail_read_ahead_size_px(&self, view_mode: ShellViewMode, zoom_step: i32) -> u16 {
        let icon_size = match view_mode {
            ShellViewMode::Icons => {
                self.zoom_icon_metric_for_step(zoom_step, ICONS_ICON_SIZE, 16.0, 256.0)
            }
            ShellViewMode::Compact => {
                self.zoom_icon_metric_for_step(zoom_step, COMPACT_ICON_SIZE, 16.0, 144.0)
            }
            ShellViewMode::Details => self.details_icon_size_for_step(zoom_step),
        };
        // Folder previews are already a composed icon-sized raster. Keeping
        // their ready/GPU size at the actual Dolphin zoom bucket avoids doing
        // four 256px child compositions for a 48px folder icon.
        icon_cache_size(icon_size)
    }

    fn thumbnail_candidate_for_pane_entry(
        &self,
        view: ShellPaneView<'_>,
        entry_index: usize,
    ) -> Option<ShellThumbnailCandidate> {
        let entry = view.entries.get(entry_index)?;
        if !entry.metadata_complete {
            return None;
        }
        let modified_secs = entry.modified_secs?;
        let path = self.entry_path_for_pane_view(view, entry_index)?;
        if entry.is_dir
            || is_network_path(&path)
            || mime_magic_resolution_required(
                entry.is_dir,
                entry.size_bytes,
                entry.mime_type.as_deref(),
                entry.mime_magic_checked,
            )
            || !thumbnail_request_may_have_preview(&path, entry.mime_type.as_deref())
        {
            return None;
        }
        Some(ShellThumbnailCandidate {
            path,
            modified_secs,
            mime_type: entry
                .mime_type
                .as_deref()
                .map(std::borrow::ToOwned::to_owned),
        })
    }

    fn folder_preview_role_requestable_for_pane_entry(
        &self,
        view: ShellPaneView<'_>,
        entry_index: usize,
    ) -> Option<()> {
        let entry = view.entries.get(entry_index)?;
        if !entry.is_dir || !entry.metadata_complete {
            return None;
        }
        let _modified_secs = entry.modified_secs?;
        let path = self.entry_path_for_pane_view(view, entry_index)?;
        (!is_network_path(&path)).then_some(())
    }

    fn push_details_header_for_projection(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let header_height = self.details_header_height();
        let header = ViewRect {
            x: projection.geometry.content.x,
            y: (projection.geometry.content.y - header_height).max(projection.geometry.top_bar.y),
            width: projection.geometry.content.width,
            height: header_height,
        };
        push_rect(
            vertices,
            ViewRect {
                x: header.x,
                y: header.y,
                width: header.width,
                height: self.scale_metric(1.0).max(1.0),
            },
            theme.field_separator(),
            size,
        );
        push_rect(
            vertices,
            ViewRect {
                x: header.x,
                y: header.bottom() - 1.0,
                width: header.width,
                height: 1.0,
            },
            theme.divider(),
            size,
        );
        let name_separator_x = header.x + self.details_name_width() - projection.view.scroll_x;
        let size_separator_x = header.x + self.details_name_width() + self.details_size_width()
            - projection.view.scroll_x;
        for separator_x in [name_separator_x, size_separator_x] {
            if separator_x > header.x && separator_x < header.right() {
                push_rect(
                    vertices,
                    ViewRect {
                        x: separator_x.round(),
                        y: header.y + self.scale_metric(6.0),
                        width: self.scale_metric(1.0).max(1.0),
                        height: (header.height - self.scale_metric(12.0)).max(1.0),
                    },
                    theme.field_separator(),
                    size,
                );
            }
        }
        for (label, x, width) in [
            (
                "Name",
                self.scale_metric(34.0),
                self.details_name_width() - self.scale_metric(42.0),
            ),
            (
                "Size",
                self.details_name_width() + self.scale_metric(8.0),
                self.details_size_width() - self.scale_metric(16.0),
            ),
            (
                "Modified",
                self.details_name_width() + self.details_size_width() + self.scale_metric(8.0),
                self.details_modified_width() - self.scale_metric(16.0),
            ),
        ] {
            text.push_label_aligned_no_wrap(
                label,
                ViewRect {
                    x: header.x + x,
                    y: header.y + self.scale_metric(6.0),
                    width: width.max(1.0),
                    height: self.text_line_height(),
                },
                header,
                theme.muted_text(),
                LabelAlignment::Start,
            );
        }
    }

    fn push_pane_status_bar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let pane = projection.view;
        let rect = projection.geometry.status_bar;
        let status = self.pane_status(pane, projection.visible_items.len());
        push_status_pane_bar(
            vertices,
            text,
            PaneStatusBarPaint {
                rect,
                status: &status,
                active: projection.geometry.kind == self.active_pane(),
                zoom_percent: self.zoom_percent_for_step(pane.zoom_step),
                zoom_fraction: self.zoom_fraction_for_step(pane.zoom_step),
                theme,
                scale: self.ui_scale(),
                line_height: self.text_line_height(),
                size,
            },
        );
    }

    fn push_places_sidebar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        size: PhysicalSize<u32>,
        paint: ShellPaintPalettes,
    ) {
        let theme = paint.shell;
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return;
        }
        let panel = self.places_panel_rect(size);
        let panel_radius = self.scale_metric(12.0);
        push_clipped_rounded_rect_outline(
            vertices,
            panel,
            sidebar,
            panel_radius,
            self.scale_metric(1.0),
            theme.divider(),
            size,
        );
        push_rect(
            vertices,
            ViewRect {
                x: sidebar.right(),
                y: sidebar.y,
                width: self.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH),
                height: sidebar.height,
            },
            theme.divider(),
            size,
        );

        let active_place_path = self
            .pane_state(self.active_pane())
            .map(|pane| pane.path.as_path())
            .unwrap_or_else(|| self.panes[ShellPaneId::SLOT_0].path.as_path());
        let active_place = active_shell_place_index(&self.places, active_place_path);
        let top_padding = self.scale_metric(PLACES_SIDEBAR_TOP_PADDING);
        let title_height = self.scale_metric(PLACES_TITLE_HEIGHT);
        let padding_x = self.scale_metric(PLACES_SIDEBAR_PADDING_X);
        let section_height = self.scale_metric(PLACES_SECTION_HEIGHT);
        let row_height = self.scale_metric(PLACES_ROW_HEIGHT);
        let row_gap = self.scale_metric(PLACES_ROW_GAP);
        let icon_size = self.scale_metric(PLACES_ICON_SIZE);
        let text_height = self.text_line_height();
        let small_text_height = self.small_text_line_height();
        let item_palette = paint.dolphin_item;
        let mut y = panel.y + top_padding + title_height - self.places_scroll_y;
        let mut previous_group = None;
        for (index, place) in self.places.iter().enumerate() {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                let section = ViewRect {
                    x: panel.x + padding_x + self.scale_metric(8.0),
                    y: y + self.scale_metric(4.0),
                    width: (panel.width - padding_x * 2.0 - self.scale_metric(16.0)).max(1.0),
                    height: small_text_height,
                };
                if section.y < panel.bottom() && section.bottom() > panel.y {
                    let line_height = self.scale_metric(1.0).max(1.0);
                    push_clipped_rounded_rect(
                        vertices,
                        ViewRect {
                            x: section.x,
                            y: section.y + small_text_height + self.scale_metric(3.0),
                            width: (section.width * 0.42).max(self.scale_metric(28.0)),
                            height: line_height,
                        },
                        panel,
                        line_height / 2.0,
                        theme.field_separator(),
                        size,
                    );
                    text.push_label_aligned(
                        place.group,
                        section,
                        panel,
                        theme.section_text(),
                        LabelAlignment::Start,
                    );
                }
                y += section_height;
            }

            let row = ViewRect {
                x: panel.x + padding_x,
                y,
                width: (panel.width - padding_x * 2.0).max(1.0),
                height: row_height,
            };
            if row.y < panel.bottom() && row.bottom() > panel.y {
                let active = active_place == Some(index);
                let hovered = self.hovered_place == Some(index);
                let hover_progress = if hovered {
                    self.hover_animation_factor()
                } else {
                    1.0
                };
                let dnd_hovered = matches!(
                    self.dnd_hover_target,
                    Some(ShellDropTarget::Place {
                        index: target_index,
                        ..
                    }) if target_index == index
                );
                if active {
                    push_clipped_rounded_rect(
                        vertices,
                        row,
                        panel,
                        self.scale_metric(BREEZE_ITEM_ROUNDNESS),
                        place_row_background_color_for_palette_with_hover_progress(
                            active,
                            hovered,
                            item_palette,
                            hover_progress,
                        ),
                        size,
                    );
                    let rail_width = self.scale_metric(3.0).max(2.0);
                    push_clipped_rounded_rect(
                        vertices,
                        ViewRect {
                            x: row.x + self.scale_metric(3.0),
                            y: row.y + self.scale_metric(6.0),
                            width: rail_width,
                            height: (row.height - self.scale_metric(12.0)).max(1.0),
                        },
                        panel,
                        rail_width / 2.0,
                        theme.accent(),
                        size,
                    );
                } else if hovered {
                    push_clipped_rounded_rect(
                        vertices,
                        row,
                        panel,
                        self.scale_metric(BREEZE_ITEM_ROUNDNESS),
                        place_row_background_color_for_palette_with_hover_progress(
                            active,
                            hovered,
                            item_palette,
                            hover_progress,
                        ),
                        size,
                    );
                }
                if dnd_hovered {
                    let drop_target = theme.drop_target();
                    push_clipped_rounded_highlight(
                        vertices,
                        row,
                        panel,
                        self.scale_metric(8.0),
                        RoundedHighlightStyle {
                            fill: drop_target.fill,
                            border: drop_target.border,
                            border_width: self.scale_metric(1.0),
                        },
                        size,
                    );
                }
                let icon = ViewRect {
                    x: row.x + self.scale_metric(8.0),
                    y: row.y + (row.height - icon_size) / 2.0,
                    width: icon_size,
                    height: icon_size,
                };
                // Hover uses the row background only (Breeze / Dolphin places).
                // Do not draw a separate icon-slot chip; it reads as a white tile
                // under theme icons and the geometric fallback glyphs.
                let trash_has_items = self.trash_place_has_items(place);
                let icon_name = if trash_has_items {
                    "user-trash-full"
                } else {
                    place.icon_name
                };
                if !icons.push_named_theme_icon(
                    icon_name,
                    NamedIconFallback::Service,
                    icon,
                    panel,
                    IconDrawLayer::Content,
                ) {
                    push_place_icon(
                        vertices,
                        icon,
                        panel,
                        place_icon_paint(place),
                        theme,
                        self.ui_scale(),
                        size,
                    );
                }
                text.push_label_aligned(
                    &place.label,
                    ViewRect {
                        x: icon.right() + self.scale_metric(8.0),
                        y: row.y + (row.height - text_height) / 2.0,
                        width: (row.right() - icon.right() - self.scale_metric(16.0)).max(1.0),
                        height: text_height,
                    },
                    panel,
                    if active {
                        theme.accent_text()
                    } else {
                        theme.primary_text()
                    },
                    LabelAlignment::Start,
                );
                if trash_has_items {
                    let dot_size = self.scale_metric(7.0);
                    push_clipped_rounded_rect(
                        vertices,
                        ViewRect {
                            x: row.right() - self.scale_metric(8.0) - dot_size,
                            y: row.y + (row.height - dot_size) / 2.0,
                            width: dot_size,
                            height: dot_size,
                        },
                        panel,
                        dot_size / 2.0,
                        theme.accent(),
                        size,
                    );
                }
            }

            y += row_height + row_gap;
            previous_group = Some(place.group);
        }

        if let Some(ShellDropTarget::PlacesGap { index }) = self.dnd_hover_target.as_ref()
            && let Some(gap) = self.place_gap_rect_for_index(*index, size)
        {
            let drop_target = theme.drop_target();
            let line_height = self.scale_metric(3.0).max(2.0);
            let line = ViewRect {
                x: gap.x + self.scale_metric(8.0),
                y: gap.y + (gap.height - line_height) / 2.0,
                width: (gap.width - self.scale_metric(16.0)).max(1.0),
                height: line_height,
            };
            push_clipped_rounded_rect(
                vertices,
                line,
                panel,
                line_height / 2.0,
                drop_target.marker,
                size,
            );
        }

        if let Some((track, thumb)) = self.places_scrollbar_rects(size) {
            push_scrollbar(vertices, track, thumb, panel, theme.scrollbar(), size);
        }
        self.push_places_task_area(vertices, text, size, theme);
    }
}
