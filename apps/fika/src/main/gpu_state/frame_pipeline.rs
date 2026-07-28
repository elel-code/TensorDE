#[derive(Clone, Copy)]
struct DialogRenderViewport {
    popup_theme: PopupTheme,
    scale: f32,
    layout_size: PhysicalSize<u32>,
}

struct DetachedDialogRenderRequest<'a> {
    window: &'a Window,
    event_loop: &'a ActiveEventLoop,
    viewport: DialogRenderViewport,
    reason: &'static str,
    dialog_label: &'static str,
}

impl WgpuState {

    fn prewarm_text_labels(
        &mut self,
        scene: &ShellScene,
        projections: &[ShellPaneProjection<'_>],
        mode: TextLabelPrewarmMode,
    ) -> TextLabelPrewarmStats {
        self.text_renderer.label_cache.begin_frame();
        self.text_renderer.metrics_cache.begin_frame();
        let mut text_builder = TextFrameBuilder::new(
            TextFrameResources::from_renderer(&mut self.text_renderer),
            self.size,
            scene.ui_scale(),
            Vec::new(),
        );
        text_builder.set_raster_miss_budget(text_label_raster_miss_budget_for_mode(mode));
        scene.prewarm_file_item_text_labels(projections, &mut text_builder, mode)
    }

    fn render_detached_dialog(
        &mut self,
        request: DetachedDialogRenderRequest<'_>,
        paint: impl FnOnce(
            &mut Vec<QuadVertex>,
            &mut TextFrameBuilder<'_>,
            &mut IconFrameBuilder<'_>,
            PhysicalSize<u32>,
        ),
    ) -> ShellRenderOutcome {
        let DetachedDialogRenderRequest {
            window,
            event_loop,
            viewport:
                DialogRenderViewport {
                    popup_theme,
                    scale,
                    layout_size,
                },
            reason,
            dialog_label,
        } = request;
        let Some(frame) = self.acquire_surface_frame(
            window,
            reason,
            ShellSurfaceFrameContext::DetachedDialog { dialog_label },
        ) else {
            return ShellRenderOutcome::NotReady;
        };

        let dialog_frame = prepare_dialog_frame(
            DialogFrameRenderers {
                text: &mut self.text_renderer,
                icons: &mut self.icon_renderer,
                quads: &mut self.quad_renderer,
            },
            FrameGpuContext {
                device: &self.device,
                queue: &self.queue,
            },
            DialogFrameRequest {
                layout_size,
                scale,
                reason,
            },
            paint,
        );
        if dialog_frame.work_pending() {
            window.request_redraw();
        }

        let (view, mut encoder) =
            self.begin_surface_frame_encoding(&frame, "fika-wgpu-detached-dialog-frame");
        self.encode_detached_dialog_pass(&mut encoder, &view, popup_theme);
        let presented_frame = self.submit_surface_frame(window, event_loop, frame, encoder);
        if presented_frame == 1 || fika_frame_log_all_enabled() {
            fika_log!(
                "[fika-wgpu] detached-dialog kind={} frame={} reason={} size={}x{} scale={:.2} icons={} icon_deferred={} text_labels={} text_deferred={} swash={}/{} reset={} vertex_uploads={}/{}",
                dialog_label,
                presented_frame,
                reason,
                self.size.width,
                self.size.height,
                scale,
                dialog_frame.icon_stats.icons,
                dialog_frame.icon_stats.deferred,
                dialog_frame.text_stats.labels,
                dialog_frame.text_stats.deferred,
                dialog_frame.swash_image_entries,
                dialog_frame.swash_outline_entries,
                dialog_frame.swash_reset as u8,
                dialog_frame.vertex_upload_stats.writes,
                dialog_frame.vertex_upload_stats.skips,
            );
        }

        ShellRenderOutcome::Presented
    }

    fn render_open_with_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        chooser: &ShellOpenWithChooser,
        viewport: DialogRenderViewport,
        caret_visible: bool,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let DialogRenderViewport {
            popup_theme,
            scale,
            layout_size: _,
        } = viewport;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: ShellDialogWindowKind::OpenWith.as_str(),
            },
            |vertices, text_builder, icon_builder, size| {
                ui::open_with::paint::push_open_with_chooser_dialog(
                    chooser,
                    vertices,
                    text_builder,
                    icon_builder,
                    ui::open_with::paint::OpenWithDialogPaintConfig {
                        theme: popup_theme,
                        scale,
                        caret_visible,
                        size,
                    },
                );
            },
        )
    }

    fn render_settings_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        state: ShellSettingsDialogState,
        snapshot: ShellSettingsSnapshot,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let DialogRenderViewport {
            popup_theme,
            scale,
            layout_size: _,
        } = viewport;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: ShellDialogWindowKind::Settings.as_str(),
            },
            |vertices, text_builder, _icon_builder, size| {
                ui::settings::paint::push_settings_dialog(
                    &state,
                    snapshot,
                    popup_theme,
                    scale,
                    vertices,
                    text_builder,
                    size,
                );
            },
        )
    }

    fn render_create_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        dialog: &ShellCreateDialog,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let DialogRenderViewport {
            popup_theme,
            scale,
            layout_size: _,
        } = viewport;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: ShellDialogWindowKind::Create.as_str(),
            },
            |vertices, text_builder, _icon_builder, size| {
                ui::create_rename::paint::push_create_dialog(
                    dialog,
                    popup_theme,
                    scale,
                    vertices,
                    text_builder,
                    size,
                );
            },
        )
    }

    fn render_rename_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        dialog: &ShellRenameDialog,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let DialogRenderViewport {
            popup_theme,
            scale,
            layout_size: _,
        } = viewport;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: ShellDialogWindowKind::Rename.as_str(),
            },
            |vertices, text_builder, _icon_builder, size| {
                ui::create_rename::paint::push_rename_dialog(
                    dialog,
                    popup_theme,
                    scale,
                    vertices,
                    text_builder,
                    size,
                );
            },
        )
    }

    fn render_properties_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        overlay: &ShellPropertiesOverlay,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let DialogRenderViewport {
            popup_theme,
            scale,
            layout_size: _,
        } = viewport;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: ShellDialogWindowKind::Properties.as_str(),
            },
            |vertices, text_builder, _icon_builder, size| {
                ui::properties::paint::push_properties_dialog(
                    overlay,
                    popup_theme,
                    scale,
                    vertices,
                    text_builder,
                    size,
                );
            },
        )
    }

    fn render_task_detail_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        statuses: &ShellTaskStatusStore,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let DialogRenderViewport {
            popup_theme,
            scale,
            layout_size: _,
        } = viewport;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: ShellDialogWindowKind::TaskDetail.as_str(),
            },
            |vertices, text_builder, _icon_builder, size| {
                ui::tasks::paint::push_task_detail_dialog(
                    statuses,
                    popup_theme,
                    scale,
                    vertices,
                    text_builder,
                    size,
                );
            },
        )
    }

    fn render_trash_conflict_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        dialog: &ShellTrashConflictDialog,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let DialogRenderViewport {
            popup_theme,
            scale,
            layout_size: _,
        } = viewport;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: ShellDialogWindowKind::TrashConflict.as_str(),
            },
            |vertices, text_builder, _icon_builder, size| {
                push_trash_conflict_dialog_surface(
                    dialog,
                    popup_theme,
                    scale,
                    vertices,
                    text_builder,
                    size,
                );
            },
        )
    }

    fn render(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        scene: &mut ShellScene,
        reason: &'static str,
        force_log: bool,
    ) -> ShellRenderOutcome {
        let icon_work_reason = icon_work_reason_for_frame(reason, self.frame_count);
        let mut metadata_result_stats = scene.drain_metadata_role_results();
        let mut projection_layouts = scene.prepare_frame_projection_layouts(self.size);
        scene.update_visible_slot_pools_for_projection_layouts(&mut projection_layouts);
        let mut frame_projections = scene.pane_projections_from_layouts(projection_layouts);
        let metadata_sync_stats = if self.frame_count == 0 {
            let (mut stats, results) = scene.resolve_visible_metadata_roles_synchronously(
                frame_projections.projections(),
            );
            drop(frame_projections);
            stats.applied = scene.apply_synchronous_metadata_role_results(results);
            metadata_result_stats.results += stats.resolved;
            metadata_result_stats.visible_results += stats.resolved;
            metadata_result_stats.applied += stats.applied;
            metadata_result_stats.visible_applied += stats.applied;

            let mut projection_layouts = scene.prepare_frame_projection_layouts(self.size);
            scene.update_visible_slot_pools_for_projection_layouts(&mut projection_layouts);
            frame_projections = scene.pane_projections_from_layouts(projection_layouts);
            Some(stats)
        } else {
            None
        };
        if let Some(stats) = metadata_sync_stats.as_ref() {
            fika_log!(
                "[fika-wgpu] visible-metadata-sync reason={} visible={} resolved={} deferred={} applied={} resolve={}us budget={}us over_budget={}",
                reason,
                stats.visible,
                stats.resolved,
                stats.deferred,
                stats.applied,
                stats.resolve_us,
                FILE_MANAGER_MAX_BLOCK_TIMEOUT.as_micros(),
                stats.over_budget as u8,
            );
        }
        let _folder_preview_role_stats =
            scene.update_folder_preview_roles_for_projections(frame_projections.projections());
        let folder_preview_results = scene.drain_folder_preview_role_results();
        let (icon_resolve_results, _icon_resolve_deferred_results) = self
            .icon_renderer
            .resolver
            .drain_results_by_priority();
        let (thumbnail_results, _thumbnail_deferred_results) = self
            .icon_renderer
            .thumbnails
            .drain_results_by_priority();
        let folder_preview_damage_rects = folder_preview_damage_rects_for_changes(
            scene,
            frame_projections.projections(),
            &folder_preview_results.changes,
        );
        let dirty_key_context = ShellRenderDirtyKeyContext::from_scene(
            scene,
            frame_projections.projections(),
        );
        let dirty_key =
            ShellRenderDirtyKey::from_scene_with_context(scene, self.size, &dirty_key_context);
        let non_folder_preview_async_results_changed = metadata_result_stats.visible_applied > 0
            || icon_resolve_results > 0
            || thumbnail_results > 0;
        let visible_folder_preview_async_results_changed =
            folder_preview_results.applied > 0 && !folder_preview_damage_rects.is_empty();
        let async_results_changed = non_folder_preview_async_results_changed
            || visible_folder_preview_async_results_changed;
        self.frame_latency
            .observe_scene_counters(frame_latency_counters_for_scene(scene), self.frame_count);
        self.frame_latency.observe_async_results(
            ShellFrameLatencyAsyncResults {
                metadata_applied: metadata_result_stats.visible_applied as u64,
                icon_resolve_results: icon_resolve_results as u64,
                thumbnail_results: thumbnail_results as u64,
                folder_preview_results: if visible_folder_preview_async_results_changed {
                    folder_preview_results.applied as u64
                } else {
                    0
                },
            },
            self.frame_count,
        );
        if self.can_skip_clean_redraw(
            reason,
            force_log,
            &dirty_key,
            async_results_changed,
        ) {
            self.clean_redraw_skips += 1;
            if fika_frame_log_all_enabled() || self.last_log.elapsed() >= Duration::from_secs(1) {
                fika_log!(
                    "[fika-wgpu] dirty-render-skip reason={} skips={} work_pending={} async_results={} key_values={}",
                    reason,
                    self.clean_redraw_skips,
                    self.render_work_pending as u8,
                    async_results_changed as u8,
                    dirty_key.values.len()
                );
                self.last_log = Instant::now();
            }
            return ShellRenderOutcome::SkippedClean;
        }
        let damage_snapshot = ShellRenderDamageSnapshot::from_scene_with_dirty_key_context(
            scene,
            self.size,
            frame_projections.projections(),
            dirty_key,
            &dirty_key_context,
        );
        let mut render_damage = ShellRenderDamage::between_with_async_damage(
            self.last_render_damage_snapshot.as_ref(),
            &damage_snapshot,
            non_folder_preview_async_results_changed,
            folder_preview_damage_rects,
        );
        if !self.retained_scene.is_valid() {
            render_damage = ShellRenderDamage::full(self.size);
        }
        let render_damage_scissor = render_damage.scissor_rect(self.size);
        if render_damage.kind == ShellRenderDamageKind::Bounded && render_damage_scissor.is_none() {
            render_damage = ShellRenderDamage::full(self.size);
        }
        let metadata_role_stats =
            scene.prewarm_file_metadata_roles(frame_projections.projections());
        if metadata_role_stats.visible
            + metadata_role_stats.deferred
            + metadata_result_stats.results
            > 0
            && (self.frame_count == 0 || force_log || fika_frame_log_all_enabled())
        {
            fika_log!(
                "[fika-wgpu] prewarm-metadata reason={} view={} visible={} deferred={} batches={} results={} applied={}",
                reason,
                scene.panes[ShellPaneId::SLOT_0].view_mode.as_str(),
                metadata_role_stats.visible,
                metadata_role_stats.deferred,
                metadata_role_stats.batches_started,
                metadata_result_stats.results,
                metadata_result_stats.applied,
            );
        }
        let text_prewarm_stats = self.prewarm_text_labels(
            scene,
            frame_projections.projections(),
            text_label_prewarm_mode_for_frame(reason),
        );
        if text_prewarm_stats.entries + text_prewarm_stats.read_ahead > 0
            && (self.frame_count == 0
                || force_log
                || fika_frame_log_all_enabled()
                || text_prewarm_stats.raster_us >= 1000)
        {
            fika_log!(
                "[fika-wgpu] prewarm-text reason={} view={} entries={} read_ahead={} hits={} misses={} deferred={} raster={}us budget={}us over_budget={}",
                reason,
                scene.panes[ShellPaneId::SLOT_0].view_mode.as_str(),
                text_prewarm_stats.entries,
                text_prewarm_stats.read_ahead,
                text_prewarm_stats.cache_hits,
                text_prewarm_stats.cache_misses,
                text_prewarm_stats.deferred,
                text_prewarm_stats.raster_us,
                VISIBLE_TEXT_LABEL_PREWARM_BUDGET.as_micros(),
                text_prewarm_stats.over_budget as u8
            );
        }

        let prewarm_start = Instant::now();
        let prewarm_stats = scene.prewarm_visible_file_icon_roles(
            frame_projections.projections(),
            &mut self.icon_renderer.resolver,
            icon_work_reason,
        );
        let prewarm_us = prewarm_start.elapsed().as_micros();
        if prewarm_stats.entries > 0
            && (self.frame_count == 0
                || force_log
                || fika_frame_log_all_enabled()
                || prewarm_stats.resolve_us >= 1000)
        {
            fika_log!(
                "[fika-wgpu] prewarm-icons reason={} view={} entries={} deferred={} read_ahead={} resolve={}us budget={}us over_budget={}",
                reason,
                scene.panes[ShellPaneId::SLOT_0].view_mode.as_str(),
                prewarm_stats.entries,
                prewarm_stats.deferred,
                prewarm_stats.read_ahead,
                prewarm_stats.resolve_us,
                icon_role_prewarm_budget_for_frame(icon_work_reason).as_micros(),
                prewarm_stats.over_budget as u8
            );
        }
        let frame_start = Instant::now();
        let surface_acquire_start = Instant::now();
        let view_label = scene.panes[ShellPaneId::SLOT_0].view_mode.as_str();
        let Some(frame) = self.acquire_surface_frame(
            window,
            reason,
            ShellSurfaceFrameContext::Main {
                view: view_label,
                force_log,
            },
        ) else {
            return ShellRenderOutcome::NotReady;
        };
        let surface_acquire_us = surface_acquire_start.elapsed().as_micros();

        let prepare_start = Instant::now();
        let overlay_text_active = scene.overlay_text_needed();
        if overlay_text_active && self.overlay_text_renderer.is_none() {
            fika_log!("[fika-wgpu] overlay-text init reason={reason}");
            self.overlay_text_renderer = Some(TextRenderer::new(&self.device, self.config.format));
        }
        let mut scene_frame = prepare_scene_frame(
            SceneFrameRenderers {
                text: &mut self.text_renderer,
                overlay_text: if overlay_text_active {
                    self.overlay_text_renderer.as_mut()
                } else {
                    None
                },
                icons: &mut self.icon_renderer,
            },
            FrameGpuContext {
                device: &self.device,
                queue: &self.queue,
            },
            SceneFrameRequest {
                scene,
                projections: &frame_projections,
                size: self.size,
                reason: icon_work_reason,
            },
        );
        drop(frame_projections);
        let prepare_us = prepare_start.elapsed().as_micros();
        let work_pending = scene_frame.work_pending(&mut self.icon_renderer, scene);
        if work_pending.any() {
            window.request_redraw();
        }
        self.render_work_pending = work_pending.any();
        scene_frame.upload_quads(
            &mut self.quad_renderer,
            &mut self.overlay_quad_renderer,
            &self.device,
            &self.queue,
        );

        // After a directory change, drop previous-folder GPU icon textures and
        // path-scoped thumbnail / failure caches (FileManager kills preview jobs and
        // clears finished items when the model is emptied). Done after upload so
        // the current frame's slots stay resident.
        let open_paths = ShellPaneId::ALL
            .into_iter()
            .filter_map(|pane| {
                scene
                    .panes
                    .is_open(pane)
                    .then(|| scene.pane_state(pane).map(|state| state.path.clone()))
                    .flatten()
            })
            .collect::<Vec<_>>();
        let path_changed = scene.path_changes != self.rendered_path_changes;
        if path_changed {
            // Free ready thumbs for directories we left (not ancestors of a still-
            // open pane — keep those for cheap Back navigation).
            let left_paths = self
                .rendered_open_paths
                .iter()
                .filter(|previous| {
                    !open_paths
                        .iter()
                        .any(|open| open == *previous || open.starts_with(previous))
                })
                .cloned()
                .collect::<Vec<_>>();
            if left_paths.is_empty() {
                self.icon_renderer.release_directory_caches(None);
            } else {
                for left in &left_paths {
                    self.icon_renderer
                        .release_directory_caches(Some(left.as_path()));
                }
            }
            self.icon_renderer.release_unused_gpu_textures();
        }

        let encode_present_start = Instant::now();
        let (view, mut encoder) = self.begin_surface_frame_encoding(&frame, "fika-wgpu-frame");

        self.encode_retained_scene_pass(
            &mut encoder,
            render_damage,
            render_damage_scissor,
            overlay_text_active,
        );
        self.retained_scene.mark_valid();
        self.encode_retained_present_pass(&mut encoder, &view);

        let presented_frame = self.submit_surface_frame(window, event_loop, frame, encoder);
        let encode_present_us = encode_present_start.elapsed().as_micros();

        let view_switch_rendered = self.rendered_view_switches != scene.view_switches;
        for report in self.frame_latency.drain_presented(presented_frame) {
            fika_log!(
                "[fika-wgpu] frame-latency event={} count={} requested_after_frame={} presented_frame={} frames={} reason={}",
                report.event,
                report.count,
                report.requested_after_frame,
                report.presented_frame,
                report.frames,
                reason
            );
        }
        if presented_frame == 1
            || view_switch_rendered
            || force_log
            || fika_frame_log_all_enabled()
            || self.last_log.elapsed() >= Duration::from_secs(1)
        {
            fika_log!(
                "[fika-wgpu] frame={} reason={} view={} scale={:.2} zoom={} zoom_changes={} path={} entries={} filtered={} show_hidden={} hidden_changes={} location_active={} location_changes={} filter_active={} filter_changes={} places={} places_visible={} places_width={:.1} place_hover={} places_changes={} places_resize_changes={} places_scroll_y={:.1} places_scroll_changes={} content_scroll_changes={} split_pane={} split_changes={} split_path={} content_scrollbar={} visible={} thumbnails={} folder_previews={} slots={}/{} slot_reused={} slot_recycled={} slot_allocated={} selected={} hover={} dnd_hover={} dnd_hover_changes={} dnd_drop_requests={} context={} context_menu={} context_changes={} context_actions={} properties={} properties_changes={} create_dialog={} create_changes={} rename_dialog={} rename_changes={} open_with={} open_with_changes={} open_changes={} copy_location_changes={} file_clipboard_changes={} paste_changes={} trash_changes={} rubber_band={} hit_tests={} selection_changes={} keyboard_nav={} rubber_band_updates={} view_switches={} path_changes={} reloads={} quads={} layout_content={:.1}x{:.1} first_item={:.1},{:.1},{:.1},{:.1} icons={} icon_quads={} icon_fallbacks={} icon_deferred={} icon_content_hash={:016x} icon_geometry_hash={:016x} icon_vertex_hash={:016x} icon_slot_hash={:016x} thumb_loaded={} thumb_quads={} thumb_deferred={} thumb_read_ahead={} thumb_ready={}/{}b folder_preview_loaded={} folder_preview_quads={} folder_preview_deferred={} folder_preview_read_ahead={} folder_preview_ready={}/{}b icon_cache={}/{} entries={} bytes={} icon_atlas={}x{}:{}b icon_atlas_uploads={}/{} icon_resolve={}us text_labels={} text_quads={} text_deferred={} text_cache={}/{} entries={} bytes={} swash_cache={}/{} swash_reset={} text_atlas_reused={} text_atlas_uploads={}/{} batches={} vertex_uploads={}/{} damage={} damage_rects={} damage_area={:.0} damage_bounds={:.1},{:.1},{:.1},{:.1} scroll_x={:.1} scroll_y={:.1} prewarm={}us surface={}us prepare={}us quad_upload={}us encode_present={}us layout={}us text_raster={}us text_atlas={}x{}:{}b dirty_skips={} dirty_pending={} render={}us",
                self.frame_count,
                reason,
                scene.panes[ShellPaneId::SLOT_0].view_mode.as_str(),
                scene.ui_scale(),
                scene.zoom_percent(),
                scene.zoom_changes,
                scene.panes[ShellPaneId::SLOT_0].path.display(),
                scene.panes[ShellPaneId::SLOT_0].entries.len(),
                scene.filtered_entry_count(),
                scene.show_hidden as u8,
                scene.hidden_changes,
                scene.is_location_editing() as u8,
                scene.location_changes,
                scene.filter_active as u8,
                scene.filter_changes,
                scene.places.len(),
                scene.places_visible as u8,
                scene.places_sidebar_width(self.size),
                scene.hovered_place.map(|index| index as i64).unwrap_or(-1),
                scene.places_changes,
                scene.places_resize_changes,
                scene.places_scroll_y,
                scene.places_scroll_changes,
                scene.content_scroll_changes,
                scene.panes.is_open(ShellPaneId::SLOT_1) as u8,
                scene.split_pane_changes,
                scene
                    .panes
                    .get(ShellPaneId::SLOT_1)
                    .map(|pane| pane.path.display().to_string())
                    .unwrap_or_else(|| "-".to_string()),
                scene_frame.content_scrollbar_visible as u8,
                scene_frame.visible_items,
                scene_frame.thumbnail_candidates,
                scene_frame.folder_preview_candidates,
                scene.visible_slot_stats.active,
                scene.visible_slot_stats.free,
                scene.visible_slot_stats.reused,
                scene.visible_slot_stats.recycled,
                scene.visible_slot_stats.allocated,
                scene.active_selection_len(),
                scene
                    .hovered_item
                    .map(|item| format!("{}:{}", item.pane.as_str(), item.index))
                    .unwrap_or_else(|| "-".to_string()),
                scene
                    .dnd_hover_target
                    .as_ref()
                    .map(ShellDropTarget::kind)
                    .unwrap_or("none"),
                scene.dnd_hover_changes,
                scene.dnd_drop_requests,
                scene
                    .context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none"),
                scene.context_menu.is_some() as u8,
                scene.context_target_changes,
                scene.context_menu_actions,
                scene.properties_overlay.is_some() as u8,
                scene.properties_changes,
                scene.create_dialog.is_some() as u8,
                scene.create_changes,
                scene.rename_dialog.is_some() as u8,
                scene.rename_changes,
                scene.open_with_chooser.is_some() as u8,
                scene.open_with_changes,
                scene.open_changes,
                scene.copy_location_changes,
                scene.file_clipboard_changes,
                scene.paste_changes,
                scene.trash_changes,
                scene.rubber_band.as_ref().is_some_and(|band| band.active) as u8,
                scene.hit_tests,
                scene.selection_changes,
                scene.keyboard_navigation,
                scene.rubber_band_updates,
                scene.view_switches,
                scene.path_changes,
                scene.directory_reloads,
                scene_frame.quad_count,
                scene_frame.content_size.width,
                scene_frame.content_size.height,
                scene_frame
                    .first_item_rect
                    .map(|rect| rect.x)
                    .unwrap_or(-1.0),
                scene_frame
                    .first_item_rect
                    .map(|rect| rect.y)
                    .unwrap_or(-1.0),
                scene_frame
                    .first_item_rect
                    .map(|rect| rect.width)
                    .unwrap_or(-1.0),
                scene_frame
                    .first_item_rect
                    .map(|rect| rect.height)
                    .unwrap_or(-1.0),
                scene_frame.icon_stats.icons,
                scene_frame.icon_stats.quads,
                scene_frame.icon_stats.fallbacks,
                scene_frame.icon_stats.deferred,
                scene_frame.icon_stats.content_hash,
                scene_frame.icon_stats.geometry_hash,
                scene_frame.icon_stats.vertex_hash,
                scene_frame.icon_stats.slot_hash,
                scene_frame.icon_stats.thumbnails,
                scene_frame.icon_stats.thumbnail_quads,
                scene_frame.icon_stats.thumbnail_deferred,
                scene_frame.icon_stats.thumbnail_read_ahead_queued,
                scene_frame.icon_stats.thumbnail_ready_entries,
                scene_frame.icon_stats.thumbnail_ready_bytes,
                scene_frame.icon_stats.folder_previews,
                scene_frame.icon_stats.folder_preview_quads,
                scene_frame.icon_stats.folder_preview_deferred,
                scene_frame.icon_stats.folder_preview_read_ahead_queued,
                scene_frame.icon_stats.folder_preview_ready_entries,
                scene_frame.icon_stats.folder_preview_ready_bytes,
                scene_frame.icon_stats.cache_hits,
                scene_frame.icon_stats.cache_misses,
                scene_frame.icon_stats.cache_entries,
                scene_frame.icon_stats.cache_bytes,
                scene_frame.icon_stats.atlas_width,
                scene_frame.icon_stats.atlas_height,
                scene_frame.icon_stats.atlas_bytes,
                scene_frame.icon_stats.atlas_uploads,
                scene_frame.icon_stats.atlas_upload_skips,
                scene_frame.icon_stats.resolve_us,
                scene_frame.text_stats.labels,
                scene_frame.text_stats.quads,
                scene_frame.text_stats.deferred,
                scene_frame.text_stats.cache_hits,
                scene_frame.text_stats.cache_misses,
                scene_frame.text_stats.cache_entries,
                scene_frame.text_stats.cache_bytes,
                scene_frame.text_stats.swash_image_entries,
                scene_frame.text_stats.swash_outline_entries,
                scene_frame.text_stats.swash_resets,
                scene_frame.text_stats.atlas_reused,
                scene_frame.text_stats.atlas_uploads,
                scene_frame.text_stats.atlas_upload_skips,
                self.quad_renderer.batch_count()
                    + self.overlay_quad_renderer.batch_count()
                    + self.icon_renderer.batch_count()
                    + self.text_renderer.batch_count()
                    + self.retained_scene.batch_count()
                    + self
                        .overlay_text_renderer
                        .as_ref()
                        .filter(|_| overlay_text_active)
                        .map(TextRenderer::batch_count)
                        .unwrap_or(0),
                scene_frame.vertex_upload_stats.writes,
                scene_frame.vertex_upload_stats.skips,
                render_damage.kind_label(),
                render_damage.rect_count,
                render_damage.area_px,
                render_damage.bounds.map(|rect| rect.x).unwrap_or(-1.0),
                render_damage.bounds.map(|rect| rect.y).unwrap_or(-1.0),
                render_damage.bounds.map(|rect| rect.width).unwrap_or(-1.0),
                render_damage.bounds.map(|rect| rect.height).unwrap_or(-1.0),
                scene.panes[ShellPaneId::SLOT_0].scroll_x,
                scene.panes[ShellPaneId::SLOT_0].scroll_y,
                prewarm_us,
                surface_acquire_us,
                prepare_us,
                scene_frame.quad_upload_us,
                encode_present_us,
                scene_frame.layout_us,
                scene_frame.text_stats.raster_us,
                scene_frame.text_stats.atlas_width,
                scene_frame.text_stats.atlas_height,
                scene_frame.text_stats.atlas_bytes,
                self.clean_redraw_skips,
                self.render_work_pending as u8,
                frame_start.elapsed().as_micros()
            );
            self.last_log = Instant::now();
        }
        self.rendered_view_switches = scene.view_switches;
        self.rendered_path_changes = scene.path_changes;
        self.rendered_open_paths = open_paths;
        self.last_render_dirty_key = Some(damage_snapshot.dirty_key.clone());
        self.last_render_damage_snapshot = Some(damage_snapshot);
        ShellRenderOutcome::Presented
    }
}
