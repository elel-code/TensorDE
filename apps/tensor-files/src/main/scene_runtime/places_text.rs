impl ShellScene {
    fn active_place_index(&self) -> Option<usize> {
        let active_path = self
            .pane_state(self.active_pane())
            .map(ShellPaneState::display_path)
            .unwrap_or_else(|| self.panes[ShellPaneId::SLOT_0].display_path());
        active_shell_place_index(&self.places, active_path)
    }

    /// Emits Places labels without resolving icons or generating CPU quads.
    /// Both renderer paths call this exact retained layout recipe.
    fn push_places_sidebar_text(
        &self,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return;
        }
        let panel = self.places_panel_rect(size);
        let active_place = self.active_place_index();
        let padding_x = self.scale_metric(PLACES_SIDEBAR_PADDING_X);
        let section_height = self.scale_metric(PLACES_SECTION_HEIGHT);
        let row_height = self.scale_metric(PLACES_ROW_HEIGHT);
        let row_gap = self.scale_metric(PLACES_ROW_GAP);
        let icon_size = self.scale_metric(PLACES_ICON_SIZE);
        let text_height = self.text_line_height();
        let small_text_height = self.small_text_line_height();
        let mut y = panel.y
            + self.scale_metric(PLACES_SIDEBAR_TOP_PADDING)
            + self.scale_metric(PLACES_TITLE_HEIGHT)
            - self.places_scroll_y;
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
                let icon_right = row.x + self.scale_metric(8.0) + icon_size;
                text.push_label_aligned(
                    &place.label,
                    ViewRect {
                        x: icon_right + self.scale_metric(8.0),
                        y: row.y + (row.height - text_height) / 2.0,
                        width: (row.right() - icon_right - self.scale_metric(16.0)).max(1.0),
                        height: text_height,
                    },
                    panel,
                    if active_place == Some(index) {
                        theme.accent_text()
                    } else {
                        theme.primary_text()
                    },
                    LabelAlignment::Start,
                );
            }

            y += row_height + row_gap;
            previous_group = Some(place.group);
        }
    }

    fn push_places_task_area_text(
        &self,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let Some(rect) = self.places_task_area_rect(size) else {
            return;
        };
        let Some(status) = self.task_statuses.front() else {
            return;
        };
        let padding = self.scale_metric(12.0);
        let marker_gap = self.scale_metric(12.0);
        let text_x = rect.x + padding + self.scale_metric(4.0) + marker_gap;
        let text_width = (rect.right() - text_x - padding).max(1.0);
        text.push_label_aligned_no_wrap(
            "Tasks",
            ViewRect {
                x: rect.x + padding,
                y: rect.y + self.scale_metric(10.0),
                width: (rect.width - padding * 2.0).max(1.0),
                height: self.small_text_line_height(),
            },
            rect,
            theme.section_text(),
            LabelAlignment::Start,
        );
        text.push_label_aligned_no_wrap(
            &status.label,
            ViewRect {
                x: text_x,
                y: rect.y + self.scale_metric(40.0),
                width: text_width,
                height: self.text_line_height(),
            },
            rect,
            theme.primary_text(),
            LabelAlignment::Start,
        );
        text.push_label_aligned_no_wrap(
            status.detail_label().as_ref(),
            ViewRect {
                x: text_x,
                y: rect.y + self.scale_metric(66.0),
                width: text_width,
                height: self.small_text_line_height(),
            },
            rect,
            theme.muted_text(),
            LabelAlignment::Start,
        );
        let summary = format!(
            "{} | {}",
            status.kind.label(),
            count_label(self.task_statuses.len(), "task", "tasks")
        );
        text.push_label_aligned_no_wrap(
            &summary,
            ViewRect {
                x: text_x,
                y: rect.bottom() - self.scale_metric(30.0),
                width: text_width,
                height: self.small_text_line_height(),
            },
            rect,
            theme.muted_text(),
            LabelAlignment::Start,
        );
    }
}
