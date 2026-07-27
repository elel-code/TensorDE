#![allow(unsafe_code)]

use std::{mem, slice};

use tensor_util::Rect;
use thiserror::Error;
use vulkanalia::vk::{
    DeviceV1_0, DeviceV1_3, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder,
};
use vulkanalia::{Device, vk};

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
    padding: f32,
    destination: [f32; 4],
    uv_origin_axis_x: [f32; 4],
    uv_axis_y_surface_size: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CursorPushData {
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
pub(super) struct PreparedCursorDraw {
    push: CursorPushData,
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
    resource_heap_base: u64,
) -> Result<Vec<PreparedDraw>, FrameRecordError> {
    if descriptor_stride == 0 {
        return Err(FrameRecordError::ZeroDescriptorStride);
    }
    let descriptor_offset = frame
        .descriptors
        .offset
        .checked_sub(resource_heap_base)
        .ok_or(FrameRecordError::DescriptorBeforeHeapBase {
            offset: frame.descriptors.offset,
            base: resource_heap_base,
        })?;
    if !descriptor_offset.is_multiple_of(descriptor_stride) {
        return Err(FrameRecordError::DescriptorOffsetMisaligned {
            offset: frame.descriptors.offset,
            base: resource_heap_base,
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
            let descriptor_index = descriptor_index(
                frame.descriptors,
                descriptor_stride,
                resource_heap_base,
                draw.image_descriptor,
            )?;
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
                    padding: 0.0,
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

pub(super) fn prepare_cursor_draw(
    frame: &FrameSubmission,
) -> Result<Option<PreparedCursorDraw>, FrameRecordError> {
    let Some(cursor) = frame.draw_plan.cursor() else {
        return Ok(None);
    };
    let viewport = validate_viewport(frame.target.viewport)?;
    Ok(Some(PreparedCursorDraw {
        push: CursorPushData {
            destination: destination_to_ndc(cursor.destination, viewport),
        },
        scissor: vk::Rect2D {
            offset: vk::Offset2D {
                x: cursor.clip.x,
                y: cursor.clip.y,
            },
            extent: vk::Extent2D {
                width: cursor.clip.width,
                height: cursor.clip.height,
            },
        },
    }))
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
    resource_heap_base: u64,
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
    let relative_heap_offset = byte_offset.checked_sub(resource_heap_base).ok_or(
        FrameRecordError::DescriptorBeforeHeapBase {
            offset: byte_offset,
            base: resource_heap_base,
        },
    )?;
    u32::try_from(relative_heap_offset / descriptor_stride)
        .map_err(|_| FrameRecordError::DescriptorIndexOverflow)
}

pub(super) struct SceneRecord<'a> {
    pub(super) frame: &'a FrameSubmission,
    pub(super) output: NativeOutputImageInfo,
    pub(super) clients: &'a [ClientImageInfo],
    pub(super) client_pipeline: Option<vk::Pipeline>,
    pub(super) focus_ring_pipeline: Option<(vk::Pipeline, vk::PipelineLayout)>,
    pub(super) cursor_pipeline: Option<(vk::Pipeline, vk::PipelineLayout)>,
    pub(super) graphics_queue_family: u32,
    pub(super) scene_draws: &'a [PreparedSceneDraw],
    pub(super) cursor: Option<PreparedCursorDraw>,
}

pub(super) unsafe fn record_scene(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene: SceneRecord<'_>,
) {
    let SceneRecord {
        frame,
        output,
        clients,
        client_pipeline,
        focus_ring_pipeline,
        cursor_pipeline,
        graphics_queue_family,
        scene_draws,
        cursor,
    } = scene;
    let subresource = color_subresource();
    let mut acquires = Vec::with_capacity(1 + clients.len());
    acquires.push(output_acquire(output, subresource, graphics_queue_family));
    acquires.extend(
        clients
            .iter()
            .copied()
            .map(|image| client_acquire(image, subresource, graphics_queue_family)),
    );
    let acquire_dependency = vk::DependencyInfo::builder().image_memory_barriers(&acquires);
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &acquire_dependency) };

    let clear = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0.018, 0.024, 0.034, 1.0],
        },
    };
    let color_attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(output.view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear)
        .build();
    let render_area = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent: vk::Extent2D {
            width: frame.target.viewport.width,
            height: frame.target.viewport.height,
        },
    };
    let rendering = vk::RenderingInfo::builder()
        .render_area(render_area)
        .layer_count(1)
        .color_attachments(slice::from_ref(&color_attachment));
    unsafe { device.cmd_begin_rendering(command_buffer, &rendering) };

    let viewport = vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: frame.target.viewport.width as f32,
        height: frame.target.viewport.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    unsafe { device.cmd_set_viewport(command_buffer, 0, slice::from_ref(&viewport)) };
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
                    unsafe {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline,
                        );
                    }
                    bound_pipeline = Some(BoundScenePipeline::Client);
                }
                let push_bytes = unsafe {
                    slice::from_raw_parts(
                        (&draw.push as *const DrawPushData).cast::<u8>(),
                        mem::size_of::<DrawPushData>(),
                    )
                };
                let push_range = vk::HostAddressRangeConstEXT::builder().address(push_bytes);
                let push = vk::PushDataInfoEXT::builder().offset(0).data(push_range);
                unsafe {
                    device.cmd_set_scissor(command_buffer, 0, slice::from_ref(&draw.scissor));
                    device.cmd_push_data_ext(command_buffer, &push);
                    device.cmd_draw(command_buffer, 6, 1, 0, 0);
                }
            }
            PreparedSceneDraw::FocusRing(ring) => {
                let Some((pipeline, layout)) = focus_ring_pipeline else {
                    continue;
                };
                if bound_pipeline != Some(BoundScenePipeline::FocusRing) {
                    unsafe {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline,
                        );
                    }
                    bound_pipeline = Some(BoundScenePipeline::FocusRing);
                }
                let push_bytes = unsafe {
                    slice::from_raw_parts(
                        (&ring.push as *const FocusRingPushData).cast::<u8>(),
                        mem::size_of::<FocusRingPushData>(),
                    )
                };
                unsafe {
                    device.cmd_set_scissor(command_buffer, 0, slice::from_ref(&ring.scissor));
                    device.cmd_push_constants(
                        command_buffer,
                        layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        push_bytes,
                    );
                    device.cmd_draw(command_buffer, 6, 1, 0, 0);
                }
            }
        }
    }
    if let (Some((pipeline, layout)), Some(cursor)) = (cursor_pipeline, cursor) {
        let push_bytes = unsafe {
            slice::from_raw_parts(
                (&cursor.push as *const CursorPushData).cast::<u8>(),
                mem::size_of::<CursorPushData>(),
            )
        };
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_set_scissor(command_buffer, 0, slice::from_ref(&cursor.scissor));
            device.cmd_push_constants(
                command_buffer,
                layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                push_bytes,
            );
            device.cmd_draw(command_buffer, 6, 1, 0, 0);
        }
    }
    unsafe { device.cmd_end_rendering(command_buffer) };

    let mut releases = Vec::with_capacity(1 + clients.len());
    releases.extend(
        clients
            .iter()
            .copied()
            .map(|image| client_release(image, subresource, graphics_queue_family)),
    );
    releases.push(output_release(output, subresource, graphics_queue_family));
    let release_dependency = vk::DependencyInfo::builder().image_memory_barriers(&releases);
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &release_dependency) };
}

