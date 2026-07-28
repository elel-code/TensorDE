use std::time::Instant;

use crate::windowing::PhysicalSize;
use fika_core::{ViewRect, ViewSize};

use crate::ui::pane::ShellPaneProjection;
use crate::ui::prewarm::icon_sync_resolve_budget_for_frame;
use crate::ui::render::gpu::VertexBufferUploadStats;
use crate::ui::render::quad::{QuadRenderer, QuadVertex};
use crate::{
    FolderPreviewCacheStats, IconFrameBuilder, IconFrameConfig, IconFrameResources, IconFrameStats,
    IconRenderer, ShellScene, TextFrameBuilder, TextFrameResources, TextFrameStats, TextRenderer,
};

pub(crate) struct SceneFrame {
    pub(crate) vertices: Vec<QuadVertex>,
    pub(crate) overlay_vertices: Vec<QuadVertex>,
    pub(crate) visible_items: usize,
    pub(crate) thumbnail_candidates: usize,
    pub(crate) folder_preview_candidates: usize,
    pub(crate) quad_count: usize,
    pub(crate) content_size: ViewSize,
    pub(crate) content_scrollbar_visible: bool,
    pub(crate) first_item_rect: Option<ViewRect>,
    pub(crate) layout_us: u128,
    pub(crate) quad_upload_us: u128,
    pub(crate) text_stats: TextFrameStats,
    pub(crate) icon_stats: IconFrameStats,
    pub(crate) vertex_upload_stats: VertexBufferUploadStats,
}

pub(crate) struct SceneFrameProjections<'a> {
    projections: Vec<ShellPaneProjection<'a>>,
    layout_us: u128,
}

impl<'a> SceneFrameProjections<'a> {
    pub(crate) fn new(projections: Vec<ShellPaneProjection<'a>>, layout_us: u128) -> Self {
        Self {
            projections,
            layout_us,
        }
    }

    pub(crate) fn projections(&self) -> &[ShellPaneProjection<'a>] {
        &self.projections
    }

    pub(crate) fn layout_us(&self) -> u128 {
        self.layout_us
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SceneFrameWorkPending {
    pub(crate) metadata: bool,
    pub(crate) icon: bool,
    pub(crate) text: bool,
}

impl SceneFrameWorkPending {
    pub(crate) fn any(self) -> bool {
        self.metadata || self.icon || self.text
    }
}

impl SceneFrame {
    pub(crate) fn upload_quads(
        &mut self,
        quad_renderer: &mut QuadRenderer,
        overlay_quad_renderer: &mut QuadRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let start = Instant::now();
        self.vertex_upload_stats
            .merge(quad_renderer.upload(device, queue, &self.vertices));
        self.vertex_upload_stats.merge(overlay_quad_renderer.upload(
            device,
            queue,
            &self.overlay_vertices,
        ));
        self.quad_upload_us = start.elapsed().as_micros();
    }

    pub(crate) fn work_pending(
        &self,
        icon_renderer: &mut IconRenderer,
        scene: &ShellScene,
    ) -> SceneFrameWorkPending {
        let metadata = scene.metadata_role_visible_work_pending();
        let icon = self.icon_stats.deferred > 0
            || self.icon_stats.thumbnail_deferred > 0
            || icon_renderer.resolver.has_visible_pending()
            || icon_renderer.thumbnails.has_visible_pending()
            || scene.folder_preview_roles.borrow().has_visible_pending();
        let text = self.text_stats.deferred > 0;
        SceneFrameWorkPending {
            metadata,
            icon,
            text,
        }
    }
}

pub(crate) struct DialogFrame {
    pub(crate) text_stats: TextFrameStats,
    pub(crate) icon_stats: IconFrameStats,
    pub(crate) vertex_upload_stats: VertexBufferUploadStats,
    pub(crate) swash_image_entries: usize,
    pub(crate) swash_outline_entries: usize,
    pub(crate) swash_reset: bool,
    pub(crate) text_work_pending: bool,
    pub(crate) icon_work_pending: bool,
}

impl DialogFrame {
    pub(crate) fn work_pending(&self) -> bool {
        self.text_work_pending || self.icon_work_pending
    }
}

pub(crate) struct SceneFrameRenderers<'a> {
    pub(crate) text: &'a mut TextRenderer,
    pub(crate) overlay_text: Option<&'a mut TextRenderer>,
    pub(crate) icons: &'a mut IconRenderer,
}

pub(crate) struct DialogFrameRenderers<'a> {
    pub(crate) text: &'a mut TextRenderer,
    pub(crate) icons: &'a mut IconRenderer,
    pub(crate) quads: &'a mut QuadRenderer,
}

