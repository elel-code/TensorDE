impl ShellScene {

    fn push_places_task_area(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let Some(rect) = self.places_task_area_rect(size) else {
            return;
        };
        push_status_places_task_area(
            vertices,
            text,
            PlacesTaskAreaPaint {
                rect,
                sidebar: self.places_sidebar_rect(size),
                statuses: &self.task_statuses,
                theme,
                scale: self.ui_scale(),
                small_line_height: self.small_text_line_height(),
                size,
            },
        );
    }

    fn push_filter_bar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let Some(rect) = self.filter_bar_rect(size) else {
            return;
        };
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.bottom() - 1.0,
                width: rect.width,
                height: 1.0,
            },
            theme.divider(),
            size,
        );
        let field = ViewRect {
            x: rect.x + self.scale_metric(62.0),
            y: rect.y + self.scale_metric(4.0),
            width: (rect.width - self.scale_metric(74.0)).max(1.0),
            height: (rect.height - self.scale_metric(8.0)).max(1.0),
        };
        push_clipped_rounded_rect(
            vertices,
            field,
            rect,
            self.scale_metric(7.0),
            theme.field_separator(),
            size,
        );
        if let Some(inner) = inset_rect(field, self.scale_metric(1.0)) {
            push_clipped_rounded_rect(
                vertices,
                inner,
                rect,
                self.scale_metric(6.0),
                theme.field(),
                size,
            );
        }
        self.push_filter_bar_text(text, size, theme);
    }

    fn push_filter_bar_text(
        &self,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let Some(rect) = self.filter_bar_rect(size) else {
            return;
        };
        let field = ViewRect {
            x: rect.x + self.scale_metric(62.0),
            y: rect.y + self.scale_metric(4.0),
            width: (rect.width - self.scale_metric(74.0)).max(1.0),
            height: (rect.height - self.scale_metric(8.0)).max(1.0),
        };
        text.push_label(
            "Filter:",
            ViewRect {
                x: rect.x + self.scale_metric(12.0),
                y: rect.y + self.scale_metric(6.0),
                width: self.scale_metric(54.0),
                height: self.text_line_height(),
            },
            rect,
            theme.muted_text(),
        );
        let pattern = if self.filter_pattern.is_empty() {
            ""
        } else {
            self.filter_pattern.as_str()
        };
        text.push_label(
            pattern,
            ViewRect {
                x: field.x + self.scale_metric(10.0),
                y: rect.y + self.scale_metric(6.0),
                width: (field.width - self.scale_metric(20.0)).max(1.0),
                height: self.text_line_height(),
            },
            field,
            theme.primary_text(),
        );
    }

    /// Native Vulkan counterpart of the texture-free filter field. Text is
    /// intentionally left to the text stage; the field itself is analytic.
    fn push_native_filter_bar_chrome(
        &self,
        instances: &mut Vec<crate::vulkan_rect::VulkanRectInstance>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let Some(rect) = self.filter_bar_rect(size) else {
            return;
        };
        push_native_rect_fill(
            instances,
            ViewRect {
                x: rect.x,
                y: rect.bottom() - 1.0,
                width: rect.width,
                height: 1.0,
            },
            rect,
            theme.divider(),
            size,
        );
        let field = ViewRect {
            x: rect.x + self.scale_metric(62.0),
            y: rect.y + self.scale_metric(4.0),
            width: (rect.width - self.scale_metric(74.0)).max(1.0),
            height: (rect.height - self.scale_metric(8.0)).max(1.0),
        };
        push_native_rounded_rect_fill(
            instances,
            field,
            rect,
            self.scale_metric(7.0),
            theme.field_separator(),
            size,
        );
        if let Some(inner) = inset_rect(field, self.scale_metric(1.0)) {
            push_native_rounded_rect_fill(
                instances,
                inner,
                rect,
                self.scale_metric(6.0),
                theme.field(),
                size,
            );
        }
    }

    fn push_rubber_band_for_projection(
        &self,
        vertices: &mut Vec<QuadVertex>,
        projection: &ShellPaneProjection<'_>,
        theme: ShellTheme,
        size: PhysicalSize<u32>,
    ) {
        let Some(rect) = self.rubber_band_screen_rect_for_projection(projection) else {
            return;
        };
        let content_clip = projection.geometry.content;
        let rubber_band = theme.rubber_band();
        push_clipped_rect(vertices, rect, content_clip, rubber_band.fill, size);
        push_clipped_rect_outline(vertices, rect, content_clip, 1.5, rubber_band.border, size);
    }

    fn push_native_rubber_band_for_projection(
        &self,
        instances: &mut Vec<VulkanRectInstance>,
        projection: &ShellPaneProjection<'_>,
        theme: ShellTheme,
        size: PhysicalSize<u32>,
    ) {
        let Some(rect) = self.rubber_band_screen_rect_for_projection(projection) else {
            return;
        };
        let rubber_band = theme.rubber_band();
        if let Some(instance) = VulkanRectInstance::fill(
            rect,
            projection.geometry.content,
            0.0,
            rubber_band.fill,
            size,
        ) {
            instances.push(instance);
        }
        if let Some(instance) = VulkanRectInstance::outline(
            rect,
            projection.geometry.content,
            0.0,
            1.5,
            rubber_band.border,
            size,
        ) {
            instances.push(instance);
        }
    }

    fn rubber_band_screen_rect_for_projection(
        &self,
        projection: &ShellPaneProjection<'_>,
    ) -> Option<ViewRect> {
        self.rubber_band
            .as_ref()
            .and_then(RubberBand::active_rect)
            .map(|rect| pane_content_rect_to_screen(rect, projection))
    }

    fn pane_status(&self, pane: ShellPaneView<'_>, visible_items: usize) -> ShellPaneStatus {
        ShellPaneStatus::for_view(
            pane,
            visible_items,
            self.show_hidden,
            self.filter_active || !self.filter_pattern.is_empty(),
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn pane_status_text(&self, pane: ShellPaneView<'_>, visible_items: usize) -> String {
        self.pane_status(pane, visible_items).plain_text()
    }

    fn active_pane_status_summary(&self) -> String {
        let pane = self.active_pane();
        let Some(state) = self.pane_state(pane) else {
            return "No active pane".to_string();
        };
        if state.selection.len() > 0 {
            count_label(state.selection.len(), "item selected", "items selected")
        } else {
            count_label(state.entries.len(), "item", "items")
        }
    }

    fn active_pane_path_label(&self) -> String {
        self.pane_state(self.active_pane())
            .map(|state| state.path.display().to_string())
            .unwrap_or_else(|| "No active pane".to_string())
    }

    fn push_pane_body_border(
        &self,
        vertices: &mut Vec<QuadVertex>,
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
        push_rect(
            vertices,
            ViewRect {
                x: body.x,
                y: body.y,
                width: body.width,
                height: 1.0,
            },
            theme.divider(),
            size,
        );
    }

    fn push_content_scrollbar_for_projection(
        &self,
        vertices: &mut Vec<QuadVertex>,
        projection: &ShellPaneProjection<'_>,
        theme: ShellTheme,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some((track, thumb)) = self.content_scrollbar_rects_for_projection(projection) else {
            return false;
        };
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_scrollbar(vertices, track, thumb, screen, theme.scrollbar(), size);
        true
    }

    /// Native Vulkan counterpart of [`Self::push_content_scrollbar_for_projection`].
    ///
    /// Scrollbar capsules are analytic rectangles, so resizing or scrolling a
    /// large directory updates two compact instances instead of CPU-tessellated
    /// rounded geometry.
    fn push_native_content_scrollbar_for_projection(
        &self,
        instances: &mut Vec<crate::vulkan_rect::VulkanRectInstance>,
        projection: &ShellPaneProjection<'_>,
        theme: ShellTheme,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some((track, thumb)) = self.content_scrollbar_rects_for_projection(projection) else {
            return false;
        };
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        let colors = theme.scrollbar();
        for (rect, color) in [(track, colors.track), (thumb, colors.thumb)] {
            let radius = rect.width.min(rect.height) * 0.5;
            if let Some(instance) = crate::vulkan_rect::VulkanRectInstance::fill(
                rect, screen, radius, color, size,
            ) {
                instances.push(instance);
            }
        }
        true
    }

    fn push_drop_menu_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        theme: ShellTheme,
        size: PhysicalSize<u32>,
    ) {
        if let Some(menu) = self.drop_menu.as_ref() {
            shell::context_menu::paint::push_drop_menu_overlay(
                menu,
                theme,
                self.ui_scale(),
                vertices,
                text,
                size,
            );
        }
    }

    fn push_context_menu_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        theme: ShellTheme,
        size: PhysicalSize<u32>,
    ) {
        if let Some(menu) = self.context_menu.as_ref() {
            shell::context_menu::paint::push_context_menu_overlay(
                menu,
                vertices,
                text,
                icons,
                shell::context_menu::paint::ContextMenuOverlayConfig {
                    show_hidden: self.show_hidden,
                    theme,
                    scale: self.ui_scale(),
                    size,
                },
            );
        }
    }

    fn content_origin_x(&self, size: PhysicalSize<u32>) -> f32 {
        let sidebar_width = self.places_sidebar_width(size);
        if sidebar_width <= 0.0 {
            0.0
        } else {
            sidebar_width
                + self.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH)
                + self.scale_metric(PLACES_TO_PANE_GAP)
        }
    }

    fn content_origin_y(&self) -> f32 {
        self.details_header_y()
            + if self.panes[ShellPaneId::SLOT_0].view_mode == ShellViewMode::Details {
                self.details_header_height()
            } else {
                0.0
            }
    }
}