fn color_subresource() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

fn output_acquire(
    image: NativeOutputImageInfo,
    subresource: vk::ImageSubresourceRange,
    graphics_queue_family: u32,
) -> vk::ImageMemoryBarrier2 {
    let (old_layout, source, destination) = if image.foreign_owned {
        (
            vk::ImageLayout::GENERAL,
            vk::QUEUE_FAMILY_FOREIGN_EXT,
            graphics_queue_family,
        )
    } else {
        (
            vk::ImageLayout::UNDEFINED,
            vk::QUEUE_FAMILY_IGNORED,
            vk::QUEUE_FAMILY_IGNORED,
        )
    };
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::NONE)
        .src_access_mask(vk::AccessFlags2::NONE)
        .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .old_layout(old_layout)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .src_queue_family_index(source)
        .dst_queue_family_index(destination)
        .image(image.image)
        .subresource_range(subresource)
        .build()
}

fn client_acquire(
    image: ClientImageInfo,
    subresource: vk::ImageSubresourceRange,
    graphics_queue_family: u32,
) -> vk::ImageMemoryBarrier2 {
    let (old_layout, source, destination) = if image.foreign_owned {
        let old_layout = if image.needs_initial_acquire {
            // An imported dma-buf starts with Vulkan's UNDEFINED layout, but
            // its contents belong to the foreign producer.  Keeping the
            // FOREIGN ownership transfer paired with UNDEFINED is the
            // content-preserving first acquire required for explicit DRM
            // modifiers; later frames use GENERAL after a release/acquire
            // round-trip.
            vk::ImageLayout::UNDEFINED
        } else {
            vk::ImageLayout::GENERAL
        };
        (
            old_layout,
            vk::QUEUE_FAMILY_FOREIGN_EXT,
            graphics_queue_family,
        )
    } else {
        (
            vk::ImageLayout::UNDEFINED,
            vk::QUEUE_FAMILY_IGNORED,
            vk::QUEUE_FAMILY_IGNORED,
        )
    };
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::NONE)
        .src_access_mask(vk::AccessFlags2::NONE)
        .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .old_layout(old_layout)
        .new_layout(vk::ImageLayout::GENERAL)
        .src_queue_family_index(source)
        .dst_queue_family_index(destination)
        .image(image.image)
        .subresource_range(subresource)
        .build()
}

fn client_release(
    image: ClientImageInfo,
    subresource: vk::ImageSubresourceRange,
    graphics_queue_family: u32,
) -> vk::ImageMemoryBarrier2 {
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .dst_stage_mask(vk::PipelineStageFlags2::NONE)
        .dst_access_mask(vk::AccessFlags2::NONE)
        .old_layout(vk::ImageLayout::GENERAL)
        .new_layout(vk::ImageLayout::GENERAL)
        .src_queue_family_index(graphics_queue_family)
        .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
        .image(image.image)
        .subresource_range(subresource)
        .build()
}

fn output_release(
    image: NativeOutputImageInfo,
    subresource: vk::ImageSubresourceRange,
    graphics_queue_family: u32,
) -> vk::ImageMemoryBarrier2 {
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::NONE)
        .dst_access_mask(vk::AccessFlags2::NONE)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::GENERAL)
        .src_queue_family_index(graphics_queue_family)
        .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
        .image(image.image)
        .subresource_range(subresource)
        .build()
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(super) enum FrameRecordError {
    #[error("descriptor stride must be non-zero")]
    ZeroDescriptorStride,
    #[error("descriptor offset {offset} is before resource heap base {base}")]
    DescriptorBeforeHeapBase { offset: u64, base: u64 },
    #[error(
        "descriptor allocation offset {offset} relative to heap base {base} is not aligned to stride {stride}"
    )]
    DescriptorOffsetMisaligned { offset: u64, base: u64, stride: u64 },
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

#[cfg(test)]
mod tests;