#[derive(Clone, Copy)]
pub(crate) struct FrameGpuContext<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) queue: &'a wgpu::Queue,
}

pub(crate) struct SceneFrameRequest<'a, 'projection> {
    pub(crate) scene: &'a ShellScene,
    pub(crate) projections: &'a SceneFrameProjections<'projection>,
    pub(crate) size: PhysicalSize<u32>,
    pub(crate) reason: &'a str,
}

pub(crate) struct DialogFrameRequest<'a> {
    pub(crate) layout_size: PhysicalSize<u32>,
    pub(crate) scale: f32,
    pub(crate) reason: &'a str,
}

pub(crate) fn prepare_scene_frame(
    renderers: SceneFrameRenderers<'_>,
    gpu: FrameGpuContext<'_>,
    request: SceneFrameRequest<'_, '_>,
) -> SceneFrame {
    let SceneFrameRenderers {
        text: text_renderer,
        overlay_text: overlay_text_renderer,
        icons: icon_renderer,
    } = renderers;
    let FrameGpuContext { device, queue } = gpu;
    let SceneFrameRequest {
        scene,
        projections: frame_projections,
        size,
        reason,
    } = request;
    text_renderer.label_cache.begin_frame();
    text_renderer.metrics_cache.begin_frame();

    if let Some(overlay_text_renderer) = overlay_text_renderer {
        overlay_text_renderer.label_cache.begin_frame();
        overlay_text_renderer.metrics_cache.begin_frame();
        let (mut scene_frame, mut text_frame, mut overlay_text_frame, mut icon_frame) = {
            let text_pixels = text_renderer.take_staging_pixels();
            let overlay_text_pixels = overlay_text_renderer.take_staging_pixels();
            let mut text_builder = TextFrameBuilder::new(
                TextFrameResources::from_renderer(text_renderer),
                size,
                scene.ui_scale(),
                text_pixels,
            );
            let mut overlay_text_builder = TextFrameBuilder::new(
                TextFrameResources::from_renderer(overlay_text_renderer),
                size,
                scene.ui_scale(),
                overlay_text_pixels,
            );
            let mut icon_builder = IconFrameBuilder::new(
                IconFrameResources::from_renderer(icon_renderer),
                IconFrameConfig {
                    surface_size: size,
                    ui_scale: scene.ui_scale(),
                    sync_resolve_budget: icon_sync_resolve_budget_for_frame(reason),
                    folder_preview_cache: FolderPreviewCacheStats {
                        ready_entries: scene.folder_preview_roles.borrow().ready_len(),
                        ready_bytes: scene.folder_preview_roles.borrow().ready_bytes(),
                    },
                },
            );
            let scene_frame = scene.build_frame(
                size,
                frame_projections.projections(),
                frame_projections.layout_us(),
                &mut text_builder,
                &mut icon_builder,
                Some(&mut overlay_text_builder),
            );
            let text_frame = text_builder.finish();
            let overlay_text_frame = overlay_text_builder.finish();
            let icon_frame = icon_builder.finish();
            (scene_frame, text_frame, overlay_text_frame, icon_frame)
        };

        let mut vertex_upload_stats = VertexBufferUploadStats::default();
        vertex_upload_stats.merge(icon_renderer.upload(device, queue, &mut icon_frame));
        vertex_upload_stats.merge(text_renderer.upload(device, queue, &mut text_frame));
        vertex_upload_stats.merge(overlay_text_renderer.upload(
            device,
            queue,
            &mut overlay_text_frame,
        ));
        let (text_swash_images, text_swash_outlines, text_swash_reset) =
            text_renderer.trim_text_engine_caches();
        let (overlay_swash_images, overlay_swash_outlines, overlay_swash_reset) =
            overlay_text_renderer.trim_text_engine_caches();
        scene_frame.icon_stats = icon_frame.stats;
        scene_frame.text_stats = text_frame.stats.merged(overlay_text_frame.stats);
        scene_frame.text_stats.swash_image_entries = text_swash_images.max(overlay_swash_images);
        scene_frame.text_stats.swash_outline_entries =
            text_swash_outlines.max(overlay_swash_outlines);
        scene_frame.text_stats.swash_resets =
            usize::from(text_swash_reset) + usize::from(overlay_swash_reset);
        scene_frame.vertex_upload_stats = vertex_upload_stats;
        scene_frame
    } else {
        let (mut scene_frame, mut text_frame, mut icon_frame) = {
            let text_pixels = text_renderer.take_staging_pixels();
            let mut text_builder = TextFrameBuilder::new(
                TextFrameResources::from_renderer(text_renderer),
                size,
                scene.ui_scale(),
                text_pixels,
            );
            let mut icon_builder = IconFrameBuilder::new(
                IconFrameResources::from_renderer(icon_renderer),
                IconFrameConfig {
                    surface_size: size,
                    ui_scale: scene.ui_scale(),
                    sync_resolve_budget: icon_sync_resolve_budget_for_frame(reason),
                    folder_preview_cache: FolderPreviewCacheStats {
                        ready_entries: scene.folder_preview_roles.borrow().ready_len(),
                        ready_bytes: scene.folder_preview_roles.borrow().ready_bytes(),
                    },
                },
            );
            let scene_frame = scene.build_frame(
                size,
                frame_projections.projections(),
                frame_projections.layout_us(),
                &mut text_builder,
                &mut icon_builder,
                None,
            );
            let text_frame = text_builder.finish();
            let icon_frame = icon_builder.finish();
            (scene_frame, text_frame, icon_frame)
        };

        let mut vertex_upload_stats = VertexBufferUploadStats::default();
        vertex_upload_stats.merge(icon_renderer.upload(device, queue, &mut icon_frame));
        vertex_upload_stats.merge(text_renderer.upload(device, queue, &mut text_frame));
        let (text_swash_images, text_swash_outlines, text_swash_reset) =
            text_renderer.trim_text_engine_caches();
        scene_frame.icon_stats = icon_frame.stats;
        scene_frame.text_stats = text_frame.stats;
        scene_frame.text_stats.swash_image_entries = text_swash_images;
        scene_frame.text_stats.swash_outline_entries = text_swash_outlines;
        scene_frame.text_stats.swash_resets = usize::from(text_swash_reset);
        scene_frame.vertex_upload_stats = vertex_upload_stats;
        scene_frame
    }
}

