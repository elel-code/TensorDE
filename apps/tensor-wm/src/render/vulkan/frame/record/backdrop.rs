use std::{mem, ops::Range, slice};

use thiserror::Error;
use vulkan_renderer::{
    BarrierBatch, ColorAttachment, ColorImageCopy, CommandEncoder, Error as RendererError,
    Extent2D, GraphicsPipeline, LoadOp, OffscreenColorTarget, Origin2D, Rect2D as RendererRect2D,
    RenderGraphImageState, RenderingDescriptor, ResolveMode, ResourceBinding, ResourceState,
    StoreOp, TextureLayout, Viewport,
};

use crate::{
    ecs::ViewId,
    render::{
        FrameSubmission,
        frame::{BackdropPass, OutputLoad},
    },
};

use super::{
    CaptureRecord, DrawPushData, PreparedCursorDraws, PreparedSceneDraw, SceneBarrierScratch,
    append_image_transition, client_release_state, client_source_state, color_attachment_state,
    destination_to_ndc, draw_over_damage, has_render_damage, local_client_state, push_client_draw,
    record_capture_tap, record_cursor_draws, record_cursor_scope, sampled_state,
    transfer_destination_state, transfer_source_state,
};
use crate::render::vulkan::{import::ClientImageInfo, target::NativeOutputImageInfo};

/// One scene-order boundary immediately preceding a backdrop-dependent view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BackdropSceneSlice {
    pub(super) draws_before: Range<usize>,
    pub(super) backdrop_index: usize,
}

/// Retained value-only scratch that lowers backdrop view IDs to draw ranges.
///
/// The first preparation may grow to the scene's effect count. Subsequent
/// frames reuse the same allocation; no Vulkan handle crosses this boundary.
#[derive(Debug)]
pub(crate) struct BackdropSceneScratch {
    slices: Vec<BackdropSceneSlice>,
    tail: Range<usize>,
}

impl BackdropSceneScratch {
    pub(crate) fn new() -> Self {
        Self {
            slices: Vec::with_capacity(8),
            tail: 0..0,
        }
    }

    pub(crate) fn prepare(
        &mut self,
        draws: &[PreparedSceneDraw],
        backdrops: &[BackdropPass],
    ) -> Result<(), BackdropScenePlanError> {
        self.slices.clear();
        self.slices.reserve(backdrops.len());
        let mut cursor = 0;
        for (backdrop_index, backdrop) in backdrops.iter().enumerate() {
            let boundary = draws[cursor..]
                .iter()
                .position(|draw| draw.view_id() == backdrop.view_id)
                .map(|relative| cursor + relative)
                .ok_or(BackdropScenePlanError::MissingView(backdrop.view_id))?;
            self.slices.push(BackdropSceneSlice {
                draws_before: cursor..boundary,
                backdrop_index,
            });
            cursor = boundary;
        }
        self.tail = cursor..draws.len();
        Ok(())
    }

    pub(super) fn slices(&self) -> &[BackdropSceneSlice] {
        &self.slices
    }

