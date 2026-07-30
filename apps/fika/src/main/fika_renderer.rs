use std::sync::Arc;
use std::time::Instant;

use crate::ui::render::quad::QuadVertex;
use crate::vulkan_rect::NativeFrameLayers;
use crate::vulkan_state::{PresentLayers, VulkanState};
use crate::windowing::{ActiveEventLoop, PhysicalSize, Window, WindowId};
use crate::{
    DetachedDialogRenderRequest, DialogRenderViewport, FolderPreviewCacheStats, IconEngine,
    IconFrameBuilder, IconFrameConfig, IconFrameResources, ShellRenderOutcome, ShellScene,
    TextEngine, TextFrameBuilder, TextFrameResources,
};

/// Fika's product-level retained renderer state.
///
/// GPU ownership and submission remain in [`VulkanState`]; this layer retains
/// the renderer-independent icon/text engines and the scheduling counters used
/// by the full file-manager controller.
pub(crate) struct FikaRenderer {
    gpu: VulkanState,
    pub(crate) icon_engine: IconEngine,
    pub(crate) text_engine: TextEngine,
    pub(crate) size: PhysicalSize<u32>,
    pub(crate) frame_count: u64,
    pub(crate) rendered_view_switches: u64,
    pub(crate) render_work_pending: bool,
    drag_preview_dmabuf_plan: Option<crate::ui::render::dmabuf::DmabufExportPlan>,
}

#[derive(Clone, Copy)]
pub(crate) struct VisibleRoleRenderPolicy {
    pub(crate) paused: bool,
    pub(crate) resolve_exact: bool,
}