pub(crate) fn prepare_dialog_frame(
    renderers: DialogFrameRenderers<'_>,
    gpu: FrameGpuContext<'_>,
    request: DialogFrameRequest<'_>,
    paint: impl FnOnce(
        &mut Vec<QuadVertex>,
        &mut TextFrameBuilder<'_>,
        &mut IconFrameBuilder<'_>,
        PhysicalSize<u32>,
    ),
) -> DialogFrame {
    let DialogFrameRenderers {
        text: text_renderer,
        icons: icon_renderer,
        quads: quad_renderer,
    } = renderers;
    let FrameGpuContext { device, queue } = gpu;
    let DialogFrameRequest {
        layout_size,
        scale,
        reason,
    } = request;
    text_renderer.label_cache.begin_frame();
    text_renderer.metrics_cache.begin_frame();
    let icon_resolve_results = icon_renderer.resolver.drain_results();
    let thumbnail_results = icon_renderer.thumbnails.drain_results();

    let (vertices, mut text_frame, mut icon_frame) = {
        let text_pixels = text_renderer.take_staging_pixels();
        let mut text_builder = TextFrameBuilder::new(
            TextFrameResources::from_renderer(text_renderer),
            layout_size,
            scale,
            text_pixels,
        );
        let mut icon_builder = IconFrameBuilder::new(
            IconFrameResources::from_renderer(icon_renderer),
            IconFrameConfig {
                surface_size: layout_size,
                ui_scale: scale,
                sync_resolve_budget: icon_sync_resolve_budget_for_frame(reason),
                folder_preview_cache: FolderPreviewCacheStats::default(),
            },
        );
        let mut vertices = Vec::with_capacity(256);
        paint(
            &mut vertices,
            &mut text_builder,
            &mut icon_builder,
            layout_size,
        );
        (vertices, text_builder.finish(), icon_builder.finish())
    };

    let text_stats = text_frame.stats;
    let icon_stats = icon_frame.stats;
    let text_work_pending = text_stats.deferred > 0;
    // Only visible-priority work keeps the dialog frame dirty. Deferred
    // read-ahead (FileManager-style off-viewport resolve) must not block clean skips.
    let icon_work_pending = icon_stats.deferred > 0
        || icon_stats.thumbnail_deferred > 0
        || icon_resolve_results > 0
        || thumbnail_results > 0
        || icon_renderer.resolver.has_visible_pending()
        || icon_renderer.thumbnails.has_visible_pending();

    let mut vertex_upload_stats = quad_renderer.upload(device, queue, &vertices);
    vertex_upload_stats.merge(icon_renderer.upload(device, queue, &mut icon_frame));
    vertex_upload_stats.merge(text_renderer.upload(device, queue, &mut text_frame));
    let (swash_image_entries, swash_outline_entries, swash_reset) =
        text_renderer.trim_text_engine_caches();

    DialogFrame {
        text_stats,
        icon_stats,
        vertex_upload_stats,
        swash_image_entries,
        swash_outline_entries,
        swash_reset,
        text_work_pending,
        icon_work_pending,
    }
}
