impl ShellScene {

    fn set_rename_dialog_error(&mut self, error: String) -> bool {
        let Some(dialog) = self.rename_dialog.as_mut() else {
            fika_log!("[fika-wgpu] rename-error {error}");
            return false;
        };
        let already = dialog.error.as_ref() == Some(&error) && !dialog.busy;
        if already {
            return false;
        }
        dialog.error = Some(error);
        dialog.busy = false;
        dialog.replace_on_insert = false;
        self.rename_changes += 1;
        self.log_rename_dialog_state();
        true
    }

    fn set_rename_dialog_busy(&mut self, busy: bool) -> bool {
        let Some(dialog) = self.rename_dialog.as_mut() else {
            return false;
        };
        if dialog.busy == busy {
            return false;
        }
        dialog.busy = busy;
        if busy {
            dialog.error = None;
        }
        self.rename_changes += 1;
        true
    }

    fn close_rename_dialog(&mut self) -> bool {
        if self.rename_dialog.take().is_none() {
            return false;
        }
        self.rename_changes += 1;
        fika_log!("[fika-wgpu] rename open=0 changes={}", self.rename_changes);
        true
    }

    fn close_rename_dialog_after_success(&mut self, request: &RenameEntryRequest) -> bool {
        if self.rename_dialog.take().is_none() {
            return false;
        }
        self.rename_changes += 1;
        fika_log!(
            "[fika-wgpu] rename source={} target={} dir={} privileged={} changes={}",
            request.source.display(),
            request.target.display(),
            request.is_dir as u8,
            request.privileged as u8,
            self.rename_changes
        );
        true
    }
    fn rename_dialog_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> RenameDialogClick {
        let Some(dialog) = self.rename_dialog.as_ref() else {
            return RenameDialogClick::Outside;
        };
        let scale = self.ui_scale();
        let rect = rename_dialog_rect_scaled(dialog, size, scale);
        if !rect.contains(point) {
            return RenameDialogClick::Outside;
        }
        if rename_dialog_cancel_button_rect_scaled(rect, scale).contains(point) {
            return RenameDialogClick::Cancel;
        }
        if rename_dialog_commit_button_rect_scaled(rect, scale).contains(point) {
            return RenameDialogClick::Commit;
        }
        RenameDialogClick::Inside
    }

    fn log_rename_dialog_state(&self) {
        match self.rename_dialog.as_ref() {
            Some(dialog) => fika_log!(
                "[fika-wgpu] rename open=1 source={} name={:?} privileged={} error={:?} changes={}",
                dialog.source.display(),
                dialog.name,
                dialog.privileged as u8,
                dialog.error,
                self.rename_changes
            ),
            None => fika_log!("[fika-wgpu] rename open=0 changes={}", self.rename_changes),
        }
    }