impl FikaRenderer {
    pub(crate) fn new(window: Arc<Window>) -> Result<Self, String> {
        let gpu = VulkanState::new(window)?;
        let size = gpu.size();
        Ok(Self {
            gpu,
            icon_engine: IconEngine::new(),
            text_engine: TextEngine::new(),
            size,
            frame_count: 0,
            rendered_view_switches: 0,
            render_work_pending: false,
            drag_preview_dmabuf_plan: None,
        })
    }

    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        self.gpu.resize(size)?;
        self.size = self.gpu.size();
        Ok(())
    }

    pub(crate) fn wait_idle(&self, label: &str) -> Result<(), String> {
        self.gpu.wait_idle(label)
    }

    pub(crate) fn gpu_icon_resident_index(&self) -> crate::IconGpuResidentIndex {
        self.gpu.icon_resident_index()
    }

    pub(crate) fn drag_preview_fourcc(&self) -> Option<u32> {
        self.drag_preview_dmabuf_plan.map(|plan| plan.fourcc)
    }

    pub(crate) fn export_drag_preview_layers(
        &mut self,
        width: u32,
        height: u32,
        colors: &[QuadVertex],
        icons: &mut crate::IconFrame,
        text: &mut crate::TextFrame,
    ) -> Result<vulkan_renderer::ExportedDmaBufImage, String> {
        let plan = self.drag_preview_dmabuf_plan.ok_or_else(|| {
            "no compositor-advertised Vulkan-exportable dma-buf format".to_string()
        })?;
        let result = self.gpu.render_exported_layers(
            plan,
            vulkan_renderer::vk::Extent2D {
                width: width.max(1),
                height: height.max(1),
            },
            colors,
            NativeFrameLayers::default().as_refs(),
            icons,
            text,
        );
        self.text_engine.staging_pixels = std::mem::take(&mut text.pixels);
        self.text_engine.staging_pixels.clear();
        self.text_engine.trim_caches();
        result
    }

    pub(crate) fn sync_drag_preview_dmabuf_plan(&mut self, event_loop: &ActiveEventLoop) {
        // The drag icon is a new role-less wl_surface, not the origin toplevel.
        // Until that surface exists, linux-dmabuf requires the default feedback
        // rather than the origin surface's scoped preferences.
        let feedback = event_loop.dmabuf_default_feedback();
        let exportable = match self.gpu.exportable_dmabuf_formats() {
            Ok(exportable) => exportable,
            Err(error) => {
                if crate::fika_log_enabled() {
                    eprintln!("[fika] outgoing-dnd-preview capability-query-failed error={error}");
                }
                self.drag_preview_dmabuf_plan = None;
                return;
            }
        };
        self.drag_preview_dmabuf_plan = feedback
            .as_ref()
            .and_then(|feedback| {
                crate::ui::render::dmabuf::pick_export_format(feedback, &exportable)
            })
            .map(|format| crate::ui::render::dmabuf::DmabufExportPlan {
                fourcc: format.format,
                modifier: format.modifier,
                main_device: feedback.as_ref().map_or(0, |feedback| feedback.main_device),
                scanout_preferred: false,
            });
    }

    pub(crate) fn log_dmabuf_readiness(
        &self,
        event_loop: &ActiveEventLoop,
        _surface: Option<WindowId>,
        reason: &'static str,
    ) {
        if crate::fika_log_enabled() {
            eprintln!(
                "[fika-vulkan] dmabuf-status reason={reason} vulkan={} wayland={} plan={:?}",
                self.gpu.external_memory_dma_buf_supported() as u8,
                event_loop.has_linux_dmabuf() as u8,
                self.drag_preview_dmabuf_plan,
            );
        }
    }

    pub(crate) fn render(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        scene: &mut ShellScene,
        reason: &'static str,
        visible_roles: VisibleRoleRenderPolicy,
        force_log: bool,
    ) -> ShellRenderOutcome {
        let started_at = Instant::now();
        let view_mode = scene.active_view_mode().as_str();
        match self.render_inner(
            window,
            event_loop,
            scene,
            reason,
            visible_roles.paused,
            visible_roles.resolve_exact,
        ) {
            Ok(()) => {
                if crate::fika_frame_log_all_enabled() || (force_log && crate::fika_log_enabled()) {
                    eprintln!(
                        "[fika-vulkan] frame={} reason={} view={} size={}x{} work_pending={} render_us={}",
                        self.frame_count,
                        reason,
                        view_mode,
                        self.size.width,
                        self.size.height,
                        self.render_work_pending as u8,
                        started_at.elapsed().as_micros(),
                    );
                }
                ShellRenderOutcome::Presented
            }
            Err(error) => {
                eprintln!("[fika-vulkan] frame failed reason={reason}: {error}");
                window.request_redraw();
                ShellRenderOutcome::NotReady
            }
        }
    }

    fn render_inner(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        scene: &mut ShellScene,
        reason: &'static str,
        role_updates_paused: bool,
        resolve_visible_exact: bool,
    ) -> Result<(), String> {
        let icon_work_reason =
            crate::ui::prewarm::icon_work_reason_for_frame(reason, self.frame_count);
        let resolve_visible_exact =
            !role_updates_paused && (resolve_visible_exact || self.frame_count == 0);
        // Worker completions must become visible without another scroll or
        // navigation event. Draining is non-blocking; only cold resolution is
        // gated by the Dolphin-compatible visible-role timers below.
        let _ = scene.drain_metadata_role_results();
        let _ = scene.drain_folder_preview_role_results();
        let mut layouts = scene.prepare_frame_projection_layouts(self.size);
        scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
        let mut projections = scene.pane_projections_from_layouts(layouts);
        if resolve_visible_exact {
            let (mut stats, results) =
                scene.resolve_visible_metadata_roles_synchronously(projections.projections());
            if !results.is_empty() {
                drop(projections);
                stats.applied = scene.apply_synchronous_metadata_role_results(results);
                let mut layouts = scene.prepare_frame_projection_layouts(self.size);
                scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
                projections = scene.pane_projections_from_layouts(layouts);
            }
            if crate::fika_log_enabled() && (stats.visible > 0 || stats.applied > 0) {
                eprintln!(
                    "[fika-vulkan] visible-metadata-sync reason={} visible={} resolved={} deferred={} applied={} resolve_us={} over_budget={}",
                    icon_work_reason,
                    stats.visible,
                    stats.resolved,
                    stats.deferred,
                    stats.applied,
                    stats.resolve_us,
                    stats.over_budget as u8,
                );
            }
            let _ = scene.update_folder_preview_roles_for_projections(projections.projections());
            let _ = scene.prewarm_file_metadata_roles(projections.projections());
            let _ = scene.prewarm_visible_file_icon_roles(
                projections.projections(),
                &mut self.icon_engine.resolver,
                icon_work_reason,
                true,
            );
        }
        let mut layers = scene.build_native_frame_layers(self.size, projections.projections());

        self.text_engine.begin_frame();
        let text_pixels = self.text_engine.take_staging_pixels();
        let mut text_builder = TextFrameBuilder::new(
            TextFrameResources::from_engine(&mut self.text_engine),
            self.size,
            scene.ui_scale(),
            text_pixels,
        );
        let resident_icons = self.gpu.icon_resident_index();
        let mut icon_builder = IconFrameBuilder::new(
            IconFrameResources::from_engine(&mut self.icon_engine, resident_icons),
            IconFrameConfig {
                surface_size: self.size,
                ui_scale: scene.ui_scale(),
                sync_resolve_budget: crate::ui::prewarm::icon_sync_resolve_budget(
                    role_updates_paused,
                ),
                role_updates_paused,
                folder_preview_cache: FolderPreviewCacheStats {
                    ready_entries: scene.folder_preview_roles.borrow().ready_len(),
                    ready_bytes: scene.folder_preview_roles.borrow().ready_bytes(),
                },
            },
        );
        scene.push_native_frame_text(&mut text_builder, projections.projections(), self.size);
        scene.push_native_location_bar_carets(
            &mut layers.overlay_rects,
            &mut text_builder,
            self.size,
            scene.theme(),
        );
        scene.push_native_frame_icons(
            &mut icon_builder,
            projections.projections(),
            self.size,
            text_builder.file_manager_midline_shift(),
        );
        let mut colors = Vec::with_capacity(192);
        scene.push_native_overlays(&mut colors, &mut text_builder, &mut icon_builder, self.size);
        let mut text_frame = text_builder.finish();
        let mut icon_frame = icon_builder.finish();
        drop(projections);

        self.render_work_pending = text_frame.stats.deferred > 0
            || icon_frame.stats.deferred > 0
            || icon_frame.stats.thumbnail_deferred > 0
            || scene.folder_preview_roles.borrow().has_visible_pending();
        if crate::fika_log_enabled() {
            log_frame_stats(&icon_frame.stats, &text_frame.stats);
        }
        self.gpu.present_layers(
            event_loop,
            window,
            PresentLayers {
                clear: [0.0, 0.0, 0.0, 0.0],
                colors: &colors,
                layers: layers.as_refs(),
                icons: &mut icon_frame,
                text: &mut text_frame,
            },
        )?;
        self.text_engine.staging_pixels = std::mem::take(&mut text_frame.pixels);
        self.text_engine.staging_pixels.clear();
        self.text_engine.trim_caches();
        self.size = self.gpu.size();
        self.frame_count = self.gpu.frame_count();
        self.rendered_view_switches = scene.view_switches;
        if self.render_work_pending {
            window.request_redraw();
        }
        Ok(())
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
        let mut colors = Vec::with_capacity(192);
        self.text_engine.begin_frame();
        let text_pixels = self.text_engine.take_staging_pixels();
        let mut text_builder = TextFrameBuilder::new(
            TextFrameResources::from_engine(&mut self.text_engine),
            self.size,
            request.viewport.scale,
            text_pixels,
        );
        let resident_icons = self.gpu.icon_resident_index();
        let mut icon_builder = IconFrameBuilder::new(
            IconFrameResources::from_engine(&mut self.icon_engine, resident_icons),
            IconFrameConfig {
                surface_size: self.size,
                ui_scale: request.viewport.scale,
                sync_resolve_budget: crate::ui::prewarm::icon_sync_resolve_budget(false),
                role_updates_paused: false,
                folder_preview_cache: FolderPreviewCacheStats::default(),
            },
        );
        paint(
            &mut colors,
            &mut text_builder,
            &mut icon_builder,
            request.viewport.layout_size,
        );
        let mut text_frame = text_builder.finish();
        let mut icon_frame = icon_builder.finish();
        self.render_work_pending = text_frame.stats.deferred > 0 || icon_frame.stats.deferred > 0;
        let layers = NativeFrameLayers::default();
        let result = self.gpu.present_layers(
            request.event_loop,
            request.window,
            PresentLayers {
                clear: [0.0, 0.0, 0.0, 0.0],
                colors: &colors,
                layers: layers.as_refs(),
                icons: &mut icon_frame,
                text: &mut text_frame,
            },
        );
        self.text_engine.staging_pixels = std::mem::take(&mut text_frame.pixels);
        self.text_engine.staging_pixels.clear();
        self.text_engine.trim_caches();
        match result {
            Ok(()) => {
                self.frame_count = self.gpu.frame_count();
                if self.render_work_pending {
                    request.window.request_redraw();
                }
                ShellRenderOutcome::Presented
            }
            Err(error) => {
                eprintln!(
                    "[fika-vulkan] {} dialog frame failed reason={}: {error}",
                    request.dialog_label, request.reason,
                );
                request.window.request_redraw();
                ShellRenderOutcome::NotReady
            }
        }
    }

    pub(crate) fn render_open_with_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        chooser: &crate::ShellOpenWithChooser,
        viewport: DialogRenderViewport,
        caret_visible: bool,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let popup_theme = viewport.popup_theme;
        let scale = viewport.scale;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: crate::ShellDialogWindowKind::OpenWith.as_str(),
            },
            |vertices, text, icons, size| {
                crate::ui::open_with::paint::push_open_with_chooser_dialog(
                    chooser,
                    vertices,
                    text,
                    icons,
                    crate::ui::open_with::paint::OpenWithDialogPaintConfig {
                        theme: popup_theme,
                        scale,
                        caret_visible,
                        size,
                    },
                );
            },
        )
    }

    pub(crate) fn render_settings_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        state: crate::ShellSettingsDialogState,
        snapshot: crate::ShellSettingsSnapshot,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let popup_theme = viewport.popup_theme;
        let scale = viewport.scale;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: crate::ShellDialogWindowKind::Settings.as_str(),
            },
            |vertices, text, _, size| {
                crate::ui::settings::paint::push_settings_dialog(
                    &state,
                    snapshot,
                    popup_theme,
                    scale,
                    vertices,
                    text,
                    size,
                );
            },
        )
    }

    pub(crate) fn render_create_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        dialog: &crate::ShellCreateDialog,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let popup_theme = viewport.popup_theme;
        let scale = viewport.scale;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: crate::ShellDialogWindowKind::Create.as_str(),
            },
            |vertices, text, _, size| {
                crate::ui::create_rename::paint::push_create_dialog(
                    dialog,
                    popup_theme,
                    scale,
                    vertices,
                    text,
                    size,
                );
            },
        )
    }

    pub(crate) fn render_rename_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        dialog: &crate::ShellRenameDialog,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let popup_theme = viewport.popup_theme;
        let scale = viewport.scale;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: crate::ShellDialogWindowKind::Rename.as_str(),
            },
            |vertices, text, _, size| {
                crate::ui::create_rename::paint::push_rename_dialog(
                    dialog,
                    popup_theme,
                    scale,
                    vertices,
                    text,
                    size,
                );
            },
        )
    }

    pub(crate) fn render_properties_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        overlay: &crate::ShellPropertiesOverlay,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let popup_theme = viewport.popup_theme;
        let scale = viewport.scale;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: crate::ShellDialogWindowKind::Properties.as_str(),
            },
            |vertices, text, _, size| {
                crate::ui::properties::paint::push_properties_dialog(
                    overlay,
                    popup_theme,
                    scale,
                    vertices,
                    text,
                    size,
                );
            },
        )
    }

    pub(crate) fn render_task_detail_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        statuses: &crate::ShellTaskStatusStore,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let popup_theme = viewport.popup_theme;
        let scale = viewport.scale;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: crate::ShellDialogWindowKind::TaskDetail.as_str(),
            },
            |vertices, text, _, size| {
                crate::ui::tasks::paint::push_task_detail_dialog(
                    statuses,
                    popup_theme,
                    scale,
                    vertices,
                    text,
                    size,
                );
            },
        )
    }

    pub(crate) fn render_trash_conflict_dialog(
        &mut self,
        window: &Window,
        event_loop: &ActiveEventLoop,
        dialog: &crate::ShellTrashConflictDialog,
        viewport: DialogRenderViewport,
        reason: &'static str,
    ) -> ShellRenderOutcome {
        let popup_theme = viewport.popup_theme;
        let scale = viewport.scale;
        self.render_detached_dialog(
            DetachedDialogRenderRequest {
                window,
                event_loop,
                viewport,
                reason,
                dialog_label: crate::ShellDialogWindowKind::TrashConflict.as_str(),
            },
            |vertices, text, _, size| {
                crate::push_trash_conflict_dialog_surface(
                    dialog,
                    popup_theme,
                    scale,
                    vertices,
                    text,
                    size,
                );
            },
        )
    }
}

