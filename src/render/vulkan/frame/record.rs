#![allow(unsafe_code)]

use std::{mem, slice};

use thiserror::Error;
use vulkanalia::vk::{
    DeviceV1_0, DeviceV1_3, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder,
};
use vulkanalia::{Device, vk};

use crate::render::{FrameSubmission, frame::HeapAllocation};

use super::super::{import::ClientImageInfo, target::NativeOutputImageInfo};

pub(super) const DRAW_PUSH_DATA_SIZE: u64 = mem::size_of::<DrawPushData>() as u64;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct DrawPushData {
    descriptor_index: u32,
    corner_radius: u32,
    opacity: f32,
    padding: f32,
    destination: [f32; 4],
    uv_origin_axis_x: [f32; 4],
    uv_axis_y_viewport: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PreparedDraw {
    push: DrawPushData,
    scissor: vk::Rect2D,
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
    let viewport = frame.target.viewport;
    if viewport.width == 0 || viewport.height == 0 {
        return Err(FrameRecordError::InvalidViewport);
    }

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
            let uv = draw.transform.uv_transform();
            Ok(PreparedDraw {
                push: DrawPushData {
                    descriptor_index,
                    corner_radius: frame
                        .target
                        .scale
                        .physical_length_round(draw.effects.corner_radius),
                    opacity: draw.effects.opacity.as_f32(),
                    padding: 0.0,
                    destination: [
                        draw.destination.x as f32,
                        draw.destination.y as f32,
                        draw.destination.width as f32,
                        draw.destination.height as f32,
                    ],
                    uv_origin_axis_x: [
                        f32::from(uv.origin.0),
                        f32::from(uv.origin.1),
                        f32::from(uv.axis_x.0),
                        f32::from(uv.axis_x.1),
                    ],
                    uv_axis_y_viewport: [
                        f32::from(uv.axis_y.0),
                        f32::from(uv.axis_y.1),
                        viewport.width as f32,
                        viewport.height as f32,
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
    pub(super) pipeline: Option<vk::Pipeline>,
    pub(super) graphics_queue_family: u32,
    pub(super) draws: &'a [PreparedDraw],
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
        pipeline,
        graphics_queue_family,
        draws,
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

    if let Some(pipeline) = pipeline {
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: frame.target.viewport.width as f32,
            height: frame.target.viewport.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_set_viewport(command_buffer, 0, slice::from_ref(&viewport));
        }
        for draw in draws {
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
}

#[cfg(test)]
mod tests {
    use vulkanalia::vk::Handle;

    use super::*;

    fn client_image(first: bool) -> ClientImageInfo {
        ClientImageInfo {
            image: vk::Image::null(),
            view_info: vk::ImageViewCreateInfo::default(),
            foreign_owned: true,
            needs_initial_acquire: first,
        }
    }

    #[test]
    fn descriptor_push_index_includes_the_frame_heap_offset() {
        assert_eq!(
            descriptor_index(
                HeapAllocation {
                    offset: 256,
                    size: 256,
                },
                32,
                128,
                3,
            )
            .unwrap(),
            7
        );
    }

    #[test]
    fn descriptor_push_index_rejects_out_of_slice_draws() {
        assert!(matches!(
            descriptor_index(
                HeapAllocation {
                    offset: 256,
                    size: 64,
                },
                32,
                128,
                2,
            ),
            Err(FrameRecordError::DescriptorOutsideAllocation { .. })
        ));
    }

    #[test]
    fn draw_push_data_stays_within_the_descriptor_heap_push_budget() {
        assert_eq!(DRAW_PUSH_DATA_SIZE, 64);
    }

    #[test]
    fn descriptor_push_index_is_relative_to_a_non_stride_aligned_reserved_range() {
        assert_eq!(
            descriptor_index(
                HeapAllocation {
                    offset: 192,
                    size: 256,
                },
                128,
                64,
                1,
            )
            .unwrap(),
            2
        );
    }

    #[test]
    fn first_foreign_client_acquire_preserves_imported_contents() {
        let barrier = client_acquire(client_image(true), color_subresource(), 7);
        assert_eq!(barrier.old_layout, vk::ImageLayout::UNDEFINED);
        assert_eq!(barrier.src_queue_family_index, vk::QUEUE_FAMILY_FOREIGN_EXT);
        assert_eq!(barrier.dst_queue_family_index, 7);
    }

    #[test]
    fn reused_foreign_client_acquire_uses_released_layout() {
        let barrier = client_acquire(client_image(false), color_subresource(), 7);
        assert_eq!(barrier.old_layout, vk::ImageLayout::GENERAL);
        assert_eq!(barrier.src_queue_family_index, vk::QUEUE_FAMILY_FOREIGN_EXT);
        assert_eq!(barrier.dst_queue_family_index, 7);
    }
}