    fn properties_overlay_for_context_target(&self) -> Option<ShellPropertiesOverlay> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Item {
                pane,
                index,
                path,
                is_dir,
                selection_count,
                ..
            } => {
                let entry = self.pane_state(*pane)?.entries.get(*index)?;
                let title_name = entry.name.as_ref().to_string();
                let location = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .map(|parent| parent.display().to_string())
                    .unwrap_or_else(|| "-".to_string());
                let mut rows = vec![
                    property_row("Name", title_name.clone()),
                    property_row("Type", if *is_dir { "Folder" } else { "File" }.to_string()),
                    property_row("Location", location),
                    property_row(
                        "Size",
                        if *is_dir {
                            "-".to_string()
                        } else {
                            format_size(entry.size_bytes)
                        },
                    ),
                    property_row("Modified", format_modified_secs(entry.modified_secs)),
                    property_row("Path", path.display().to_string()),
                ];
                if *selection_count > 1 {
                    rows.push(property_row(
                        "Selection",
                        format!("{selection_count} items"),
                    ));
                }
                if let Some(mime) = entry.mime_type.as_ref() {
                    rows.push(property_row("MIME", mime.to_string()));
                }
                Some(ShellPropertiesOverlay {
                    title: format!("Properties - {title_name}"),
                    rows,
                })
            }
            ShellContextTarget::Blank { pane, path, .. } => {
                let pane = self.pane_state(*pane)?;
                Some(ShellPropertiesOverlay {
                    title: format!("Properties - {}", path.display()),
                    rows: vec![
                        property_row("Name", path_name_or_display(path)),
                        property_row("Type", "Folder".to_string()),
                        property_row("Entries", pane.entries.len().to_string()),
                        property_row("Folders", pane.dir_count.to_string()),
                        property_row(
                            "Files",
                            pane.entries
                                .len()
                                .saturating_sub(pane.dir_count)
                                .to_string(),
                        ),
                        property_row("Path", path.display().to_string()),
                    ],
                })
            }
            ShellContextTarget::Place {
                label,
                path,
                group,
                network,
                trash,
                root,
                editable,
                ..
            } => Some(ShellPropertiesOverlay {
                title: format!("Properties - {label}"),
                rows: vec![
                    property_row("Name", label.clone()),
                    property_row("Type", "Place".to_string()),
                    property_row(
                        "Section",
                        if group.is_empty() {
                            "Places".to_string()
                        } else {
                            (*group).to_string()
                        },
                    ),
                    property_row("Path", path.display().to_string()),
                    property_row("Network", yes_no(*network)),
                    property_row("Trash", yes_no(*trash)),
                    property_row("Root", yes_no(*root)),
                    property_row("Editable", yes_no(*editable)),
                ],
            }),
        }
    }

    fn parent_directory_path_for_pane(&self, pane: ShellPaneId) -> Option<PathBuf> {
        let path = &self.pane_state(pane)?.path;
        if is_network_path(path) {
            network_parent_path(path)
        } else {
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(Path::to_path_buf)
        }
    }

    fn directory_path_for_pane_index(&self, pane_id: ShellPaneId, index: usize) -> Option<PathBuf> {
        let pane = self.pane_view(pane_id)?;
        let entry = pane.entries.get(index)?;
        entry
            .is_dir
            .then(|| self.entry_path_for_pane_view(pane, index))
            .flatten()
    }

    fn entry_path_for_pane_view(&self, pane: ShellPaneView<'_>, index: usize) -> Option<PathBuf> {
        let entry = pane.entries.get(index)?;
        Some(
            entry
                .target_path
                .clone()
                .unwrap_or_else(|| pane.path.join(entry.name.as_ref())),
        )
    }

    fn pane_state(&self, kind: ShellPaneId) -> Option<&ShellPaneState> {
        match kind {
            ShellPaneId::SLOT_0 => Some(&self.panes[ShellPaneId::SLOT_0]),
            ShellPaneId::SLOT_1 => self.panes.get(ShellPaneId::SLOT_1),
        }
    }

    fn pane_state_mut(&mut self, kind: ShellPaneId) -> Option<&mut ShellPaneState> {
        match kind {
            ShellPaneId::SLOT_0 => Some(&mut self.panes[ShellPaneId::SLOT_0]),
            ShellPaneId::SLOT_1 => self.panes.get_mut(ShellPaneId::SLOT_1),
        }
    }

    fn pane_selection(&self, kind: ShellPaneId) -> Option<&ShellSelection> {
        self.pane_state(kind).map(|pane| &pane.selection)
    }

    fn pane_selection_mut(&mut self, kind: ShellPaneId) -> Option<&mut ShellSelection> {
        self.pane_state_mut(kind).map(|pane| &mut pane.selection)
    }

    fn pane_history(&self, kind: ShellPaneId) -> &PathHistory {
        self.histories.get(self.normalized_pane_id(kind))
    }

    fn pane_history_mut(&mut self, kind: ShellPaneId) -> &mut PathHistory {
        let kind = self.normalized_pane_id(kind);
        self.histories.get_mut(kind)
    }

    fn filter_pattern_for_pane(&self, _kind: ShellPaneId) -> &str {
        &self.filter_pattern
    }

    fn active_selection_len(&self) -> usize {
        self.pane_selection(self.active_pane())
            .map(ShellSelection::len)
            .unwrap_or(0)
    }

    fn normalized_pane_id(&self, kind: ShellPaneId) -> ShellPaneId {
        match kind {
            ShellPaneId::SLOT_0 => ShellPaneId::SLOT_0,
            ShellPaneId::SLOT_1 if self.panes.is_open(ShellPaneId::SLOT_1) => ShellPaneId::SLOT_1,
            ShellPaneId::SLOT_1 => ShellPaneId::SLOT_0,
        }
    }

    fn active_pane(&self) -> ShellPaneId {
        self.normalized_pane_id(self.active_pane)
    }

    fn active_view_mode(&self) -> ShellViewMode {
        self.pane_state(self.active_pane())
            .map(|pane| pane.view_mode)
            .unwrap_or(ShellViewMode::Icons)
    }

    fn focus_pane_at_screen_point(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let Some(kind) = self.pane_id_at_screen_point(point, size) else {
            return false;
        };
        let kind = self.normalized_pane_id(kind);
        let old = self.active_pane();
        self.active_pane = kind;
        old != kind
    }

    fn item_activation_for_press(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        now: Instant,
    ) -> Option<ShellItemActivation> {
        let Some(target) = self.pane_item_at_screen_point(point, size) else {
            self.last_item_click = None;
            return None;
        };

        let double_click = self.last_item_click.is_some_and(|click| {
            click.pane == target.pane
                && click.index == target.index
                && now.duration_since(click.time) <= DOUBLE_CLICK_MAX_INTERVAL
                && point_distance(click.point, point) <= DOUBLE_CLICK_MAX_DISTANCE
        });
        self.last_item_click = Some(PaneClick {
            pane: target.pane,
            index: target.index,
            point,
            time: now,
        });

        if !double_click {
            return None;
        }

        let view = self.pane_view(target.pane)?;
        let entry = view.entries.get(target.index)?;
        let path = self.entry_path_for_pane_view(view, target.index)?;
        if entry.is_dir {
            Some(ShellItemActivation::Directory {
                pane: target.pane,
                path,
            })
        } else {
            Some(ShellItemActivation::File(OpenFileRequest::from_path(
                path,
                entry.mime_type.as_deref(),
            )))
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn pane_view(&self, kind: ShellPaneId) -> Option<ShellPaneView<'_>> {
        self.pane_state(kind).map(ShellPaneView::from_state)
    }

    fn pane_rect_for_id(&self, kind: ShellPaneId, size: PhysicalSize<u32>) -> Option<ViewRect> {
        self.pane_state(kind)?;
        match kind {
            ShellPaneId::SLOT_0 => Some(self.pane_rect(size)),
            ShellPaneId::SLOT_1 => self
                .split_pane_metrics(size)
                .map(|metrics| metrics.right_pane),
        }
    }

    fn pane_filter_bar_height(&self, kind: ShellPaneId) -> f32 {
        if kind == ShellPaneId::SLOT_0 {
            self.filter_bar_height()
        } else {
            0.0
        }
    }

    fn pane_geometry(
        &self,
        kind: ShellPaneId,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPaneGeometry> {
        let pane_state = self.pane_state(kind)?;
        let pane = self.pane_rect_for_id(kind, size)?;
        let status_height = self.status_bar_height().min(pane.height);
        let status_bar = ViewRect {
            x: pane.x,
            y: pane.bottom() - status_height,
            width: pane.width,
            height: status_height,
        };
        let content_y = pane.y
            + self.top_bar_height()
            + self.pane_filter_bar_height(kind)
            + if pane_state.view_mode == ShellViewMode::Details {
                self.details_header_height()
            } else {
                0.0
            };
        let reserved_right = if scrollbar_axis_for_view_mode(pane_state.view_mode)
            == ContentScrollbarAxis::Vertical
        {
            self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT)
        } else {
            0.0
        };
        let reserved_bottom = if scrollbar_axis_for_view_mode(pane_state.view_mode)
            == ContentScrollbarAxis::Horizontal
        {
            self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT)
        } else {
            0.0
        };
        Some(ShellPaneGeometry {
            kind,
            pane,
            top_bar: ViewRect {
                x: pane.x,
                y: pane.y,
                width: pane.width,
                height: self.top_bar_height(),
            },
            content: ViewRect {
                x: pane.x,
                y: content_y,
                width: (pane.width - reserved_right).max(1.0),
                height: (status_bar.y - content_y - reserved_bottom).max(1.0),
            },
            status_bar,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn pane_geometries(&self, size: PhysicalSize<u32>) -> Vec<ShellPaneGeometry> {
        ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| self.pane_geometry(kind, size))
            .collect()
    }

    fn pane_projection(
        &self,
        kind: ShellPaneId,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPaneProjection<'_>> {
        let prepared = self.pane_projection_layout(kind, size)?;
        self.pane_projection_from_prepared(prepared)
    }

    pub(crate) fn prepare_frame_projection_layouts(
        &self,
        size: PhysicalSize<u32>,
    ) -> ShellPreparedFrameProjectionLayouts {
        let projection_layout_start = Instant::now();
        let layouts = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| self.pane_projection_layout(kind, size))
            .collect();
        ShellPreparedFrameProjectionLayouts {
            layouts,
            layout_us: projection_layout_start.elapsed().as_micros(),
        }
    }

    fn pane_projection_layout(
        &self,
        kind: ShellPaneId,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPreparedPaneProjection> {
        let view = self.pane_view(kind)?;
        let geometry = self.pane_geometry(kind, size)?;
        let layout = self.pane_layout_for_pane(
            geometry.kind,
            view,
            geometry.content.width,
            geometry.content.height,
        );
        let mut visible_items = Vec::with_capacity(layout.visible_item_count());
        layout.for_each_visible_item(|layout| {
            let path = view
                .filtered_indexes
                .get(layout.model_index)
                .and_then(|entry_index| self.entry_path_for_pane_view(view, *entry_index));
            visible_items.push(ShellPreparedPaneVisibleItem {
                layout,
                path,
                slot_id: 0,
            });
        });
        let scroll_metrics = ShellPaneScrollMetrics::new(layout.content_size(), geometry.content);
        Some(ShellPreparedPaneProjection {
            geometry,
            visible_items,
            scroll_metrics,
        })
    }
}
