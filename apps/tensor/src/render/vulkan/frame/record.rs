#![allow(unsafe_code)]

use std::{mem, slice};

use tensor_util::Rect;
use thiserror::Error;
use vulkan_renderer::vulkanalia::vk;
use vulkan_renderer::{
    BarrierBatch, ColorAttachment, CommandEncoder, Error as RendererError, ForeignImageState,
    LoadOp, Rect2D as RendererRect2D, RenderGraphImageState, RenderingDescriptor, ResolveMode,
    ResourceBinding, ResourceState, StoreOp, TextureLayout, Viewport,
};

use crate::render::{
    FrameSubmission,
    frame::{HeapAllocation, SceneDrawCommand},
};

use super::super::{import::ClientImageInfo, target::NativeOutputImageInfo};

pub(super) const DRAW_PUSH_DATA_SIZE: u64 = mem::size_of::<DrawPushData>() as u64;
pub(super) const CURSOR_PUSH_DATA_SIZE: u64 = mem::size_of::<CursorPushData>() as u64;
pub(super) const FOCUS_RING_PUSH_DATA_SIZE: u64 = mem::size_of::<FocusRingPushData>() as u64;

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
    push: DrawPushData,
    scissor: vk::Rect2D,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PreparedFocusRingDraw {
    push: FocusRingPushData,
    scissor: vk::Rect2D,
}

/// Pipeline-ready scene command. Keeping this sequence separate from resource
/// preparation lets client descriptors remain batched while Vulkan records the
/// exact scene stack supplied by ECS extraction.
#[derive(Clone, Copy, Debug)]
pub(super) enum PreparedSceneDraw {
    Client(PreparedDraw),
    FocusRing(PreparedFocusRingDraw),
}

pub(super) fn prepare_draws(
    frame: &FrameSubmission,
    descriptor_stride: u64,
    sampler_index: u32,
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
            let origin = draw.sample_transform.origin();
            let axis_x = draw.sample_transform.axis_x();
            let axis_y = draw.sample_transform.axis_y();
            Ok(PreparedDraw {
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
                scissor: vk::Rect2D {
                    offset: vk::Offset2D {
                        x: draw.clip.x,
                        y: draw.clip.y,
                    },
                    extent: vk::Extent2D {
                        width: draw.clip.width,
                        height: draw.clip.height,
                    },
                },
            })
        })
        .collect()
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
                scissor: vk::Rect2D {
                    offset: vk::Offset2D {
                        x: ring.clip.x,
                        y: ring.clip.y,
                    },
                    extent: vk::Extent2D {
                        width: ring.clip.width,
                        height: ring.clip.height,
                    },
                },
            })
        })
        .collect()
}

pub(super) fn prepare_scene_draws(
    commands: &[SceneDrawCommand],
    draws: &[PreparedDraw],
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

fn descriptor_index(
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
    pub(super) focus_ring_pipeline: Option<&'a vulkan_renderer::GraphicsPipeline>,
    pub(super) cursor_pipeline: Option<&'a vulkan_renderer::GraphicsPipeline>,
    pub(super) graphics_queue_family: u32,
    pub(super) scene_draws: &'a [PreparedSceneDraw],
    pub(super) cursors: &'a PreparedCursorDraws,
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
        focus_ring_pipeline,
        cursor_pipeline,
        graphics_queue_family,
        scene_draws,
        cursors,
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
        load_op: LoadOp::Clear([0.018, 0.024, 0.034, 1.0]),
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
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum BoundScenePipeline {
        Client,
        FocusRing,
    }
    let mut bound_pipeline = None;
    for draw in scene_draws {
        match draw {
            PreparedSceneDraw::Client(draw) => {
                let Some(pipeline) = client_pipeline else {
                    continue;
                };
                if bound_pipeline != Some(BoundScenePipeline::Client) {
                    rendering.bind_pipeline(pipeline)?;
                    bound_pipeline = Some(BoundScenePipeline::Client);
                }
                let push_bytes = unsafe {
                    slice::from_raw_parts(
                        (&draw.push as *const DrawPushData).cast::<u8>(),
                        mem::size_of::<DrawPushData>(),
                    )
                };
                rendering.set_scissor(renderer_scissor(draw.scissor))?;
                rendering.push_data(0, push_bytes)?;
                unsafe { rendering.draw(0..6, 0..1)? };
            }
            PreparedSceneDraw::FocusRing(ring) => {
                let Some(pipeline) = focus_ring_pipeline else {
                    continue;
                };
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
                rendering.set_scissor(renderer_scissor(ring.scissor))?;
                rendering.push_data(0, push_bytes)?;
                unsafe { rendering.draw(0..6, 0..1)? };
            }
        }
    }
    for cursor in cursors.iter() {
        match cursor {
            PreparedCursorDraw::Vector { push, scissor } => {
                let Some(pipeline) = cursor_pipeline else {
                    continue;
                };
                rendering.bind_pipeline(pipeline)?;
                let push_bytes = unsafe {
                    slice::from_raw_parts(
                        (&push as *const CursorPushData).cast::<u8>(),
                        mem::size_of::<CursorPushData>(),
                    )
                };
                rendering.set_scissor(renderer_scissor(scissor))?;
                rendering.push_data(0, push_bytes)?;
                unsafe { rendering.draw(0..6, 0..1)? };
            }
            PreparedCursorDraw::Texture(draw) => {
                let Some(pipeline) = client_pipeline else {
                    continue;
                };
                rendering.bind_pipeline(pipeline)?;
                let push_bytes = unsafe {
                    slice::from_raw_parts(
                        (&draw.push as *const DrawPushData).cast::<u8>(),
                        mem::size_of::<DrawPushData>(),
                    )
                };
                rendering.set_scissor(renderer_scissor(draw.scissor))?;
                rendering.push_data(0, push_bytes)?;
                unsafe { rendering.draw(0..6, 0..1)? };
            }
        }
    }
    rendering.end();

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
        color_attachment_state(graphics_queue_family),
        ResourceState::foreign_image(ForeignImageState::General),
    )?;
    unsafe { encoder.pipeline_barrier(&barriers.release) };
    Ok(())
}

fn renderer_scissor(scissor: vk::Rect2D) -> RendererRect2D {
    RendererRect2D::new(
        scissor.offset.x,
        scissor.offset.y,
        scissor.extent.width,
        scissor.extent.height,
    )
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
pub(super) use cursor::{PreparedCursorDraw, PreparedCursorDraws, prepare_cursor_draws};

#[cfg(test)]
mod tests;
