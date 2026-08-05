#![allow(unsafe_code)]

use std::{mem, slice};

use tensor_util::Rect;
use thiserror::Error;
use vulkan_renderer::{
    BarrierBatch, ColorAttachment, ColorShaderEncoding, ColorTransformShaderData, CommandEncoder,
    Error as RendererError, ForeignImageState, LoadOp, Rect2D as RendererRect2D,
    RenderGraphImageState, RenderingDescriptor, ResolveMode, ResourceBinding, ResourceState,
    StoreOp, TextureFormat, TextureLayout, Viewport,
};

use crate::{
    ecs::ViewId,
    render::{
        FrameSubmission,
        frame::{HeapAllocation, OutputLoad, SceneDrawCommand},
    },
};

use super::super::{import::ClientImageInfo, target::NativeOutputImageInfo};

mod backdrop;
mod capture;
pub(super) use backdrop::{
    BackdropScenePlanError, BackdropSceneRecord, BackdropSceneScratch, record_backdrop_scene,
};
pub(super) use capture::{CaptureRecord, record_capture_tap};

pub(super) const DRAW_PUSH_DATA_SIZE: u64 = mem::size_of::<DrawPushData>() as u64;
pub(super) const MANAGED_DRAW_PUSH_DATA_SIZE: u64 = mem::size_of::<ManagedDrawPushData>() as u64;
pub(super) const CURSOR_PUSH_DATA_SIZE: u64 = mem::size_of::<CursorPushData>() as u64;
pub(super) const FOCUS_RING_PUSH_DATA_SIZE: u64 = mem::size_of::<FocusRingPushData>() as u64;
pub(super) const SHADOW_PUSH_DATA_SIZE: u64 = mem::size_of::<ShadowPushData>() as u64;
const OUTPUT_CLEAR: [f32; 4] = [0.018, 0.024, 0.034, 1.0];

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct DrawPushData {
    descriptor_index: u32,
    corner_radius: u32,
    opacity: f32,
    sampler_index: u32,
    destination: [f32; 4],
    uv_origin_axis_x: [f32; 4],
    uv_axis_y_surface_size: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct ManagedDrawPushData {
    draw: DrawPushData,
    color: ColorTransformShaderData,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(super) struct CursorPushData {
    destination: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct FocusRingPushData {
    destination: [f32; 4],
    color: [f32; 4],
    inner_rect: [f32; 4],
    shape: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PreparedDraw {
    view_id: Option<ViewId>,
    push: DrawPushData,
    color: Option<ColorTransformShaderData>,
    scissor: RendererRect2D,
}

impl PreparedDraw {
    pub(super) const fn is_color_managed(&self) -> bool {
        self.color.is_some()
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PreparedFocusRingDraw {
    view_id: ViewId,
    push: FocusRingPushData,
    scissor: RendererRect2D,
}

mod shadow;
pub(super) use shadow::{PreparedShadowDraw, ShadowPushData, prepare_shadow_draws};

/// Pipeline-ready scene command. Keeping this sequence separate from resource
/// preparation lets client descriptors remain batched while Vulkan records the
/// exact scene stack supplied by ECS extraction.
#[derive(Clone, Copy, Debug)]
pub(super) enum PreparedSceneDraw {
    Client(PreparedDraw),
    Shadow(PreparedShadowDraw),
    FocusRing(PreparedFocusRingDraw),
}

impl PreparedSceneDraw {
    pub(super) const fn view_id(self) -> ViewId {
        match self {
            Self::Client(draw) => draw
                .view_id
                .expect("scene client draws always retain their source view"),
            Self::Shadow(shadow) => shadow.view_id,
            Self::FocusRing(ring) => ring.view_id,
        }
    }
}

pub(super) fn prepare_draws(
    frame: &FrameSubmission,
    descriptor_stride: u64,
    sampler_index: u32,
    source_format: impl Fn(u32) -> Option<TextureFormat>,
) -> Result<Vec<PreparedDraw>, FrameRecordError> {
    if descriptor_stride == 0 {
        return Err(FrameRecordError::ZeroDescriptorStride);
    }
    // Bindless indices are absolute heap element indices, so the frame
    // allocation itself must sit on a stride boundary.
    if !frame.descriptors.offset.is_multiple_of(descriptor_stride) {
        return Err(FrameRecordError::DescriptorOffsetMisaligned {
            offset: frame.descriptors.offset,
            stride: descriptor_stride,
        });
    }
    let viewport = validate_viewport(frame.target.viewport)?;

    frame
        .draw_plan
        .draws()
        .iter()
        .map(|draw| {
            if draw.image_descriptor == 0 || draw.image_descriptor > frame.client_image_descriptors
            {
                return Err(FrameRecordError::InvalidImageDescriptor(
                    draw.image_descriptor,
                ));
            }
            let descriptor_index =
                descriptor_index(frame.descriptors, descriptor_stride, draw.image_descriptor)?;
            let texture_format = source_format(draw.image_descriptor).ok_or(
                FrameRecordError::MissingClientImageFormat(draw.image_descriptor),
            )?;
            let origin = draw.sample_transform.origin();
            let axis_x = draw.sample_transform.axis_x();
            let axis_y = draw.sample_transform.axis_y();
            Ok(PreparedDraw {
                view_id: Some(draw.view_id),
                push: DrawPushData {
                    descriptor_index,
                    corner_radius: frame
                        .target
                        .scale
                        .physical_length_round(draw.effects.corner_radius),
                    opacity: draw.effects.opacity.as_f32() * draw.alpha.as_f32(),
                    sampler_index,
                    destination: destination_to_ndc(draw.destination, viewport),
                    uv_origin_axis_x: [origin.0, origin.1, axis_x.0, axis_x.1],
                    uv_axis_y_surface_size: [
                        axis_y.0,
                        axis_y.1,
                        draw.destination.width as f32,
                        draw.destination.height as f32,
                    ],
                },
                color: (!draw.color.is_identity()).then(|| {
                    draw.color.shader_data(ColorShaderEncoding {
                        source_view_decodes_transfer: is_srgb(texture_format),
                        target_attachment_encodes_transfer: is_srgb(draw.color.target.format),
                    })
                }),
                scissor: RendererRect2D::new(
                    draw.clip.x,
                    draw.clip.y,
                    draw.clip.width,
                    draw.clip.height,
                ),
            })
        })
        .collect()
}

const fn is_srgb(format: TextureFormat) -> bool {
    matches!(format, TextureFormat::Rgba8Srgb | TextureFormat::Bgra8Srgb)
}

pub(super) fn prepare_focus_ring_draws(
    frame: &FrameSubmission,
) -> Result<Vec<PreparedFocusRingDraw>, FrameRecordError> {
    let viewport = validate_viewport(frame.target.viewport)?;
    frame
        .draw_plan
        .focus_rings()
        .iter()
        .map(|ring| {
            if !ring.destination.contains_rect(ring.inner) {
                return Err(FrameRecordError::InvalidFocusRingGeometry);
            }
            let inner_x = ring.inner.x.saturating_sub(ring.destination.x);
            let inner_y = ring.inner.y.saturating_sub(ring.destination.y);
            Ok(PreparedFocusRingDraw {
                view_id: ring.view_id,
                push: FocusRingPushData {
                    destination: destination_to_ndc(ring.destination, viewport),
                    color: linear_rgba(ring.color),
                    inner_rect: [
                        inner_x as f32,
                        inner_y as f32,
                        ring.inner.width as f32,
                        ring.inner.height as f32,
                    ],
                    shape: [
                        ring.outer_radius as f32,
                        ring.inner_radius as f32,
                        ring.destination.width as f32,
                        ring.destination.height as f32,
                    ],
                },
                scissor: RendererRect2D::new(
                    ring.clip.x,
                    ring.clip.y,
                    ring.clip.width,
                    ring.clip.height,
                ),
            })
        })
        .collect()
}

pub(super) fn prepare_scene_draws(
    commands: &[SceneDrawCommand],
    draws: &[PreparedDraw],
    shadows: &[PreparedShadowDraw],
    focus_rings: &[PreparedFocusRingDraw],
) -> Result<Vec<PreparedSceneDraw>, FrameRecordError> {
    commands
        .iter()
        .map(|command| match *command {
            SceneDrawCommand::Client(index) => draws
                .get(index)
                .copied()
                .map(PreparedSceneDraw::Client)
                .ok_or(FrameRecordError::MissingSceneDraw {
                    kind: "client",
                    index,
                }),
            SceneDrawCommand::Shadow(index) => shadows
                .get(index)
                .copied()
                .map(PreparedSceneDraw::Shadow)
                .ok_or(FrameRecordError::MissingSceneDraw {
                    kind: "shadow",
                    index,
                }),
            SceneDrawCommand::FocusRing(index) => focus_rings
                .get(index)
                .copied()
                .map(PreparedSceneDraw::FocusRing)
                .ok_or(FrameRecordError::MissingSceneDraw {
                    kind: "focus-ring",
                    index,
                }),
        })
        .collect()
}

fn push_client_draw(
    rendering: &mut vulkan_renderer::RenderingEncoder<'_>,
    draw: &PreparedDraw,
) -> Result<(), RendererError> {
    if let Some(color) = draw.color {
        let push = ManagedDrawPushData {
            draw: draw.push,
            color,
        };
        let bytes = unsafe {
            slice::from_raw_parts(
                (&push as *const ManagedDrawPushData).cast::<u8>(),
                mem::size_of::<ManagedDrawPushData>(),
            )
        };
        rendering.push_data(0, bytes)
    } else {
        let bytes = unsafe {
            slice::from_raw_parts(
                (&draw.push as *const DrawPushData).cast::<u8>(),
                mem::size_of::<DrawPushData>(),
            )
        };
        rendering.push_data(0, bytes)
    }
}

fn linear_rgba(color: crate::scene::LinearRgba16) -> [f32; 4] {
    [
        f32::from(color.red) / f32::from(u16::MAX),
        f32::from(color.green) / f32::from(u16::MAX),
        f32::from(color.blue) / f32::from(u16::MAX),
        f32::from(color.alpha) / f32::from(u16::MAX),
    ]
}

fn validate_viewport(viewport: Rect) -> Result<Rect, FrameRecordError> {
    if viewport.width == 0 || viewport.height == 0 {
        return Err(FrameRecordError::InvalidViewport);
    }
    Ok(viewport)
}

/// Convert Tensor's top-left physical rectangle convention into the Vulkan
/// NDC convention used with the positive-height native output viewport.
///
/// The fragment shader still needs the physical dimensions for rounded-corner
/// coverage, so those travel separately in the final two push-constant words.
fn destination_to_ndc(destination: Rect, viewport: Rect) -> [f32; 4] {
    let width = viewport.width as f32;
    let height = viewport.height as f32;
    let x = (i64::from(destination.x) - i64::from(viewport.x)) as f32;
    let y = (i64::from(destination.y) - i64::from(viewport.y)) as f32;
    [
        x / width * 2.0 - 1.0,
        y / height * 2.0 - 1.0,
        destination.width as f32 / width * 2.0,
        destination.height as f32 / height * 2.0,
    ]
}

pub(in crate::render::vulkan::frame) fn descriptor_index(
    allocation: HeapAllocation,
    descriptor_stride: u64,
    relative_index: u32,
) -> Result<u32, FrameRecordError> {
    let relative_offset = descriptor_stride
        .checked_mul(u64::from(relative_index))
        .ok_or(FrameRecordError::DescriptorIndexOverflow)?;
    let byte_offset = allocation
        .offset
        .checked_add(relative_offset)
        .ok_or(FrameRecordError::DescriptorIndexOverflow)?;
    if byte_offset >= allocation.offset.saturating_add(allocation.size) {
        return Err(FrameRecordError::DescriptorOutsideAllocation {
            index: relative_index,
            allocation,
        });
    }
    u32::try_from(byte_offset / descriptor_stride)
        .map_err(|_| FrameRecordError::DescriptorIndexOverflow)
}

pub(super) struct SceneRecord<'a> {
    pub(super) frame: &'a FrameSubmission,
    pub(super) output: &'a NativeOutputImageInfo,
    pub(super) clients: &'a [ClientImageInfo],
    pub(super) client_pipeline: Option<&'a vulkan_renderer::GraphicsPipeline>,
    pub(super) managed_client_pipeline: Option<&'a vulkan_renderer::GraphicsPipeline>,
    pub(super) shadow_pipeline: Option<&'a vulkan_renderer::GraphicsPipeline>,
    pub(super) focus_ring_pipeline: Option<&'a vulkan_renderer::GraphicsPipeline>,
    pub(super) cursor_pipeline: Option<&'a vulkan_renderer::GraphicsPipeline>,
    pub(super) graphics_queue_family: u32,
    pub(super) scene_draws: &'a [PreparedSceneDraw],
    pub(super) cursors: &'a PreparedCursorDraws,
    pub(super) capture: Option<CaptureRecord<'a>>,
}

/// Reusable frame-local synchronization scratch. Its vectors retain capacity
/// across repaints so scene image count changes do not allocate on the hot
/// command-recording path.
#[derive(Debug)]
pub(super) struct SceneBarrierScratch {
    upload: BarrierBatch,
    acquire: BarrierBatch,
    release: BarrierBatch,
}

impl SceneBarrierScratch {
    pub(super) fn new() -> Self {
        Self {
            upload: BarrierBatch::with_capacity(0, 64),
            acquire: BarrierBatch::with_capacity(0, 65),
            release: BarrierBatch::with_capacity(0, 65),
        }
    }

    fn clear(&mut self) {
        self.upload.clear();
        self.acquire.clear();
        self.release.clear();
    }
}

pub(super) fn record_scene(
    encoder: &mut CommandEncoder,
    scene: SceneRecord<'_>,
    barriers: &mut SceneBarrierScratch,
) -> Result<(), RendererError> {
    let SceneRecord {
        frame,
        output,
        clients,
        client_pipeline,
        managed_client_pipeline,
        shadow_pipeline,
        focus_ring_pipeline,
        cursor_pipeline,
        graphics_queue_family,
        scene_draws,
        cursors,
        capture,
    } = scene;
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
        output_source_state(output.foreign_owned, graphics_queue_family),
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

    let color_attachments = [Some(ColorAttachment {
        view: output.image.as_attachment(),
        layout: TextureLayout::ColorAttachment,
        resolve_target: None,
        resolve_layout: TextureLayout::Undefined,
        resolve_mode: ResolveMode::None,
        load_op: match frame.pass_plan.output_load() {
            OutputLoad::Clear => LoadOp::Clear(OUTPUT_CLEAR),
            OutputLoad::Preserve => LoadOp::Load,
        },
        store_op: StoreOp::Store,
    })];
    let rendering_descriptor = RenderingDescriptor {
        label: Some("tensor-client-frame"),
        render_area: RendererRect2D::new(
            0,
            0,
            frame.target.viewport.width,
            frame.target.viewport.height,
        ),
        layer_count: 1,
        view_mask: 0,
        color_attachments: &color_attachments,
        depth_attachment: None,
        stencil_attachment: None,
        multisampled_render_to_single_sampled: None,
    };
    let mut rendering = unsafe { encoder.begin_rendering(&rendering_descriptor)? };
    rendering.set_viewport(Viewport {
        x: 0.0,
        y: 0.0,
        width: frame.target.viewport.width as f32,
        height: frame.target.viewport.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    })?;
    if frame.pass_plan.output_load() == OutputLoad::Preserve {
        for damage in frame.render_damage.regions() {
            let rect = RendererRect2D::new(damage.x, damage.y, damage.width, damage.height);
            rendering.clear_color_attachment(0, OUTPUT_CLEAR, std::slice::from_ref(&rect))?;
        }
    }
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum BoundScenePipeline {
        Client,
        ManagedClient,
        Shadow,
        FocusRing,
    }
    let mut bound_pipeline = None;
    for draw in scene_draws {
        match draw {
            PreparedSceneDraw::Client(draw) => {
                let (pipeline, pipeline_kind) = if draw.color.is_some() {
                    (managed_client_pipeline, BoundScenePipeline::ManagedClient)
                } else {
                    (client_pipeline, BoundScenePipeline::Client)
                };
                let Some(pipeline) = pipeline else {
                    continue;
                };
                if !has_render_damage(draw.scissor, frame) {
                    continue;
                }
                if bound_pipeline != Some(pipeline_kind) {
                    rendering.bind_pipeline(pipeline)?;
                    bound_pipeline = Some(pipeline_kind);
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
                if bound_pipeline != Some(BoundScenePipeline::Shadow) {
                    rendering.bind_pipeline(pipeline)?;
                    bound_pipeline = Some(BoundScenePipeline::Shadow);
                }
                let push_bytes = unsafe {
                    slice::from_raw_parts(
                        (&shadow.push as *const ShadowPushData).cast::<u8>(),
                        mem::size_of::<ShadowPushData>(),
                    )
                };
                rendering.push_data(0, push_bytes)?;
                draw_over_damage(&mut rendering, shadow.scissor, frame)?;
            }
            PreparedSceneDraw::FocusRing(ring) => {
                let Some(pipeline) = focus_ring_pipeline else {
                    continue;
                };
                if !has_render_damage(ring.scissor, frame) {
                    continue;
                }
                if bound_pipeline != Some(BoundScenePipeline::FocusRing) {
                    rendering.bind_pipeline(pipeline)?;
                    bound_pipeline = Some(BoundScenePipeline::FocusRing);
                }
                let push_bytes = unsafe {
                    slice::from_raw_parts(
                        (&ring.push as *const FocusRingPushData).cast::<u8>(),
                        mem::size_of::<FocusRingPushData>(),
                    )
                };
                rendering.push_data(0, push_bytes)?;
                draw_over_damage(&mut rendering, ring.scissor, frame)?;
            }
        }
    }
    let capture_before_cursors =
        capture.is_some_and(|capture| capture.request.tap_before_software_cursors());
    if !capture_before_cursors {
        record_cursor_draws(
            &mut rendering,
            frame,
            cursors,
            client_pipeline,
            cursor_pipeline,
        )?;
    }
    rendering.end();

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
            client_pipeline,
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
        ResourceState::foreign_image(ForeignImageState::General),
    )?;
    unsafe { encoder.pipeline_barrier(&barriers.release) };
    Ok(())
}

fn has_render_damage(scissor: RendererRect2D, frame: &FrameSubmission) -> bool {
    frame
        .render_damage
        .regions()
        .iter()
        .any(|damage| intersect_scissor(scissor, *damage).is_some())
}

fn draw_over_damage(
    rendering: &mut vulkan_renderer::RenderingEncoder<'_>,
    scissor: RendererRect2D,
    frame: &FrameSubmission,
) -> Result<(), RendererError> {
    for damage in frame.render_damage.regions() {
        let Some(damaged_scissor) = intersect_scissor(scissor, *damage) else {
            continue;
        };
        rendering.set_scissor(damaged_scissor)?;
        unsafe { rendering.draw(0..6, 0..1)? };
    }
    Ok(())
}

fn intersect_scissor(scissor: RendererRect2D, damage: Rect) -> Option<RendererRect2D> {
    let scissor = Rect::new(
        scissor.origin.x,
        scissor.origin.y,
        scissor.extent.width,
        scissor.extent.height,
    );
    let intersection = scissor.intersection(damage)?;
    Some(RendererRect2D::new(
        intersection.x,
        intersection.y,
        intersection.width,
        intersection.height,
    ))
}

fn append_image_transition(
    barriers: &mut BarrierBatch,
    binding: ResourceBinding,
    source: ResourceState,
    destination: ResourceState,
) -> Result<(), RendererError> {
    barriers
        .add_image_transition(binding, source, destination)
        .map_err(|error| RendererError::Validation(error.to_string()))
}

fn color_attachment_state(graphics_queue_family: u32) -> ResourceState {
    ResourceState::image(
        RenderGraphImageState::ColorAttachmentWrite,
        graphics_queue_family,
    )
}

fn transfer_destination_state(graphics_queue_family: u32) -> ResourceState {
    ResourceState::image(
        RenderGraphImageState::TransferDestination,
        graphics_queue_family,
    )
}

fn transfer_source_state(graphics_queue_family: u32) -> ResourceState {
    ResourceState::image(RenderGraphImageState::TransferSource, graphics_queue_family)
}

fn sampled_state(graphics_queue_family: u32) -> ResourceState {
    ResourceState::image(
        RenderGraphImageState::FragmentSampledReadGeneral,
        graphics_queue_family,
    )
}

fn local_client_state(needs_initial_acquire: bool, graphics_queue_family: u32) -> ResourceState {
    if needs_initial_acquire {
        ResourceState::image(RenderGraphImageState::Undefined, graphics_queue_family)
    } else {
        sampled_state(graphics_queue_family)
    }
}

fn output_source_state(foreign_owned: bool, graphics_queue_family: u32) -> ResourceState {
    if foreign_owned {
        ResourceState::foreign_image(ForeignImageState::General)
    } else {
        ResourceState::image(RenderGraphImageState::Undefined, graphics_queue_family)
    }
}

fn client_source_state(image: &ClientImageInfo, graphics_queue_family: u32) -> ResourceState {
    if image.foreign_owned {
        // An imported dma-buf begins in Vulkan's UNDEFINED layout while its
        // contents still belong to the foreign producer.  Retaining FOREIGN
        // ownership in this semantic transition preserves the first sampled
        // contents; later frames round-trip through GENERAL.
        foreign_client_source_state(image.needs_initial_acquire)
    } else if image.upload_pending {
        transfer_destination_state(graphics_queue_family)
    } else {
        local_client_state(image.needs_initial_acquire, graphics_queue_family)
    }
}

fn foreign_client_source_state(needs_initial_acquire: bool) -> ResourceState {
    ResourceState::foreign_image(if needs_initial_acquire {
        ForeignImageState::Undefined
    } else {
        ForeignImageState::General
    })
}

fn client_release_state(foreign_owned: bool, graphics_queue_family: u32) -> ResourceState {
    if foreign_owned {
        ResourceState::foreign_image(ForeignImageState::General)
    } else {
        sampled_state(graphics_queue_family)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(super) enum FrameRecordError {
    #[error("descriptor stride must be non-zero")]
    ZeroDescriptorStride,
    #[error("descriptor allocation offset {offset} is not aligned to stride {stride}")]
    DescriptorOffsetMisaligned { offset: u64, stride: u64 },
    #[error("descriptor index arithmetic overflowed the push-index representation")]
    DescriptorIndexOverflow,
    #[error("draw references invalid client image descriptor {0}")]
    InvalidImageDescriptor(u32),
    #[error("draw references client image descriptor {0} without a resolved texture format")]
    MissingClientImageFormat(u32),
    #[error("draw descriptor {index} is outside frame allocation {allocation:?}")]
    DescriptorOutsideAllocation {
        index: u32,
        allocation: HeapAllocation,
    },
    #[error("frame viewport must be non-empty")]
    InvalidViewport,
    #[error("focus-ring inner geometry must be contained by its outer geometry")]
    InvalidFocusRingGeometry,
    #[error("scene command references missing {kind} draw {index}")]
    MissingSceneDraw { kind: &'static str, index: usize },
}

mod cursor;
#[cfg(test)]
pub(super) use cursor::PreparedCursorDraw;
pub(super) use cursor::{
    PreparedCursorDraws, prepare_cursor_draws, record_cursor_draws, record_cursor_scope,
};

#[cfg(test)]
mod tests;