    pub(super) fn tail(&self) -> Range<usize> {
        self.tail.clone()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct BackdropFilterPushData {
    descriptor_index: u32,
    sampler_index: u32,
    radius: u32,
    horizontal: u32,
    inverse_extent: [f32; 2],
    uv_scale: [f32; 2],
}

struct BackdropComposite<'a> {
    frame: &'a FrameSubmission,
    output: &'a NativeOutputImageInfo,
    backdrop: BackdropPass,
    descriptor_index: u32,
    sampler_index: u32,
    intermediate_extent: Extent2D,
    pipeline: &'a GraphicsPipeline,
}

pub(crate) struct BackdropSceneRecord<'a> {
    pub(crate) frame: &'a FrameSubmission,
    pub(crate) output: &'a NativeOutputImageInfo,
    pub(crate) clients: &'a [ClientImageInfo],
    pub(crate) client_pipeline: &'a GraphicsPipeline,
    pub(crate) managed_client_pipeline: Option<&'a GraphicsPipeline>,
    pub(crate) shadow_pipeline: Option<&'a GraphicsPipeline>,
    pub(crate) focus_ring_pipeline: Option<&'a GraphicsPipeline>,
    pub(crate) cursor_pipeline: Option<&'a GraphicsPipeline>,
    pub(crate) filter_pipeline: &'a GraphicsPipeline,
    pub(crate) graphics_queue_family: u32,
    pub(crate) scene_draws: &'a [PreparedSceneDraw],
    pub(crate) cursors: &'a PreparedCursorDraws,
    pub(crate) backdrops: &'a [BackdropPass],
    pub(crate) lanes: [OffscreenColorTarget<'a>; 2],
    pub(crate) lane_descriptor_indices: [u32; 2],
    pub(crate) sampler_index: u32,
    pub(crate) intermediate_extent: Extent2D,
    pub(crate) capture: Option<CaptureRecord<'a>>,
}

pub(crate) unsafe fn record_backdrop_scene(
    encoder: &mut CommandEncoder,
    record: BackdropSceneRecord<'_>,
    barriers: &mut SceneBarrierScratch,
    scratch: &BackdropSceneScratch,
) -> Result<(), RendererError> {
    let BackdropSceneRecord {
        frame,
        output,
        clients,
        client_pipeline,
        managed_client_pipeline,
        shadow_pipeline,
        focus_ring_pipeline,
        cursor_pipeline,
        filter_pipeline,
        graphics_queue_family,
        scene_draws,
        cursors,
        backdrops,
        lanes,
        lane_descriptor_indices,
        sampler_index,
        intermediate_extent,
        capture,
    } = record;
    barriers.clear();
    encoder.retain_resource(&output.image);
    for image in clients {
        image.retain_for_submission(encoder);
        if image.upload_pending {
            append_image_transition(
                &mut barriers.upload,
                image.resource_binding(),
                local_client_state(image.needs_initial_acquire, graphics_queue_family),
                transfer_destination_state(graphics_queue_family),
            )?;
        }
    }
    unsafe { encoder.pipeline_barrier(&barriers.upload) };
    for image in clients {
        unsafe { image.record_upload(encoder)? };
    }

    append_image_transition(
        &mut barriers.acquire,
        output.image.resource_binding(),
        super::output_source_state(output.foreign_owned, graphics_queue_family),
        color_attachment_state(graphics_queue_family),
    )?;
    for image in clients {
        append_image_transition(
            &mut barriers.acquire,
            image.resource_binding(),
            client_source_state(image, graphics_queue_family),
            sampled_state(graphics_queue_family),
        )?;
    }
    unsafe { encoder.pipeline_barrier(&barriers.acquire) };

    for (index, slice) in scratch.slices().iter().enumerate() {
        record_output_segment(
            encoder,
            frame,
            output,
            &scene_draws[slice.draws_before.clone()],
            client_pipeline,
            managed_client_pipeline,
            shadow_pipeline,
            focus_ring_pipeline,
            None,
            if index == 0 {
                frame.pass_plan.output_load()
            } else {
                OutputLoad::Preserve
            },
            index == 0,
        )?;
        let backdrop = backdrops
            .get(slice.backdrop_index)
            .expect("prepared backdrop slice retains a valid pass index");
        unsafe {
            record_filter_and_composite(
                encoder,
                frame,
                output,
                *backdrop,
                lanes,
                lane_descriptor_indices,
                sampler_index,
                intermediate_extent,
                client_pipeline,
                filter_pipeline,
                graphics_queue_family,
                index != 0,
                &mut barriers.acquire,
            )?;
        }
    }

    let capture_before_cursors =
        capture.is_some_and(|capture| capture.request.tap_before_software_cursors());
    let tail = scratch.tail();
    if !tail.is_empty() || (!capture_before_cursors && !cursors.is_empty()) {
        record_output_segment(
            encoder,
            frame,
            output,
            &scene_draws[tail],
            client_pipeline,
            managed_client_pipeline,
            shadow_pipeline,
            focus_ring_pipeline,
            (!capture_before_cursors).then_some((cursor_pipeline, cursors)),
            OutputLoad::Preserve,
            false,
        )?;
    }

    let mut output_release_source = if let Some(capture) = capture {
        unsafe {
            record_capture_tap(
                encoder,
                output,
                capture,
                graphics_queue_family,
                &mut barriers.release,
            )?;
        }
        transfer_source_state(graphics_queue_family)
    } else {
        color_attachment_state(graphics_queue_family)
    };
    if capture_before_cursors && !cursors.is_empty() {
        barriers.release.clear();
        append_image_transition(
            &mut barriers.release,
            output.image.resource_binding(),
            transfer_source_state(graphics_queue_family),
            color_attachment_state(graphics_queue_family),
        )?;
        unsafe { encoder.pipeline_barrier(&barriers.release) };
        record_cursor_scope(
            encoder,
            frame,
            output,
            cursors,
            Some(client_pipeline),
            cursor_pipeline,
        )?;
        output_release_source = color_attachment_state(graphics_queue_family);
    }
    barriers.release.clear();
    for image in clients {
        append_image_transition(
            &mut barriers.release,
            image.resource_binding(),
            sampled_state(graphics_queue_family),
            client_release_state(image.foreign_owned, graphics_queue_family),
        )?;
    }
    append_image_transition(
        &mut barriers.release,
        output.image.resource_binding(),
        output_release_source,
        ResourceState::foreign_image(vulkan_renderer::ForeignImageState::General),
    )?;
    unsafe { encoder.pipeline_barrier(&barriers.release) };
    Ok(())
}

#[allow(clippy::too_many_arguments)]
unsafe fn record_filter_and_composite(
    encoder: &mut CommandEncoder,
    frame: &FrameSubmission,
    output: &NativeOutputImageInfo,
    backdrop: BackdropPass,
    lanes: [OffscreenColorTarget<'_>; 2],
    lane_descriptor_indices: [u32; 2],
    sampler_index: u32,
    intermediate_extent: Extent2D,
    client_pipeline: &GraphicsPipeline,
    filter_pipeline: &GraphicsPipeline,
    graphics_queue_family: u32,
    lanes_initialized: bool,
    barriers: &mut BarrierBatch,
) -> Result<(), RendererError> {
    barriers.clear();
    append_image_transition(
        barriers,
        output.image.resource_binding(),
        color_attachment_state(graphics_queue_family),
        transfer_source_state(graphics_queue_family),
    )?;
    append_image_transition(
        barriers,
        ResourceBinding::whole_color_image(lanes[0].image),
        if lanes_initialized {
            sampled_state(graphics_queue_family)
        } else {
            undefined_state(graphics_queue_family)
        },
        transfer_destination_state(graphics_queue_family),
    )?;
    unsafe { encoder.pipeline_barrier(barriers) };
    let copy = ColorImageCopy {
        source_mip_level: 0,
        source_base_array_layer: 0,
        source_origin: Origin2D::new(backdrop.sample_region.x, backdrop.sample_region.y),
        destination_mip_level: 0,
        destination_base_array_layer: 0,
        destination_origin: Origin2D::new(0, 0),
        extent: Extent2D::new(backdrop.sample_region.width, backdrop.sample_region.height),
        layer_count: 1,
    };
    unsafe {
        encoder.copy_exported_color_image_to_image(
            &output.image,
            lanes[0].image,
            slice::from_ref(&copy),
        )?;
    }

    barriers.clear();
    append_image_transition(
        barriers,
        ResourceBinding::whole_color_image(lanes[0].image),
        transfer_destination_state(graphics_queue_family),
        sampled_state(graphics_queue_family),
    )?;
    append_image_transition(
        barriers,
        ResourceBinding::whole_color_image(lanes[1].image),
        if lanes_initialized {
            sampled_state(graphics_queue_family)
        } else {
            undefined_state(graphics_queue_family)
        },
        color_attachment_state(graphics_queue_family),
    )?;
    unsafe { encoder.pipeline_barrier(barriers) };
    record_filter_pass(
        encoder,
        lanes[1],
        lane_descriptor_indices[0],
        backdrop,
        intermediate_extent,
        filter_pipeline,
        sampler_index,
        true,
    )?;

    barriers.clear();
    append_image_transition(
        barriers,
        ResourceBinding::whole_color_image(lanes[1].image),
        color_attachment_state(graphics_queue_family),
        sampled_state(graphics_queue_family),
    )?;
    append_image_transition(
        barriers,
        ResourceBinding::whole_color_image(lanes[0].image),
        sampled_state(graphics_queue_family),
        color_attachment_state(graphics_queue_family),
    )?;
    unsafe { encoder.pipeline_barrier(barriers) };
    record_filter_pass(
        encoder,
        lanes[0],
        lane_descriptor_indices[1],
        backdrop,
        intermediate_extent,
        filter_pipeline,
        sampler_index,
        false,
    )?;

    barriers.clear();
    append_image_transition(
        barriers,
        ResourceBinding::whole_color_image(lanes[0].image),
        color_attachment_state(graphics_queue_family),
        sampled_state(graphics_queue_family),
    )?;
    append_image_transition(
        barriers,
        output.image.resource_binding(),
        transfer_source_state(graphics_queue_family),
        color_attachment_state(graphics_queue_family),
    )?;
    unsafe { encoder.pipeline_barrier(barriers) };
    record_backdrop_composite(
        encoder,
        BackdropComposite {
            frame,
            output,
            backdrop,
            descriptor_index: lane_descriptor_indices[0],
            sampler_index,
            intermediate_extent,
            pipeline: client_pipeline,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn record_filter_pass(
    encoder: &mut CommandEncoder,
    destination: OffscreenColorTarget<'_>,
    source_descriptor_index: u32,
    backdrop: BackdropPass,
    intermediate_extent: Extent2D,
    pipeline: &GraphicsPipeline,
    sampler_index: u32,
    horizontal: bool,
) -> Result<(), RendererError> {
    let active_extent = Extent2D::new(backdrop.sample_region.width, backdrop.sample_region.height);
    let attachments = [Some(ColorAttachment {
        view: destination.view.as_attachment(),
        layout: TextureLayout::ColorAttachment,
        resolve_target: None,
        resolve_layout: TextureLayout::Undefined,
        resolve_mode: ResolveMode::None,
        load_op: LoadOp::Discard,
        store_op: StoreOp::Store,
    })];
    let descriptor = RenderingDescriptor {
        label: Some(if horizontal {
            "tensor-backdrop-horizontal"
        } else {
            "tensor-backdrop-vertical"
        }),
        render_area: RendererRect2D::new(0, 0, active_extent.width, active_extent.height),
        layer_count: 1,
        view_mask: 0,
        color_attachments: &attachments,
        depth_attachment: None,
        stencil_attachment: None,
        multisampled_render_to_single_sampled: None,
    };
    let mut rendering = unsafe { encoder.begin_rendering(&descriptor)? };
    rendering.set_viewport(Viewport {
        x: 0.0,
        y: 0.0,
        width: active_extent.width as f32,
        height: active_extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    })?;
    rendering.set_scissor(RendererRect2D::new(
        0,
        0,
        active_extent.width,
        active_extent.height,
    ))?;
    rendering.bind_pipeline(pipeline)?;
    let push = backdrop_filter_push(
        source_descriptor_index,
        sampler_index,
        backdrop,
        active_extent,
        intermediate_extent,
        horizontal,
    );
    let bytes = unsafe {
        slice::from_raw_parts(
            (&push as *const BackdropFilterPushData).cast::<u8>(),
            mem::size_of::<BackdropFilterPushData>(),
        )
    };
    rendering.push_data(0, bytes)?;
    unsafe { rendering.draw(0..6, 0..1)? };
    rendering.end();
    Ok(())
}

fn backdrop_filter_push(
    descriptor_index: u32,
    sampler_index: u32,
    backdrop: BackdropPass,
    active_extent: Extent2D,
    intermediate_extent: Extent2D,
    horizontal: bool,
) -> BackdropFilterPushData {
    BackdropFilterPushData {
        descriptor_index,
        sampler_index,
        radius: backdrop.radius,
        horizontal: u32::from(horizontal),
        inverse_extent: [
            1.0 / intermediate_extent.width as f32,
            1.0 / intermediate_extent.height as f32,
        ],
        uv_scale: [
            active_extent.width as f32 / intermediate_extent.width as f32,
            active_extent.height as f32 / intermediate_extent.height as f32,
        ],
    }
}

#[allow(clippy::too_many_arguments)]
fn record_output_segment(
    encoder: &mut CommandEncoder,
    frame: &FrameSubmission,
    output: &NativeOutputImageInfo,
    scene_draws: &[PreparedSceneDraw],
    client_pipeline: &GraphicsPipeline,
    managed_client_pipeline: Option<&GraphicsPipeline>,
    shadow_pipeline: Option<&GraphicsPipeline>,
    focus_ring_pipeline: Option<&GraphicsPipeline>,
    cursors: Option<(Option<&GraphicsPipeline>, &PreparedCursorDraws)>,
    output_load: OutputLoad,
    clear_partial_damage: bool,
) -> Result<(), RendererError> {
    let attachments = [Some(ColorAttachment {
        view: output.image.as_attachment(),
        layout: TextureLayout::ColorAttachment,
        resolve_target: None,
        resolve_layout: TextureLayout::Undefined,
        resolve_mode: ResolveMode::None,
        load_op: match output_load {
            OutputLoad::Clear => LoadOp::Clear(super::OUTPUT_CLEAR),
            OutputLoad::Preserve => LoadOp::Load,
        },
        store_op: StoreOp::Store,
    })];
    let descriptor = RenderingDescriptor {
        label: Some("tensor-backdrop-output-segment"),
        render_area: RendererRect2D::new(
            0,
            0,
            frame.target.viewport.width,
            frame.target.viewport.height,
        ),
        layer_count: 1,
        view_mask: 0,
        color_attachments: &attachments,
        depth_attachment: None,
        stencil_attachment: None,
        multisampled_render_to_single_sampled: None,
    };
    let mut rendering = unsafe { encoder.begin_rendering(&descriptor)? };
    rendering.set_viewport(Viewport {
        x: 0.0,
        y: 0.0,
        width: frame.target.viewport.width as f32,
        height: frame.target.viewport.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    })?;
    if clear_partial_damage && output_load == OutputLoad::Preserve {
        for damage in frame.render_damage.regions() {
            let rect = RendererRect2D::new(damage.x, damage.y, damage.width, damage.height);
            rendering.clear_color_attachment(0, super::OUTPUT_CLEAR, slice::from_ref(&rect))?;
        }
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum BoundPipeline {
        Client,
        ManagedClient,
        Shadow,
        FocusRing,
    }
    let mut bound = None;
    for draw in scene_draws {
        match draw {
            PreparedSceneDraw::Client(draw) => {
                if !has_render_damage(draw.scissor, frame) {
                    continue;
                }
                let (pipeline, kind) = if draw.is_color_managed() {
                    let Some(pipeline) = managed_client_pipeline else {
                        continue;
                    };
                    (pipeline, BoundPipeline::ManagedClient)
                } else {
                    (client_pipeline, BoundPipeline::Client)
                };
                if bound != Some(kind) {
                    rendering.bind_pipeline(pipeline)?;
                    bound = Some(kind);
                }
                push_client_draw(&mut rendering, draw)?;
                draw_over_damage(&mut rendering, draw.scissor, frame)?;
            }
            PreparedSceneDraw::Shadow(shadow) => {
                let Some(pipeline) = shadow_pipeline else {
                    continue;
                };
                if !has_render_damage(shadow.scissor, frame) {
                    continue;
                }
                if bound != Some(BoundPipeline::Shadow) {
                    rendering.bind_pipeline(pipeline)?;
                    bound = Some(BoundPipeline::Shadow);
                }
                push_struct(&mut rendering, &shadow.push)?;
                draw_over_damage(&mut rendering, shadow.scissor, frame)?;
            }
            PreparedSceneDraw::FocusRing(ring) => {
                let Some(pipeline) = focus_ring_pipeline else {
                    continue;
                };
                if !has_render_damage(ring.scissor, frame) {
                    continue;
                }
                if bound != Some(BoundPipeline::FocusRing) {
                    rendering.bind_pipeline(pipeline)?;
                    bound = Some(BoundPipeline::FocusRing);
                }
                push_struct(&mut rendering, &ring.push)?;
                draw_over_damage(&mut rendering, ring.scissor, frame)?;
            }
        }
    }
    if let Some((cursor_pipeline, cursors)) = cursors {
        record_cursor_draws(
            &mut rendering,
            frame,
            cursors,
            Some(client_pipeline),
            cursor_pipeline,
        )?;
    }
    rendering.end();
    Ok(())
}

fn record_backdrop_composite(
    encoder: &mut CommandEncoder,
    composite: BackdropComposite<'_>,
) -> Result<(), RendererError> {
    let BackdropComposite {
        frame,
        output,
        backdrop,
        descriptor_index,
        sampler_index,
        intermediate_extent,
        pipeline,
    } = composite;
    let attachments = [Some(ColorAttachment {
        view: output.image.as_attachment(),
        layout: TextureLayout::ColorAttachment,
        resolve_target: None,
        resolve_layout: TextureLayout::Undefined,
        resolve_mode: ResolveMode::None,
        load_op: LoadOp::Load,
        store_op: StoreOp::Store,
    })];
    let descriptor = RenderingDescriptor {
        label: Some("tensor-backdrop-composite"),
        render_area: RendererRect2D::new(
            0,
            0,
            frame.target.viewport.width,
            frame.target.viewport.height,
        ),
        layer_count: 1,
        view_mask: 0,
        color_attachments: &attachments,
        depth_attachment: None,
        stencil_attachment: None,
        multisampled_render_to_single_sampled: None,
    };
    let mut rendering = unsafe { encoder.begin_rendering(&descriptor)? };
    rendering.set_viewport(Viewport {
        x: 0.0,
        y: 0.0,
        width: frame.target.viewport.width as f32,
        height: frame.target.viewport.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    })?;
    rendering.bind_pipeline(pipeline)?;
    for &region in frame.pass_plan.composite_regions(backdrop) {
        let push = backdrop_composite_push(
            backdrop,
            region,
            descriptor_index,
            sampler_index,
            intermediate_extent,
            frame.target.viewport,
        );
        push_struct(&mut rendering, &push)?;
        draw_over_damage(
            &mut rendering,
            RendererRect2D::new(region.x, region.y, region.width, region.height),
            frame,
        )?;
    }
    rendering.end();
    Ok(())
}

fn backdrop_composite_push(
    backdrop: BackdropPass,
    region: tensor_util::Rect,
    descriptor_index: u32,
    sampler_index: u32,
    intermediate_extent: Extent2D,
    viewport: tensor_util::Rect,
) -> DrawPushData {
    let offset_x = region.x.saturating_sub(backdrop.sample_region.x) as f32;
    let offset_y = region.y.saturating_sub(backdrop.sample_region.y) as f32;
    let width = intermediate_extent.width as f32;
    let height = intermediate_extent.height as f32;
    DrawPushData {
        descriptor_index,
        corner_radius: 0,
        opacity: 1.0,
        sampler_index,
        destination: destination_to_ndc(region, viewport),
        uv_origin_axis_x: [
            offset_x / width,
            offset_y / height,
            region.width as f32 / width,
            0.0,
        ],
        uv_axis_y_surface_size: [
            0.0,
            region.height as f32 / height,
            region.width as f32,
            region.height as f32,
        ],
    }
}

fn push_struct<T: Copy>(
    rendering: &mut vulkan_renderer::RenderingEncoder<'_>,
    value: &T,
) -> Result<(), RendererError> {
    let bytes =
        unsafe { slice::from_raw_parts((value as *const T).cast::<u8>(), mem::size_of::<T>()) };
    rendering.push_data(0, bytes)
}

fn undefined_state(queue_family: u32) -> ResourceState {
    ResourceState::image(RenderGraphImageState::Undefined, queue_family)
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum BackdropScenePlanError {
    #[error("backdrop-dependent view {0:?} has no prepared scene draw")]
    MissingView(ViewId),
}

#[cfg(test)]
mod tests;