pub(crate) fn push_trash_conflict_dialog_surface(
    dialog: &ShellTrashConflictDialog,
    popup_theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let rect = trash_conflict_dialog_window_rect(size);
    let title_height = scaled_dialog_metric(TRASH_CONFLICT_DIALOG_TITLE_HEIGHT, scale);
    let margin = scaled_dialog_metric(16.0, scale);
    push_clipped_rounded_rect(
        vertices,
        rect,
        rect,
        scaled_dialog_metric(8.0, scale),
        popup_theme.surface,
        size,
    );
    push_clipped_rect_outline(
        vertices,
        rect,
        rect,
        1.0,
        popup_theme.button_warning,
        size,
    );
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: title_height,
        },
        popup_theme.warning_header,
        size,
    );
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y + title_height - scaled_dialog_metric(1.0, scale).max(1.0),
            width: rect.width,
            height: scaled_dialog_metric(1.0, scale).max(1.0),
        },
        popup_theme.warning_divider,
        size,
    );
    text.push_label(
        "Restore Conflict",
        ViewRect {
            x: rect.x + margin,
            y: rect.y + scaled_dialog_metric(12.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        popup_theme.warning_text,
    );

    let count = dialog.conflicts.len();
    text.push_label(
        &format!("{count} item(s) already exist at the original location."),
        ViewRect {
            x: rect.x + margin,
            y: rect.y + title_height + scaled_dialog_metric(18.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        popup_theme.body_text,
    );
    if let Some(conflict) = dialog.first_conflict() {
        text.push_label(
            &format!("Original: {}", conflict.original_path.display()),
            ViewRect {
                x: rect.x + margin,
                y: rect.y + title_height + scaled_dialog_metric(48.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            popup_theme.soft_text,
        );
        text.push_label(
            &format!("Trash: {}", conflict.trash_path.display()),
            ViewRect {
                x: rect.x + margin,
                y: rect.y + title_height + scaled_dialog_metric(76.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            popup_theme.soft_text,
        );
    }

    let cancel = trash_conflict_dialog_cancel_button_rect_scaled(rect, scale);
    let replace = trash_conflict_dialog_replace_button_rect_scaled(rect, scale);
    for (label, button, active) in [("Cancel", cancel, false), ("Replace", replace, true)] {
        push_clipped_rounded_rect(
            vertices,
            button,
            rect,
            scaled_dialog_metric(5.0, scale),
            if active {
                popup_theme.button_warning
            } else {
                popup_theme.button_secondary
            },
            size,
        );
        push_clipped_rect_outline(vertices, button, rect, 1.0, popup_theme.border, size);
        text.push_label_aligned(
            label,
            ViewRect {
                x: button.x + scaled_dialog_metric(10.0, scale),
                y: button.y + scaled_dialog_metric(4.0, scale),
                width: (button.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            if active {
                popup_theme.inverse_text
            } else {
                popup_theme.body_text
            },
            LabelAlignment::Center,
        );
    }
}