#[cfg(test)]
pub(crate) fn synchronize_visible_metadata_roles(
    scene: &mut ShellScene,
    size: PhysicalSize<u32>,
    role_updates_paused: bool,
) -> crate::ui::metadata_roles::MetadataRoleSyncStats {
    if role_updates_paused {
        return crate::ui::metadata_roles::MetadataRoleSyncStats::default();
    }
    let mut layouts = scene.prepare_frame_projection_layouts(size);
    scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
    let projections = scene.pane_projections_from_layouts(layouts);
    let (mut stats, results) =
        scene.resolve_visible_metadata_roles_synchronously(projections.projections());
    drop(projections);
    stats.applied = scene.apply_synchronous_metadata_role_results(results);
    stats
}

fn log_frame_stats(icons: &crate::IconFrameStats, text: &crate::TextFrameStats) {
    eprintln!(
        "[fika-vulkan] icons={} quads={} fallback={} deferred={} thumbnails={}/{}/{} read_ahead={} ready={}/{}B previews={}/{}/{} read_ahead={} ready={}/{}B atlas={}/{} {}x{} {}B cache={}/{} {}/{}B hashes={:x}/{:x}/{:x}/{:x} resolve={}us text={}/{} deferred={} reused={} atlas={}/{} {}x{} {}B cache={}/{} {}/{}B swash={}/{}/{} raster={}us",
        icons.icons,
        icons.quads,
        icons.fallbacks,
        icons.deferred,
        icons.thumbnails,
        icons.thumbnail_quads,
        icons.thumbnail_deferred,
        icons.thumbnail_read_ahead_queued,
        icons.thumbnail_ready_entries,
        icons.thumbnail_ready_bytes,
        icons.folder_previews,
        icons.folder_preview_quads,
        icons.folder_preview_deferred,
        icons.folder_preview_read_ahead_queued,
        icons.folder_preview_ready_entries,
        icons.folder_preview_ready_bytes,
        icons.atlas_uploads,
        icons.atlas_upload_skips,
        icons.atlas_width,
        icons.atlas_height,
        icons.atlas_bytes,
        icons.cache_hits,
        icons.cache_misses,
        icons.cache_entries,
        icons.cache_bytes,
        icons.content_hash,
        icons.geometry_hash,
        icons.vertex_hash,
        icons.slot_hash,
        icons.resolve_us,
        text.labels,
        text.quads,
        text.deferred,
        text.atlas_reused,
        text.atlas_uploads,
        text.atlas_upload_skips,
        text.atlas_width,
        text.atlas_height,
        text.atlas_bytes,
        text.cache_hits,
        text.cache_misses,
        text.cache_entries,
        text.cache_bytes,
        text.swash_image_entries,
        text.swash_outline_entries,
        text.swash_resets,
        text.raster_us,
    );
}
